use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;

use crate::profile::GraphProfile;
use crate::rules::Diagnostic;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionSession, InteractionTerminalReason,
};
use crate::runtime::tests::fixtures::{current_port_instance, keyboard_connection_port_by_id};
use crate::runtime::view_session::{ViewSessionError, ViewSessionOptions};

use super::fixtures::{graph_with_routed_edge, publish_graph_layouts};

#[test]
fn keyboard_connection_uses_exact_ports_and_the_fresh_connection_mutation_pipeline() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard connection runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard connection view");
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port)
        .expect("real source capability");
    let target =
        keyboard_connection_port_by_id(&view, ids.target_port).expect("real target capability");
    let edge = EdgeId::from_u128(0x301);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("keyboard interaction session");

    let activated = session
        .activate_connection_port(source, edge, NodeGraphConnectionMode::Strict)
        .expect("source activation");
    assert!(activated.effects().is_empty());
    let work = activated
        .port_connection_query_work()
        .expect("semantic source activation retains operation work");
    assert_eq!(work.incident_bucket_reads(), 0);
    assert_eq!(work.incident_entries_read(), 0);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
    assert_eq!(session.armed_keyboard_connection_source(), Some(source));
    assert_eq!(session.retained_keyboard_connection_source(), Some(source));

    let reactivated = session
        .activate_keyboard_connection(source, edge, NodeGraphConnectionMode::Strict)
        .expect("source reactivation is a no-op");
    assert!(reactivated.effects().is_empty());
    assert_eq!(session.armed_keyboard_connection_source(), Some(source));
    assert_eq!(session.retained_keyboard_connection_source(), Some(source));

    let submitted = session
        .activate_connection_port(target, edge, NodeGraphConnectionMode::Strict)
        .expect("compatible exact target");
    let [CanvasInteractionEffect::SubmitGraph { submission }] = submitted.effects() else {
        panic!("keyboard target must emit one ordinary graph mutation");
    };
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    let ticket = submission.ticket();
    let submitted = submitted.into_dispatch();
    let CanvasInteractionEffect::SubmitGraph { submission } = submitted
        .effects
        .into_iter()
        .next()
        .expect("keyboard mutation")
    else {
        unreachable!("the only effect was checked above")
    };
    let command = session
        .bind_submission(submission)
        .expect("fresh binding")
        .expect("accepted connection command");
    let completion = runtime.execute(command);
    completion
        .result()
        .expect("keyboard connection execution")
        .applied()
        .expect("keyboard connection applies");
    session
        .complete_submission(ticket, completion)
        .expect("keyboard settlement");

    let graph = runtime.snapshot().expect("connected graph");
    let connected = graph.graph().edge(&edge).expect("new keyboard edge");
    assert_eq!(connected.from, ids.alternate_source_port);
    assert_eq!(connected.to, ids.target_port);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(session.armed_keyboard_connection_source(), None);
    assert_eq!(session.retained_keyboard_connection_source(), Some(source));

    publish_graph_layouts(&view);
    session
        .reconcile_authoritative_state()
        .expect("published keyboard route settles the interaction");
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(session.armed_keyboard_connection_source(), None);
    assert_eq!(session.retained_keyboard_connection_source(), None);
}

#[test]
fn keyboard_connection_reconciles_a_lost_applied_completion_before_route_publication() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard uncertainty runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard uncertainty view");
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port)
        .expect("keyboard uncertainty source");
    let target = keyboard_connection_port_by_id(&view, ids.target_port)
        .expect("keyboard uncertainty target");
    let edge = EdgeId::from_u128(0x306);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("keyboard uncertainty session");

    session
        .activate_keyboard_connection(source, edge, NodeGraphConnectionMode::Strict)
        .expect("arm keyboard source");
    let update = session
        .activate_keyboard_connection_target(target)
        .expect("submit keyboard target");
    let CanvasInteractionEffect::SubmitGraph { submission } = update
        .into_dispatch()
        .effects
        .into_iter()
        .next()
        .expect("keyboard uncertainty mutation")
    else {
        unreachable!("keyboard target emits one graph submission")
    };
    let ticket = submission.ticket();
    let command = session
        .bind_submission(submission)
        .expect("bind keyboard uncertainty submission")
        .expect("keyboard uncertainty emits a command");
    runtime
        .execute(command)
        .into_result()
        .expect("keyboard command executes")
        .applied()
        .expect("keyboard command applies");
    session
        .submission_outcome_unknown(ticket)
        .expect("lost callback enters shared uncertainty settlement");

    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Uncertain)
    );
    assert_eq!(session.armed_keyboard_connection_source(), None);
    assert_eq!(session.retained_keyboard_connection_source(), Some(source));
    session
        .reconcile_authoritative_state()
        .expect("authoritative graph proves the exact keyboard attempt");
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );

    publish_graph_layouts(&view);
    session
        .reconcile_authoritative_state()
        .expect("published route closes keyboard settlement");
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(session.armed_keyboard_connection_source(), None);
    assert_eq!(session.retained_keyboard_connection_source(), None);
}

