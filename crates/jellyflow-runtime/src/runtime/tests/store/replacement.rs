use super::super::fixtures::{default_editor_config, make_graph, make_store};

use crate::io::NodeGraphViewState;
use crate::runtime::events::NodeGraphStoreEvent;
use jellyflow_core::core::{CanvasPoint, EdgeId, GraphBuilder, GraphId, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn store_replace_view_state_emits_view_changed_event() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut vs = NodeGraphViewState::default();
    vs.pan = jellyflow_core::core::CanvasPoint { x: 10.0, y: 20.0 };
    vs.zoom = 1.5;
    store.replace_view_state(vs);

    assert_eq!(events.borrow().as_slice(), &["view"]);
}

#[test]
fn store_set_viewport_emits_exact_zoom_changes_below_projection_epsilon() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let pan = CanvasPoint { x: 10.0, y: 20.0 };
    store.set_viewport(pan, 1.0);

    let zooms: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    let zooms2 = zooms.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::ViewChanged { changes, .. } = ev {
            for change in changes {
                if let crate::runtime::events::ViewChange::Viewport { zoom, .. } = change {
                    zooms2.borrow_mut().push(*zoom);
                }
            }
        }
    });

    let zoom = 1.0 + 5.0e-7;
    store.set_viewport(pan, zoom);

    assert_eq!(zooms.borrow().as_slice(), &[zoom]);
}

#[test]
fn store_replace_document_emits_single_document_event_and_clears_history() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let replacement_node = g0.nodes().get(&b).expect("replacement node").clone();
    let mut store = make_store(g0);

    let from = store.graph().nodes().get(&a).expect("node a").pos;
    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from,
        to: CanvasPoint {
            x: from.x + 10.0,
            y: from.y + 5.0,
        },
    }])
    .with_label("seed history");
    store.dispatch_transaction(&tx).expect("seed history");
    assert!(store.can_undo());

    let before_revision = store.graph_revision();
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    type DocumentEventDetail = (GraphId, GraphId, u64, u64, Vec<NodeId>, bool);
    let details: Rc<RefCell<Option<DocumentEventDetail>>> = Rc::new(RefCell::new(None));
    let details2 = details.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { before, after } => {
            events2.borrow_mut().push("document");
            *details2.borrow_mut() = Some((
                before.graph.graph_id(),
                after.graph.graph_id(),
                before.graph_revision,
                after.graph_revision,
                after.view_state.selected_nodes.clone(),
                after
                    .editor_config
                    .runtime_tuning
                    .only_render_visible_elements,
            ));
        }
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut next_graph = GraphBuilder::new(GraphId::from_u128(2));
    next_graph.insert_node(b, replacement_node);
    let mut next_view_state = NodeGraphViewState {
        selected_nodes: vec![a, b],
        ..NodeGraphViewState::default()
    };
    next_view_state.pan = CanvasPoint { x: 8.0, y: 13.0 };
    next_view_state.zoom = 1.75;
    let mut next_editor_config = default_editor_config();
    next_editor_config
        .runtime_tuning
        .only_render_visible_elements = false;

    store.replace_document(
        next_graph.clone().into(),
        next_view_state,
        next_editor_config.clone(),
    );

    assert_eq!(events.borrow().as_slice(), &["document"]);
    let detail = details.borrow().clone().expect("document event detail");
    assert_eq!(detail.0, GraphId::from_u128(1));
    assert_eq!(detail.1, GraphId::from_u128(2));
    assert_eq!(detail.2, before_revision);
    assert!(detail.3 > detail.2);
    assert_eq!(detail.4, vec![b]);
    assert_eq!(
        detail.5,
        next_editor_config
            .runtime_tuning
            .only_render_visible_elements
    );
    assert_eq!(store.graph().graph_id(), next_graph.graph_id());
    assert_eq!(store.view_state().selected_nodes, vec![b]);
    assert_eq!(store.editor_config(), next_editor_config);
    assert!(!store.can_undo());
    assert!(!store.can_redo());
}

