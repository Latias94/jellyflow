use super::*;

use crate::profile::ApplyPipelineError;
use crate::runtime::graph_runtime::{GraphRuntimeStats, GraphSnapshot, HistoryState, RuntimePatch};
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};

struct CommandHookCounters {
    before: Rc<Cell<usize>>,
    after: Rc<Cell<usize>>,
    events: Rc<Cell<usize>>,
}

impl CommandHookCounters {
    fn new() -> Self {
        Self {
            before: Rc::new(Cell::new(0)),
            after: Rc::new(Cell::new(0)),
            events: Rc::new(Cell::new(0)),
        }
    }

    fn values(&self) -> (usize, usize, usize) {
        (self.before.get(), self.after.get(), self.events.get())
    }
}

struct CountingMiddleware {
    before: Rc<Cell<usize>>,
    after: Rc<Cell<usize>>,
}

impl GraphRuntimeMiddleware for CountingMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.before.set(self.before.get() + 1);
        Ok(())
    }

    fn after_command(&mut self, _snapshot: GraphRuntimeMiddlewareSnapshot, _patch: &RuntimePatch) {
        self.after.set(self.after.get() + 1);
    }
}

struct RuntimeBaseline {
    snapshot: GraphSnapshot,
    history: HistoryState,
    stats: GraphRuntimeStats,
    hook_counts: (usize, usize, usize),
}

impl RuntimeBaseline {
    fn capture(runtime: &GraphRuntime, counters: &CommandHookCounters) -> Self {
        Self {
            snapshot: runtime.snapshot().unwrap(),
            history: runtime.history_state().unwrap(),
            stats: runtime.stats().unwrap(),
            hook_counts: counters.values(),
        }
    }
}

