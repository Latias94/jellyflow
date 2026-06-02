use super::super::harness::{HarnessEvent, InteractionHarness};
use super::support::{selection_fixture, selection_rect};

use crate::io::NodeGraphViewState;
use crate::runtime::selection::{
    SelectionBoxInput, SelectionBoxOptions, SelectionBoxResult, SelectionModifier,
};
use jellyflow_core::core::CanvasPoint;

#[test]
fn selection_box_replaces_selection_with_policy_filtered_sorted_result() {
    let fixture = selection_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![fixture.outside],
        vec![fixture.non_selectable_edge],
        Vec::new(),
    );
    let mut harness =
        InteractionHarness::with_view_state("selection box replacement", fixture.graph, view_state);

    let result = harness
        .store_mut()
        .apply_selection_box(SelectionBoxInput::replace(selection_rect()));

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high],
        edges: vec![fixture.connected_edge, fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}

#[test]
fn selection_box_additive_mode_unions_with_existing_selection_and_sorts() {
    let fixture = selection_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![fixture.outside], Vec::new(), Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("selection box additive", fixture.graph, view_state);

    let result = harness
        .store_mut()
        .apply_selection_box(SelectionBoxInput::new(
            selection_rect(),
            SelectionBoxOptions {
                modifier: SelectionModifier::Additive,
                ..SelectionBoxOptions::default()
            },
        ));

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high, fixture.outside],
        edges: vec![fixture.connected_edge, fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}

#[test]
fn selection_box_skips_hidden_edges() {
    let mut fixture = selection_fixture();
    fixture
        .graph
        .edges
        .get_mut(&fixture.connected_edge)
        .expect("edge")
        .hidden = true;
    let mut harness = InteractionHarness::new("selection box hidden edge", fixture.graph);

    let result = harness
        .store_mut()
        .apply_selection_box(SelectionBoxInput::replace(selection_rect()));

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high],
        edges: vec![fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}

#[test]
fn selection_box_input_from_drag_normalizes_reverse_drag_rect() {
    let fixture = selection_fixture();
    let rect = selection_rect();
    let start = CanvasPoint {
        x: rect.origin.x + rect.size.width,
        y: rect.origin.y + rect.size.height,
    };
    let input = SelectionBoxInput::replace_from_drag(start, rect.origin);
    assert_eq!(input.rect, rect);

    let mut harness = InteractionHarness::new("selection box reverse drag", fixture.graph);

    let result = harness.store_mut().apply_selection_box(input);

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high],
        edges: vec![fixture.connected_edge, fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}
