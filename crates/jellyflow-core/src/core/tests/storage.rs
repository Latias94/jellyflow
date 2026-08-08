use super::*;
use crate::core::{
    CanvasRect, EdgeViewDescriptor, GraphBuilder, Group, NodeExtent, NodeOrigin, StickyNote,
    StickyNoteId, validate_graph_storage,
};

#[test]
fn storage_validation_rejects_every_non_finite_node_geometry_field() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    let mut node = make_node("core.non_finite_node");
    node.pos.x = f32::NAN;
    node.origin = Some(NodeOrigin {
        x: 0.0,
        y: f32::INFINITY,
    });
    node.extent = Some(NodeExtent::Rect {
        rect: CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: f32::NEG_INFINITY,
                height: 10.0,
            },
        },
    });
    graph.insert_node(node_id, node);

    let errors = validate_graph_storage(&graph).into_errors();

    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::NodePositionNotFinite { node } if *node == node_id
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::NodeOriginNotFinite { node } if *node == node_id
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::NodeExtentNotFinite { node } if *node == node_id
    )));
}

#[test]
fn storage_validation_rejects_finite_components_with_unrepresentable_node_bounds() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    let mut node = make_node("core.overflowing_node_bounds");
    node.pos = CanvasPoint {
        x: -f32::MAX,
        y: 0.0,
    };
    node.origin = Some(NodeOrigin { x: 1.0, y: 0.0 });
    node.size = Some(CanvasSize {
        width: f32::MAX,
        height: 10.0,
    });
    graph.insert_node(node_id, node);

    let errors = validate_graph_storage(&graph).into_errors();

    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::NodeBoundsNotFinite { node } if *node == node_id
    )));
}

#[test]
fn storage_validation_rejects_non_finite_group_and_sticky_note_rects() {
    let mut graph = GraphBuilder::new(GraphId::new());
    let group_id = GroupId::new();
    let sticky_note_id = StickyNoteId::new();
    graph.insert_group(
        group_id,
        Group {
            title: "invalid group".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint {
                    x: f32::INFINITY,
                    y: 0.0,
                },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
            color: None,
        },
    );
    graph.insert_sticky_note(
        sticky_note_id,
        StickyNote {
            text: "invalid note".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 100.0,
                    height: f32::NAN,
                },
            },
            color: None,
        },
    );

    let errors = validate_graph_storage(graph.graph()).into_errors();

    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::GroupRectNotFinite { group } if *group == group_id
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::StickyNoteRectNotFinite { sticky_note }
            if *sticky_note == sticky_note_id
    )));
    assert!(
        graph.build().is_err(),
        "validated builders must reject the same graph"
    );
}

#[test]
fn storage_validation_rejects_non_finite_edge_geometry() {
    let mut graph = Graph::default();
    let edge_id = EdgeId::new();
    let mut edge = Edge::new(EdgeKind::Data, PortId::new(), PortId::new());
    edge.interaction_width = Some(f32::NAN);
    edge.view = EdgeViewDescriptor::new().with_route(EdgeRouteSpec::SmoothStep {
        offset: EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
        step_position: EdgeRouteSpec::DEFAULT_STEP_POSITION,
        border_radius: EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
        center: Some(CanvasPoint {
            x: 0.0,
            y: f32::NEG_INFINITY,
        }),
    });
    graph.insert_edge(edge_id, edge);

    let errors = validate_graph_storage(&graph).into_errors();

    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::EdgeInteractionWidthNotFinite { edge } if *edge == edge_id
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::EdgeRouteSpecInvalid {
            edge,
            source: EdgeRouteSpecValidationError::SmoothStepCenter { .. },
        } if *edge == edge_id
    )));
}

#[test]
fn edge_route_spec_constructors_reject_every_invalid_parameter_class() {
    assert!(matches!(
        EdgeRouteSpec::bezier(f32::NAN),
        Err(EdgeRouteSpecValidationError::BezierCurvature { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::orthogonal(-1.0, 0.5, None),
        Err(EdgeRouteSpecValidationError::OrthogonalOffset { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::orthogonal(20.0, 1.01, None),
        Err(EdgeRouteSpecValidationError::OrthogonalStepPosition { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::orthogonal(
            20.0,
            0.5,
            Some(CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            }),
        ),
        Err(EdgeRouteSpecValidationError::OrthogonalCenter { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::smooth_step(-1.0, 0.5, 5.0, None),
        Err(EdgeRouteSpecValidationError::SmoothStepOffset { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::smooth_step(20.0, -0.01, 5.0, None),
        Err(EdgeRouteSpecValidationError::SmoothStepStepPosition { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::smooth_step(20.0, 0.5, f32::MAX, None),
        Err(EdgeRouteSpecValidationError::SmoothStepBorderRadius { .. })
    ));
    assert!(matches!(
        EdgeRouteSpec::smooth_step(
            20.0,
            0.5,
            5.0,
            Some(CanvasPoint {
                x: 0.0,
                y: f32::NAN,
            }),
        ),
        Err(EdgeRouteSpecValidationError::SmoothStepCenter { .. })
    ));

    assert_eq!(EdgeRouteSpec::straight().validate(), Ok(()));
    assert_eq!(EdgeRouteSpec::default().validate(), Ok(()));
    assert_eq!(EdgeRouteSpec::default_orthogonal().validate(), Ok(()));
    assert_eq!(EdgeRouteSpec::default_smooth_step().validate(), Ok(()));
}

#[test]
fn validated_graph_builder_rejects_an_invalid_route_on_otherwise_valid_topology() {
    let source_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let source_port = PortId::from_u128(3);
    let target_port = PortId::from_u128(4);
    let edge_id = EdgeId::from_u128(5);
    let mut graph = Graph::new(GraphId::from_u128(6));
    graph.insert_node(source_node, make_node("route.source"));
    graph.insert_node(target_node, make_node("route.target"));
    attach_port(
        &mut graph,
        source_port,
        make_port(
            source_node,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        target_port,
        make_port(
            target_node,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    let mut edge = Edge::new(EdgeKind::Data, source_port, target_port);
    edge.view = EdgeViewDescriptor::new().with_route(EdgeRouteSpec::Bezier {
        curvature: EdgeRouteSpec::MAX_CURVATURE + 1.0,
    });
    graph.insert_edge(edge_id, edge);

    let errors = GraphBuilder::from_detached_graph(graph)
        .build()
        .expect_err("invalid route must fail validated graph assembly");

    assert!(errors.iter().any(|error| matches!(
        error,
        GraphValidationError::EdgeRouteSpecInvalid {
            edge,
            source: EdgeRouteSpecValidationError::BezierCurvature { .. },
        } if *edge == edge_id
    )));
}
