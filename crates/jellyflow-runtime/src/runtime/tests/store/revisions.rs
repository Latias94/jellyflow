use super::super::fixtures::{default_editor_config, make_graph};

use crate::io::NodeGraphViewState;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, Node, NodeId, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn store_graph_revision_stays_stable_for_view_only_updates() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());
    let before = store.graph_revision();

    store.set_viewport(CanvasPoint { x: 3.0, y: 4.0 }, 1.5);
    assert_eq!(store.graph_revision(), before);

    store.set_selection(vec![a], Vec::new(), Vec::new());
    assert_eq!(store.graph_revision(), before);

    store.update_view_state(|state| {
        state.draw_order = vec![a];
    });
    assert_eq!(store.graph_revision(), before);
}

#[test]
fn store_graph_revision_advances_for_graph_mutations() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());
    let node_id = NodeId::new();
    let before = store.graph_revision();

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddNode {
            id: node_id,
            node: Node {
                kind: NodeKindKey::new("demo.c"),
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
            },
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.graph_revision() > before);

    let after_dispatch = store.graph_revision();
    store.undo().expect("undo").expect("undo outcome");
    assert!(store.graph_revision() > after_dispatch);

    let after_undo = store.graph_revision();
    store.redo().expect("redo").expect("redo outcome");
    assert!(store.graph_revision() > after_undo);
}
