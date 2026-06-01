use jellyflow_core::core::{CanvasPoint, Graph, Node, NodeId, NodeKindKey, PortId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use crate::profile::GraphProfile;
use crate::rules::{Diagnostic, DiagnosticTarget};

use super::*;

#[test]
fn apply_transaction_with_profile_preserves_label_and_appends_derived_ops() {
    let node = NodeId::new();
    let mut graph = Graph::default();
    graph.nodes.insert(node, make_node());

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: node,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }])
    .with_label("Move");

    let committed = apply_transaction_with_profile(&mut graph, &mut OneDerivedOp::new(node), &tx)
        .expect("profile apply");

    assert_eq!(committed.label(), Some("Move"));
    assert_eq!(committed.ops().len(), 2);
    assert!(matches!(committed.ops()[0], GraphOp::SetNodePos { id, .. } if id == node));
    assert!(matches!(
        committed.ops()[1],
        GraphOp::SetNodeHidden {
            id,
            from: false,
            to: true
        } if id == node
    ));
    assert_eq!(
        graph.nodes.get(&node).expect("node").pos,
        CanvasPoint { x: 10.0, y: 20.0 }
    );
    assert!(graph.nodes.get(&node).expect("node").hidden);
}

#[test]
fn apply_transaction_with_profile_rejection_leaves_graph_unchanged() {
    let node = NodeId::new();
    let mut graph = Graph::default();
    graph.nodes.insert(node, make_node());

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: node,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }]);

    let err = apply_transaction_with_profile(&mut graph, &mut RejectingProfile, &tx)
        .expect_err("profile should reject");

    assert!(matches!(err, ApplyPipelineError::Rejected { .. }));
    assert_eq!(
        graph.nodes.get(&node).expect("node").pos,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
}

struct OneDerivedOp {
    node: NodeId,
    emitted: bool,
}

impl OneDerivedOp {
    fn new(node: NodeId) -> Self {
        Self {
            node,
            emitted: false,
        }
    }
}

impl GraphProfile for OneDerivedOp {
    fn type_of_port(
        &mut self,
        _graph: &Graph,
        _port: PortId,
    ) -> Option<jellyflow_core::types::TypeDesc> {
        None
    }

    fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn concretize(&mut self, _graph: &Graph) -> Vec<GraphOp> {
        if self.emitted {
            return Vec::new();
        }

        self.emitted = true;
        vec![GraphOp::SetNodeHidden {
            id: self.node,
            from: false,
            to: true,
        }]
    }
}

struct RejectingProfile;

impl GraphProfile for RejectingProfile {
    fn type_of_port(
        &mut self,
        _graph: &Graph,
        _port: PortId,
    ) -> Option<jellyflow_core::types::TypeDesc> {
        None
    }

    fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
        vec![Diagnostic::error(
            "profile.reject",
            DiagnosticTarget::Graph,
            "profile rejected graph",
        )]
    }
}

fn make_node() -> Node {
    Node {
        kind: NodeKindKey::new("demo.node"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
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
