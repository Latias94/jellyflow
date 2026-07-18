use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{
    AdapterFrame, FrameRequest, LayoutNodeStatus, NodeLayout, RendererStamp, ViewPoint, ViewRect,
    ViewSize, side_anchor,
};
use crate::runtime::commands::{GraphCommand, GraphCommandOutcome};
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::{GraphRuntime, RuntimePatch};
use crate::runtime::view_session::ViewSessionOptions;

use super::fixtures::{graph_with_nodes, graph_with_routed_edge};

fn request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint {
                x: -200.0,
                y: -200.0,
            },
            size: CanvasSize {
                width: 1_200.0,
                height: 800.0,
            },
        },
        RendererStamp::new("test.node", 1),
    )
}

fn layout_for(frame: &AdapterFrame, node: NodeId) -> NodeLayout {
    let record = frame
        .nodes()
        .iter()
        .find(|record| record.id() == node)
        .expect("fixture node is materialized");
    let canvas = record.bounds();
    let transform = frame.transform();
    let origin = transform
        .screen_point_for_canvas(canvas.origin)
        .expect("fixture bounds are finite");
    let bounds = ViewRect::new(
        ViewPoint::new(origin.x, origin.y),
        ViewSize::new(
            canvas.size.width * transform.zoom,
            canvas.size.height * transform.zoom,
        ),
    );
    let mut layout = record.begin_layout(bounds);
    for (index, port) in record.ports().iter().enumerate() {
        let direction = port.port().expect("fixture port exists").dir;
        let outward = match direction {
            PortDirection::In => HandlePosition::Left,
            PortDirection::Out => HandlePosition::Right,
        };
        let x = match outward {
            HandlePosition::Left => bounds.origin.x - 6.0,
            HandlePosition::Right => bounds.origin.x + bounds.size.width - 6.0,
            HandlePosition::Top | HandlePosition::Bottom => unreachable!(),
        };
        let visual = ViewRect::new(
            ViewPoint::new(x, bounds.origin.y + 32.0 + index as f32 * 18.0),
            ViewSize::new(12.0, 12.0),
        );
        layout
            .port_rendered(
                port.layout_target(),
                visual,
                visual,
                side_anchor(visual, outward).expect("fixture port bounds are valid"),
                outward,
            )
            .unwrap();
    }
    layout.finish().unwrap()
}

fn applied_patch(runtime: &GraphRuntime, command: GraphCommand) -> RuntimePatch {
    match runtime.execute(command).unwrap() {
        GraphCommandOutcome::Applied(receipt) => receipt.patch().clone(),
        other => panic!("fixture command must apply: {other:?}"),
    }
}

#[test]
fn runtime_patch_geometry_impact_covers_payload_move_size_and_topology() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    let payload = applied_patch(
        &runtime,
        GraphCommand::set_node_data(0, ids.source, serde_json::json!({ "expanded": true })),
    );
    assert!(!payload.geometry_impact().is_global());
    assert!(payload.geometry_impact().nodes().contains(&ids.source));

    let position = applied_patch(
        &runtime,
        GraphCommand::set_node_position(1, ids.source, CanvasPoint { x: 30.0, y: 40.0 }),
    );
    assert!(!position.geometry_impact().is_global());
    assert!(position.geometry_impact().nodes().contains(&ids.source));

    let size = applied_patch(
        &runtime,
        GraphCommand::set_node_size(
            2,
            ids.target,
            Some(CanvasSize {
                width: 220.0,
                height: 140.0,
            }),
        ),
    );
    assert!(!size.geometry_impact().is_global());
    assert!(size.geometry_impact().nodes().contains(&ids.target));

    let topology = applied_patch(
        &runtime,
        GraphCommand::reconnect(
            3,
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ),
    );
    let impact = topology.geometry_impact();
    assert!(!impact.is_global());
    assert!(impact.nodes().contains(&ids.source));
    assert!(impact.nodes().contains(&ids.target));
    assert!(impact.nodes().contains(&ids.alternate_source));
    assert!(impact.ports().contains(&ids.source_port));
    assert!(impact.ports().contains(&ids.target_port));
    assert!(impact.ports().contains(&ids.alternate_source_port));
    assert!(impact.incident_edges().contains(&ids.edge));
}

