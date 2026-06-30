use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::CanvasSize;

use super::super::super::scenario::{ConformanceAction, ConformanceLayoutFactsExpectation};

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::ReportNodeMeasurement { measurement } => store
            .report_node_measurement(measurement.clone())
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::AssertLayoutFacts {
            viewport_size,
            expected,
        } => assert_layout_facts(store, *viewport_size, expected),
        _ => return None,
    })
}

fn assert_layout_facts(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
    expected: &ConformanceLayoutFactsExpectation,
) -> Result<(), String> {
    let facts = store.layout_facts_query(viewport_size);
    if facts.rendering.visible_node_ids != expected.visible_node_ids {
        return Err(format!(
            "layout facts visible node ids resolved to {:?}, expected {:?}",
            facts.rendering.visible_node_ids, expected.visible_node_ids
        ));
    }
    if facts.rendering.visible_edge_ids != expected.visible_edge_ids {
        return Err(format!(
            "layout facts visible edge ids resolved to {:?}, expected {:?}",
            facts.rendering.visible_edge_ids, expected.visible_edge_ids
        ));
    }

    for expected_position in &expected.edge_positions {
        let Some(actual) = facts.visible_edge_position(expected_position.edge) else {
            return Err(format!(
                "layout facts missing edge position for {:?}",
                expected_position.edge
            ));
        };
        if !expected_position.matches_edge_position(actual) {
            return Err(format!(
                "layout facts edge position for {:?} resolved to {:?}, expected {:?}",
                expected_position.edge, actual, expected_position
            ));
        }
    }

    for expected_route in &expected.edge_routes {
        let Some(actual) = facts.visible_edge_route_facts(expected_route.edge) else {
            return Err(format!(
                "layout facts missing edge route facts for {:?}",
                expected_route.edge
            ));
        };
        if actual.kind != expected_route.kind
            || (actual.hit_test.interaction_width - expected_route.interaction_width).abs()
                > f32::EPSILON
            || actual.interaction.selected != expected_route.selected
        {
            return Err(format!(
                "layout facts edge route facts for {:?} resolved to {:?}, expected {:?}",
                expected_route.edge, actual, expected_route
            ));
        }
    }

    if let Some(expected_target) = &expected.connection_target {
        let actual = store.resolve_connection_target_from_layout_facts(
            expected_target.pointer,
            expected_target.from,
        );
        if actual != expected_target.expected {
            return Err(format!(
                "layout facts connection target resolved to {actual:?}, expected {:?}",
                expected_target.expected
            ));
        }
    }

    Ok(())
}
