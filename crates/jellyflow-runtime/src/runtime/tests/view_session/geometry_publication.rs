use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, FeedbackSubmissionError, FrameMaterializationError,
    FrameRequest, LayoutNodeStatus, NodeLayout, NodeLayoutBuilder, NodeLayoutBuilderError,
    NodeLayoutTarget, PortLayoutOutcome, PortLayoutTarget, PublishedPortOutcome,
    RenderedPortLayout, RendererStamp, StaleFrameReason, ViewPoint, ViewRect, ViewSize,
    side_anchor,
};
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::geometry::{EdgePathAnchor, HandlePosition};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionError, CanvasInteractionPhase,
    CanvasInteractionQueryPageReceipt, CanvasInteractionSession, CanvasInteractionUpdate,
    CanvasPressTarget, ConnectionGestureBinding, ConnectionInteractionStart, ConnectionProbe,
    SelectionMode,
};
use crate::runtime::tests::fixtures::{port_connection_drag_intent_by_id, set_selection_by_id};
use crate::runtime::view_session::{
    ConnectionProbeQuery, ConnectionProbeQueryOutcome, ConnectionProbeTargeting,
    EdgePickCursorStaleReason, EdgePickQuery, EdgePickQueryError, EdgePickQueryOutcome,
    ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
    VisibleRouteCursorStaleReason, VisibleRouteQuery, VisibleRouteQueryError,
    VisibleRouteQueryOutcome,
};
use crate::runtime::viewport::{ViewportPointerSample, ViewportTransform};

use super::fixtures::{
    RoutedGraphIds, complete_nodes_intersecting, graph_with_nodes, graph_with_routed_edge, node,
    publish_node_layouts,
};

fn root_rect(x: f32) -> ViewRect {
    ViewRect {
        origin: ViewPoint { x, y: 20.0 },
        size: ViewSize {
            width: 180.0,
            height: 120.0,
        },
    }
}

fn port_rect(x: f32) -> ViewRect {
    ViewRect {
        origin: ViewPoint { x, y: 72.0 },
        size: ViewSize {
            width: 12.0,
            height: 12.0,
        },
    }
}

fn connection_binding(view: &ViewSession, port: PortId, edge: EdgeId) -> ConnectionGestureBinding {
    let drag =
        port_connection_drag_intent_by_id(view, port).expect("connection facts are readable");
    ConnectionInteractionStart::binding_from_port_drag(drag, edge, NodeGraphConnectionMode::Strict)
        .expect("source port can begin a connection")
}

fn pointer_sample(view: &ViewSession, point: ViewPoint) -> ViewportPointerSample {
    view.viewport_pointer_sample(point)
        .expect("fixture pointer is valid")
}

fn advance_terminal_query_page(
    interaction: &mut CanvasInteractionSession,
    update: CanvasInteractionUpdate,
    expected: impl FnOnce(&CanvasInteractionQueryPageReceipt) -> bool,
) -> CanvasInteractionUpdate {
    assert!(update.query_work_pending());
    assert!(interaction.query_work_pending());
    let (resolved, needs_tick, receipt, terminal_error) =
        interaction.advance_query_work(1).into_parts();
    assert!(terminal_error.is_none());
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    let [page] = receipt.page_receipts() else {
        panic!("one terminal query page must issue exactly one receipt");
    };
    assert!(expected(page), "unexpected interaction query page receipt");
    assert!(!needs_tick);
    assert!(!interaction.query_work_pending());
    assert!(!resolved.query_work_pending());
    resolved
}

#[derive(Clone)]
struct RenderedPortSpec {
    target: PortLayoutTarget,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    route_anchor: ViewPoint,
    outward: HandlePosition,
}

fn port_layout(
    target: &PortLayoutTarget,
    visual_bounds: ViewRect,
    outward: HandlePosition,
) -> RenderedPortSpec {
    port_layout_with_geometry(
        target,
        visual_bounds,
        visual_bounds,
        side_anchor(visual_bounds, outward).expect("fixture port bounds are valid"),
        outward,
    )
}

fn port_layout_with_geometry(
    target: &PortLayoutTarget,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    route_anchor: ViewPoint,
    outward: HandlePosition,
) -> RenderedPortSpec {
    RenderedPortSpec {
        target: target.clone(),
        visual_bounds,
        hit_bounds,
        route_anchor,
        outward,
    }
}

fn node_layout(
    target: &NodeLayoutTarget,
    bounds: ViewRect,
    ports: impl IntoIterator<Item = RenderedPortSpec>,
) -> NodeLayout {
    let ports = ports.into_iter().collect::<Vec<_>>();
    let mut layout =
        NodeLayoutBuilder::new(target, bounds, ports.iter().map(|port| port.target.clone()));
    for port in ports {
        layout
            .port_rendered(
                &port.target,
                port.visual_bounds,
                port.hit_bounds,
                port.route_anchor,
                port.outward,
            )
            .unwrap();
    }
    layout.finish().unwrap()
}

fn raw_node_layout(
    target: &NodeLayoutTarget,
    bounds: ViewRect,
    ports: impl IntoIterator<Item = RenderedPortSpec>,
) -> NodeLayout {
    NodeLayout::raw_for_test(
        target,
        bounds,
        ports.into_iter().map(|port| {
            PortLayoutOutcome::Rendered(RenderedPortLayout::raw_for_test(
                &port.target,
                port.visual_bounds,
                port.hit_bounds,
                port.route_anchor,
                port.outward,
            ))
        }),
    )
}

#[derive(Clone, Copy)]
struct OutcomeGraphIds {
    node: NodeId,
    ports: [PortId; 4],
    target_nodes: [NodeId; 4],
    target_ports: [PortId; 4],
    edges: [EdgeId; 4],
}

fn graph_with_four_port_outcomes() -> (Graph, OutcomeGraphIds) {
    let ids = OutcomeGraphIds {
        node: NodeId::from_u128(0x701),
        ports: std::array::from_fn(|index| PortId::from_u128(0x710 + index as u128)),
        target_nodes: std::array::from_fn(|index| NodeId::from_u128(0x720 + index as u128)),
        target_ports: std::array::from_fn(|index| PortId::from_u128(0x730 + index as u128)),
        edges: std::array::from_fn(|index| EdgeId::from_u128(0x740 + index as u128)),
    };
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x700));
    let mut source = node("test.outcomes", 0.0);
    source.ports = ids.ports.to_vec();
    builder.insert_node(ids.node, source);
    for (index, port) in ids.ports.into_iter().enumerate() {
        builder.insert_port(
            port,
            outcome_test_port(ids.node, format!("out-{index}"), PortDirection::Out),
        );
    }
    for index in 0..4 {
        let target_node = ids.target_nodes[index];
        let target_port = ids.target_ports[index];
        let mut target = node("test.outcome-target", 260.0 + index as f32 * 220.0);
        target.ports = vec![target_port];
        builder.insert_node(target_node, target);
        builder.insert_port(
            target_port,
            outcome_test_port(target_node, format!("in-{index}"), PortDirection::In),
        );
        builder.insert_edge(
            ids.edges[index],
            Edge::new(EdgeKind::Data, ids.ports[index], target_port),
        );
    }
    (builder.build().unwrap(), ids)
}

