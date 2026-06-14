use super::fixtures::node_at;

use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{
    GetNodesBoundsOptions, get_node_position_with_origin, get_node_rect, get_nodes_bounds,
};
use jellyflow_core::core::{CanvasPoint, CanvasSize, GraphBuilder, GraphId, NodeId, NodeOrigin};

#[test]
fn get_node_position_with_origin_matches_bounds_top_left() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.insert_node(
        a,
        node_at(
            CanvasPoint { x: 20.0, y: 10.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 6.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let p = get_node_position_with_origin(&lookups, a, (0.5, 0.5), None).expect("pos");
    assert!((p.x - 15.0).abs() <= 1.0e-6);
    assert!((p.y - 7.0).abs() <= 1.0e-6);
}

#[test]
fn get_node_position_with_origin_uses_node_origin_override() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let mut node = node_at(
        CanvasPoint { x: 20.0, y: 10.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 6.0,
        }),
    );
    node.origin = Some(NodeOrigin { x: 0.2, y: 1.0 });
    g.insert_node(a, node);

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let p = get_node_position_with_origin(&lookups, a, (0.5, 0.5), None).expect("pos");
    assert!((p.x - 18.0).abs() <= 1.0e-6);
    assert!((p.y - 4.0).abs() <= 1.0e-6);
}

#[test]
fn get_node_rect_is_consistent_with_get_nodes_bounds_singleton() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.insert_node(
        a,
        node_at(
            CanvasPoint { x: 20.0, y: 10.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 6.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = get_node_rect(&lookups, a, (0.5, 0.5), None).expect("rect");
    let bounds = get_nodes_bounds(
        &lookups,
        [a],
        GetNodesBoundsOptions {
            node_origin: (0.5, 0.5),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");

    assert!((rect.origin.x - bounds.origin.x).abs() <= 1.0e-6);
    assert!((rect.origin.y - bounds.origin.y).abs() <= 1.0e-6);
    assert!((rect.size.width - bounds.size.width).abs() <= 1.0e-6);
    assert!((rect.size.height - bounds.size.height).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_respects_node_origin() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.insert_node(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.insert_node(
        b,
        node_at(
            CanvasPoint { x: 20.0, y: 5.0 },
            Some(CanvasSize {
                width: 5.0,
                height: 5.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let bounds_top_left = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");
    assert!((bounds_top_left.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.origin.y - 0.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.size.width - 25.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.size.height - 10.0).abs() <= 1.0e-6);

    let bounds_center = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            node_origin: (0.5, 0.5),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");
    assert!((bounds_center.origin.x - (-5.0)).abs() <= 1.0e-6);
    assert!((bounds_center.origin.y - (-5.0)).abs() <= 1.0e-6);
    assert!((bounds_center.size.width - 27.5).abs() <= 1.0e-6);
    assert!((bounds_center.size.height - 12.5).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_uses_fallback_size_for_unsized_nodes() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();

    g.insert_node(a, node_at(CanvasPoint { x: 5.0, y: 7.0 }, None));

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert!(
        get_nodes_bounds(&lookups, [a], GetNodesBoundsOptions::default()).is_none(),
        "unsized nodes should not contribute without fallback dimensions"
    );

    let bounds = get_nodes_bounds(
        &lookups,
        [a],
        GetNodesBoundsOptions {
            fallback_size: Some(CanvasSize {
                width: 10.0,
                height: 20.0,
            }),
            ..GetNodesBoundsOptions::default()
        },
    )
    .expect("fallback bounds");

    assert!((bounds.origin.x - 5.0).abs() <= 1.0e-6);
    assert!((bounds.origin.y - 7.0).abs() <= 1.0e-6);
    assert!((bounds.size.width - 10.0).abs() <= 1.0e-6);
    assert!((bounds.size.height - 20.0).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_skips_hidden_nodes_unless_included() {
    let mut g = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.insert_node(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut hidden = node_at(
        CanvasPoint { x: 100.0, y: 0.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
    );
    hidden.hidden = true;
    g.insert_node(b, hidden);

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let visible_bounds =
        get_nodes_bounds(&lookups, [a, b], GetNodesBoundsOptions::default()).expect("bounds");
    assert!((visible_bounds.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((visible_bounds.size.width - 10.0).abs() <= 1.0e-6);

    let all_bounds = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            include_hidden: true,
            ..GetNodesBoundsOptions::default()
        },
    )
    .expect("bounds");
    assert!((all_bounds.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((all_bounds.size.width - 110.0).abs() <= 1.0e-6);
}