fn prepare_mixed_history(runtime: &GraphRuntime, node: NodeId) {
    let first = runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            node,
            serde_json::json!({ "step": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("first history seed applies")
        .version();
    let second = runtime
        .execute(GraphCommand::set_node_data(
            first,
            node,
            serde_json::json!({ "step": 2 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("second history seed applies")
        .version();
    runtime
        .execute(GraphCommand::undo(second))
        .into_result()
        .unwrap()
        .applied()
        .expect("history seed undo applies");

    let history = runtime.history_state().unwrap();
    assert!(history.can_undo());
    assert!(history.can_redo());
}

fn advance_with_redo(runtime: &GraphRuntime) {
    runtime
        .execute(GraphCommand::redo(runtime.snapshot().unwrap().version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("redo advances the runtime");
}

#[derive(Clone, Copy)]
enum ExpectedRevisionRelation {
    Equal,
    Stale,
}

fn assert_foreign_command_has_zero_side_effects(
    runtime: &GraphRuntime,
    command: GraphCommand,
    case: &str,
    baseline: &RuntimeBaseline,
    counters: &CommandHookCounters,
    relation: ExpectedRevisionRelation,
) {
    match relation {
        ExpectedRevisionRelation::Equal => assert_eq!(
            command.expected_version().revision(),
            baseline.snapshot.revision(),
            "{case}: equal-revision foreign path was not exercised",
        ),
        ExpectedRevisionRelation::Stale => assert_ne!(
            command.expected_version().revision(),
            baseline.snapshot.revision(),
            "{case}: command must also be stale so provenance precedence is exercised",
        ),
    }
    let command_provenance = command.expected_version().provenance();
    let command_identity = command.identity().clone();
    let completion = runtime.execute(command);
    assert_eq!(completion.identity(), &command_identity);
    assert_eq!(
        completion.kind(),
        crate::runtime::commands::CommandExecutionKind::Failed,
    );
    let error = completion
        .into_result()
        .expect_err("foreign command must fail before conflict handling");
    let GraphRuntimeError::ForeignCommandRuntime {
        command: actual_command,
        runtime: actual_runtime,
    } = error
    else {
        panic!("{case}: expected ForeignCommandRuntime, got {error:?}");
    };
    assert_eq!(actual_command, command_provenance, "{case}: command owner");
    assert_eq!(
        actual_runtime,
        baseline.snapshot.provenance(),
        "{case}: target runtime",
    );

    let after = runtime.snapshot().unwrap();
    assert!(
        baseline.snapshot.ptr_eq(&after),
        "{case}: graph snapshot changed",
    );
    assert_eq!(
        after.version(),
        baseline.snapshot.version(),
        "{case}: version changed",
    );
    assert_eq!(
        runtime.history_state().unwrap(),
        baseline.history,
        "{case}: history changed",
    );
    assert_eq!(
        runtime.stats().unwrap(),
        baseline.stats,
        "{case}: runtime stats changed",
    );
    assert_eq!(
        counters.values(),
        baseline.hook_counts,
        "{case}: middleware or subscription callbacks ran",
    );
}

#[test]
fn foreign_command_provenance_wins_over_stale_conflicts_without_side_effects() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime_a = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let runtime_b = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    prepare_mixed_history(&runtime_a, ids.source);
    prepare_mixed_history(&runtime_b, ids.source);

    let issued = runtime_a.snapshot().unwrap();
    assert_eq!(issued.revision(), runtime_b.snapshot().unwrap().revision());
    let issued_history = runtime_a.history_state().unwrap();
    assert!(issued_history.can_undo() && issued_history.can_redo());
    let commands = [
        (
            "data",
            GraphCommand::set_node_data(
                issued.version(),
                ids.target,
                serde_json::json!({ "foreign": true }),
            ),
        ),
        (
            "topology",
            GraphCommand::topology(
                issued.version(),
                TopologyEdit::new().remove_node(ids.alternate_source, IncidentEdgePolicy::Reject),
            ),
        ),
        (
            "connection",
            GraphCommand::disconnect(issued.version(), ids.edge),
        ),
        ("undo", GraphCommand::undo(issued.version())),
        ("redo", GraphCommand::redo(issued.version())),
    ];

    let equal_a = runtime_a.snapshot().unwrap();
    let equal_b = runtime_b.snapshot().unwrap();
    assert_eq!(equal_a.revision(), equal_b.revision());
    assert_ne!(equal_a.provenance(), equal_b.provenance());
    for node in [ids.source, ids.target, ids.alternate_source] {
        assert!(equal_a.graph().node(&node).is_some());
        assert!(equal_b.graph().node(&node).is_some());
    }
    for port in [ids.source_port, ids.target_port, ids.alternate_source_port] {
        assert!(equal_a.graph().port(&port).is_some());
        assert!(equal_b.graph().port(&port).is_some());
    }
    assert!(equal_a.graph().edge(&ids.edge).is_some());
    assert!(equal_b.graph().edge(&ids.edge).is_some());

    let counters = CommandHookCounters::new();
    let runtime_b = runtime_b.with_middleware(CountingMiddleware {
        before: counters.before.clone(),
        after: counters.after.clone(),
    });
    let event_counter = counters.events.clone();
    runtime_b
        .subscribe(move |_| event_counter.set(event_counter.get() + 1))
        .expect("subscription is installed");
    let equal_baseline = RuntimeBaseline::capture(&runtime_b, &counters);

    for (name, command) in &commands {
        let cloned = (*command).clone();
        assert_eq!(cloned.expected_version(), command.expected_version());
        assert_foreign_command_has_zero_side_effects(
            &runtime_b,
            cloned,
            &format!("{name} clone"),
            &equal_baseline,
            &counters,
            ExpectedRevisionRelation::Equal,
        );
    }

    advance_with_redo(&runtime_b);
    let stale_baseline = RuntimeBaseline::capture(&runtime_b, &counters);
    assert_ne!(issued.revision(), stale_baseline.snapshot.revision());
    for (name, command) in commands {
        assert_foreign_command_has_zero_side_effects(
            &runtime_b,
            command,
            name,
            &stale_baseline,
            &counters,
            ExpectedRevisionRelation::Stale,
        );
    }

    let legal_version = runtime_b.snapshot().unwrap().version();
    runtime_b
        .execute(GraphCommand::set_node_data(
            legal_version,
            ids.target,
            serde_json::json!({ "local": "still accepted" }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("the target runtime remains usable after foreign rejections");
    assert_eq!(
        runtime_b
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids.target)
            .unwrap()
            .data,
        serde_json::json!({ "local": "still accepted" }),
    );
}