fn outcome_test_port(node: NodeId, key: String, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

#[derive(Clone, Copy)]
enum FirstPortOutcome {
    Rendered,
    Hidden,
    Unavailable,
}

fn outcome_layouts(
    frame: &AdapterFrame,
    ids: OutcomeGraphIds,
    first: FirstPortOutcome,
) -> Vec<NodeLayout> {
    let source = frame
        .nodes()
        .iter()
        .find(|node| node.id() == ids.node)
        .unwrap();
    let port_for = |port| {
        source
            .ports()
            .iter()
            .find(|candidate| candidate.id() == port)
            .unwrap()
    };
    let mut source_layout = source.begin_layout(root_rect(0.0));
    match first {
        FirstPortOutcome::Rendered => {
            let bounds = ViewRect::new(ViewPoint::new(174.0, 36.0), ViewSize::new(12.0, 12.0));
            source_layout
                .port_rendered(
                    port_for(ids.ports[0]).layout_target(),
                    bounds,
                    bounds,
                    side_anchor(bounds, HandlePosition::Right)
                        .expect("fixture port bounds are valid"),
                    HandlePosition::Right,
                )
                .unwrap();
        }
        FirstPortOutcome::Hidden => {
            source_layout
                .port_hidden(port_for(ids.ports[0]).layout_target())
                .unwrap();
        }
        FirstPortOutcome::Unavailable => {
            source_layout
                .port_unavailable(port_for(ids.ports[0]).layout_target())
                .unwrap();
        }
    }
    source_layout
        .port_hidden(port_for(ids.ports[1]).layout_target())
        .unwrap();
    source_layout
        .port_culled(port_for(ids.ports[2]).layout_target())
        .unwrap();
    source_layout
        .port_unavailable(port_for(ids.ports[3]).layout_target())
        .unwrap();

    let mut layouts = vec![source_layout.finish().unwrap()];
    for index in 0..4 {
        let x = 260.0 + index as f32 * 220.0;
        layouts.push(node_layout(
            &node_target(frame, ids.target_nodes[index]),
            root_rect(x),
            [port_layout(
                &port_target(frame, ids.target_ports[index]),
                port_rect(x),
                HandlePosition::Left,
            )],
        ));
    }
    layouts
}

fn node_target(frame: &AdapterFrame, node: jellyflow_core::core::NodeId) -> NodeLayoutTarget {
    frame
        .nodes()
        .iter()
        .find(|record| record.id() == node)
        .unwrap()
        .layout_target()
        .clone()
}

fn port_target(frame: &AdapterFrame, port: jellyflow_core::core::PortId) -> PortLayoutTarget {
    frame
        .nodes()
        .iter()
        .flat_map(|node| node.ports())
        .find(|record| record.id() == port)
        .unwrap()
        .layout_target()
        .clone()
}

fn publish_routed_geometry(session: &ViewSession, frame: &AdapterFrame, ids: RoutedGraphIds) {
    let source_target = node_target(frame, ids.source);
    let source_port_target = port_target(frame, ids.source_port);
    let target_target = node_target(frame, ids.target);
    let target_port_target = port_target(frame, ids.target_port);
    let collector = frame.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &target_target,
            root_rect(240.0),
            [port_layout(
                &target_port_target,
                port_rect(240.0),
                HandlePosition::Left,
            )],
        ))
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
}

fn routed_view_with_geometry() -> (GraphRuntime, ViewSession, RoutedGraphIds) {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    publish_routed_geometry(&view, &frame, ids);
    (runtime, view, ids)
}

#[test]
fn frame_node_layout_builder_requires_exact_real_port_coverage() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let source = frame
        .nodes()
        .iter()
        .find(|node| node.id() == ids.source)
        .unwrap();
    let source_port = source.ports().first().unwrap();
    let target = frame
        .nodes()
        .iter()
        .find(|node| node.id() == ids.target)
        .unwrap();
    let target_port = target.ports().first().unwrap();

    let missing = source.begin_layout(root_rect(0.0)).finish().unwrap_err();
    assert_eq!(
        missing,
        NodeLayoutBuilderError::MissingPorts {
            ports: vec![ids.source_port],
        }
    );

    let mut duplicate = source.begin_layout(root_rect(0.0));
    duplicate.port_hidden(source_port.layout_target()).unwrap();
    let error = duplicate
        .port_hidden(source_port.layout_target())
        .unwrap_err();
    assert_eq!(
        error,
        NodeLayoutBuilderError::DuplicatePort {
            port: ids.source_port,
        }
    );

    let mut foreign = source.begin_layout(root_rect(0.0));
    assert_eq!(
        foreign
            .port_hidden(target_port.layout_target())
            .unwrap_err(),
        NodeLayoutBuilderError::ForeignPort {
            expected_node: ids.source,
            actual_node: ids.target,
            port: ids.target_port,
        }
    );

    let unknown_port = PortId::from_u128(0xdead);
    let unknown_target = source_port.layout_target().with_port_for_test(unknown_port);
    let mut unknown = source.begin_layout(root_rect(0.0));
    assert_eq!(
        unknown.port_hidden(&unknown_target).unwrap_err(),
        NodeLayoutBuilderError::UnknownPort {
            node: ids.source,
            port: unknown_port,
        }
    );
}