#[test]
fn unrelated_external_patch_publishes_only_unaffected_layout_and_counts_work() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session.frame(request()).unwrap();

    applied_patch(
        &runtime,
        GraphCommand::set_node_data(0, nodes[1], serde_json::json!({ "expanded": true })),
    );

    let collector = frame.collector();
    collector.layout(layout_for(&frame, nodes[0])).unwrap();
    collector.layout(layout_for(&frame, nodes[1])).unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Published)
    );
    assert_eq!(
        receipt.operations()[1].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert_eq!(
        receipt.publication().unwrap().published_nodes(),
        &[nodes[0]]
    );
    let stats = session.stats().unwrap();
    assert_eq!(stats.geometry_impact_patches_merged, 1);
    assert_eq!(stats.geometry_impact_global_fallbacks, 0);
    assert_eq!(stats.layout_impact_candidates_checked, 2);
    assert_eq!(stats.layout_impact_candidates_stale, 1);
}

#[test]
fn same_frame_applied_command_stales_only_intersecting_candidate() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session.frame(request()).unwrap();
    let collector = frame.collector();
    collector.layout(layout_for(&frame, nodes[0])).unwrap();
    collector.layout(layout_for(&frame, nodes[1])).unwrap();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "expanded": true }),
        ))
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert_eq!(
        receipt.operations()[1].layout_status(),
        Some(LayoutNodeStatus::Published)
    );
    assert!(matches!(
        receipt.operations()[2].command_outcome(),
        Some(Ok(GraphCommandOutcome::Applied(_)))
    ));
    assert_eq!(
        receipt.publication().unwrap().published_nodes(),
        &[nodes[1]]
    );
    let stats = session.stats().unwrap();
    assert_eq!(stats.geometry_impact_patches_merged, 1);
    assert_eq!(stats.layout_impact_candidates_checked, 2);
    assert_eq!(stats.layout_impact_candidates_stale, 1);
}

#[test]
fn same_frame_node_translation_rejects_old_layout_without_withdrawing_geometry() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let initial = session.frame(request()).unwrap();
    let initial_collector = initial.collector();
    for node in [ids.source, ids.target, ids.alternate_source] {
        initial_collector
            .layout(layout_for(&initial, node))
            .unwrap();
    }
    session
        .submit_feedback(initial_collector.finish().unwrap())
        .unwrap();
    let before = session.geometry_snapshot().unwrap();
    let before_source_anchor = before
        .port(ids.source_port)
        .expect("source port is initially published")
        .route_anchor();
    let before_route = before
        .route(ids.edge)
        .expect("incident route is initially published")
        .facts()
        .path
        .clone();

    let frame = session.frame(request()).unwrap();
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_position(
            0,
            ids.source,
            CanvasPoint { x: 80.0, y: 40.0 },
        ))
        .unwrap();
    collector.layout(layout_for(&frame, ids.source)).unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations()[0].command_outcome(),
        Some(Ok(GraphCommandOutcome::Applied(_)))
    ));
    assert_eq!(
        receipt.operations()[1].layout_status(),
        Some(LayoutNodeStatus::Stale),
        "the measurement belongs to the pre-command frame and must still be rejected",
    );
    assert!(receipt.publication().is_none());

    let after = session.geometry_snapshot().unwrap();
    let source = after
        .node(ids.source)
        .expect("translated node geometry remains published");
    assert!(source.is_fresh());
    assert_eq!(source.bounds().origin, CanvasPoint { x: 80.0, y: 40.0 });
    let after_source_anchor = after
        .port(ids.source_port)
        .expect("translated port geometry remains published")
        .route_anchor();
    assert_eq!(after_source_anchor.x - before_source_anchor.x, 80.0);
    assert_eq!(after_source_anchor.y - before_source_anchor.y, 40.0);
    let after_route = after
        .route(ids.edge)
        .expect("incident route remains continuously published");
    assert_ne!(after_route.facts().path, before_route);
}