#[test]
fn keyboard_target_activation_defers_profile_hooks_to_authoritative_execution() {
    struct CountingProfile {
        calls: Rc<Cell<u64>>,
    }

    impl GraphProfile for CountingProfile {
        fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            self.calls.set(self.calls.get().saturating_add(1));
            graph.port(&port).and_then(|port| port.ty.clone())
        }

        fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
            self.calls.set(self.calls.get().saturating_add(1));
            Vec::new()
        }
    }

    let (graph, ids) = graph_with_routed_edge();
    let calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .with_profile(Box::new(CountingProfile {
            calls: Rc::clone(&calls),
        }))
        .build(graph)
        .expect("profiled keyboard runtime");
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port).unwrap();
    let target = keyboard_connection_port_by_id(&view, ids.target_port).unwrap();
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("profiled keyboard session");
    calls.set(0);

    session
        .activate_keyboard_connection(
            source,
            EdgeId::from_u128(0x305),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap();
    assert_eq!(calls.get(), 0);
    let submitted = session
        .activate_keyboard_connection_target(target)
        .expect("target activation only validates interaction identities");
    assert_eq!(
        calls.get(),
        0,
        "input callbacks must not invoke profile hooks"
    );

    let submitted = submitted.into_dispatch();
    let CanvasInteractionEffect::SubmitGraph { submission } = submitted
        .effects
        .into_iter()
        .next()
        .expect("authoritative keyboard submission")
    else {
        unreachable!("keyboard target emits one graph submission")
    };
    let ticket = submission.ticket();
    let command = session.bind_submission(submission).unwrap().unwrap();
    assert_eq!(
        calls.get(),
        0,
        "binding remains a profile-free identity check"
    );
    let completion = runtime.execute(command);
    assert!(
        calls.get() > 0,
        "command planning owns product profile hooks"
    );
    session.complete_submission(ticket, completion).unwrap();
}

#[test]
fn keyboard_connection_retains_structured_invalid_feedback_and_escape_cancels() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard feedback runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard feedback view");
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port).unwrap();
    let invalid_target = keyboard_connection_port_by_id(&view, ids.source_port).unwrap();
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("keyboard feedback session");
    session
        .activate_keyboard_connection(
            source,
            EdgeId::from_u128(0x302),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap();

    let rejected = session
        .activate_keyboard_connection_target(invalid_target)
        .expect("target activation defers policy planning to command execution");
    let [CanvasInteractionEffect::SubmitGraph { submission }] = rejected.effects() else {
        panic!("keyboard target must submit one authoritative command");
    };
    let ticket = submission.ticket();
    let rejected = rejected.into_dispatch();
    let CanvasInteractionEffect::SubmitGraph { submission } = rejected
        .effects
        .into_iter()
        .next()
        .expect("keyboard mutation")
    else {
        unreachable!("the only effect was checked above")
    };
    let command = session
        .bind_submission(submission)
        .expect("fresh binding")
        .expect("connection command");
    let completion = runtime.execute(command);
    assert!(completion.result().unwrap().rejection().is_some());
    session
        .complete_submission(ticket, completion)
        .expect("policy rejection becomes structured interaction feedback");
    let feedback = session
        .keyboard_connection_feedback()
        .expect("invalid target feedback");
    assert_eq!(feedback.source(), ids.alternate_source_port);
    assert_eq!(feedback.target(), ids.source_port);
    assert!(!feedback.diagnostics().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
    assert_eq!(session.armed_keyboard_connection_source(), Some(source));
    assert_eq!(session.retained_keyboard_connection_source(), Some(source));

    let cancelled = session.cancel().unwrap();
    assert!(cancelled.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
    assert_eq!(session.armed_keyboard_connection_source(), None);
    assert_eq!(session.retained_keyboard_connection_source(), None);
    assert_eq!(session.keyboard_connection_feedback(), None);
}

#[test]
fn keyboard_port_capabilities_are_view_scoped() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard capability runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let foreign = keyboard_connection_port_by_id(&first, ids.alternate_source_port).unwrap();
    let mut session = CanvasInteractionSession::attach(&second, CanvasInteractionConfig::default())
        .expect("second interaction session");

    assert!(matches!(
        session.activate_keyboard_connection(
            foreign,
            EdgeId::from_u128(0x303),
            NodeGraphConnectionMode::Strict,
        ),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn keyboard_port_capability_is_stale_after_same_id_replacement() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard replacement runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard replacement view");
    let stale_instance = current_port_instance(&view, ids.alternate_source_port);
    let stale = view.keyboard_connection_port(stale_instance).unwrap();
    let before = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::topology(
            before.version(),
            TopologyEdit::new().remove_node(ids.alternate_source, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("source retirement applies");
    runtime
        .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("source replacement applies");
    assert!(matches!(
        view.keyboard_connection_port(stale_instance),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    let current = keyboard_connection_port_by_id(&view, ids.alternate_source_port).unwrap();
    assert_ne!(stale, current);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("replacement interaction session");

    assert!(matches!(
        session.activate_keyboard_connection(
            stale,
            EdgeId::from_u128(0x304),
            NodeGraphConnectionMode::Strict,
        ),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}
