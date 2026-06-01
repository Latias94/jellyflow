use super::super::fixtures::make_graph;
use super::super::harness::{HarnessEvent, InteractionHarness};

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::selection::{
    NodeDragStartSelectionAction, NodeDragStartSelectionInput, resolve_node_drag_start_selection,
};

#[test]
fn node_drag_start_selection_selects_unselected_node_by_default() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    assert_eq!(
        resolve_node_drag_start_selection(
            &graph,
            &view_state,
            &NodeGraphInteractionState::default(),
            NodeDragStartSelectionInput::new(node, false),
        ),
        NodeDragStartSelectionAction::SelectOnly(node),
    );

    let mut harness =
        InteractionHarness::with_view_state("node drag start select", graph, view_state);
    let action = harness
        .store_mut()
        .apply_node_drag_start_selection(NodeDragStartSelectionInput::new(node, false));

    assert_eq!(action, NodeDragStartSelectionAction::SelectOnly(node));
    harness.assert_events(&[HarnessEvent::selection(vec![node], Vec::new(), Vec::new())]);
}

#[test]
fn node_drag_start_selection_adds_or_removes_with_multi_selection() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());
    let mut expected_nodes = vec![other, node];
    expected_nodes.sort();

    let mut harness = InteractionHarness::with_view_state(
        "node drag start multi add",
        graph.clone(),
        view_state.clone(),
    );
    let action = harness
        .store_mut()
        .apply_node_drag_start_selection(NodeDragStartSelectionInput::new(node, true));

    assert_eq!(action, NodeDragStartSelectionAction::Add(node));
    harness.assert_events(&[HarnessEvent::selection(
        expected_nodes,
        vec![edge],
        Vec::new(),
    )]);

    view_state.set_selection(vec![node, other], vec![edge], Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("node drag start multi remove", graph, view_state);
    let action = harness
        .store_mut()
        .apply_node_drag_start_selection(NodeDragStartSelectionInput::new(node, true));

    assert_eq!(action, NodeDragStartSelectionAction::Remove(node));
    harness.assert_events(&[HarnessEvent::selection(vec![other], vec![edge], Vec::new())]);
}

#[test]
fn node_drag_start_selection_respects_disabled_select_nodes_on_drag() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    let mut harness = InteractionHarness::with_view_state(
        "node drag start clears when disabled",
        graph,
        view_state,
    );
    harness.store_mut().update_editor_config(|config| {
        config.interaction.select_nodes_on_drag = false;
    });
    let action = harness
        .store_mut()
        .apply_node_drag_start_selection(NodeDragStartSelectionInput::new(node, false));

    assert_eq!(action, NodeDragStartSelectionAction::Clear);
    harness.assert_events(&[HarnessEvent::selection(Vec::new(), Vec::new(), Vec::new())]);
}

#[test]
fn node_drag_start_selection_does_not_select_non_selectable_or_hidden_nodes() {
    let (mut graph, node, other, _, _, edge) = make_graph();
    graph.nodes.get_mut(&node).unwrap().selectable = Some(false);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    assert_eq!(
        resolve_node_drag_start_selection(
            &graph,
            &view_state,
            &NodeGraphInteractionState::default(),
            NodeDragStartSelectionInput::new(node, true),
        ),
        NodeDragStartSelectionAction::Unchanged,
    );

    graph.nodes.get_mut(&node).unwrap().hidden = true;
    assert_eq!(
        resolve_node_drag_start_selection(
            &graph,
            &view_state,
            &NodeGraphInteractionState::default(),
            NodeDragStartSelectionInput::new(node, false),
        ),
        NodeDragStartSelectionAction::Unchanged,
    );
}
