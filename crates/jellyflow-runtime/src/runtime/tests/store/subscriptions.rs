use super::super::fixtures::{make_graph, make_store};

use crate::runtime::events::NodeGraphStoreEvent;
use crate::runtime::xyflow::changes::{NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, Node, NodeId, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn store_subscription_receives_graph_and_view_events_and_can_unsubscribe() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();

    let token = store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
        NodeGraphStoreEvent::ViewChanged { changes, .. } => {
            assert!(!changes.is_empty());
            events2.borrow_mut().push("view");
        }
    });

    store.set_viewport(jellyflow_core::core::CanvasPoint { x: 1.0, y: 2.0 }, 1.25);
    store.set_selection(vec![a], Vec::new(), Vec::new());

    let changes = NodeGraphChanges::from_parts(
        vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 5.0, y: 6.0 },
        }],
        Vec::new(),
    );
    store.dispatch_changes(&changes).expect("dispatch");

    let got = events.borrow().clone();
    assert!(got.contains(&"view"));
    assert!(got.contains(&"graph"));

    assert!(store.unsubscribe(token));
    assert!(!store.unsubscribe(token));

    let before_len = events.borrow().len();
    store.set_viewport(jellyflow_core::core::CanvasPoint { x: 3.0, y: 4.0 }, 2.0);
    store.dispatch_changes(&changes).expect("dispatch");
    assert_eq!(events.borrow().len(), before_len);
}

#[test]
fn store_selector_subscription_dedupes_and_tracks_graph_and_view_projections() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let node_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let node_counts2 = node_counts.clone();
    store.subscribe_selector(
        |s| s.graph.nodes.len(),
        move |v| node_counts2.borrow_mut().push(*v),
    );

    let selection_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let selection_counts2 = selection_counts.clone();
    store.subscribe_selector(
        |s| s.view_state.selected_nodes.len(),
        move |v| selection_counts2.borrow_mut().push(*v),
    );

    #[derive(PartialEq)]
    struct NonCloneSelectionCount(usize);

    let non_clone_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let non_clone_counts2 = non_clone_counts.clone();
    store.subscribe_selector(
        |s| NonCloneSelectionCount(s.view_state.selected_nodes.len()),
        move |v| non_clone_counts2.borrow_mut().push(v.0),
    );

    // Same selection twice should dedupe (no extra callback).
    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(vec![a], Vec::new(), Vec::new());

    assert_eq!(selection_counts.borrow().as_slice(), &[1]);
    assert_eq!(non_clone_counts.borrow().as_slice(), &[1]);
    assert!(node_counts.borrow().is_empty());

    // Adding a node should trigger only the node-count selector.
    let new_id = NodeId::new();
    let node = Node {
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
    };

    let tx = GraphTransaction::from_ops([GraphOp::AddNode { id: new_id, node }]);
    store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(node_counts.borrow().as_slice(), &[3]);
    assert_eq!(selection_counts.borrow().as_slice(), &[1]);
}

#[test]
fn store_selector_diff_provides_prev_and_next() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let deltas: Rc<RefCell<Vec<(usize, usize)>>> = Rc::new(RefCell::new(Vec::new()));
    let deltas2 = deltas.clone();
    store.subscribe_selector_diff(
        |s| s.view_state.selected_nodes.len(),
        move |prev, next| deltas2.borrow_mut().push((*prev, *next)),
    );

    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(Vec::new(), Vec::new(), Vec::new());

    assert_eq!(deltas.borrow().as_slice(), &[(0, 1), (1, 0)]);
}