#[test]
fn published_routes_are_bound_to_the_exact_issuing_view_generation() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let issuing = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let foreign = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    for view in [&issuing, &foreign] {
        let frame = view
            .frame(FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -100.0,
                        y: -100.0,
                    },
                    size: CanvasSize {
                        width: 1_000.0,
                        height: 600.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            ))
            .unwrap();
        publish_routed_geometry(view, &frame, ids);
    }
    let geometry = issuing.geometry_snapshot().unwrap();
    let route = geometry.route(ids.edge).expect("issued route");
    assert_eq!(geometry.provenance(), route.provenance());
    assert_eq!(geometry.provenance(), issuing.provenance().unwrap());

    assert!(
        issuing
            .connection_start_from_route(
                route,
                EdgeEndpoint::From,
                route.facts().endpoints.target,
                NodeGraphConnectionMode::Strict,
                false,
            )
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        foreign.connection_start_from_route(
            route,
            EdgeEndpoint::From,
            route.facts().endpoints.target,
            NodeGraphConnectionMode::Strict,
            false,
        ),
        Err(ViewSessionError::ForeignInteractionFacts)
    ));

    let mut foreign_interaction =
        CanvasInteractionSession::attach(&foreign, CanvasInteractionConfig::default()).unwrap();
    let issuing_sample = issuing
        .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
        .unwrap();
    assert!(matches!(
        foreign_interaction.press(CanvasPressTarget::Pan, issuing_sample),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert!(matches!(
        foreign_interaction.delete_edge(route),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(
        foreign_interaction.presentation().phase(),
        CanvasInteractionPhase::Idle,
        "foreign pointer and route facts must leave the interaction idle"
    );
    assert_eq!(foreign_interaction.presentation().active_edge(), None);

    let mut issuing_interaction =
        CanvasInteractionSession::attach(&issuing, CanvasInteractionConfig::default()).unwrap();
    assert!(
        !issuing_interaction
            .delete_edge(route)
            .unwrap()
            .effects()
            .is_empty()
    );
}

#[test]
fn published_route_reconnect_rejects_an_edge_id_restored_by_undo() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    publish_routed_geometry(&view, &frame, ids);
    let old_route = view
        .geometry_snapshot()
        .unwrap()
        .route(ids.edge)
        .expect("issued route")
        .clone();

    let initial = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::disconnect(initial.version(), ids.edge))
        .into_result()
        .unwrap()
        .applied()
        .expect("edge retirement applies");
    let retired = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::undo(retired.version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores the edge id");

    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let position = pointer_sample(&view, ViewPoint::new(0.0, 0.0));
    assert!(matches!(
        interaction.press(
            CanvasPressTarget::Edge {
                instance: old_route.instance(),
                selection_mode: SelectionMode::Replace,
            },
            position,
        ),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(
        interaction.presentation().phase(),
        CanvasInteractionPhase::Idle,
        "a retired edge capability must not activate the replacement edge"
    );
    assert_eq!(interaction.presentation().active_edge(), None);
    assert!(
        view.selection().unwrap().edges().is_empty(),
        "a retired edge capability must not emit a replacement selection effect"
    );

    assert!(matches!(
        view.connection_start_from_route(
            &old_route,
            EdgeEndpoint::From,
            old_route.facts().endpoints.target,
            NodeGraphConnectionMode::Strict,
            false,
        ),
        Err(ViewSessionError::StaleInteractionFacts)
    ));

    let restored_geometry = view.geometry_snapshot().unwrap();
    let restored_route = restored_geometry.route(ids.edge).expect("restored route");
    assert_ne!(
        restored_route.instance(),
        old_route.instance(),
        "the refreshed route must carry the replacement edge lifetime"
    );
    assert!(
        view.connection_start_from_route(
            restored_route,
            EdgeEndpoint::From,
            restored_route.facts().endpoints.target,
            NodeGraphConnectionMode::Strict,
            false,
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn hovered_edge_instance_does_not_alias_an_edge_id_restored_by_undo() {
    let (runtime, view, ids) = routed_view_with_geometry();
    let old_route = view
        .geometry_snapshot()
        .unwrap()
        .route(ids.edge)
        .expect("issued route")
        .clone();
    let center = old_route
        .facts()
        .path
        .sample(EdgePathAnchor::Center)
        .expect("published route center")
        .point;
    let transform = view.viewport_transform().unwrap();
    let screen = transform
        .screen_point_for_canvas(center)
        .expect("route center projects into the view");
    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let pointer = pointer_sample(&view, ViewPoint::new(screen.x, screen.y));
    let requested = interaction.request_edge_pick(pointer).unwrap();
    assert!(requested.effects().is_empty());
    advance_terminal_query_page(&mut interaction, requested, |page| {
        matches!(page, CanvasInteractionQueryPageReceipt::EdgePick(_))
    });
    assert_eq!(
        interaction.presentation().hovered_edge(),
        Some(old_route.instance())
    );

    let initial = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::disconnect(initial.version(), ids.edge))
        .into_result()
        .unwrap()
        .applied()
        .expect("edge retirement applies");
    let retired = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::undo(retired.version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores the edge id");

    let replacement = view
        .geometry_snapshot()
        .unwrap()
        .route(ids.edge)
        .expect("replacement route")
        .instance();
    assert_ne!(replacement, old_route.instance());
    assert_eq!(
        interaction.presentation().hovered_edge(),
        Some(old_route.instance()),
        "presentation must retain the lifetime it actually picked"
    );
    assert_ne!(
        interaction.presentation().hovered_edge(),
        Some(replacement),
        "a renderer comparing current route instances must not highlight the replacement"
    );
}

#[test]
fn runtime_owned_edge_pick_resolves_against_latest_view_facts() {
    let (_runtime, view, ids) = routed_view_with_geometry();

    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let pointer = pointer_sample(&view, ViewPoint::new(210.0, 78.0));
    let requested = interaction.request_edge_pick(pointer).unwrap();
    assert!(requested.query_work_pending());

    set_selection_by_id(&view, [], [ids.edge], Vec::new()).unwrap();
    let resolved = advance_terminal_query_page(&mut interaction, requested, |page| {
        matches!(
            page,
            CanvasInteractionQueryPageReceipt::EdgePick(receipt)
                if receipt.work().entries_examined > 0
                    && receipt.work().exact_tests > 0
                    && receipt.work().candidates_matched > 0
                    && receipt.work().flatten_cache_hits
                        + receipt.work().flatten_cache_misses
                        > 0
        )
    });
    assert!(resolved.effects().is_empty());
    assert!(interaction.presentation().hovered_edge().is_some());
}

#[test]
fn runtime_owned_connection_probe_resolves_against_latest_graph_revision() {
    let (runtime, view, ids) = routed_view_with_geometry();
    let drag_intent = port_connection_drag_intent_by_id(&view, ids.source_port)
        .expect("source port must admit a reconnect drag");
    let route = view
        .geometry_snapshot()
        .unwrap()
        .route(ids.edge)
        .unwrap()
        .clone();
    let start = view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(0xfeed),
            NodeGraphConnectionMode::Strict,
            route.facts().endpoints.target.point,
            HandlePosition::Right,
            HandlePosition::Left,
            false,
        )
        .unwrap()
        .expect("source reconnect must produce an interaction start");
    let source_point = route.facts().endpoints.source.point;
    let target_point = route.facts().endpoints.target.point;
    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let source = pointer_sample(&view, ViewPoint::new(source_point.x, source_point.y));
    let target = pointer_sample(&view, ViewPoint::new(target_point.x, target_point.y));
    interaction
        .press(CanvasPressTarget::Connection(start), source)
        .unwrap();
    let moved = interaction.move_to(target).unwrap();
    assert!(moved.query_work_pending());

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            version,
            ids.source,
            serde_json::json!({ "receipt_epoch": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("unrelated graph mutation advances the revision");
    let resolved = advance_terminal_query_page(&mut interaction, moved, |page| {
        matches!(
            page,
            CanvasInteractionQueryPageReceipt::ConnectionProbe(receipt)
                if receipt.work().entries_examined > 0
                    && receipt.work().exact_tests > 0
                    && receipt.work().policy_evaluations == 1
        )
    });
    assert!(resolved.effects().is_empty());
}

#[test]
fn runtime_owned_node_intersection_resolves_after_viewport_change() {
    let (_runtime, view, _ids) = routed_view_with_geometry();
    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let start = pointer_sample(&view, ViewPoint::new(0.0, 0.0));
    let end = pointer_sample(&view, ViewPoint::new(400.0, 240.0));
    let baseline = view.selection_baseline().unwrap();
    interaction
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline,
            },
            start,
        )
        .unwrap();
    let moved = interaction.move_to(end).unwrap();
    assert!(moved.query_work_pending());

    view.set_viewport(ViewportTransform::new(CanvasPoint { x: 1.0, y: 0.0 }, 1.0).unwrap())
        .unwrap();
    let resolved = advance_terminal_query_page(&mut interaction, moved, |page| {
        matches!(page, CanvasInteractionQueryPageReceipt::NodeIntersection(_))
    });
    assert!(resolved.effects().is_empty());
}

#[test]
fn runtime_owned_node_intersection_resolves_against_latest_node_index() {
    let (graph, ids) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_node_layouts(
        &view,
        [
            (
                ids[0],
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 100.0,
                        height: 80.0,
                    },
                },
            ),
            (
                ids[1],
                CanvasRect {
                    origin: CanvasPoint { x: 140.0, y: 0.0 },
                    size: CanvasSize {
                        width: 100.0,
                        height: 80.0,
                    },
                },
            ),
        ],
    );
    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let start = pointer_sample(&view, ViewPoint::new(0.0, 0.0));
    let end = pointer_sample(&view, ViewPoint::new(300.0, 160.0));
    let baseline = view.selection_baseline().unwrap();
    interaction
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline,
            },
            start,
        )
        .unwrap();
    let moved = interaction.move_to(end).unwrap();
    assert!(moved.query_work_pending());

    publish_node_layouts(
        &view,
        [(
            ids[1],
            CanvasRect {
                origin: CanvasPoint { x: 180.0, y: 20.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        )],
    );
    let resolved = advance_terminal_query_page(&mut interaction, moved, |page| {
        matches!(page, CanvasInteractionQueryPageReceipt::NodeIntersection(_))
    });
    assert!(resolved.effects().is_empty());
}

#[test]
fn clipped_interaction_bounds_preserve_route_geometry_and_restore_probe_membership() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    let publish = |frame: &AdapterFrame, target_interaction: Option<ViewRect>| {
        let collector = frame.collector();
        for frame_node in frame.nodes() {
            let node = frame_node.id();
            let (root_x, visual_x, outward) = if node == ids.source {
                (0.0, 174.0, HandlePosition::Right)
            } else if node == ids.target {
                (240.0, 240.0, HandlePosition::Left)
            } else if node == ids.alternate_source {
                (480.0, 654.0, HandlePosition::Right)
            } else {
                panic!("unexpected routed fixture node {node:?}");
            };
            let frame_port = frame_node.ports().first().expect("fixture port");
            let visual_bounds = port_rect(visual_x);
            let hit_bounds = ViewRect::new(
                ViewPoint::new(visual_x - 6.0, visual_bounds.origin.y - 6.0),
                ViewSize::new(24.0, 24.0),
            );
            let route_anchor =
                side_anchor(visual_bounds, outward).expect("fixture port bounds are valid");
            let mut layout = frame_node.begin_layout(root_rect(root_x));
            if node == ids.target {
                layout
                    .port_rendered_with_interaction_bounds(
                        frame_port.layout_target(),
                        visual_bounds,
                        hit_bounds,
                        target_interaction,
                        route_anchor,
                        outward,
                    )
                    .unwrap();
            } else {
                layout
                    .port_rendered(
                        frame_port.layout_target(),
                        visual_bounds,
                        hit_bounds,
                        route_anchor,
                        outward,
                    )
                    .unwrap();
            }
            collector.layout(layout.finish().unwrap()).unwrap();
        }
        session
            .submit_feedback(collector.finish().unwrap())
            .unwrap();
    };

    let clipped_frame = session.frame(request()).unwrap();
    let target_instance = clipped_frame
        .nodes()
        .iter()
        .find(|node| node.id() == ids.target)
        .unwrap()
        .instance()
        .clone();
    publish(&clipped_frame, None);

    let clipped = session.geometry_snapshot().unwrap();
    let target = clipped
        .port(ids.target_port)
        .expect("clipped port keeps published route geometry");
    assert_eq!(target.hit_bounds(), None);
    assert_eq!(target.route_anchor(), CanvasPoint { x: 240.0, y: 78.0 });
    assert_eq!(
        clipped
            .route(ids.edge)
            .unwrap()
            .facts()
            .endpoints
            .target
            .point,
        target.route_anchor(),
    );
    let intent = connection_binding(
        &session,
        ids.alternate_source_port,
        EdgeId::from_u128(0xfeed),
    );
    assert!(matches!(
        session
            .connection_probe(ConnectionProbeQuery::new(
                intent.clone(),
                ViewPoint::new(240.0, 78.0),
                ConnectionProbeTargeting::new(12.0, 12.0).unwrap(),
                64,
            ))
            .unwrap(),
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::None,
            ..
        }
    ));

    assert!(session.invalidate_node_layout(&target_instance).unwrap());
    let visible_frame = session.frame(request()).unwrap();
    let visible_hit = ViewRect::new(ViewPoint::new(234.0, 66.0), ViewSize::new(24.0, 24.0));
    publish(&visible_frame, Some(visible_hit));

    let restored = session.geometry_snapshot().unwrap();
    let restored_target = restored.port(ids.target_port).unwrap();
    assert_eq!(restored_target.port(), target.port());
    assert_eq!(restored_target.route_anchor(), target.route_anchor());
    assert_eq!(
        restored_target.hit_bounds(),
        Some(CanvasRect {
            origin: CanvasPoint { x: 234.0, y: 66.0 },
            size: CanvasSize {
                width: 24.0,
                height: 24.0,
            },
        })
    );
    assert!(matches!(
        session
            .connection_probe(ConnectionProbeQuery::new(
                intent,
                ViewPoint::new(240.0, 78.0),
                ConnectionProbeTargeting::new(12.0, 12.0).unwrap(),
                64,
            ))
            .unwrap(),
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::Valid { target, .. },
            ..
        } if target.port() == ids.target_port
    ));
}

#[test]
fn complete_port_outcomes_control_hits_routes_culling_and_retirement() {
    let (graph, ids) = graph_with_four_port_outcomes();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_400.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.outcomes", 1),
        )
    };
    let publish = |frame: &AdapterFrame, first| {
        let collector = frame.collector();
        for layout in outcome_layouts(frame, ids, first) {
            collector.layout(layout).unwrap();
        }
        session
            .submit_feedback(collector.finish().unwrap())
            .unwrap()
    };

    let frame = session.frame(request()).unwrap();
    let receipt = publish(&frame, FirstPortOutcome::Rendered);
    assert_eq!(receipt.publication().unwrap().layout_revision(), 1);
    let geometry = session.geometry_snapshot().unwrap();
    assert_eq!(geometry.node(ids.node).unwrap().port_outcomes().len(), 4);
    assert!(matches!(
        geometry.port_outcome(ids.ports[0]),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(matches!(
        geometry.port_outcome(ids.ports[1]),
        Some(PublishedPortOutcome::Hidden)
    ));
    assert!(matches!(
        geometry.port_outcome(ids.ports[2]),
        Some(PublishedPortOutcome::Culled)
    ));
    assert!(matches!(
        geometry.port_outcome(ids.ports[3]),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.port(ids.ports[0]).is_some());
    assert!(
        ids.ports[1..]
            .iter()
            .all(|port| geometry.port(*port).is_none())
    );
    assert!(geometry.route(ids.edges[0]).is_some());
    assert!(
        ids.edges[1..]
            .iter()
            .all(|edge| geometry.route(*edge).is_none())
    );

    let equivalent = session.frame(request()).unwrap();
    let receipt = publish(&equivalent, FirstPortOutcome::Rendered);
    assert!(receipt.publication().is_none());
    assert_eq!(session.geometry_snapshot().unwrap().layout_revision(), 1);

    let hidden = session.frame(request()).unwrap();
    let receipt = publish(&hidden, FirstPortOutcome::Hidden);
    assert_eq!(receipt.publication().unwrap().layout_revision(), 2);
    let geometry = session.geometry_snapshot().unwrap();
    assert!(matches!(
        geometry.port_outcome(ids.ports[0]),
        Some(PublishedPortOutcome::Hidden)
    ));
    assert!(geometry.port(ids.ports[0]).is_none());
    assert!(geometry.route(ids.edges[0]).is_none());

    let unavailable = session.frame(request()).unwrap();
    let receipt = publish(&unavailable, FirstPortOutcome::Unavailable);
    assert_eq!(receipt.publication().unwrap().layout_revision(), 3);
    assert!(matches!(
        session
            .geometry_snapshot()
            .unwrap()
            .port_outcome(ids.ports[0]),
        Some(PublishedPortOutcome::Unavailable)
    ));

    let culled = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: 10_000.0,
                    y: 10_000.0,
                },
                size: CanvasSize {
                    width: 400.0,
                    height: 300.0,
                },
            },
            RendererStamp::new("test.outcomes", 1),
        ))
        .unwrap();
    assert!(culled.nodes().is_empty());
    let geometry = session.geometry_snapshot().unwrap();
    assert_eq!(geometry.layout_revision(), 4);
    assert!(ids.ports.iter().all(|port| matches!(
        geometry.port_outcome(*port),
        Some(PublishedPortOutcome::Culled)
    )));
    assert!(geometry.ports().get(&ids.ports[0]).is_none());

    let repeated_cull = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: 10_000.0,
                    y: 10_000.0,
                },
                size: CanvasSize {
                    width: 400.0,
                    height: 300.0,
                },
            },
            RendererStamp::new("test.outcomes", 1),
        ))
        .unwrap();
    assert!(repeated_cull.nodes().is_empty());
    assert_eq!(session.geometry_snapshot().unwrap().layout_revision(), 4);

    session
        .execute(GraphCommand::topology(
            session.graph_snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(ids.node, IncidentEdgePolicy::Cascade),
        ))
        .unwrap()
        .into_result()
        .unwrap();
    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.node).is_none());
    assert!(
        ids.ports
            .iter()
            .all(|port| geometry.port_outcome(*port).is_none())
    );
}