#[test]
fn store_replace_graph_emits_document_event_and_preserves_history_policy() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let replacement_node = g0.nodes().get(&a).expect("replacement node").clone();
    let mut store = make_store(g0);
    store.set_selection(vec![b], Vec::new(), Vec::new());

    let from = store.graph().nodes().get(&a).expect("node a").pos;
    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from,
        to: CanvasPoint {
            x: from.x + 10.0,
            y: from.y + 5.0,
        },
    }])
    .with_label("seed history");
    store.dispatch_transaction(&tx).expect("seed history");
    assert!(store.can_undo());

    let before_revision = store.graph_revision();
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    let selected_after: Rc<RefCell<Option<Vec<NodeId>>>> = Rc::new(RefCell::new(None));
    let selected_after2 = selected_after.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { before, after } => {
            events2.borrow_mut().push("document");
            assert_eq!(before.graph_revision, before_revision);
            assert!(after.graph_revision > before.graph_revision);
            *selected_after2.borrow_mut() = Some(after.view_state.selected_nodes.clone());
        }
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut next_graph = GraphBuilder::new(GraphId::from_u128(3));
    next_graph.insert_node(a, replacement_node);
    store.replace_graph(next_graph.into());

    assert_eq!(events.borrow().as_slice(), &["document"]);
    assert_eq!(selected_after.borrow().clone(), Some(Vec::new()));
    assert!(store.can_undo());
}

#[test]
fn store_replace_editor_config_notifies_selectors_for_runtime_tuning_only_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let runtime_flags: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let runtime_flags2 = runtime_flags.clone();
    store.subscribe_selector(
        |s| s.runtime_tuning.only_render_visible_elements,
        move |value| runtime_flags2.borrow_mut().push(*value),
    );

    let mut editor_config = store.editor_config();
    editor_config.runtime_tuning.only_render_visible_elements = false;
    store.replace_editor_config(editor_config);

    assert!(events.borrow().is_empty());
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);
    assert!(!store.runtime_tuning().only_render_visible_elements);
}

#[test]
fn store_update_editor_config_notifies_selectors_only_when_changed() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let runtime_flags: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let runtime_flags2 = runtime_flags.clone();
    store.subscribe_selector(
        |s| s.runtime_tuning.only_render_visible_elements,
        move |value| runtime_flags2.borrow_mut().push(*value),
    );

    store.update_editor_config(|_| {});
    assert!(runtime_flags.borrow().is_empty());

    store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);

    store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);
}

#[test]
fn store_update_view_state_notifies_selectors_for_draw_order_only_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let draw_order_snapshots: Rc<RefCell<Vec<Vec<NodeId>>>> = Rc::new(RefCell::new(Vec::new()));
    let draw_order_snapshots2 = draw_order_snapshots.clone();
    store.subscribe_selector(
        |s| s.view_state.draw_order.clone(),
        move |value| draw_order_snapshots2.borrow_mut().push(value.clone()),
    );

    store.update_view_state(|s| {
        s.draw_order = vec![b, a];
    });

    assert!(events.borrow().is_empty());
    assert_eq!(draw_order_snapshots.borrow().as_slice(), &[vec![b, a]]);
    assert_eq!(store.view_state().draw_order.as_slice(), &[b, a]);
}

#[test]
fn store_update_view_state_notifies_selectors_for_edge_draw_order_only_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = make_store(g0);

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let edge_draw_order_snapshots: Rc<RefCell<Vec<Vec<EdgeId>>>> =
        Rc::new(RefCell::new(Vec::new()));
    let edge_draw_order_snapshots2 = edge_draw_order_snapshots.clone();
    store.subscribe_selector(
        |s| s.view_state.edge_draw_order.clone(),
        move |value| edge_draw_order_snapshots2.borrow_mut().push(value.clone()),
    );

    store.update_view_state(|s| {
        s.edge_draw_order = vec![eid];
    });

    assert!(events.borrow().is_empty());
    assert_eq!(edge_draw_order_snapshots.borrow().as_slice(), &[vec![eid]]);
    assert_eq!(store.view_state().edge_draw_order.as_slice(), &[eid]);
}
