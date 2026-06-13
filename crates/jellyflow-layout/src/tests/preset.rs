use jellyflow_core::{CanvasSize, NodeId};

use crate::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutDirection, LayoutEngineId, LayoutEngineRequest,
    LayoutPresetBuilder, LayoutScope, LayoutSpacing, MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID,
    MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, TIDY_TREE_LAYOUT_ENGINE_ID,
};

#[test]
fn workflow_preset_uses_dugong_by_default() {
    let request = LayoutPresetBuilder::workflow().build();

    assert_eq!(request.engine, LayoutEngineId::dugong());
    assert_eq!(request.layout.scope, LayoutScope::All);
}

#[test]
fn tree_preset_sets_layered_defaults() {
    let request = LayoutPresetBuilder::tree().build();

    assert_eq!(request.engine, LayoutEngineId::tidy_tree());
    assert_eq!(request.engine.as_str(), TIDY_TREE_LAYOUT_ENGINE_ID);
    assert_eq!(
        request.layout.options.direction,
        LayoutDirection::TopToBottom
    );
    assert_eq!(
        request.layout.options.spacing,
        LayoutSpacing {
            nodesep: 32.0,
            ranksep: 72.0,
            edgesep: 16.0,
        }
    );
}

#[test]
fn mind_map_preset_uses_radial_engine() {
    let request = LayoutPresetBuilder::mind_map().build();

    assert_eq!(request.engine, LayoutEngineId::mind_map_radial());
    assert_eq!(request.engine.as_str(), MIND_MAP_RADIAL_LAYOUT_ENGINE_ID);
}

#[test]
fn freeform_preset_uses_freeform_engine_and_spacing() {
    let request = LayoutPresetBuilder::freeform().build();

    assert_eq!(request.engine, LayoutEngineId::mind_map_freeform());
    assert_eq!(request.engine.as_str(), MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID);
    assert_eq!(request.layout.options.spacing.nodesep, 24.0);
    assert_eq!(request.layout.options.spacing.ranksep, 24.0);
    assert_eq!(request.layout.options.spacing.edgesep, 24.0);
}

#[test]
fn preset_builder_can_switch_scope_and_options() {
    let node = NodeId::from_u128(1);
    let request = LayoutPresetBuilder::workflow()
        .with_engine("custom.engine")
        .with_direction(LayoutDirection::LeftToRight)
        .with_spacing(LayoutSpacing {
            nodesep: 11.0,
            ranksep: 22.0,
            edgesep: 33.0,
        })
        .with_margin(CanvasSize {
            width: 8.0,
            height: 9.0,
        })
        .with_default_node_size(CanvasSize {
            width: 144.0,
            height: 72.0,
        })
        .with_node_origin((0.25, 0.75))
        .nodes([node])
        .with_measured_node_sizes([(
            node,
            CanvasSize {
                width: 10.0,
                height: 20.0,
            },
        )])
        .build();

    assert_eq!(request.engine.as_str(), "custom.engine");
    assert_eq!(
        request.layout.scope,
        LayoutScope::Nodes {
            nodes: [node].into_iter().collect()
        }
    );
    assert_eq!(
        request.layout.options.direction,
        LayoutDirection::LeftToRight
    );
    assert_eq!(
        request.layout.options.margin,
        CanvasSize {
            width: 8.0,
            height: 9.0
        }
    );
    assert_eq!(
        request.layout.options.default_node_size,
        CanvasSize {
            width: 144.0,
            height: 72.0,
        }
    );
    assert_eq!(request.layout.options.node_origin, (0.25, 0.75));
    assert_eq!(
        request.layout.measured_node_sizes.get(&node),
        Some(&CanvasSize {
            width: 10.0,
            height: 20.0
        })
    );
}

#[test]
fn preset_builder_round_trips_through_engine_request() {
    let request = LayoutPresetBuilder::workflow().build();
    let layout_request = LayoutPresetBuilder::workflow().layout_request();
    let engine_request: LayoutEngineRequest = LayoutPresetBuilder::workflow().into();

    assert_eq!(request, engine_request);
    assert_eq!(layout_request, request.layout);
    assert_eq!(request.engine.as_str(), DUGONG_LAYOUT_ENGINE_ID);
}
