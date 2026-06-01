use super::super::fixtures::make_graph;

use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange};
use jellyflow_core::core::{CanvasPoint, EdgeId, EdgeKind, NodeId, NodeOrigin};

#[test]
fn apply_node_changes_removes_ports_and_incident_edges() {
    let (mut g0, a, b, out_port, in_port, eid) = make_graph();

    let report = apply_node_changes(&mut g0, &[NodeChange::Remove { id: a }]);
    assert!(report.did_change());
    assert_eq!(report.ignored(), 0);

    assert!(!g0.nodes.contains_key(&a));
    assert!(g0.nodes.contains_key(&b));

    assert!(!g0.ports.contains_key(&out_port));
    assert!(g0.ports.contains_key(&in_port));

    assert!(!g0.edges.contains_key(&eid));
}

#[test]
fn apply_node_changes_updates_origin_and_ignores_missing() {
    let (mut g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let missing = NodeId::new();

    let report = apply_node_changes(
        &mut g0,
        &[
            NodeChange::Position {
                id: a,
                position: CanvasPoint { x: 12.0, y: 24.0 },
            },
            NodeChange::Origin {
                id: a,
                origin: Some(NodeOrigin { x: 0.5, y: 0.25 }),
            },
            NodeChange::Origin {
                id: missing,
                origin: Some(NodeOrigin { x: 1.0, y: 1.0 }),
            },
        ],
    );

    assert!(report.did_change());
    assert_eq!(report.ignored(), 1);
    assert_eq!(
        g0.nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 12.0, y: 24.0 }
    );
    assert_eq!(
        g0.nodes.get(&a).unwrap().origin,
        Some(NodeOrigin { x: 0.5, y: 0.25 })
    );
}

#[test]
fn apply_edge_changes_updates_kind_and_ignores_missing() {
    let (mut g0, _a, _b, _out_port, _in_port, eid) = make_graph();
    let missing = EdgeId::new();

    let report = apply_edge_changes(
        &mut g0,
        &[
            EdgeChange::Kind {
                id: eid,
                kind: EdgeKind::Exec,
            },
            EdgeChange::Hidden {
                id: eid,
                hidden: true,
            },
            EdgeChange::InteractionWidth {
                id: eid,
                interaction_width: Some(30.0),
            },
            EdgeChange::Remove { id: missing },
        ],
    );
    assert!(report.did_change());
    assert_eq!(report.ignored(), 1);
    assert_eq!(g0.edges.get(&eid).unwrap().kind, EdgeKind::Exec);
    assert!(g0.edges.get(&eid).unwrap().hidden);
    assert_eq!(g0.edges.get(&eid).unwrap().interaction_width, Some(30.0));
}
