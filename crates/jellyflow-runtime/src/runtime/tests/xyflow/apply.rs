use super::super::fixtures::make_graph;

use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange};
use jellyflow_core::core::{EdgeId, EdgeKind};

#[test]
fn apply_node_changes_removes_ports_and_incident_edges() {
    let (mut g0, a, b, out_port, in_port, eid) = make_graph();

    let report = apply_node_changes(&mut g0, &[NodeChange::Remove { id: a }]);
    assert!(report.did_change());
    assert_eq!(report.ignored, 0);

    assert!(!g0.nodes.contains_key(&a));
    assert!(g0.nodes.contains_key(&b));

    assert!(!g0.ports.contains_key(&out_port));
    assert!(g0.ports.contains_key(&in_port));

    assert!(!g0.edges.contains_key(&eid));
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
            EdgeChange::Remove { id: missing },
        ],
    );
    assert!(report.did_change());
    assert_eq!(report.ignored, 1);
    assert_eq!(g0.edges.get(&eid).unwrap().kind, EdgeKind::Exec);
}
