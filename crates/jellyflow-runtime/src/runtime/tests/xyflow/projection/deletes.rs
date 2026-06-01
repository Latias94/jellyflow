use super::super::super::fixtures::make_graph;

use crate::runtime::xyflow::callbacks::delete_changes_from_transaction;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, GroupId, StickyNoteId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn delete_changes_from_transaction_maps_resource_removals_and_cascaded_edges() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let node = g.nodes.get(&a).expect("node").clone();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();
    let group_id = GroupId::new();
    let note_id = StickyNoteId::new();
    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    };

    let tx = GraphTransaction::from_ops([
        GraphOp::RemoveNode {
            id: a,
            node,
            ports: vec![(out_port, port.clone())],
            edges: vec![(eid, edge.clone())],
        },
        GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge)],
        },
        GraphOp::RemoveGroup {
            id: group_id,
            group: jellyflow_core::core::Group {
                title: "group".to_owned(),
                rect,
                color: None,
            },
            detached: vec![(a, None)],
        },
        GraphOp::RemoveStickyNote {
            id: note_id,
            note: jellyflow_core::core::StickyNote {
                text: "note".to_owned(),
                rect,
                color: None,
            },
        },
    ]);

    let deleted = delete_changes_from_transaction(&tx);

    assert_eq!(deleted.nodes(), &[a]);
    assert_eq!(deleted.edges(), &[eid]);
    assert_eq!(deleted.groups(), &[group_id]);
    assert_eq!(deleted.sticky_notes(), &[note_id]);
}