#[test]
fn view_space_layout_uses_the_issued_transform_for_all_canvas_geometry() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    session
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 10.0, y: -20.0 }, 2.0).unwrap())
        .unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let source_target = node_target(&frame, ids.source);
    let source_port_target = port_target(&frame, ids.source_port);
    let target_target = node_target(&frame, ids.target);
    let target_port_target = port_target(&frame, ids.target_port);
    let collector = frame.collector();
    collector
        .layout(node_layout(
            &source_target,
            ViewRect::new(ViewPoint::new(20.0, 0.0), ViewSize::new(360.0, 240.0)),
            [port_layout_with_geometry(
                &source_port_target,
                ViewRect::new(ViewPoint::new(368.0, 104.0), ViewSize::new(24.0, 24.0)),
                ViewRect::new(ViewPoint::new(356.0, 92.0), ViewSize::new(48.0, 48.0)),
                ViewPoint::new(386.0, 118.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &target_target,
            ViewRect::new(ViewPoint::new(500.0, 0.0), ViewSize::new(360.0, 240.0)),
            [port_layout_with_geometry(
                &target_port_target,
                ViewRect::new(ViewPoint::new(500.0, 104.0), ViewSize::new(24.0, 24.0)),
                ViewRect::new(ViewPoint::new(488.0, 92.0), ViewSize::new(48.0, 48.0)),
                ViewPoint::new(506.0, 118.0),
                HandlePosition::Left,
            )],
        ))
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(receipt.publication().unwrap().published_nodes().len(), 2);
    let geometry = session.geometry_snapshot().unwrap();
    assert_eq!(
        geometry.node(ids.source).unwrap().bounds(),
        CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 20.0 },
            size: CanvasSize {
                width: 180.0,
                height: 120.0,
            },
        }
    );
    assert_eq!(
        geometry.port(ids.source_port).unwrap().visual_bounds(),
        CanvasRect {
            origin: CanvasPoint { x: 174.0, y: 72.0 },
            size: CanvasSize {
                width: 12.0,
                height: 12.0,
            },
        }
    );
    assert_eq!(
        geometry.port(ids.source_port).unwrap().hit_bounds(),
        Some(CanvasRect {
            origin: CanvasPoint { x: 168.0, y: 66.0 },
            size: CanvasSize {
                width: 24.0,
                height: 24.0,
            },
        })
    );
    assert_eq!(
        geometry.port(ids.source_port).unwrap().route_anchor(),
        CanvasPoint { x: 183.0, y: 79.0 }
    );
    assert_eq!(
        geometry.port(ids.source_port).unwrap().outward(),
        HandlePosition::Right
    );
    let route = geometry.route(ids.edge).unwrap();
    assert_eq!(
        route.facts().endpoints.source.point,
        CanvasPoint { x: 183.0, y: 79.0 }
    );
    assert_eq!(
        route.facts().endpoints.target.point,
        CanvasPoint { x: 243.0, y: 79.0 }
    );
    assert!(route.facts().interaction.selectable);
    assert!(!route.facts().interaction.selected);
    assert!(route.facts().interaction.deletable);
    assert!(route.facts().interaction.reconnect_source);
    assert!(route.facts().interaction.reconnect_target);
    assert_eq!(route.facts().hit_test.interaction_width, 18.0);

    let intent = connection_binding(
        &session,
        ids.alternate_source_port,
        EdgeId::from_u128(0xfeed),
    );
    match session
        .connection_probe(ConnectionProbeQuery::new(
            intent.clone(),
            ViewPoint::new(506.0, 118.0),
            ConnectionProbeTargeting::new(12.0, 12.0).unwrap(),
            64,
        ))
        .unwrap()
    {
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::Valid { target, .. },
            receipt,
        } => {
            assert_eq!(target.port(), ids.target_port);
            assert_eq!(receipt.work().policy_evaluations, 1);
        }
        other => panic!("published target port must be a valid candidate: {other:?}"),
    }
    match session
        .connection_probe(ConnectionProbeQuery::new(
            intent,
            ViewPoint::new(386.0, 118.0),
            ConnectionProbeTargeting::new(12.0, 12.0).unwrap(),
            64,
        ))
        .unwrap()
    {
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::Invalid { diagnostics, .. },
            receipt,
        } => {
            assert_eq!(receipt.work().policy_evaluations, 1);
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.key == "connect.direction_mismatch")
            );
        }
        other => panic!("same-direction port must remain an invalid candidate: {other:?}"),
    }

    set_selection_by_id(&session, [], [ids.edge], Vec::new()).unwrap();
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .unwrap()
            .facts()
            .interaction
            .selected
    );
    set_selection_by_id(&session, [], [], Vec::new()).unwrap();
    assert!(
        !session
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .unwrap()
            .facts()
            .interaction
            .selected
    );

    let crossing_bounds = ViewRect::new(ViewPoint::new(440.0, 100.0), ViewSize::new(20.0, 40.0));
    let cursor = match session
        .visible_routes(VisibleRouteQuery::new(crossing_bounds, 0))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Incomplete { cursor, work } => {
            assert_eq!(work.entries_examined, 0);
            cursor
        }
        other => panic!("zero-budget route query must be resumable: {other:?}"),
    };
    set_selection_by_id(&session, [], [ids.edge], Vec::new()).unwrap();
    let routes = match session
        .visible_routes(VisibleRouteQuery::resume(cursor, 64))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Complete { routes, work } => {
            assert!(work.entries_examined > 0);
            assert_eq!(work.flatten_cache_misses, 1);
            routes
        }
        other => panic!("resumed route query must complete: {other:?}"),
    };
    assert_eq!(routes.edge_ids(), &[ids.edge]);
    assert!(
        routes
            .iter()
            .find(|route| route.edge() == ids.edge)
            .unwrap()
            .facts()
            .interaction
            .selected
    );
    set_selection_by_id(&session, [], [], Vec::new()).unwrap();
    match session
        .visible_routes(VisibleRouteQuery::new(crossing_bounds, 64))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Complete { routes, work } => {
            assert_eq!(routes.edge_ids(), &[ids.edge]);
            assert_eq!(work.flatten_cache_hits, 1);
            assert_eq!(work.flatten_cache_misses, 0);
        }
        other => panic!("cached route query must complete: {other:?}"),
    }

    let pick_cursor = match session
        .edge_pick(EdgePickQuery::new(ViewPoint::new(440.0, 118.0), 0.0, 0))
        .unwrap()
    {
        EdgePickQueryOutcome::Incomplete { cursor, receipt } => {
            assert_eq!(receipt.work().entries_examined, 0);
            cursor
        }
        other => panic!("zero-budget edge pick must be resumable: {other:?}"),
    };
    set_selection_by_id(&session, [], [ids.edge], Vec::new()).unwrap();
    assert!(matches!(
        session.edge_pick(EdgePickQuery::resume(pick_cursor, 64)),
        Err(EdgePickQueryError::StaleCursor(
            EdgePickCursorStaleReason::PaintOrder
        ))
    ));
    set_selection_by_id(&session, [], [], Vec::new()).unwrap();
    match session
        .edge_pick(EdgePickQuery::new(ViewPoint::new(440.0, 118.0), 0.0, 64))
        .unwrap()
    {
        EdgePickQueryOutcome::Complete {
            pick: Some(pick),
            receipt,
        } => {
            assert_eq!(pick.edge(), ids.edge);
            assert_eq!(pick.distance_in_view(), 0.0);
            assert_eq!(receipt.work().exact_tests, 1);
            assert_eq!(receipt.work().flatten_cache_hits, 1);
        }
        other => panic!("published route must be edge-pickable: {other:?}"),
    }
    let mut interaction =
        CanvasInteractionSession::attach(&session, CanvasInteractionConfig::default()).unwrap();
    let pointer_in_view = ViewPoint::new(440.0, 118.0);
    let pointer = pointer_sample(&session, pointer_in_view);
    let requested = interaction.request_edge_pick(pointer).unwrap();
    assert!(requested.effects().is_empty());
    advance_terminal_query_page(&mut interaction, requested, |page| {
        matches!(page, CanvasInteractionQueryPageReceipt::EdgePick(_))
    });
    assert_eq!(
        interaction.presentation().hovered_edge(),
        session.edge_instance_key(ids.edge).unwrap()
    );
    assert_eq!(
        complete_nodes_intersecting(
            &session,
            CanvasRect {
                origin: CanvasPoint { x: -1.0, y: 19.0 },
                size: CanvasSize {
                    width: 182.0,
                    height: 122.0,
                },
            },
        ),
        vec![ids.source]
    );
}

