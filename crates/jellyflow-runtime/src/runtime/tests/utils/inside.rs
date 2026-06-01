use super::fixtures::node_at;

use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{GetNodesInsideOptions, NodeInclusion, get_nodes_inside};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, NodeId, NodeOrigin,
};

#[test]
fn get_nodes_inside_supports_partial_vs_full_inclusion() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.nodes.insert(
        b,
        node_at(
            CanvasPoint { x: 9.0, y: 9.0 },
            Some(CanvasSize {
                width: 5.0,
                height: 5.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    };

    let partial = get_nodes_inside(
        &lookups,
        rect,
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(partial, expected);

    let full = get_nodes_inside(
        &lookups,
        rect,
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Full,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );
    assert_eq!(full, vec![a]);
}

#[test]
fn get_nodes_inside_uses_node_origin_override() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let mut node = node_at(
        CanvasPoint { x: 20.0, y: 10.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 6.0,
        }),
    );
    node.origin = Some(NodeOrigin { x: 0.2, y: 1.0 });
    g.nodes.insert(a, node);

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let found = get_nodes_inside(
        &lookups,
        CanvasRect {
            origin: CanvasPoint { x: 17.0, y: 3.0 },
            size: CanvasSize {
                width: 2.0,
                height: 2.0,
            },
        },
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.5, 0.5),
            include_hidden: true,
            fallback_size: None,
        },
    );

    assert_eq!(found, vec![a]);
}

#[test]
fn get_nodes_inside_linear_fallback_sorts_results() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let high = NodeId::from_u128(30);
    let low = NodeId::from_u128(10);

    g.nodes.insert(
        high,
        node_at(
            CanvasPoint { x: 5.0, y: 5.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.nodes.insert(
        low,
        node_at(
            CanvasPoint { x: 15.0, y: 5.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let found = get_nodes_inside(
        &lookups,
        CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 40.0,
                height: 40.0,
            },
        },
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );

    assert_eq!(found, vec![low, high]);
}

#[test]
fn get_nodes_inside_applies_hidden_and_fallback_size_policies() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let visible = NodeId::from_u128(1);
    let hidden_id = NodeId::from_u128(2);
    let unsized_node = NodeId::from_u128(3);

    g.nodes.insert(
        visible,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut hidden = node_at(
        CanvasPoint { x: 20.0, y: 0.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
    );
    hidden.hidden = true;
    g.nodes.insert(hidden_id, hidden);
    g.nodes
        .insert(unsized_node, node_at(CanvasPoint { x: 40.0, y: 0.0 }, None));

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 80.0,
            height: 40.0,
        },
    };

    assert_eq!(
        get_nodes_inside(&lookups, rect, GetNodesInsideOptions::default()),
        vec![visible]
    );

    assert_eq!(
        get_nodes_inside(
            &lookups,
            rect,
            GetNodesInsideOptions {
                include_hidden: true,
                ..GetNodesInsideOptions::default()
            },
        ),
        vec![visible, hidden_id]
    );

    assert_eq!(
        get_nodes_inside(
            &lookups,
            rect,
            GetNodesInsideOptions {
                include_hidden: true,
                fallback_size: Some(CanvasSize {
                    width: 10.0,
                    height: 10.0,
                }),
                ..GetNodesInsideOptions::default()
            },
        ),
        vec![visible, hidden_id, unsized_node]
    );
}

#[test]
fn get_nodes_inside_rejects_non_finite_query_rect() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let found = get_nodes_inside(
        &lookups,
        CanvasRect {
            origin: CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        },
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );

    assert!(found.is_empty());
}
