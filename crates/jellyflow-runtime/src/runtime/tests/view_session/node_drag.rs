use jellyflow_core::core::{GraphBuilder, GraphId, NodeId};

use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::SelectionMode;
use crate::runtime::view_session::ViewSessionOptions;

use super::fixtures::node;

#[test]
fn node_drag_start_filters_ineligible_selected_peers_and_owns_selection_semantics() {
    let clicked = NodeId::from_u128(1);
    let eligible = NodeId::from_u128(2);
    let hidden = NodeId::from_u128(3);
    let not_selectable = NodeId::from_u128(4);
    let not_draggable = NodeId::from_u128(5);
    let unselected = NodeId::from_u128(6);
    let mut hidden_node = node("test.node", 0.0);
    hidden_node.hidden = true;
    let mut non_selectable_node = node("test.node", 0.0);
    non_selectable_node.selectable = Some(false);
    let mut non_draggable_node = node("test.node", 0.0);
    non_draggable_node.draggable = Some(false);
    let graph = GraphBuilder::new(GraphId::from_u128(0x240))
        .with_node(clicked, node("test.node", 0.0))
        .with_node(eligible, node("test.node", 20.0))
        .with_node(hidden, hidden_node)
        .with_node(not_selectable, non_selectable_node)
        .with_node(not_draggable, non_draggable_node)
        .with_node(unselected, node("test.node", 40.0))
        .build()
        .unwrap();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    view.set_selection(
        vec![clicked, eligible, hidden, not_selectable, not_draggable],
        Vec::new(),
        Vec::new(),
    )
    .unwrap();

    let selected_start = view
        .node_drag_start(clicked, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    assert_eq!(selected_start.nodes(), &[clicked, eligible]);
    assert!(selected_start.clicked_was_selected());

    let replace_start = view
        .node_drag_start(unselected, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    assert_eq!(replace_start.nodes(), &[unselected]);
    assert!(!replace_start.clicked_was_selected());

    let additive_start = view
        .node_drag_start(unselected, SelectionMode::Add)
        .unwrap()
        .unwrap();
    assert_eq!(additive_start.nodes(), &[clicked, eligible, unselected]);

    let toggle_drag_start = view
        .node_drag_start(unselected, SelectionMode::Toggle)
        .unwrap()
        .unwrap();
    assert_eq!(toggle_drag_start.nodes(), &[clicked, eligible, unselected]);

    for ineligible in [hidden, not_selectable, not_draggable] {
        assert!(
            view.node_drag_start(ineligible, SelectionMode::Replace)
                .unwrap()
                .is_none()
        );
    }
}