#[test]
fn valid_subset_is_published_atomically_with_complete_port_coverage() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let observations = Rc::new(RefCell::new(Vec::new()));
    let callback_observations = observations.clone();
    let weak = session.downgrade();
    session
        .subscribe(move |event| {
            let ViewSessionEvent::LayoutPublished {
                layout_revision,
                nodes,
            } = event
            else {
                return;
            };
            let geometry = weak.geometry_snapshot().unwrap();
            callback_observations.borrow_mut().push((
                layout_revision,
                nodes,
                geometry.layout_revision(),
                geometry.port(ids.source_port).is_some(),
                geometry.route(ids.edge).is_some(),
            ));
        })
        .unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let source_target = node_target(&frame, ids.source);
    let source_port_target = port_target(&frame, ids.source_port);
    let target_target = node_target(&frame, ids.target);
    let collector = frame.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    collector
        .layout(raw_node_layout(&target_target, root_rect(240.0), []))
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let statuses = receipt
        .layout_receipts()
        .map(|operation| operation.status().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            LayoutNodeStatus::Published,
            LayoutNodeStatus::IncompletePorts
        ]
    );
    assert_eq!(receipt.publication().unwrap().layout_revision(), 1);
    assert_eq!(receipt.publication().unwrap().spatial_updates(), 1);
    assert_eq!(receipt.publication().unwrap().route_recomputations(), 1);
    assert!(receipt.request_next_frame());
    assert_eq!(
        receipt.publication().unwrap().published_nodes(),
        &[ids.source]
    );

    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.source).is_some());
    assert!(geometry.port(ids.source_port).is_some());
    assert!(geometry.node(ids.target).is_none());
    assert!(geometry.port(ids.target_port).is_none());
    assert!(geometry.route(ids.edge).is_none());

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let target_target = node_target(&frame, ids.target);
    let target_port_target = port_target(&frame, ids.target_port);
    let collector = frame.collector();
    collector
        .layout(node_layout(
            &target_target,
            root_rect(240.0),
            [port_layout(
                &target_port_target,
                port_rect(240.0),
                HandlePosition::Left,
            )],
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(receipt.publication().unwrap().layout_revision(), 2);
    assert_eq!(receipt.publication().unwrap().spatial_updates(), 1);
    assert_eq!(receipt.publication().unwrap().route_recomputations(), 1);
    assert!(receipt.request_next_frame());
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .is_some()
    );

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let target_target = node_target(&frame, ids.target);
    let collector = frame.collector();
    collector
        .layout(raw_node_layout(&target_target, root_rect(240.0), []))
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(receipt.publication().unwrap().layout_revision(), 3);
    assert_eq!(receipt.publication().unwrap().spatial_updates(), 0);
    assert_eq!(receipt.publication().unwrap().route_recomputations(), 1);
    assert!(receipt.request_next_frame());
    let geometry = session.geometry_snapshot().unwrap();
    assert!(!geometry.node(ids.target).unwrap().is_fresh());
    assert!(geometry.port(ids.target_port).is_none());
    assert!(matches!(
        geometry.port_outcome(ids.target_port),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.route(ids.edge).is_none());
    assert_eq!(session.stats().unwrap().spatial_updates, 2);
    assert_eq!(session.stats().unwrap().route_recomputations, 3);
    assert_eq!(
        observations.borrow().as_slice(),
        &[
            (1, vec![ids.source], 1, true, false),
            (2, vec![ids.target], 2, true, true),
            (3, vec![ids.target], 3, true, false),
        ]
    );
}

#[test]
fn invalid_port_candidates_do_not_block_the_valid_subset() {
    #[derive(Clone, Copy)]
    enum InvalidPortCase {
        Duplicate,
        Foreign,
        NonFinite,
        InteractionOutsideDeclaredHit,
    }

    for (case, expected_status) in [
        (InvalidPortCase::Duplicate, LayoutNodeStatus::DuplicatePorts),
        (InvalidPortCase::Foreign, LayoutNodeStatus::ForeignPort),
        (InvalidPortCase::NonFinite, LayoutNodeStatus::InvalidBounds),
        (
            InvalidPortCase::InteractionOutsideDeclaredHit,
            LayoutNodeStatus::InvalidBounds,
        ),
    ] {
        let (graph, ids) = graph_with_routed_edge();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        let layout_events = Rc::new(Cell::new(0));
        let callback_events = layout_events.clone();
        session
            .subscribe(move |event| {
                if matches!(event, ViewSessionEvent::LayoutPublished { .. }) {
                    callback_events.set(callback_events.get() + 1);
                }
            })
            .unwrap();
        let frame = session
            .frame(FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -100.0,
                        y: -100.0,
                    },
                    size: CanvasSize {
                        width: 1_000.0,
                        height: 600.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            ))
            .unwrap();
        let source_target = node_target(&frame, ids.source);
        let source_port_target = port_target(&frame, ids.source_port);
        let target_target = node_target(&frame, ids.target);
        let target_port_target = port_target(&frame, ids.target_port);
        let invalid_source = match case {
            InvalidPortCase::Duplicate => raw_node_layout(
                &source_target,
                root_rect(0.0),
                [
                    port_layout(&source_port_target, port_rect(174.0), HandlePosition::Right),
                    port_layout(&source_port_target, port_rect(174.0), HandlePosition::Right),
                ],
            ),
            InvalidPortCase::Foreign => raw_node_layout(
                &source_target,
                root_rect(0.0),
                [port_layout(
                    &target_port_target,
                    port_rect(174.0),
                    HandlePosition::Right,
                )],
            ),
            InvalidPortCase::NonFinite => {
                let invalid_bounds = ViewRect {
                    origin: ViewPoint {
                        x: f32::NAN,
                        y: 72.0,
                    },
                    size: ViewSize {
                        width: 12.0,
                        height: 12.0,
                    },
                };
                raw_node_layout(
                    &source_target,
                    root_rect(0.0),
                    [port_layout_with_geometry(
                        &source_port_target,
                        invalid_bounds,
                        invalid_bounds,
                        invalid_bounds.origin,
                        HandlePosition::Right,
                    )],
                )
            }
            InvalidPortCase::InteractionOutsideDeclaredHit => {
                let visual = port_rect(174.0);
                let declared_hit =
                    ViewRect::new(ViewPoint::new(168.0, 66.0), ViewSize::new(24.0, 24.0));
                let floating_hit =
                    ViewRect::new(ViewPoint::new(300.0, 66.0), ViewSize::new(8.0, 8.0));
                let mut layout = NodeLayoutBuilder::new(
                    &source_target,
                    root_rect(0.0),
                    [source_port_target.clone()],
                );
                layout
                    .port_rendered_with_interaction_bounds(
                        &source_port_target,
                        visual,
                        declared_hit,
                        Some(floating_hit),
                        side_anchor(visual, HandlePosition::Right).unwrap(),
                        HandlePosition::Right,
                    )
                    .unwrap();
                layout.finish().unwrap()
            }
        };
        let collector = frame.collector();
        collector.layout(invalid_source).unwrap();
        collector
            .layout(node_layout(
                &target_target,
                root_rect(240.0),
                [port_layout(
                    &target_port_target,
                    port_rect(240.0),
                    HandlePosition::Left,
                )],
            ))
            .unwrap();

        let receipt = session
            .submit_feedback(collector.finish().unwrap())
            .unwrap();

        let statuses = receipt
            .layout_receipts()
            .map(|operation| operation.status().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(statuses, vec![expected_status, LayoutNodeStatus::Published]);
        let publication = receipt.publication().unwrap();
        assert_eq!(publication.layout_revision(), 1);
        assert_eq!(publication.published_nodes(), &[ids.target]);
        assert_eq!(layout_events.get(), 1);
        let geometry = session.geometry_snapshot().unwrap();
        assert!(geometry.node(ids.source).is_none());
        assert!(geometry.port(ids.source_port).is_none());
        assert!(geometry.node(ids.target).is_some());
        assert!(geometry.port(ids.target_port).is_some());
    }
}

#[test]
fn duplicate_node_candidates_reject_the_entire_node_and_withdraw_old_hits() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    let initial = session.frame(request()).unwrap();
    publish_routed_geometry(&session, &initial, ids);
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .is_some()
    );

    let frame = session.frame(request()).unwrap();
    let source_target = node_target(&frame, ids.source);
    let source_port_target = port_target(&frame, ids.source_port);
    let collector = frame.collector();
    for x in [12.0, 24.0] {
        collector
            .layout(node_layout(
                &source_target,
                root_rect(x),
                [port_layout(
                    &source_port_target,
                    port_rect(x + 174.0),
                    HandlePosition::Right,
                )],
            ))
            .unwrap();
    }

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt
            .layout_receipts()
            .map(|operation| operation.status().unwrap())
            .collect::<Vec<_>>(),
        vec![
            LayoutNodeStatus::DuplicateNode,
            LayoutNodeStatus::DuplicateNode,
        ],
    );
    assert_eq!(
        receipt.publication().unwrap().withdrawn_nodes(),
        &[ids.source]
    );
    let geometry = session.geometry_snapshot().unwrap();
    assert!(!geometry.node(ids.source).unwrap().is_fresh());
    assert!(geometry.port(ids.source_port).is_none());
    assert!(matches!(
        geometry.port_outcome(ids.source_port),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.route(ids.edge).is_none());
}

