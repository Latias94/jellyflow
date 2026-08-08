use jellyflow_core::{CanvasSize, NodeId};

use crate::{LayoutDirection, LayoutPresetBuilder, LayoutScope, LayoutSpacing};

#[test]
fn tree_preset_sets_layered_spacing_defaults() {
    let request = LayoutPresetBuilder::tree().build();

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
fn freeform_preset_sets_uniform_spacing() {
    let request = LayoutPresetBuilder::freeform().build();

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
    assert_eq!(
        request.layout.measured_node_sizes.get(&node),
        Some(&CanvasSize {
            width: 10.0,
            height: 20.0
        })
    );
}
