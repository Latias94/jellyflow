use jellyflow_core::core::{CanvasPoint, Graph, GraphBuilder, Node, NodeId, NodeKindKey, PortId};

use crate::profile::GraphProfile;
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::GraphRuntime;

#[test]
fn graph_runtime_applies_profiled_commands() {
    let node = NodeId::new();
    let mut graph = GraphBuilder::default();
    graph.insert_node(node, make_node());
    let runtime = GraphRuntime::builder(Default::default())
        .with_profile(Box::new(PassThroughProfile))
        .build(graph.build_unchecked())
        .expect("profiled runtime");
    let version = runtime.snapshot().expect("snapshot").version();
    runtime
        .execute(GraphCommand::set_node_position(
            version,
            node,
            CanvasPoint { x: 10.0, y: 20.0 },
        ))
        .into_result()
        .expect("command execution")
        .applied()
        .expect("profiled command applies");

    assert_eq!(
        runtime
            .snapshot()
            .expect("snapshot")
            .graph()
            .nodes()
            .get(&node)
            .expect("node")
            .pos,
        CanvasPoint { x: 10.0, y: 20.0 }
    );
    assert_eq!(runtime.history_state().expect("history").undo_len(), 1);
}

#[test]
fn graph_runtime_profile_rejection_leaves_state_unchanged() {
    let node = NodeId::new();
    let mut graph = GraphBuilder::default();
    graph.insert_node(node, make_node());
    let runtime = GraphRuntime::builder(Default::default())
        .with_profile(Box::new(RejectingProfile { node }))
        .build(graph.build_unchecked())
        .expect("initial graph is accepted");
    let before = runtime.snapshot().expect("snapshot");
    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            before.version(),
            node,
            CanvasPoint { x: 10.0, y: 20.0 },
        ))
        .into_result()
        .expect("command execution");

    assert_eq!(
        outcome
            .rejection()
            .expect("profile should reject")
            .diagnostics()[0]
            .key,
        "profile.reject",
    );
    assert!(before.ptr_eq(&runtime.snapshot().expect("unchanged snapshot")));
    assert_eq!(
        runtime
            .snapshot()
            .expect("snapshot")
            .graph()
            .nodes()
            .get(&node)
            .expect("node")
            .pos,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
    assert_eq!(runtime.history_state().expect("history").undo_len(), 0);
}

struct PassThroughProfile;

impl GraphProfile for PassThroughProfile {
    fn type_of_port(
        &self,
        _graph: &Graph,
        _port: PortId,
    ) -> Option<jellyflow_core::types::TypeDesc> {
        None
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

struct RejectingProfile {
    node: NodeId,
}

impl GraphProfile for RejectingProfile {
    fn type_of_port(
        &self,
        _graph: &Graph,
        _port: PortId,
    ) -> Option<jellyflow_core::types::TypeDesc> {
        None
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        if graph
            .node(&self.node)
            .is_some_and(|node| node.pos != CanvasPoint { x: 0.0, y: 0.0 })
        {
            vec![Diagnostic::error(
                "profile.reject",
                DiagnosticTarget::Graph,
                "profile rejected graph",
            )]
        } else {
            Vec::new()
        }
    }
}

fn make_node() -> Node {
    Node {
        kind: NodeKindKey::new("demo.node"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}