#[test]
fn node_data_layout_refresh_preserves_last_valid_route_until_atomic_replacement() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    let initial = session.frame(request()).unwrap();
    publish_routed_geometry(&session, &initial, ids);
    let before = session.geometry_snapshot().unwrap();
    let before_source = before.node(ids.source).unwrap().clone();
    let before_route = before.route(ids.edge).unwrap().clone();

    session
        .execute(GraphCommand::set_node_data(
            session.graph_snapshot().unwrap().version(),
            ids.source,
            serde_json::json!({ "expanded": true }),
        ))
        .unwrap()
        .into_result()
        .unwrap();

    let pending = session.geometry_snapshot().unwrap();
    assert_eq!(pending.node(ids.source), Some(&before_source));
    assert_eq!(pending.route(ids.edge), Some(&before_route));
    assert!(pending.port(ids.source_port).is_some());

    let replacement = session.frame(request()).unwrap();
    let source_target = node_target(&replacement, ids.source);
    let source_port_target = port_target(&replacement, ids.source_port);
    assert_eq!(
        replacement
            .nodes()
            .iter()
            .find(|node| node.id() == ids.source)
            .unwrap()
            .bounds_source(),
        crate::runtime::adapter_protocol::FrameBoundsSource::Published,
    );
    let collector = replacement.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(24.0),
            [port_layout(
                &source_port_target,
                port_rect(198.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Published,
    );
    let after = session.geometry_snapshot().unwrap();
    assert_eq!(
        after.node(ids.source).unwrap().bounds(),
        CanvasRect {
            origin: CanvasPoint { x: 24.0, y: 20.0 },
            size: CanvasSize {
                width: 180.0,
                height: 120.0,
            },
        }
    );
    assert_ne!(after.route(ids.edge).unwrap(), &before_route);
}

#[test]
fn stale_node_target_withdraws_old_port_hits_and_routes() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    let initial = session.frame(request()).unwrap();
    let stale_source_target = node_target(&initial, ids.source);
    let stale_source_port_target = port_target(&initial, ids.source_port);
    publish_routed_geometry(&session, &initial, ids);

    let current = session.frame(request()).unwrap();
    let collector = current.collector();
    collector
        .layout(node_layout(
            &stale_source_target,
            root_rect(12.0),
            [port_layout(
                &stale_source_port_target,
                port_rect(186.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Stale,
    );
    assert!(receipt.request_next_frame());
    assert_eq!(
        receipt.publication().unwrap().withdrawn_nodes(),
        &[ids.source]
    );
    let geometry = session.geometry_snapshot().unwrap();
    assert!(!geometry.node(ids.source).unwrap().is_fresh());
    assert!(geometry.port(ids.source_port).is_none());
    assert!(matches!(
        geometry.port_outcome(ids.source_port),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.route(ids.edge).is_none());
}

#[test]
fn node_instance_invalidation_is_view_local_and_stales_pending_feedback() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };

    for view in [&first, &second] {
        let frame = view.frame(request()).unwrap();
        let source_target = node_target(&frame, ids.source);
        let source_port_target = port_target(&frame, ids.source_port);
        let collector = frame.collector();
        collector
            .layout(node_layout(
                &source_target,
                root_rect(0.0),
                [port_layout(
                    &source_port_target,
                    port_rect(174.0),
                    HandlePosition::Right,
                )],
            ))
            .unwrap();
        view.submit_feedback(collector.finish().unwrap()).unwrap();
    }
    assert!(
        first
            .geometry_snapshot()
            .unwrap()
            .port(ids.source_port)
            .is_some()
    );
    assert!(
        second
            .geometry_snapshot()
            .unwrap()
            .port(ids.source_port)
            .is_some()
    );

    let pending = first.frame(request()).unwrap();
    let instance = pending
        .nodes()
        .iter()
        .find(|node| node.id() == ids.source)
        .unwrap()
        .instance()
        .clone();
    let layout_target = node_target(&pending, ids.source);
    let port_layout_target = port_target(&pending, ids.source_port);
    let collector = pending.collector();
    collector
        .command(GraphCommand::set_node_data(
            pending.graph_version(),
            ids.source,
            serde_json::json!({ "must_not_apply": true }),
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &layout_target,
            root_rect(0.0),
            [port_layout(
                &port_layout_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();

    assert!(first.invalidate_node_layout(&instance).unwrap());
    let first_geometry = first.geometry_snapshot().unwrap();
    assert!(!first_geometry.node(ids.source).unwrap().is_fresh());
    assert!(first_geometry.port(ids.source_port).is_none());
    assert!(matches!(
        first_geometry.port_outcome(ids.source_port),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(
        second
            .geometry_snapshot()
            .unwrap()
            .port(ids.source_port)
            .is_some()
    );

    let error = first
        .submit_feedback(collector.finish().unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::StaleFrame {
            reason: StaleFrameReason::NodeLayoutGeneration,
            ..
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[test]
fn adapter_environment_invalidation_is_exact_and_view_local() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    for view in [&first, &second] {
        let frame = view.frame(request()).unwrap();
        let target = frame.nodes()[0].layout_target().clone();
        let collector = frame.collector();
        collector
            .layout(node_layout(&target, root_rect(0.0), []))
            .unwrap();
        view.submit_feedback(collector.finish().unwrap()).unwrap();
    }
    let first_events = Rc::new(Cell::new(0));
    let callback_events = first_events.clone();
    first
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::LayoutPublished { .. }) {
                callback_events.set(callback_events.get() + 1);
            }
        })
        .unwrap();
    let first_before = first.stats().unwrap();
    let second_geometry = second.geometry_snapshot().unwrap();
    assert_eq!(second_geometry.layout_revision(), 1);
    assert!(second_geometry.node(nodes[0]).unwrap().is_fresh());
    let second_before = second.stats().unwrap();
    let environment = AdapterEnvironment::new(2, 3, 5, 7);

    assert!(first.set_adapter_environment(environment).unwrap());

    assert_eq!(first_events.get(), 1);
    let first_geometry = first.geometry_snapshot().unwrap();
    assert_eq!(first_geometry.layout_revision(), 2);
    assert!(!first_geometry.node(nodes[0]).unwrap().is_fresh());
    let first_after = first.stats().unwrap();
    assert_eq!(
        first_after.layout_publications,
        first_before.layout_publications + 1
    );
    assert_eq!(
        first_after.node_invalidations,
        first_before.node_invalidations + 1
    );
    assert_eq!(second.stats().unwrap(), second_before);

    assert!(!first.set_adapter_environment(environment).unwrap());
    assert_eq!(first_events.get(), 1);
    assert_eq!(first.stats().unwrap(), first_after);
    assert_eq!(second.stats().unwrap(), second_before);
}

#[test]
fn culling_preserves_root_admission_but_withdraws_ports_and_routes() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let visible = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let source_target = node_target(&visible, ids.source);
    let source_port_target = port_target(&visible, ids.source_port);
    let target_target = node_target(&visible, ids.target);
    let target_port_target = port_target(&visible, ids.target_port);
    let collector = visible.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &target_target,
            root_rect(240.0),
            [port_layout(
                &target_port_target,
                port_rect(240.0),
                HandlePosition::Left,
            )],
        ))
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let before = session.geometry_snapshot().unwrap();
    assert!(before.port(ids.source_port).is_some());
    assert!(before.port(ids.target_port).is_some());
    assert!(before.route(ids.edge).is_some());

    let culled = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: 10_000.0,
                    y: 10_000.0,
                },
                size: CanvasSize {
                    width: 400.0,
                    height: 300.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    assert!(culled.nodes().is_empty());
    let culling_work = culled.materialization_receipt().geometry();
    assert_eq!(culling_work.node_invalidations(), 2);
    assert_eq!(culling_work.spatial_updates(), 0);
    assert_eq!(culling_work.route_invalidations(), 1);
    assert_eq!(culling_work.route_recomputations(), 1);
    let after_cull = session.geometry_snapshot().unwrap();
    assert_eq!(after_cull.layout_revision(), 2);
    assert!(after_cull.node(ids.source).unwrap().is_fresh());
    assert!(after_cull.node(ids.target).unwrap().is_fresh());
    assert!(after_cull.port(ids.source_port).is_none());
    assert!(after_cull.port(ids.target_port).is_none());
    assert!(matches!(
        after_cull.port_outcome(ids.source_port),
        Some(PublishedPortOutcome::Culled)
    ));
    assert!(matches!(
        after_cull.port_outcome(ids.target_port),
        Some(PublishedPortOutcome::Culled)
    ));
    assert!(after_cull.route(ids.edge).is_none());

    let culled_collector = culled.collector();
    culled_collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(culled_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Culled
    );
    assert!(receipt.publication().is_none());
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .port(ids.source_port)
            .is_none()
    );
}

#[test]
fn renderer_replacement_withdraws_old_instance_ports_before_new_layout() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = |renderer_incarnation| {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", renderer_incarnation),
        )
    };

    let first_frame = session.frame(request(1)).unwrap();
    let old_instance = first_frame
        .nodes()
        .iter()
        .find(|node| node.id() == ids.source)
        .unwrap()
        .instance()
        .clone();
    let source_target = node_target(&first_frame, ids.source);
    let source_port_target = port_target(&first_frame, ids.source_port);
    let collector = first_frame.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .port(ids.source_port)
            .is_some()
    );
    let layout_events = Rc::new(Cell::new(0));
    let callback_events = layout_events.clone();
    session
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::LayoutPublished { .. }) {
                callback_events.set(callback_events.get() + 1);
            }
        })
        .unwrap();

    let replacement = session.frame(request(2)).unwrap();
    let replacement_work = replacement.materialization_receipt().geometry();
    assert_eq!(replacement_work.node_invalidations(), 3);
    assert_eq!(replacement_work.spatial_updates(), 1);
    assert_eq!(replacement_work.route_invalidations(), 1);
    assert_eq!(replacement_work.route_recomputations(), 1);
    assert_eq!(layout_events.get(), 1);
    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.source).is_none());
    assert!(geometry.port(ids.source_port).is_none());
    assert!(geometry.port_outcome(ids.source_port).is_none());
    assert!(
        !complete_nodes_intersecting(
            &session,
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 20.0 },
                size: CanvasSize {
                    width: 180.0,
                    height: 120.0,
                },
            },
        )
        .contains(&ids.source)
    );
    assert!(!session.invalidate_node_layout(&old_instance).unwrap());

    let source_target = node_target(&replacement, ids.source);
    let source_port_target = port_target(&replacement, ids.source_port);
    let collector = replacement.collector();
    collector
        .layout(node_layout(
            &source_target,
            root_rect(0.0),
            [port_layout(
                &source_port_target,
                port_rect(174.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(receipt.publication().unwrap().layout_revision(), 3);
    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.source).unwrap().is_fresh());
    assert_eq!(
        geometry.node(ids.source).unwrap().renderer().incarnation(),
        2
    );
    let revision = geometry.layout_revision();
    let stable = session.frame(request(2)).unwrap();
    let stable_work = stable.materialization_receipt().geometry();
    assert_eq!(stable_work.node_invalidations(), 0);
    assert_eq!(stable_work.spatial_updates(), 0);
    assert_eq!(stable_work.route_invalidations(), 0);
    assert_eq!(stable_work.route_recomputations(), 0);
    assert_eq!(layout_events.get(), 2);
    assert_eq!(
        session.geometry_snapshot().unwrap().layout_revision(),
        revision
    );
}

#[test]
fn edge_only_reconnect_disconnect_and_restore_each_advance_geometry_revision_once() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    for (node, port, root_x, port_x, outward) in [
        (
            ids.source,
            ids.source_port,
            0.0,
            174.0,
            HandlePosition::Right,
        ),
        (
            ids.target,
            ids.target_port,
            240.0,
            240.0,
            HandlePosition::Left,
        ),
        (
            ids.alternate_source,
            ids.alternate_source_port,
            480.0,
            654.0,
            HandlePosition::Right,
        ),
    ] {
        collector
            .layout(node_layout(
                &node_target(&frame, node),
                root_rect(root_x),
                [port_layout(
                    &port_target(&frame, port),
                    port_rect(port_x),
                    outward,
                )],
            ))
            .unwrap();
    }
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let initial = session.geometry_snapshot().unwrap();
    assert_eq!(initial.layout_revision(), 1);
    let initial_route = initial.route(ids.edge).unwrap().clone();
    let route_view = ViewRect::new(ViewPoint::new(0.0, 0.0), ViewSize::new(800.0, 200.0));
    let reconnect_cursor = match session
        .visible_routes(VisibleRouteQuery::new(route_view, 0))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Incomplete { cursor, .. } => cursor,
        other => panic!("live route query should pause at zero budget: {other:?}"),
    };

    session
        .execute(GraphCommand::reconnect(
            session.graph_snapshot().unwrap().version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap()
        .into_result()
        .unwrap()
        .applied()
        .expect("reconnect applies");
    assert!(matches!(
        session.visible_routes(VisibleRouteQuery::resume(reconnect_cursor, 64)),
        Err(VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::RouteGeometry
        ))
    ));
    let reconnected = session.geometry_snapshot().unwrap();
    assert_eq!(reconnected.layout_revision(), 2);
    assert_ne!(reconnected.route(ids.edge).unwrap(), &initial_route);
    let disconnect_cursor = match session
        .visible_routes(VisibleRouteQuery::new(route_view, 0))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Incomplete { cursor, .. } => cursor,
        other => panic!("reconnected route query should pause at zero budget: {other:?}"),
    };

    session
        .execute(GraphCommand::disconnect(
            session.graph_snapshot().unwrap().version(),
            ids.edge,
        ))
        .unwrap()
        .into_result()
        .unwrap()
        .applied()
        .expect("disconnect applies");
    assert!(matches!(
        session.visible_routes(VisibleRouteQuery::resume(disconnect_cursor, 64)),
        Err(VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::RouteGeometry
        ))
    ));
    let disconnected = session.geometry_snapshot().unwrap();
    assert_eq!(disconnected.layout_revision(), 3);
    assert!(disconnected.route(ids.edge).is_none());
    match session
        .visible_routes(VisibleRouteQuery::new(route_view, 64))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Complete { routes, .. } => {
            assert!(routes.edge_ids().is_empty());
        }
        other => panic!("removed route must leave no indexed entry: {other:?}"),
    }

    session
        .execute(GraphCommand::undo(
            session.graph_snapshot().unwrap().version(),
        ))
        .unwrap()
        .into_result()
        .unwrap();
    let restored = session.geometry_snapshot().unwrap();
    assert_eq!(restored.layout_revision(), 4);
    assert!(restored.route(ids.edge).is_some());
    match session
        .visible_routes(VisibleRouteQuery::new(route_view, 64))
        .unwrap()
    {
        VisibleRouteQueryOutcome::Complete { routes, .. } => {
            assert_eq!(routes.edge_ids(), &[ids.edge]);
        }
        other => panic!("restored route must be indexed again: {other:?}"),
    }
}