#[test]
fn noop_rejected_and_conflicting_commands_do_not_create_geometry_impact() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session.frame(request()).unwrap();
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::Value::Null,
        ))
        .unwrap();
    collector
        .command(GraphCommand::set_node_data(
            0,
            NodeId::from_u128(0xffff),
            serde_json::json!({ "missing": true }),
        ))
        .unwrap();
    collector
        .command(GraphCommand::set_node_data(
            99,
            nodes[0],
            serde_json::json!({ "conflict": true }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame, nodes[0])).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations()[0].command_outcome(),
        Some(Ok(GraphCommandOutcome::Noop(_)))
    ));
    assert!(matches!(
        receipt.operations()[1].command_outcome(),
        Some(Ok(GraphCommandOutcome::Rejected(_)))
    ));
    assert!(matches!(
        receipt.operations()[2].command_outcome(),
        Some(Ok(GraphCommandOutcome::Conflict(_)))
    ));
    assert_eq!(
        receipt.operations()[3].layout_status(),
        Some(LayoutNodeStatus::Published)
    );
    let stats = session.stats().unwrap();
    assert_eq!(stats.geometry_impact_patches_merged, 0);
    assert_eq!(stats.layout_impact_candidates_checked, 1);
    assert_eq!(stats.layout_impact_candidates_stale, 0);
}

#[test]
fn multi_view_pending_impacts_are_scoped_per_candidate() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let first_frame = first.frame(request()).unwrap();
    let second_frame = second.frame(request()).unwrap();

    applied_patch(
        &runtime,
        GraphCommand::set_node_data(0, nodes[0], serde_json::json!({ "expanded": true })),
    );

    let first_collector = first_frame.collector();
    first_collector
        .layout(layout_for(&first_frame, nodes[0]))
        .unwrap();
    let first_receipt = first
        .submit_feedback(first_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        first_receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );

    let second_collector = second_frame.collector();
    second_collector
        .layout(layout_for(&second_frame, nodes[1]))
        .unwrap();
    let second_receipt = second
        .submit_feedback(second_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        second_receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Published)
    );
    assert_eq!(first.stats().unwrap().geometry_impact_patches_merged, 1);
    assert_eq!(second.stats().unwrap().geometry_impact_patches_merged, 1);
}

#[test]
fn new_edge_route_is_removed_when_impacted_endpoint_layout_is_withdrawn() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    applied_patch(&runtime, GraphCommand::disconnect(0, ids.edge));
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let initial = session.frame(request()).unwrap();
    let initial_collector = initial.collector();
    for node in [ids.source, ids.target, ids.alternate_source] {
        initial_collector
            .layout(layout_for(&initial, node))
            .unwrap();
    }
    session
        .submit_feedback(initial_collector.finish().unwrap())
        .unwrap();
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .is_none()
    );

    let frame = session.frame(request()).unwrap();
    let collector = frame.collector();
    collector.layout(layout_for(&frame, ids.source)).unwrap();
    collector
        .command(GraphCommand::connect(
            1,
            ids.edge,
            ids.source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert!(matches!(
        receipt.operations()[1].command_outcome(),
        Some(Ok(GraphCommandOutcome::Applied(_)))
    ));
    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.port(ids.source_port).is_none());
    assert!(geometry.port(ids.target_port).is_some());
    assert!(geometry.route(ids.edge).is_none());
}

#[test]
fn document_reset_patch_has_global_geometry_impact() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let (replacement, _) = graph_with_nodes(2);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    let patch = runtime.replace_document(replacement).unwrap();

    assert!(patch.geometry_impact().is_global());
    assert!(patch.geometry_impact().nodes().is_empty());
    assert!(patch.geometry_impact().ports().is_empty());
    assert!(patch.geometry_impact().incident_edges().is_empty());
}

#[test]
fn geometry_impact_is_available_from_the_public_runtime_surface() {
    fn assert_public_value(_: &crate::GeometryImpact) {}

    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let patch = applied_patch(
        &runtime,
        GraphCommand::set_node_data(0, nodes[0], serde_json::json!({ "expanded": true })),
    );

    assert_public_value(patch.geometry_impact());
}
