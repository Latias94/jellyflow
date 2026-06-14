use super::super::fixtures::make_graph;
use super::super::harness::{HarnessEvent, InteractionHarness};

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::drag::PointerGestureClaim;
use crate::runtime::selection::{
    NodeDragStartSelectionAction, NodeDragStartSelectionInput, NodePointerDownDecision,
    NodePointerDownInput, resolve_node_drag_start_selection, resolve_node_pointer_down,
};
use jellyflow_core::core::CanvasPoint;

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
    graph
        .update_node(&node, |node| node.selectable = Some(false))
        .expect("node exists");
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

    graph
        .update_node(&node, |node| node.hidden = true)
        .expect("node exists");
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

#[test]
fn node_pointer_down_combines_selection_and_drag_readiness() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    let decision = resolve_node_pointer_down(
        &graph,
        &view_state,
        &NodeGraphInteractionState::default(),
        NodePointerDownInput::new(node, false, CanvasPoint { x: 3.0, y: 4.0 }),
    );

    assert_eq!(
        decision,
        NodePointerDownDecision::new(
            NodeDragStartSelectionAction::SelectOnly(node),
            PointerGestureClaim::NodeDrag,
        )
    );
}

#[test]
fn node_pointer_down_keeps_drag_unclaimed_without_threshold_crossing() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    let decision = resolve_node_pointer_down(
        &graph,
        &view_state,
        &NodeGraphInteractionState::default(),
        NodePointerDownInput::new(node, false, CanvasPoint::default()),
    );

    assert_eq!(
        decision,
        NodePointerDownDecision::new(
            NodeDragStartSelectionAction::SelectOnly(node),
            PointerGestureClaim::None,
        )
    );
}

#[test]
fn store_apply_node_pointer_down_updates_selection_and_returns_decision() {
    let (graph, node, other, _, _, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    let mut harness =
        InteractionHarness::with_view_state("node pointer down store facade", graph, view_state);
    let decision = harness
        .store_mut()
        .apply_node_pointer_down(NodePointerDownInput::new(
            node,
            false,
            CanvasPoint { x: 3.0, y: 4.0 },
        ));

    assert_eq!(
        decision,
        NodePointerDownDecision::new(
            NodeDragStartSelectionAction::SelectOnly(node),
            PointerGestureClaim::NodeDrag,
        )
    );
    harness.assert_events(&[HarnessEvent::selection(vec![node], Vec::new(), Vec::new())]);
}

#[test]
fn store_apply_node_pointer_down_keeps_hidden_and_non_selectable_nodes_unchanged() {
    let (mut graph, node, other, _, _, edge) = make_graph();
    graph
        .update_node(&node, |node| node.selectable = Some(false))
        .expect("node exists");
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge], Vec::new());

    let mut harness = InteractionHarness::with_view_state(
        "node pointer down non-selectable unchanged",
        graph.clone(),
        view_state.clone(),
    );
    let decision = harness
        .store_mut()
        .apply_node_pointer_down(NodePointerDownInput::new(
            node,
            true,
            CanvasPoint::default(),
        ));

    assert_eq!(
        decision,
        NodePointerDownDecision::new(
            NodeDragStartSelectionAction::Unchanged,
            PointerGestureClaim::None
        )
    );
    assert_eq!(harness.store().view_state().selected_nodes, vec![other]);
    assert_eq!(harness.store().view_state().selected_edges, vec![edge]);
    harness.assert_events(&[]);

    graph
        .update_node(&node, |node| node.hidden = true)
        .expect("node exists");
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![other], Vec::new(), Vec::new());
    let mut harness = InteractionHarness::with_view_state(
        "node pointer down hidden unchanged",
        graph,
        view_state,
    );
    let decision = harness
        .store_mut()
        .apply_node_pointer_down(NodePointerDownInput::new(
            node,
            false,
            CanvasPoint::default(),
        ));

    assert_eq!(
        decision,
        NodePointerDownDecision::new(
            NodeDragStartSelectionAction::Unchanged,
            PointerGestureClaim::None
        )
    );
    assert_eq!(harness.store().view_state().selected_nodes, vec![other]);
    assert!(harness.store().view_state().selected_edges.is_empty());
    harness.assert_events(&[]);
}