#[test]
fn command_before_layout_withdraws_frame_routes_after_reconnect_and_disconnect() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 1_000.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };

    let initial = session.frame(request()).unwrap();
    let collector = initial.collector();
    for (node, port, root_x, port_x, outward) in [
        (
            ids.source,
            ids.source_port,
            0.0,
            174.0,
            HandlePosition::Right,
        ),
        (
            ids.target,
            ids.target_port,
            240.0,
            240.0,
            HandlePosition::Left,
        ),
        (
            ids.alternate_source,
            ids.alternate_source_port,
            480.0,
            654.0,
            HandlePosition::Right,
        ),
    ] {
        collector
            .layout(node_layout(
                &node_target(&initial, node),
                root_rect(root_x),
                [port_layout(
                    &port_target(&initial, port),
                    port_rect(port_x),
                    outward,
                )],
            ))
            .unwrap();
    }
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let initial_route = session
        .geometry_snapshot()
        .unwrap()
        .route(ids.edge)
        .unwrap()
        .clone();

    let reconnect_frame = session.frame(request()).unwrap();
    let collector = reconnect_frame.collector();
    collector
        .command(GraphCommand::reconnect(
            reconnect_frame.graph_version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &node_target(&reconnect_frame, ids.source),
            root_rect(12.0),
            [port_layout(
                &port_target(&reconnect_frame, ids.source_port),
                port_rect(186.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    let reconnect_receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        reconnect_receipt
            .layout_receipts()
            .next()
            .unwrap()
            .status()
            .unwrap(),
        LayoutNodeStatus::Stale
    );
    let reconnected = session.geometry_snapshot().unwrap();
    assert!(reconnected.port(ids.source_port).is_none());
    assert!(reconnected.port(ids.alternate_source_port).is_some());
    assert!(reconnected.port(ids.target_port).is_some());
    assert_ne!(reconnected.route(ids.edge).unwrap(), &initial_route);

    let disconnect_frame = session.frame(request()).unwrap();
    let collector = disconnect_frame.collector();
    collector
        .command(GraphCommand::disconnect(
            disconnect_frame.graph_version(),
            ids.edge,
        ))
        .unwrap();
    collector
        .layout(node_layout(
            &node_target(&disconnect_frame, ids.alternate_source),
            root_rect(492.0),
            [port_layout(
                &port_target(&disconnect_frame, ids.alternate_source_port),
                port_rect(666.0),
                HandlePosition::Right,
            )],
        ))
        .unwrap();
    let disconnect_receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        disconnect_receipt
            .layout_receipts()
            .next()
            .unwrap()
            .status()
            .unwrap(),
        LayoutNodeStatus::Stale
    );
    let disconnected = session.geometry_snapshot().unwrap();
    assert!(disconnected.port(ids.alternate_source_port).is_none());
    assert!(disconnected.route(ids.edge).is_none());
}

#[test]
fn renderer_event_reentrant_frame_supersedes_the_outer_frame_without_losing_the_inner_one() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    let request = |renderer_incarnation| {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", renderer_incarnation),
        )
    };
    let initial = session.frame(request(1)).unwrap();
    let collector = initial.collector();
    collector
        .layout(
            initial.nodes()[0]
                .begin_layout(root_rect(0.0))
                .finish()
                .unwrap(),
        )
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    let nested_frame = Rc::new(RefCell::new(None));
    let callback_frame = nested_frame.clone();
    let weak_session = Rc::downgrade(&session);
    session
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::LayoutPublished { .. }) {
                let session = weak_session.upgrade().unwrap();
                *callback_frame.borrow_mut() = Some(session.frame(request(2)).unwrap());
            }
        })
        .unwrap();

    let error = session.frame(request(2)).unwrap_err();
    assert!(matches!(
        error,
        FrameMaterializationError::StaleFrame {
            reason: StaleFrameReason::Superseded,
            ..
        }
    ));

    let nested = nested_frame.borrow_mut().take().unwrap();
    let collector = nested.collector();
    collector
        .layout(
            nested.nodes()[0]
                .begin_layout(root_rect(0.0))
                .finish()
                .unwrap(),
        )
        .unwrap();
    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Published
    );
}

#[test]
fn layout_publication_reentrancy_queues_nested_events() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let instance = frame.nodes()[0].instance().clone();
    let revisions = Rc::new(RefCell::new(Vec::new()));
    let callback_revisions = revisions.clone();
    let callback_count = Rc::new(Cell::new(0));
    let nested_count = callback_count.clone();
    let weak_session = Rc::downgrade(&session);
    session
        .subscribe(move |event| {
            let ViewSessionEvent::LayoutPublished {
                layout_revision, ..
            } = event
            else {
                return;
            };
            callback_revisions.borrow_mut().push(layout_revision);
            if nested_count.replace(nested_count.get() + 1) == 0 {
                assert!(
                    weak_session
                        .upgrade()
                        .unwrap()
                        .invalidate_node_layout(&instance)
                        .unwrap()
                );
            }
        })
        .unwrap();
    let collector = frame.collector();
    collector
        .layout(
            frame.nodes()[0]
                .begin_layout(root_rect(0.0))
                .finish()
                .unwrap(),
        )
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(receipt.publication().unwrap().layout_revision(), 1);
    assert_eq!(revisions.borrow().as_slice(), &[1, 2]);
    assert_eq!(callback_count.get(), 2);
    let geometry = session.geometry_snapshot().unwrap();
    assert_eq!(geometry.layout_revision(), 2);
    assert!(!geometry.node(nodes[0]).unwrap().is_fresh());
}
