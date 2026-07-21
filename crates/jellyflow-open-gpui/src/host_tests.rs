use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable,
    EdgeReconnectableEndpoint, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{
    AdapterEnvironment, ApplyPipelineError, CanvasInteractionPhase, ConnectionGestureIntent,
    EdgeEndpoint, FeedbackSubmissionError, FrameRequest, GraphCommand, GraphRuntime,
    GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot, HandlePosition, IncidentEdgePolicy,
    InteractionTerminalReason, LayoutNodeStatus, NodeGeometryEdit, PublishedPortOutcome,
    RendererStamp, RuntimePatch, StaleFrameReason, TopologyEdit, TopologyPort, ViewCloseReason,
    ViewPoint, ViewRect, ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
    ViewSize,
};
use open_gpui::{
    AppContext, Context, Entity, HitboxId, InteractiveElement, KeyUpEvent, Keystroke, Modifiers,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Render, ScrollDelta,
    ScrollWheelEvent, Styled, TouchPhase, VisualContext, Window, div, point, px,
};

use crate::authoring::OpenGpuiPortHandle;
use crate::chrome::OpenGpuiEdgeVisualState;
use crate::command_journal::OPEN_GPUI_COMMAND_JOURNAL_CAPACITY;
use crate::interaction::{
    OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource, OpenGpuiPortDragCancelReason,
    OpenGpuiPortDragEvent, OpenGpuiPortDragPhase, OpenGpuiPortEvent, OpenGpuiSceneGestureKind,
    port_drag_owner,
};
use crate::size_policy::{OpenGpuiNodeSizeEpoch, OpenGpuiNodeSizeUpdate};
use crate::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiContentHeightPolicy,
    OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeSizePolicy,
};

use super::scene_render::{edge_path_endpoints, visit_edge_chrome_wires_in_paint_order};
use super::{
    LayoutReceiptAction, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY, OPEN_GPUI_SCENE_LAYER_ORDER,
    OpenGpuiConnectionPreviewEvidence, OpenGpuiNodeBinding, OpenGpuiNodeRegistry,
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGesturePhase, OpenGpuiSceneHost,
    OpenGpuiSceneLayer, layout_receipt_action,
};

#[path = "host/tests/interaction_characterization.rs"]
mod interaction_characterization;

#[path = "host/tests/canonical_conformance.rs"]
mod canonical_conformance;

#[path = "host/tests/frame_transaction.rs"]
mod frame_transaction;

const KIND: &str = "test.scene-layer-node";
const RENDERER: &str = "test.native.scene-layer-node";

#[test]
fn production_binding_and_host_have_no_synchronous_dispatch_escape_hatch() {
    assert!(!include_str!("binding.rs").contains("pub fn execute"));
    assert!(!include_str!("host.rs").contains("self.view.execute("));
}

struct RoutedNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

struct RootOnlyNode;

struct WholeDragNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

struct StickyPortNode {
    binding: Entity<OpenGpuiNodeBinding>,
    handle: Option<OpenGpuiPortHandle>,
}

impl Render for RoutedNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live node authoring");
        let node = authoring.node_id();
        let port = authoring
            .ports()
            .find(|port| {
                matches!(
                    (authoring.node().pos.x, port.facts().dir),
                    (x, PortDirection::Out) if x < 300.0
                ) || matches!(
                    (authoring.node().pos.x, port.facts().dir),
                    (x, PortDirection::In) if x >= 300.0
                )
            })
            .expect("one routed port for this fixture node");
        let direction = port.facts().dir;
        let handle = port.handle().clone();
        let drag_handle = binding.drag_handle();

        let selector = test_port_selector(port.id(), direction);
        let port = div()
            .debug_selector(move || selector.clone())
            .absolute()
            .top(px(43.0))
            .w(px(14.0))
            .h(px(14.0));
        let port = match direction {
            PortDirection::In => port.left(px(0.0)),
            PortDirection::Out => port.right(px(0.0)),
        };

        let node_selector = test_node_selector(node);
        drag_handle.wrap(
            div()
                .debug_selector(move || node_selector)
                .relative()
                .size_full()
                .child(handle.measure(port)),
        )
    }
}

struct InteractiveRoutedNode {
    binding: Entity<OpenGpuiNodeBinding>,
    events: std::rc::Rc<std::cell::Cell<u32>>,
}

struct InteractiveScene {
    host: Entity<OpenGpuiSceneHost>,
}

struct OffsetInteractiveScene {
    host: Entity<OpenGpuiSceneHost>,
}

impl Render for InteractiveScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child(self.host.clone())
    }
}

impl Render for OffsetInteractiveScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().relative().size_full().child(
            div()
                .absolute()
                .left(px(37.0))
                .top(px(29.0))
                .w(px(800.0))
                .h(px(640.0))
                .child(self.host.clone()),
        )
    }
}

impl Render for InteractiveRoutedNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live node authoring");
        let port = authoring
            .ports()
            .find(|port| {
                matches!(
                    (authoring.node().pos.x, port.facts().dir),
                    (x, PortDirection::Out) if x < 300.0
                ) || matches!(
                    (authoring.node().pos.x, port.facts().dir),
                    (x, PortDirection::In) if x >= 300.0
                )
            })
            .expect("one routed port for this fixture node");
        let direction = port.facts().dir;
        let handle = port.handle().clone();
        let drag_handle = binding.drag_handle();
        let selector = test_port_selector(port.id(), direction);
        let marker = div()
            .debug_selector(move || selector.clone())
            .w(px(14.0))
            .h(px(14.0));
        let positioned = match direction {
            PortDirection::In => div().absolute().left(px(0.0)).top(px(43.0)),
            PortDirection::Out => div().absolute().right(px(0.0)).top(px(43.0)),
        };
        let interactive = handle.interactive_with_observer(marker, {
            let events = self.events.clone();
            move |_, _, _| events.set(events.get() + 1)
        });
        drag_handle.wrap(
            div()
                .relative()
                .size_full()
                .child(positioned.child(interactive)),
        )
    }
}

impl Render for RootOnlyNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full()
    }
}

impl Render for WholeDragNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.binding.read(cx).drag_handle().wrap(div().size_full())
    }
}

impl Render for StickyPortNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        if self.handle.is_none() {
            let binding = self.binding.read(cx);
            let authoring = binding.authoring().expect("live sticky binding");
            self.handle = authoring.ports().next().map(|port| port.handle().clone());
        }

        let mut root = div().relative().size_full();
        if let Some(handle) = &self.handle {
            root = root.child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(43.0))
                    .child(handle.measure(div().w(px(14.0)).h(px(14.0)))),
            );
        }
        root
    }
}

#[open_gpui::test]
fn mounted_host_scopes_environment_and_layout_invalidation_to_its_view(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let sibling_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("sibling view");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("host view");
    let weak_view = view.downgrade();
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let source_binding = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured_binding = source_binding.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        if binding
            .read(cx)
            .authoring()
            .expect("live node authoring")
            .node_id()
            == source
        {
            *captured_binding.borrow_mut() = Some(binding.clone());
        }
        cx.new(|_| RoutedNode { binding })
    });

    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let old_host_frame = cx.read_entity(&host, |host, _| {
        host.view
            .frame(host.frame_request())
            .expect("old host frame")
    });
    let sibling_frame = sibling_view
        .frame(
            jellyflow::runtime::FrameRequest::new(viewport(), RendererStamp::new(RENDERER, 1))
                .with_incident_routing_dependencies(),
        )
        .expect("sibling frame");
    let old_host_feedback = old_host_frame
        .collector()
        .finish()
        .expect("old host feedback");
    let (environment_changed, old_host_error) = host.update(cx, |host, cx| {
        let changed = host
            .set_adapter_environment(AdapterEnvironment::new(2, 3, 5, 7), cx)
            .expect("environment update");
        let error = host
            .view
            .submit_feedback(old_host_feedback)
            .expect_err("host frame must become stale");
        (changed, error)
    });
    assert!(environment_changed);
    assert!(
        matches!(
            &old_host_error,
            FeedbackSubmissionError::StaleFrame {
                reason: StaleFrameReason::Environment,
                ..
            }
        ),
        "unexpected stale host feedback: {old_host_error:?}"
    );
    sibling_view
        .submit_feedback(
            sibling_frame
                .collector()
                .finish()
                .expect("sibling feedback"),
        )
        .expect("another view keeps its environment");
    draw(cx);
    assert!(
        weak_view
            .geometry_snapshot()
            .expect("environment-refreshed geometry")
            .nodes()
            .values()
            .all(|node| node.is_fresh())
    );

    let generations_before = cx.read_entity(&host, |host, _| {
        let frame = host
            .view
            .frame(host.frame_request())
            .expect("generation frame");
        let source_generation = frame
            .nodes()
            .iter()
            .find(|node| node.id() == source)
            .expect("source frame")
            .layout_generation();
        let target_generation = frame
            .nodes()
            .iter()
            .find(|node| node.id() == target)
            .expect("target frame")
            .layout_generation();
        (frame, source_generation, target_generation)
    });
    let stale_generation_feedback = generations_before
        .0
        .collector()
        .finish()
        .expect("stale generation feedback");
    let (layout_invalidated, stale_generation_error) = host.update(cx, |host, cx| {
        let invalidated = host
            .invalidate_node_layout(source, cx)
            .expect("source invalidation");
        let error = host
            .view
            .submit_feedback(stale_generation_feedback)
            .expect_err("old generation must be rejected");
        (invalidated, error)
    });
    assert!(layout_invalidated);
    assert!(
        matches!(
            &stale_generation_error,
            FeedbackSubmissionError::StaleFrame {
                reason: StaleFrameReason::NodeLayoutGeneration,
                ..
            }
        ),
        "unexpected stale generation feedback: {stale_generation_error:?}"
    );

    let fresh_generations = cx.read_entity(&host, |host, _| {
        let frame = host.view.frame(host.frame_request()).expect("fresh frame");
        let source_generation = frame
            .nodes()
            .iter()
            .find(|node| node.id() == source)
            .expect("fresh source")
            .layout_generation();
        let target_generation = frame
            .nodes()
            .iter()
            .find(|node| node.id() == target)
            .expect("fresh target")
            .layout_generation();
        (source_generation, target_generation)
    });
    assert_eq!(fresh_generations.0, generations_before.1 + 1);
    assert_eq!(fresh_generations.1, generations_before.2);

    let source_binding = source_binding
        .borrow()
        .clone()
        .expect("source binding captured");
    assert!(
        source_binding
            .update(cx, |binding, cx| binding.invalidate_layout(cx))
            .expect("binding invalidation")
    );
    draw(cx);
    let geometry = weak_view
        .geometry_snapshot()
        .expect("fresh invalidated geometry");
    assert!(geometry.nodes()[&source].is_fresh());
    assert_eq!(
        geometry.nodes()[&source].layout_generation(),
        fresh_generations.0 + 1
    );
    assert_eq!(
        geometry.nodes()[&target].layout_generation(),
        fresh_generations.1
    );
}

#[test]
fn layout_receipt_actions_only_retry_stale_feedback() {
    assert_eq!(
        layout_receipt_action(LayoutNodeStatus::Published),
        LayoutReceiptAction::Benign
    );
    assert_eq!(
        layout_receipt_action(LayoutNodeStatus::Unchanged),
        LayoutReceiptAction::Benign
    );
    assert_eq!(
        layout_receipt_action(LayoutNodeStatus::Stale),
        LayoutReceiptAction::Refresh
    );
    for status in [
        LayoutNodeStatus::MissingNode,
        LayoutNodeStatus::Hidden,
        LayoutNodeStatus::Culled,
        LayoutNodeStatus::InvalidBounds,
        LayoutNodeStatus::IncompletePorts,
        LayoutNodeStatus::DuplicatePorts,
        LayoutNodeStatus::ForeignPort,
        LayoutNodeStatus::DuplicateNode,
    ] {
        assert_eq!(
            layout_receipt_action(status),
            LayoutReceiptAction::Reject,
            "{status:?} must be diagnosed without an automatic retry"
        );
    }
}

#[open_gpui::test]
fn incomplete_real_port_measurement_preserves_authoritative_geometry_without_retry(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    publish_edge_geometry_with(&view, [], viewport(), |_, _, bounds, direction| {
        let x = match direction {
            PortDirection::In => bounds.origin.x,
            PortDirection::Out => bounds.origin.x + bounds.size.width - 12.0,
        };
        ViewRect::new(
            ViewPoint::new(x, bounds.origin.y + 44.0),
            ViewSize::new(12.0, 12.0),
        )
    });
    let authoritative = weak_view
        .geometry_snapshot()
        .expect("authoritative geometry");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |_binding, _window, cx| cx.new(|_| RootOnlyNode));
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");

    draw(cx);
    let counts_after_draw = cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("incomplete native measurement is explicit");
        assert!(diagnostic.message().contains("missing real ports"));
        assert_eq!(host.stats().layout_rejections(), 0);
        assert!(host.stats().measurement_failures() >= 2);
        assert!(host.stats().feedback_failures() >= 2);
        (
            host.stats().feedback_submissions(),
            host.stats().layout_rejections(),
            host.stats().feedback_failures(),
        )
    });
    let geometry = weak_view
        .geometry_snapshot()
        .expect("retained authoritative geometry");
    assert_eq!(geometry.layout_revision(), authoritative.layout_revision());
    for (node, port) in [
        (NodeId::from_u128(10), PortId::from_u128(20)),
        (NodeId::from_u128(11), PortId::from_u128(21)),
    ] {
        assert_eq!(geometry.node(node), authoritative.node(node));
        assert_eq!(geometry.port(port), authoritative.port(port));
        assert!(matches!(
            geometry.port_outcome(port),
            Some(PublishedPortOutcome::Rendered(_)),
        ));
    }
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&host, |host, _| (
            host.stats().feedback_submissions(),
            host.stats().layout_rejections(),
            host.stats().feedback_failures(),
        )),
        counts_after_draw,
        "incomplete coverage must not schedule an unbounded retry"
    );
}

#[open_gpui::test]
fn retired_dynamic_port_handle_reports_prepaint_error_with_node_identity(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |binding, _window, cx| {
        cx.new(|_| StickyPortNode {
            binding,
            handle: None,
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(
                NodeId::from_u128(10),
                std::iter::empty::<TopologyPort>(),
                IncidentEdgePolicy::Reject,
            ),
        ))
        .expect("replace source ports")
        .applied()
        .expect("port replacement applies");
    cx.run_until_parked();
    draw(cx);

    let counts_after_draw = cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("prepaint measurement diagnostic");
        assert_eq!(diagnostic.node(), Some(NodeId::from_u128(10)));
        assert!(!diagnostic.message().is_empty());
        assert!(host.stats().measurement_failures() >= 1);
        assert_eq!(
            host.stats().feedback_failures(),
            host.stats().measurement_failures()
        );
        (
            host.stats().feedback_submissions(),
            host.stats().measurement_failures(),
            host.stats().feedback_failures(),
        )
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&host, |host, _| (
            host.stats().feedback_submissions(),
            host.stats().measurement_failures(),
            host.stats().feedback_failures(),
        )),
        counts_after_draw,
        "a deterministic prepaint error must not trigger an unbounded retry"
    );
}

#[open_gpui::test]
fn fresh_route_is_painted_after_edges_and_before_retained_nodes(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });

    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let geometry = weak_view.geometry_snapshot().expect("published geometry");
    assert_eq!(geometry.routes().len(), 1);
    assert!(geometry.nodes().values().all(|node| node.is_fresh()));
    assert!(
        cx.debug_bounds(super::scene_render::OPEN_GPUI_GRID_SELECTOR)
            .is_some(),
        "the default visible grid must share the edge layer behind nodes",
    );

    cx.read_entity(&host, |host, _| {
        let evidence = &host.scene_evidence;
        assert_eq!(
            evidence.first_paint_wire_count.get(),
            Some(0),
            "the first paint must not invent a wire before fresh port geometry exists",
        );
        assert!(
            evidence
                .first_non_empty_paint_pass
                .get()
                .is_some_and(|pass| pass >= 2),
            "a real route must enter the edge paint layer on a later fresh-layout frame",
        );
        assert_eq!(evidence.last_planned_wire_count.get(), 1);
        assert_eq!(evidence.last_painted_wire_count.get(), 1);
        assert_eq!(
            evidence.last_layers.get(),
            Some(OPEN_GPUI_SCENE_LAYER_ORDER)
        );

        let edge = OPEN_GPUI_SCENE_LAYER_ORDER
            .iter()
            .position(|layer| *layer == OpenGpuiSceneLayer::Edge)
            .expect("edge layer");
        let node = OPEN_GPUI_SCENE_LAYER_ORDER
            .iter()
            .position(|layer| *layer == OpenGpuiSceneLayer::Node)
            .expect("node layer");
        let edge_interaction = OPEN_GPUI_SCENE_LAYER_ORDER
            .iter()
            .position(|layer| *layer == OpenGpuiSceneLayer::EdgeInteraction)
            .expect("edge interaction layer");
        assert!(edge < node, "retained nodes must paint above graph wires");
        assert!(
            edge_interaction < node,
            "node controls must paint and receive pointer hits above edge focus controls",
        );
        assert_eq!(
            host.stats().mounts(),
            2,
            "fresh frames retain node identity"
        );
    });

    let before = cx.read_entity(&host, |host, _| host.stats());
    cx.update_entity(&host, |_, cx| cx.notify());
    draw_once(cx);
    let after = cx.read_entity(&host, |host, _| host.stats());
    assert_eq!(
        after.edge_route_query_entries_examined(),
        before.edge_route_query_entries_examined(),
        "an unchanged native redraw must reuse the complete visible-route plan",
    );
    assert_eq!(
        after.edge_path_cache_misses(),
        before.edge_path_cache_misses(),
        "an unchanged native redraw must not rebuild route paths",
    );
    assert_eq!(
        after.edge_paint_preparations(),
        before.edge_paint_preparations(),
        "an unchanged native redraw must not regroup the edge batch plan",
    );
    assert_eq!(
        after.edge_paint_item_builds(),
        before.edge_paint_item_builds(),
    );
    assert!(after.edge_paint_plan_reuses() > before.edge_paint_plan_reuses());
    assert_eq!(after.retained_edge_entities(), 0);

    let pointer = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("edge geometry");
        let sample = geometry
            .route(EdgeId::from_u128(30))
            .expect("published edge")
            .facts()
            .path
            .sample(jellyflow::runtime::runtime::geometry::EdgePathAnchor::Center)
            .expect("edge center");
        let position = host
            .view
            .viewport_transform()
            .expect("viewport transform")
            .screen_point_for_canvas(sample.point)
            .expect("project edge center");
        let origin = host.scene_origin.expect("scene origin");
        point(origin.x + px(position.x), origin.y + px(position.y))
    });
    cx.simulate_mouse_move(pointer, None, Default::default());
    cx.run_until_parked();
    let hovered = cx.read_entity(&host, |host, _| host.stats());
    assert_eq!(
        hovered.edge_route_query_entries_examined(),
        after.edge_route_query_entries_examined(),
        "hover must not restart the visible-route query",
    );
    assert_eq!(
        hovered.edge_layer_plans(),
        after.edge_layer_plans(),
        "hover must reuse the immutable prepared route list",
    );
    assert!(hovered.edge_route_list_reuses() > after.edge_route_list_reuses());
    assert_eq!(
        hovered.edge_path_cache_misses() - after.edge_path_cache_misses(),
        1,
        "only the changed edge builds its wider hover tessellation",
    );
    assert!(hovered.edge_paint_plan_reuses() > after.edge_paint_plan_reuses());
    assert_eq!(
        hovered.edge_native_paint_calls() - after.edge_native_paint_calls(),
        2,
        "one base batch and one changed-edge overlay are submitted",
    );
    assert_eq!(hovered.retained_edge_entities(), 0);
}

#[open_gpui::test]
fn same_style_edges_merge_into_fewer_native_paint_submissions(cx: &mut open_gpui::TestAppContext) {
    const EDGE_COUNT: usize = 4;
    let (graph, _) = bounded_edge_graph(EDGE_COUNT, 0);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);

    draw(cx);
    draw(cx);

    let stats = cx.read_entity(&host, |host, _| host.stats());
    assert!(stats.edge_native_paint_calls() > 0);
    assert!(stats.edge_native_edges_painted() >= EDGE_COUNT as u64);
    assert!(
        stats.edge_native_paint_calls() < stats.edge_native_edges_painted(),
        "same-style route meshes must share native paint submissions",
    );
    assert_eq!(stats.retained_edge_entities(), 0);
}

#[open_gpui::test]
fn matching_edge_markers_share_native_submissions_and_reuse_their_plan(
    cx: &mut open_gpui::TestAppContext,
) {
    const EDGE_COUNT: usize = 4;
    let (graph, _) = bounded_edge_graph_with(EDGE_COUNT, 0, |output, input| {
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.view.target_marker_key = Some("arrow".to_owned());
        edge
    });
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);

    draw(cx);
    draw(cx);

    let before = cx.read_entity(&host, |host, _| host.stats());
    assert!(before.edge_native_markers_painted() >= EDGE_COUNT as u64);
    assert!(
        before.edge_native_marker_paint_calls() < before.edge_native_markers_painted(),
        "equal marker colors must share one native path submission",
    );

    cx.update_entity(&host, |_, cx| cx.notify());
    draw_once(cx);
    let after = cx.read_entity(&host, |host, _| host.stats());
    assert_eq!(
        after.edge_marker_preparations(),
        before.edge_marker_preparations(),
        "an unchanged frame must not regroup marker paths",
    );
    assert!(after.edge_marker_plan_reuses() > before.edge_marker_plan_reuses());
    assert_eq!(after.retained_edge_entities(), 0);
}

#[open_gpui::test]
fn alternating_edge_styles_merge_globally_within_their_visual_layer(
    cx: &mut open_gpui::TestAppContext,
) {
    const EDGE_COUNT: usize = 6;
    let mut index = 0usize;
    let (graph, _) = bounded_edge_graph_with(EDGE_COUNT, 0, |output, input| {
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.view.style_token = Some(if index.is_multiple_of(2) {
            "data.primary".to_owned()
        } else {
            "warning".to_owned()
        });
        edge.view.target_marker_key = Some("arrow".to_owned());
        index = index.saturating_add(1);
        edge
    });
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);

    draw(cx);
    draw(cx);
    let before = cx.read_entity(&host, |host, _| host.stats());
    cx.update_entity(&host, |_, cx| cx.notify());
    draw_once(cx);
    let after = cx.read_entity(&host, |host, _| host.stats());

    let painted_edges = after.edge_native_edges_painted() - before.edge_native_edges_painted();
    let path_passes = painted_edges / EDGE_COUNT as u64;
    assert_eq!(painted_edges % EDGE_COUNT as u64, 0);
    assert!(path_passes > 0);
    assert_eq!(
        after.edge_native_paint_calls() - before.edge_native_paint_calls(),
        path_passes * 2,
        "each paint pass must globally merge alternating edges into two style batches",
    );
    let painted_markers =
        after.edge_native_markers_painted() - before.edge_native_markers_painted();
    let marker_passes = painted_markers / EDGE_COUNT as u64;
    assert_eq!(painted_markers % EDGE_COUNT as u64, 0);
    assert!(marker_passes > 0);
    assert_eq!(
        after.edge_native_marker_paint_calls() - before.edge_native_marker_paint_calls(),
        marker_passes * 2,
        "each marker pass must use the same two complete visual-style keys",
    );
    assert_eq!(after.retained_edge_entities(), 0);
}

#[open_gpui::test]
fn edge_chrome_paints_hovered_labels_between_ordinary_and_selected_chrome(
    cx: &mut open_gpui::TestAppContext,
) {
    let (graph, edges) = bounded_edge_graph_with(3, 0, |output, input| {
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.view.label = Some("payload".to_owned());
        edge.view.target_marker_key = Some("arrow".to_owned());
        edge
    });
    let ordinary = edges[0];
    let hovered = edges[1];
    let selected = edges[2];
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);

    draw(cx);
    draw(cx);
    cx.update_entity(&host, |host, _| {
        host.view
            .set_selection(Vec::new(), vec![selected], Vec::new())
            .expect("select emphasized edge");
    });
    cx.run_until_parked();
    draw(cx);
    let before_hover = cx.read_entity(&host, |host, _| host.stats());

    let pointer = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("edge geometry");
        let sample = geometry
            .route(hovered)
            .expect("hover route")
            .facts()
            .path
            .sample(jellyflow::runtime::runtime::geometry::EdgePathAnchor::Center)
            .expect("hover route center");
        let position = host
            .view
            .viewport_transform()
            .expect("viewport transform")
            .screen_point_for_canvas(sample.point)
            .expect("project hover route");
        let origin = host.scene_origin.expect("scene origin");
        point(origin.x + px(position.x), origin.y + px(position.y))
    });
    cx.simulate_mouse_move(pointer, None, Default::default());
    cx.run_until_parked();
    draw_once(cx);
    let after_hover = cx.read_entity(&host, |host, _| host.stats());
    assert_eq!(
        after_hover.edge_marker_preparations() - before_hover.edge_marker_preparations(),
        1,
        "hover builds one marker overlay instead of regrouping the published marker list",
    );
    assert!(
        after_hover.edge_marker_plan_reuses() >= before_hover.edge_marker_plan_reuses() + 2,
        "ordinary and emphasized marker plans remain reusable across hover",
    );

    let paint_order = cx.update_entity(&host, |host, _| {
        let wires = host.scene_wires().expect("layered edge chrome plan");
        let mut order = Vec::new();
        visit_edge_chrome_wires_in_paint_order(&wires, |wire| {
            order.push((wire.edge, wire.visual_state));
        });
        order
    });
    assert_eq!(
        paint_order,
        vec![
            (ordinary, OpenGpuiEdgeVisualState::Ordinary),
            (hovered, OpenGpuiEdgeVisualState::Hovered),
            (selected, OpenGpuiEdgeVisualState::Selected),
        ],
        "paths, endpoint markers, and labels must share one explicit visual-layer order",
    );
}

#[test]
fn edge_layer_budget_is_clamped_to_the_defensive_native_limit() {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty registry"),
        viewport(),
    )
    .with_edge_layer_budget(usize::MAX);

    assert_eq!(
        host.edge_layer_budget(),
        super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET,
    );
}

#[test]
fn endpoint_geometry_fallbacks_are_deduplicated_in_host_diagnostics_and_stats() {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty registry"),
        viewport(),
    );

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 3 endpoint fallback selected PublishedPort".to_owned()],
        1,
    );
    assert_eq!(host.stats().endpoint_geometry_fallbacks(), 1);
    assert_eq!(
        host.last_diagnostic().map(|diagnostic| diagnostic.kind()),
        Some(super::OpenGpuiSceneHostDiagnosticKind::EndpointGeometry)
    );

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 3 endpoint fallback selected PublishedPort".to_owned()],
        1,
    );
    assert_eq!(
        host.stats().endpoint_geometry_fallbacks(),
        1,
        "the same retained fallback must not inflate telemetry every frame",
    );

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 4 endpoint fallback selected ExplicitFallback".to_owned()],
        2,
    );
    assert_eq!(host.stats().endpoint_geometry_fallbacks(), 3);
    assert!(
        host.last_diagnostic()
            .is_some_and(|diagnostic| diagnostic.message().contains("additional endpoint"))
    );
}

#[test]
fn committed_wire_endpoints_match_the_exact_same_revision_port_publication() {
    let (graph, edges) = bounded_edge_graph(1, 0);
    let edge = edges[0];
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    publish_all_edge_geometry(&view, std::iter::empty());
    let graph = view.graph_snapshot().expect("graph snapshot");
    let graph_edge = graph.graph().edge(&edge).expect("published edge");
    let source_port = graph_edge.from;
    let target_port = graph_edge.to;
    let geometry = view.geometry_snapshot().expect("geometry snapshot");
    let layout_revision = geometry.layout_revision();
    let source_geometry = geometry.port(source_port).expect("source geometry");
    let target_geometry = geometry.port(target_port).expect("target geometry");
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty registry"),
        viewport(),
    );

    let wires = host.scene_wires().expect("published wire plan");
    let wire = wires.published.first().expect("one published wire");

    assert_eq!(wire.edge, edge);
    assert_eq!(wire.source_endpoint.point(), source_geometry.route_anchor());
    assert_eq!(wire.source_endpoint.outward(), source_geometry.outward());
    assert_eq!(wire.target_endpoint.point(), target_geometry.route_anchor());
    assert_eq!(wire.target_endpoint.outward(), target_geometry.outward());
    assert_eq!(
        wire.source_endpoint.source(),
        super::endpoint_resolver::PublishedEndpointSource::PublishedRoute
    );
    assert_eq!(
        wire.target_endpoint.source(),
        super::endpoint_resolver::PublishedEndpointSource::PublishedRoute
    );
    assert_eq!(
        wire.source_endpoint.identity().layout_revision(),
        layout_revision
    );
    assert_eq!(wire.source_endpoint.identity().port(), source_port);
    assert_eq!(wire.target_endpoint.identity().port(), target_port);
    assert_eq!(wire.source_endpoint.diagnostic(), None);
    assert_eq!(wire.target_endpoint.diagnostic(), None);
    assert_eq!(host.stats().endpoint_geometry_fallbacks(), 0);
}

#[open_gpui::test]
fn edge_layer_stays_empty_without_fresh_endpoint_geometry(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, |_binding, _window, cx| cx.new(|_| RootOnlyNode));

    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    draw(cx);

    assert!(
        weak_view
            .geometry_snapshot()
            .expect("published geometry")
            .routes()
            .is_empty(),
    );
    cx.read_entity(&host, |host, _| {
        let evidence = &host.scene_evidence;
        assert_eq!(evidence.last_planned_wire_count.get(), 0);
        assert_eq!(evidence.last_painted_wire_count.get(), 0);
        assert_eq!(evidence.first_non_empty_paint_pass.get(), None);
    });
}

#[test]
fn edge_layer_culls_offscreen_routes_and_prioritizes_selection_within_its_own_budget() {
    const VISIBLE_EDGES: usize = 4;
    const OFFSCREEN_EDGES: usize = 12;
    const EDGE_BUDGET: usize = 2;

    let (graph, visible_edges) = bounded_edge_graph(VISIBLE_EDGES, OFFSCREEN_EDGES);
    let selected = *visible_edges.last().expect("visible selected edge");
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid bounded edge graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach bounded edge view");
    publish_all_edge_geometry(&view, [selected]);

    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        viewport(),
    )
    .with_edge_layer_budget(EDGE_BUDGET);
    let wires = host.scene_wires().expect("bounded edge plan");

    assert_eq!(
        wires
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
        vec![visible_edges[2], selected],
        "the topmost ordinary route is admitted before the selected route paints above it",
    );
    assert_eq!(
        wires.published[0].visual_state,
        OpenGpuiEdgeVisualState::Ordinary
    );
    assert_eq!(
        wires.published[1].visual_state,
        OpenGpuiEdgeVisualState::Selected
    );

    let overflow = host.last_edge_layer_overflow();
    assert!(!overflow.query_pending());
    assert!(!overflow.query_budget_exhausted());
    assert!(!overflow.reused_previous_plan());
    assert_eq!(overflow.query_candidates_matched(), VISIBLE_EDGES as u64);
    assert_eq!(overflow.budget_omitted(), VISIBLE_EDGES - EDGE_BUDGET);
    assert!(!overflow.is_empty());

    let stats = host.stats();
    assert_eq!(stats.edge_layer_plans(), 1);
    assert_eq!(
        stats.edge_route_query_candidates_matched(),
        VISIBLE_EDGES as u64
    );
    assert_eq!(stats.edge_route_query_pending(), 0);
    assert_eq!(stats.edge_routes_planned(), EDGE_BUDGET as u64);
    assert_eq!(
        stats.edge_routes_budget_omitted(),
        (VISIBLE_EDGES - EDGE_BUDGET) as u64,
    );
    assert_eq!(stats.edge_layer_overflows(), 1);

    let candidates_before_reuse = stats.edge_route_query_candidates_matched();
    let entries_before_reuse = stats.edge_route_query_entries_examined();
    let cached = host
        .scene_wires()
        .expect("unchanged complete route plan is reusable");
    assert_eq!(
        cached
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
        vec![visible_edges[2], selected],
    );
    let overflow = host.last_edge_layer_overflow();
    assert!(overflow.reused_previous_plan());
    assert_eq!(overflow.query_entries_examined(), 0);
    assert_eq!(overflow.query_candidates_matched(), 0);
    let reused_stats = host.stats();
    assert_eq!(
        reused_stats.edge_route_query_candidates_matched(),
        candidates_before_reuse,
    );
    assert_eq!(
        reused_stats.edge_route_query_entries_examined(),
        entries_before_reuse,
    );
    assert_eq!(reused_stats.edge_route_previous_plan_reuses(), 1);

    let reselected = visible_edges[0];
    host.view
        .set_selection(Vec::new(), vec![reselected], Vec::new())
        .expect("change paint order before a bounded refresh");
    host.visible_route_query_budget = 1;
    let resumed = host
        .scene_wires()
        .expect("incomplete query retains the complete paint plan");
    assert_eq!(resumed.published.len(), EDGE_BUDGET);
    assert_eq!(
        resumed.published.last().map(|wire| wire.edge),
        Some(reselected),
        "the current selection remains emphasized while the spatial refresh is incomplete",
    );
    let overflow = host.last_edge_layer_overflow();
    assert!(overflow.query_pending());
    assert!(overflow.reused_previous_plan());
    assert!(overflow.query_entries_examined() <= 1);
}

#[test]
fn ten_thousand_routes_keep_each_visible_query_page_hard_bounded() {
    const EDGE_COUNT: usize = 10_000;
    const ROUTE_SCAN_LIMIT: usize = 64;
    const EDGE_BUDGET: usize = 4;

    let (graph, edges) = dense_edge_graph(EDGE_COUNT);
    let selected = edges.iter().rev().copied().collect::<Vec<_>>();
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid dense edge graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach dense edge view");
    publish_all_edge_geometry(&view, selected.iter().copied());
    let host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        viewport(),
    );
    assert_eq!(
        host.visible_route_query_budget(),
        super::OPEN_GPUI_DEFAULT_VISIBLE_ROUTE_QUERY_BUDGET,
    );
    let mut host = host
        .with_edge_layer_budget(EDGE_BUDGET)
        .with_visible_route_query_budget(ROUTE_SCAN_LIMIT);

    let mut pending_pages = 0_u64;
    loop {
        let wires = host.scene_wires().expect("bounded dense edge plan");
        assert!(wires.published.is_empty());
        let overflow = host.last_edge_layer_overflow();
        assert!(overflow.query_entries_examined() <= ROUTE_SCAN_LIMIT as u64);
        assert!(overflow.query_candidates_matched() <= ROUTE_SCAN_LIMIT as u64);
        assert!(!overflow.reused_previous_plan());
        if overflow.query_pending() {
            pending_pages = pending_pages.saturating_add(1);
            assert!(
                pending_pages < 300,
                "the bounded query must reach a terminal state"
            );
            continue;
        }
        assert!(overflow.query_budget_exhausted());
        break;
    }

    let stats = host.stats();
    assert_eq!(stats.edge_layer_plans(), 1);
    assert!(stats.edge_route_list_reuses() >= pending_pages);
    assert_eq!(
        stats.edge_route_query_entries_examined(),
        super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET as u64,
    );
    assert_eq!(stats.edge_route_query_pending(), pending_pages);
    assert_eq!(stats.edge_route_query_budget_exhausted(), 1);
    assert_eq!(stats.edge_routes_planned(), 0);

    let terminal = host
        .scene_wires()
        .expect("hard exhaustion is terminal for an unchanged query key");
    assert!(terminal.published.is_empty());
    let overflow = host.last_edge_layer_overflow();
    assert!(overflow.query_budget_exhausted());
    assert_eq!(overflow.query_entries_examined(), 0);
    assert_eq!(overflow.query_candidates_matched(), 0);
    let terminal_stats = host.stats();
    assert_eq!(
        terminal_stats.edge_route_query_entries_examined(),
        stats.edge_route_query_entries_examined(),
        "terminal exhaustion must not restart the 10k scan every frame",
    );
}

#[test]
fn ten_thousand_spatial_routes_materialize_only_the_local_hundred() {
    const GRID_SIDE: usize = 100;
    const SPACING: f32 = 512.0;
    const QUERY_BUDGET: usize = 256;

    let (graph, edges) = spatial_edge_graph(GRID_SIDE, SPACING);
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid spatial edge graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach spatial edge view");
    publish_spatial_edge_geometry(&view, GRID_SIDE, SPACING);
    let query_viewport = CanvasRect {
        origin: CanvasPoint { x: -16.0, y: -16.0 },
        size: CanvasSize {
            width: 9.0 * SPACING + 132.0,
            height: 9.0 * SPACING + 32.0,
        },
    };
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        query_viewport,
    )
    .with_visible_route_query_budget(QUERY_BUDGET);

    let wires = host
        .scene_wires()
        .expect("spatial edge plan completes in one bounded query");
    assert_eq!(edges.len(), GRID_SIDE * GRID_SIDE);
    assert_eq!(wires.published.len(), 100);
    assert_eq!(
        wires
            .published
            .iter()
            .map(|wire| wire.paint_key)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "the local hundred remain one batchable ordinary style",
    );
    let overflow = host.last_edge_layer_overflow();
    assert!(!overflow.query_pending());
    assert!(!overflow.query_budget_exhausted());
    assert!(overflow.query_entries_examined() <= QUERY_BUDGET as u64);
    assert_eq!(overflow.query_candidates_matched(), 100);
    let stats = host.stats();
    assert_eq!(stats.edge_routes_planned(), 100);
    assert_eq!(stats.retained_edge_entities(), 0);
}

#[open_gpui::test]
fn edge_delete_action_commits_one_undoable_disconnect(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    assert_eq!(runtime.history_state().expect("history").undo_len(), 0);
    cx.update(|window, cx| {
        assert!(host.update(cx, |host, cx| { host.delete_edge_action(edge, window, cx) }));
    });
    draw(cx);

    let deleted = runtime.snapshot().expect("deleted graph");
    assert!(deleted.graph().edge(&edge).is_none());
    assert_eq!(runtime.history_state().expect("history").undo_len(), 1);
    runtime
        .execute(GraphCommand::undo(deleted.revision()))
        .expect("undo edge deletion")
        .applied()
        .expect("undo applies");
    assert!(
        runtime
            .snapshot()
            .expect("restored graph")
            .graph()
            .edge(&edge)
            .is_some()
    );
}

#[open_gpui::test]
fn pointer_selects_the_painted_edge_through_the_renderer_neutral_path_picker(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let pointer = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("edge geometry");
        let sample = geometry
            .route(edge)
            .expect("published edge")
            .facts()
            .path
            .sample(jellyflow::runtime::runtime::geometry::EdgePathAnchor::Center)
            .expect("edge center");
        let transform = host.view.viewport_transform().expect("viewport transform");
        let position = transform
            .screen_point_for_canvas(sample.point)
            .expect("projected edge center");
        let origin = host.scene_origin.expect("measured scene origin");
        point(origin.x + px(position.x), origin.y + px(position.y))
    });

    cx.simulate_click(pointer, Default::default());
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .selection()
                .expect("pointer edge selection")
                .edges(),
            [edge]
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn forbidden_edge_delete_and_endpoint_reconnect_have_zero_side_effects(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, restricted_edge_action_graph());
    draw(cx);
    draw(cx);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            assert!(!host.delete_edge_action(edge, window, cx));
            assert!(
                host.edge_reconnect_press_target(edge, EdgeEndpoint::From)
                    .is_none()
            );
            assert!(
                host.edge_reconnect_press_target(edge, EdgeEndpoint::To)
                    .is_some()
            );
            host.interaction_owner.clear_connection();
        });
    });
    draw(cx);

    let snapshot = runtime.snapshot().expect("unchanged graph");
    assert_eq!(snapshot.revision(), 0);
    assert!(snapshot.graph().edge(&edge).is_some());
    assert_eq!(runtime.history_state().expect("history").undo_len(), 0);
}

#[open_gpui::test]
fn explicit_reconnect_handle_keeps_scene_owned_connection_armed_during_native_sync(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    let pointer = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("reconnect endpoint bounds")
        .center();
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let target = host
                .edge_reconnect_press_target(edge, EdgeEndpoint::To)
                .expect("reconnect target");
            let position = host
                .interaction_position(pointer)
                .expect("reconnect interaction position");
            let pressed = host
                .interaction_owner
                .press(target, position)
                .expect("arm reconnect handle");
            host.apply_canvas_interaction_update(pressed, window, cx);
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Armed
            );
            assert!(host.interaction_owner.has_connection());

            let moved = host
                .interaction_owner
                .move_to(
                    host.interaction_position(point(pointer.x + px(24.0), pointer.y))
                        .expect("moved reconnect interaction position"),
                )
                .expect("move reconnect handle");
            host.apply_canvas_interaction_update(moved, window, cx);
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::ConnectionDrag
            );
            assert!(host.interaction_owner.has_connection());
        });
    });
}

#[open_gpui::test]
fn removing_the_authoritative_edge_retires_an_active_reconnect_without_a_blank_frame(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    cx.update_window_entity(&host, |host, window, cx| {
        assert!(host.activate_edge_reconnect_action(edge, EdgeEndpoint::To, window, cx));
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        let preview = host
            .scene_evidence
            .last_connection_preview
            .get()
            .expect("active reconnect preview");
        assert_eq!(preview.suppressed_edge, Some(edge));
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1
        );
    });

    let revision = runtime
        .snapshot()
        .expect("snapshot before disconnect")
        .revision();
    runtime
        .execute(GraphCommand::disconnect(revision, edge))
        .expect("external edge removal")
        .applied()
        .expect("disconnect applies");
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            presentation.terminal(),
            Some(InteractionTerminalReason::OwnerRetired)
        );
        assert!(presentation.connection().is_none());
        assert!(!host.interaction_owner.has_connection());
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            0
        );
    });
}

#[open_gpui::test]
fn binding_presentation_projects_real_node_and_port_focus(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let (root_focus, binding) = cx.read_entity(&host, |host, _| {
        let binding = host
            .mounts
            .active_in_order()
            .next()
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("first mounted node binding");
        (
            host.interaction_owner.focus_handle().expect("canvas focus"),
            binding,
        )
    });
    cx.update(|window, _| window.blur());

    cx.update(|window, cx| {
        window.focus_next(cx);
        assert_eq!(window.focused(cx), Some(root_focus.clone()));
    });

    cx.simulate_keystrokes("tab");
    draw(cx);
    cx.read_entity(&binding, |binding, _| {
        assert!(binding.presentation().node().focused());
        assert!(
            !binding
                .presentation()
                .port(PortId::from_u128(20))
                .expect("output presentation")
                .focused()
        );
    });

    cx.simulate_keystrokes("tab");
    draw(cx);
    cx.read_entity(&binding, |binding, _| {
        assert!(
            binding
                .presentation()
                .port(PortId::from_u128(20))
                .expect("output presentation")
                .focused()
        );
        assert!(
            binding.presentation().node().focused(),
            "a focused descendant keeps its node focus state observable"
        );
    });
}

#[open_gpui::test]
fn binding_presentation_tracks_real_port_hit_bounds_hover(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let binding = cx.read_entity(&host, |host, _| {
        host.mounts
            .active_in_order()
            .find(|(instance, _)| instance.node() == NodeId::from_u128(10))
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("source node binding")
    });
    let marker = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output marker");

    cx.simulate_mouse_move(marker.center(), None, Default::default());
    draw(cx);
    cx.read_entity(&binding, |binding, _| {
        assert!(binding.presentation().node().hovered());
        assert!(
            binding
                .presentation()
                .port(PortId::from_u128(20))
                .expect("source output presentation")
                .hovered()
        );
    });

    cx.simulate_mouse_move(point(px(700.0), px(500.0)), None, Default::default());
    draw(cx);
    cx.read_entity(&binding, |binding, _| {
        assert!(!binding.presentation().node().hovered());
        assert!(
            !binding
                .presentation()
                .port(PortId::from_u128(20))
                .expect("source output presentation")
                .hovered()
        );
    });
}

#[open_gpui::test]
fn measured_port_wrapper_tracks_the_same_hover_presentation(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let binding = cx.read_entity(&host, |host, _| {
        host.mounts
            .active_in_order()
            .find(|(instance, _)| instance.node() == NodeId::from_u128(10))
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("source node binding")
    });
    let marker = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output marker");

    cx.simulate_mouse_move(marker.center(), None, Default::default());
    draw(cx);

    cx.read_entity(&binding, |binding, _| {
        assert!(
            binding
                .presentation()
                .port(PortId::from_u128(20))
                .expect("measured output presentation")
                .hovered()
        );
    });
}

#[open_gpui::test]
fn focused_edge_delete_uses_native_key_semantics_and_falls_back_in_graph_order(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));

    for _ in 0..4 {
        cx.simulate_keystrokes("tab");
    }
    let previous_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("target port focus before the edge");
    cx.simulate_keystrokes("tab");
    let edge_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("edge focus after nodes and ports");
    assert_ne!(edge_focus, previous_focus);

    simulate_key_up(cx, "enter");
    cx.run_until_parked();
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view.selection().expect("edge selection").edges(),
            [edge]
        );
    });
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(edge_focus)));

    simulate_key_up(cx, "delete");
    cx.run_until_parked();
    draw(cx);

    assert!(
        runtime
            .snapshot()
            .expect("graph after native delete")
            .graph()
            .edge(&edge)
            .is_none()
    );
    assert_eq!(runtime.history_state().expect("history").undo_len(), 1);
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
}

#[open_gpui::test]
fn explicit_node_drag_handle_commits_one_geometry_command_and_canvas_pan_zoom_stay_view_local(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let source_before = runtime
        .snapshot()
        .expect("snapshot")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source")
        .pos;
    let history_before = runtime.history_state().expect("history");
    let drag_start = point(px(100.0), px(100.0));
    let drag_end = point(px(150.0), px(140.0));
    cx.simulate_mouse_down(drag_start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(drag_end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    let source_after = runtime
        .snapshot()
        .expect("moved snapshot")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source after drag")
        .pos;
    assert_ne!(
        source_after, source_before,
        "node chrome must own a drag gesture"
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after drag")
            .undo_len(),
        history_before.undo_len() + 1,
        "one pointer gesture is one graph history boundary",
    );

    let transform_before = cx.read_entity(&host, |host, _| {
        host.scene_transform.expect("scene transform")
    });
    let pan_start = point(px(700.0), px(500.0));
    let pan_end = point(px(740.0), px(530.0));
    cx.simulate_mouse_down(pan_start, MouseButton::Middle, Default::default());
    cx.simulate_mouse_move(pan_end, Some(MouseButton::Middle), Default::default());
    cx.simulate_mouse_up(pan_end, MouseButton::Middle, Default::default());
    cx.run_until_parked();
    draw(cx);
    let transform_after_pan = cx.read_entity(&host, |host, _| {
        host.scene_transform.expect("panned transform")
    });
    assert_ne!(transform_after_pan.pan, transform_before.pan);

    cx.simulate_event(ScrollWheelEvent {
        position: point(px(420.0), px(280.0)),
        delta: ScrollDelta::Lines(point(0.0, 1.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    draw(cx);
    let transform_after_zoom = cx.read_entity(&host, |host, _| {
        host.scene_transform.expect("zoomed transform")
    });
    assert_ne!(transform_after_zoom.zoom, transform_after_pan.zoom);

    cx.read_entity(&host, |host, _| {
        assert!(!host.active_scene_gesture());
    });
}

#[open_gpui::test]
fn host_overview_drag_surface_moves_node_while_public_native_handle_is_inert(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    view.set_viewport(CanvasPoint::default(), 0.5)
        .expect("overview zoom");
    let binding_capture = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured_binding = binding_capture.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        if binding
            .read(cx)
            .authoring()
            .expect("live node authoring")
            .node_id()
            == NodeId::from_u128(10)
        {
            *captured_binding.borrow_mut() = Some(binding.clone());
        }
        cx.new(|_| RoutedNode { binding })
    });
    let (_scene, cx) = cx.add_window_view(move |_, cx| InteractiveScene {
        host: cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        }),
    });
    draw(cx);
    draw(cx);

    let binding = binding_capture
        .borrow()
        .clone()
        .expect("captured source binding");
    cx.read_entity(&binding, |binding, _| {
        assert!(!binding.drag_handle().interaction_enabled());
    });
    let before = runtime
        .snapshot()
        .expect("snapshot before overview drag")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source before overview drag")
        .pos;
    let start = point(px(60.0), px(60.0));
    let end = point(px(90.0), px(80.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let after = runtime
        .snapshot()
        .expect("snapshot after overview drag")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source after overview drag")
        .pos;
    assert_ne!(after, before);
}

#[open_gpui::test]
fn explicit_drag_handle_snaps_against_active_nodes_and_paints_alignment_guides(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    draw(cx);

    let start = point(px(100.0), px(100.0));
    let near_alignment = point(px(236.0), px(100.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(near_alignment, Some(MouseButton::Left), Default::default());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_node_drag_delta(NodeId::from_u128(10)),
            Some(CanvasPoint { x: 140.0, y: 0.0 }),
        );
    });
    assert!(
        cx.debug_bounds(super::scene_render::OPEN_GPUI_ALIGNMENT_GUIDE_SELECTOR)
            .is_some(),
        "a snapped drag must expose native guide chrome",
    );

    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(near_alignment, MouseButton::Left, Default::default());
}

#[open_gpui::test]
fn pending_native_cancellation_cannot_strand_the_semantic_gesture(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    let position = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(position, MouseButton::Left, Default::default());
    let first_generation = cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        presentation.generation().expect("G1 semantic generation")
    });

    let (native, listener) = cx.update_entity(&host, |host, cx| {
        (
            host.interaction_owner.native_capability(),
            OpenGpuiSceneHost::host_scene_listener(cx),
        )
    });
    let mouse_down = MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    };

    cx.update(|window, cx| {
        assert!(native.cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::Escape,));
        assert!(native.begin(
            OpenGpuiSceneGestureKind::Selection,
            MouseButton::Left,
            listener.clone(),
            None,
            &mouse_down,
            window,
            cx,
        ));
    });
    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        assert_eq!(presentation.generation(), Some(first_generation));
    });

    cx.update(|window, cx| assert!(native.drain_pending(window, cx)));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
            "draining G1 must deliver its semantic terminal callback",
        );
    });

    cx.update(|window, cx| {
        assert!(native.begin(
            OpenGpuiSceneGestureKind::Selection,
            MouseButton::Left,
            listener,
            None,
            &mouse_down,
            window,
            cx,
        ));
    });
    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        assert_ne!(presentation.generation(), Some(first_generation));
    });

    cx.update(|window, cx| {
        assert!(native.handle_mouse_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            },
            window,
            cx,
        ));
    });
}

#[open_gpui::test]
fn routed_node_drag_keeps_release_position_and_published_route_until_commit(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output_before = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output before drag");
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .expect("initial geometry")
            .routes()
            .len(),
        1,
    );
    let initial_route = weak_view
        .geometry_snapshot()
        .expect("initial route geometry")
        .route(EdgeId::from_u128(30))
        .expect("initial route")
        .facts()
        .path
        .clone();
    let (initial_source, initial_target) =
        edge_path_endpoints(&initial_route).expect("initial route endpoints");
    let position_before = runtime
        .snapshot()
        .expect("graph before routed drag")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source before routed drag")
        .pos;
    let history_before = runtime.history_state().expect("history before routed drag");

    let drag_start = point(px(100.0), px(100.0));
    let drag_end = point(px(150.0), px(140.0));
    cx.simulate_mouse_down(drag_start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(drag_end, Some(MouseButton::Left), Default::default());
    let dragged_path = cx.update_entity(&host, |host, _| {
        host.scene_wires()
            .expect("dragged scene wires")
            .published
            .iter()
            .find(|wire| wire.edge == EdgeId::from_u128(30))
            .and_then(|wire| wire.drag_path.clone())
            .expect("incident edge has an optimistic drag path")
    });
    let (dragged_source, dragged_target) =
        edge_path_endpoints(&dragged_path).expect("dragged route endpoints");
    assert_eq!(
        dragged_source,
        CanvasPoint {
            x: initial_source.x + 50.0,
            y: initial_source.y + 40.0,
        },
    );
    assert_eq!(dragged_target, initial_target);
    draw(cx);
    let output_during_drag = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output during drag");
    assert_eq!(
        output_during_drag.origin.x,
        output_before.origin.x + px(50.0)
    );
    assert_eq!(
        output_during_drag.origin.y,
        output_before.origin.y + px(40.0)
    );
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .expect("geometry during optimistic drag")
            .route(EdgeId::from_u128(30))
            .expect("published route stays available")
            .facts()
            .path,
        initial_route,
        "optimistic drag measurements must not replace authoritative geometry",
    );

    cx.read_entity(&host, |host, _| {
        host.scene_evidence.painted_wire_counts.borrow_mut().clear();
    });

    let prefix_hold = cx.read_entity(&host, |host, _| host.commands.hold_prefix_resolutions());
    cx.simulate_mouse_up(drag_end, MouseButton::Left, Default::default());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_node_drag_delta(NodeId::from_u128(10)),
            Some(CanvasPoint { x: 50.0, y: 40.0 }),
            "the optimistic drag delta must remain authoritative while its prefix is held",
        );
        assert!(host.has_pending_interaction_prefix_task());
    });

    for frame in 0..3 {
        draw_once(cx);
        assert_eq!(
            cx.debug_bounds("jellyflow-test-output-port")
                .expect("source output during held prefix"),
            output_during_drag,
            "frame {frame}: the node must not flash back while the prefix is pending",
        );
        cx.update_entity(&host, |host, _| {
            assert_eq!(
                host.interaction_node_drag_delta(NodeId::from_u128(10)),
                Some(CanvasPoint { x: 50.0, y: 40.0 }),
                "frame {frame}: the optimistic delta must remain continuous",
            );
            let wires = host.scene_wires().expect("held-prefix scene wires");
            assert_eq!(wires.published.len(), 1, "frame {frame}: one routed edge");
            assert_eq!(
                wires.published[0].drag_path.as_ref(),
                Some(&dragged_path),
                "frame {frame}: the incident route must keep the same optimistic path",
            );
        });
        assert_eq!(
            runtime
                .snapshot()
                .expect("graph while routed drag prefix is held")
                .graph()
                .node(&NodeId::from_u128(10))
                .expect("source while prefix is held")
                .pos,
            position_before,
            "frame {frame}: a held prefix cannot mutate the authoritative graph",
        );
        assert_eq!(
            runtime
                .history_state()
                .expect("history while routed drag prefix is held"),
            history_before,
            "frame {frame}: no history boundary exists before prefix release",
        );
        assert_eq!(
            weak_view
                .geometry_snapshot()
                .expect("geometry while prefix is held")
                .route(EdgeId::from_u128(30))
                .expect("route while prefix is held")
                .facts()
                .path,
            initial_route,
            "frame {frame}: published geometry remains authoritative",
        );
    }

    prefix_hold.release();

    cx.run_until_parked();
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(
            !host.scene_evidence.painted_wire_counts.borrow().is_empty(),
            "release-to-commit must paint at least one frame",
        );
        assert!(
            host.scene_evidence
                .painted_wire_counts
                .borrow()
                .iter()
                .all(|count| *count == 1),
            "the routed edge must remain present in every release-to-commit paint",
        );
    });
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .expect("geometry after drag command")
            .routes()
            .len(),
        1,
        "a command frame must not withdraw the route using pre-command measurements",
    );
    let final_position = runtime
        .snapshot()
        .expect("graph after routed drag")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source after routed drag")
        .pos;
    assert_eq!(
        final_position,
        CanvasPoint {
            x: position_before.x + 50.0,
            y: position_before.y + 40.0,
        },
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after routed drag")
            .undo_len(),
        history_before.undo_len() + 1,
        "one routed drag must create exactly one history boundary",
    );
    assert_eq!(
        cx.debug_bounds("jellyflow-test-output-port")
            .expect("source output after drag"),
        output_during_drag,
    );
}

#[open_gpui::test]
fn late_scheduled_tick_after_close_cannot_revive_or_mutate_a_node_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    let graph_before = runtime.snapshot().expect("graph before late tick");
    let history_before = runtime.history_state().expect("history before late tick");
    cx.simulate_mouse_down(
        point(px(100.0), px(100.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(150.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        assert!(host.interaction_owner.test_tick_scheduled());
        assert_eq!(host.interaction_owner.test_tick_callback_attempts(), 0);
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("close before the scheduled tick enters GPUI");
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Closed,
            );
            assert!(host.interaction_owner.test_pending_work_empty());
            assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1);
            assert!(!host.interaction_owner.has_focus_state());
        });
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph immediately after close")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history immediately after close"),
        history_before,
    );

    let frame_delivered = cx.simulate_frame(open_gpui::RequestFrameOptions::default());
    assert!(
        frame_delivered,
        "the test platform must deliver the requested interaction frame",
    );

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_owner.test_tick_callback_attempts(),
            1,
            "the already scheduled window callback must enter exactly once after close",
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed,
        );
        assert!(host.interaction_owner.test_pending_work_empty());
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1);
        assert!(!host.interaction_owner.has_focus_state());
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after late tick")));
    assert_eq!(
        runtime.history_state().expect("history after late tick"),
        history_before,
    );
}

#[open_gpui::test]
fn terminal_finish_does_not_release_successor_pointer_or_focus(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    cx.simulate_mouse_down(
        point(px(100.0), px(100.0)),
        MouseButton::Left,
        Default::default(),
    );
    let jellyflow_capture = cx
        .update(|window, _| window.captured_hitbox())
        .expect("Jellyflow owns the active gesture capture");

    let successor_capture = HitboxId::placeholder();
    assert_ne!(successor_capture, jellyflow_capture);
    let successor_focus = cx.update(|window, cx| {
        let focus = cx.focus_handle();
        host.update(cx, |host, cx| {
            host.enter_terminal(ViewCloseReason::HostClosed);
            assert_eq!(host.interaction_owner.terminal_window_transition_count(), 0);

            window.capture_pointer(successor_capture);
            window.focus(&focus, cx);
            assert!(
                host.interaction_owner
                    .finish_terminal_window_transition(window, cx)
            );
        });
        assert_eq!(window.captured_hitbox(), Some(successor_capture));
        assert_eq!(window.focused(cx), Some(focus.clone()));
        focus
    });

    cx.update(|window, cx| {
        assert_eq!(window.captured_hitbox(), Some(successor_capture));
        assert_eq!(window.focused(cx), Some(successor_focus.clone()));
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        assert_eq!(window.captured_hitbox(), Some(successor_capture));
        assert_eq!(window.focused(cx), Some(successor_focus));
        window.release_pointer();
        window.blur();
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1);
        assert!(!host.interaction_owner.has_focus_state());
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
    });
}

#[open_gpui::test]
fn port_redraw_cancels_instead_of_reclaiming_replacement_pointer_capture(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_interactive_gesture_scene(cx);
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output port bounds");
    let graph_before = runtime
        .snapshot()
        .expect("graph before capture replacement");
    let history_before = runtime
        .history_state()
        .expect("history before capture replacement");

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    let jellyflow_capture = cx
        .update(|window, _| window.captured_hitbox())
        .expect("port drag owns pointer capture");
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed,
        );
    });

    let successor = HitboxId::placeholder();
    assert_ne!(successor, jellyflow_capture);
    cx.update(|window, _| window.capture_pointer(successor));
    draw_once(cx);
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(successor),
        "port rebind must not reclaim another component's capture",
    );
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
        assert!(host.active_port_drag().is_none());
    });
    assert!(
        graph_before.ptr_eq(&runtime.snapshot().expect("graph after capture loss")),
        "capture loss must not commit a connection",
    );
    assert_eq!(
        runtime.history_state().expect("history after capture loss"),
        history_before,
    );
    cx.update(|window, _| window.release_pointer());
}

#[open_gpui::test]
fn scene_redraw_cancels_instead_of_reclaiming_replacement_pointer_capture(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let graph_before = runtime
        .snapshot()
        .expect("graph before capture replacement");
    let history_before = runtime
        .history_state()
        .expect("history before capture replacement");

    cx.simulate_mouse_down(
        point(px(100.0), px(100.0)),
        MouseButton::Left,
        Default::default(),
    );
    let jellyflow_capture = cx
        .update(|window, _| window.captured_hitbox())
        .expect("scene gesture owns pointer capture");
    assert_ne!(
        cx.read_entity(&host, |host, _| { host.interaction_presentation().phase() }),
        CanvasInteractionPhase::Idle,
    );

    let successor = HitboxId::placeholder();
    assert_ne!(successor, jellyflow_capture);
    cx.update(|window, _| window.capture_pointer(successor));
    draw_once(cx);
    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(successor),
        "scene rebind must not reclaim another component's capture",
    );
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
    });
    assert!(
        graph_before.ptr_eq(&runtime.snapshot().expect("graph after capture loss")),
        "capture loss must not commit a scene gesture",
    );
    assert_eq!(
        runtime.history_state().expect("history after capture loss"),
        history_before,
    );
    cx.update(|window, _| window.release_pointer());
}

#[open_gpui::test]
fn sixty_fifth_real_node_gesture_cancels_without_side_effects_then_retries_after_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(protected_capacity_scene_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let leases = std::rc::Rc::new(std::cell::RefCell::new(std::collections::BTreeMap::<
        NodeId,
        OpenGpuiActivityLease,
    >::new()));
    let factory_leases = leases.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = binding
            .read(cx)
            .authoring()
            .expect("live node authoring")
            .node_id();
        if node != protected_capacity_scene_node_id() {
            let activity = binding.read(cx).activity().clone();
            let lease = activity.acquire(OpenGpuiActivityKind::Focus).unwrap();
            factory_leases.borrow_mut().insert(node, lease);
        }
        cx.new(|_| WholeDragNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    assert_eq!(leases.borrow().len(), OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);

    let previous_focus = cx.update(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        focus
    });
    let graph_before = runtime
        .snapshot()
        .expect("snapshot before capacity rejection");
    let history_before = runtime
        .history_state()
        .expect("history before capacity rejection");
    let overflows_before = cx.read_entity(&host, |host, _| host.stats().protected_overflows());
    let pointer = point(px(100.0), px(100.0));

    cx.simulate_mouse_down(pointer, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus.clone())));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_scene_phase.get(),
            Some(OpenGpuiSceneGesturePhase::Cancel(
                OpenGpuiSceneGestureCancelReason::ProtectedCapacityExceeded,
            )),
        );
        assert!(!host.active_scene_gesture());
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        assert_eq!(host.stats().protected_overflows(), overflows_before + 1);
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after rejection")));
    assert_eq!(
        runtime.history_state().expect("history after rejection"),
        history_before,
    );

    let released_node = *leases.borrow().keys().next().expect("one protected owner");
    drop(leases.borrow_mut().remove(&released_node));
    cx.simulate_mouse_down(pointer, MouseButton::Left, Default::default());

    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_scene_phase.get(),
            Some(OpenGpuiSceneGesturePhase::Start),
        );
        assert!(host.active_scene_gesture());
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    cx.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_scene_phase.get(),
            Some(OpenGpuiSceneGesturePhase::Release),
        );
        assert!(!host.active_scene_gesture());
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY - 1,
        );
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after retry")));
    assert_eq!(
        runtime.history_state().expect("history after retry"),
        history_before,
    );
}

#[open_gpui::test]
fn explicit_node_drag_handle_and_box_selection_respect_disabled_node_overrides(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, locked_source_graph());
    draw(cx);
    let before = runtime.snapshot().expect("snapshot before drag");
    let source_before = before
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("locked source")
        .pos;
    let history_before = runtime.history_state().expect("history before drag");

    let drag_start = point(px(100.0), px(100.0));
    let drag_end = point(px(170.0), px(150.0));
    cx.simulate_mouse_down(drag_start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(drag_end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert_eq!(
        runtime
            .snapshot()
            .expect("snapshot after locked drag")
            .graph()
            .node(&NodeId::from_u128(10))
            .expect("locked source after drag")
            .pos,
        source_before,
    );
    assert_eq!(
        runtime.history_state().expect("history after locked drag"),
        history_before,
    );

    cx.simulate_mouse_down(
        point(px(10.0), px(50.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(250.0), px(200.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(
        point(px(250.0), px(200.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    let selected = cx.read_entity(&host, |host, _| {
        host.view
            .export_seed()
            .expect("view seed")
            .selection()
            .nodes()
            .to_vec()
    });
    assert!(!selected.contains(&NodeId::from_u128(10)));
}

#[open_gpui::test]
fn explicit_node_drag_handle_additive_modifiers_preserve_selection_and_release_capture(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let graph_before = runtime.snapshot().expect("snapshot before additive clicks");
    let history_before = runtime
        .history_state()
        .expect("history before additive clicks");
    let viewport_before = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("viewport before additive clicks")
    });
    let pointer = point(px(100.0), px(100.0));

    for modifiers in [Modifiers::shift(), Modifiers::secondary_key()] {
        cx.read_entity(&host, |host, _| {
            host.view
                .set_selection(vec![target], Vec::new(), Vec::new())
                .expect("seed additive selection");
        });

        cx.simulate_mouse_down(pointer, MouseButton::Left, modifiers);
        assert!(
            cx.update(|window, _| window.captured_hitbox()).is_some(),
            "node chrome owns pointer capture while the gesture is active",
        );
        cx.read_entity(&host, |host, _| {
            assert!(host.active_scene_gesture());
            assert!(host.active_port_drag().is_none());
            assert_eq!(
                host.view
                    .selection()
                    .expect("selection after node press")
                    .nodes(),
                [target],
            );
        });

        cx.simulate_mouse_up(pointer, MouseButton::Left, modifiers);
        cx.run_until_parked();

        assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
        assert!(
            cx.debug_bounds(super::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR)
                .is_none(),
        );
        cx.read_entity(&host, |host, _| {
            assert!(!host.active_scene_gesture());
            assert!(host.active_port_drag().is_none());
            assert_eq!(
                host.view
                    .selection()
                    .expect("selection after node release")
                    .nodes(),
                [target, source],
            );
        });
    }

    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after additive clicks")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after additive clicks"),
        history_before,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .viewport_transform()
                .expect("viewport after additive clicks"),
            viewport_before,
        );
    });
}

#[open_gpui::test]
fn selected_node_chrome_matches_native_node_bounds(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    let selected = NodeId::from_u128(10);
    cx.read_entity(&host, |host, _| {
        host.view
            .set_selection(vec![selected], Vec::new(), Vec::new())
            .expect("select native node");
    });

    draw(cx);

    let node_bounds = cx
        .debug_bounds(&test_node_selector(selected))
        .expect("selected native node bounds");
    let chrome_bounds = cx
        .debug_bounds(&super::scene_render::open_gpui_node_chrome_selector(
            selected,
        ))
        .expect("selected native node chrome bounds");
    assert_eq!(chrome_bounds, node_bounds);
}

#[open_gpui::test]
fn box_selection_additive_modifiers_preserve_existing_selection_and_release_overlay(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let graph_before = runtime.snapshot().expect("snapshot before additive boxes");
    let history_before = runtime
        .history_state()
        .expect("history before additive boxes");
    let viewport_before = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("viewport before additive boxes")
    });
    let start = point(px(10.0), px(50.0));
    let end = point(px(250.0), px(200.0));

    for modifiers in [Modifiers::shift(), Modifiers::secondary_key()] {
        cx.read_entity(&host, |host, _| {
            host.view
                .set_selection(vec![target], Vec::new(), Vec::new())
                .expect("seed additive box selection");
        });

        cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
        assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), modifiers);
        draw(cx);
        assert!(
            cx.debug_bounds(super::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR)
                .is_some(),
            "box selection exposes its native overlay while captured",
        );
        cx.read_entity(&host, |host, _| {
            assert!(host.active_scene_gesture());
            assert!(host.active_port_drag().is_none());
        });

        cx.simulate_mouse_up(end, MouseButton::Left, modifiers);
        cx.run_until_parked();
        draw(cx);

        assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
        assert!(
            cx.debug_bounds(super::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR)
                .is_none(),
            "box overlay retires with its pointer owner",
        );
        cx.read_entity(&host, |host, _| {
            assert!(!host.active_scene_gesture());
            assert!(host.active_port_drag().is_none());
            assert_eq!(
                host.view
                    .selection()
                    .expect("selection after additive box")
                    .nodes(),
                [target, source],
            );
        });
    }

    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("snapshot after additive boxes")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after additive boxes"),
        history_before,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .viewport_transform()
                .expect("viewport after additive boxes"),
            viewport_before,
        );
    });
}

#[open_gpui::test]
fn node_drag_filters_non_draggable_selected_peers(cx: &mut open_gpui::TestAppContext) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, locked_peer_graph());
    cx.read_entity(&host, |host, _| {
        host.view
            .set_selection(
                vec![NodeId::from_u128(10), NodeId::from_u128(11)],
                Vec::new(),
                Vec::new(),
            )
            .expect("seed selection");
    });
    draw(cx);
    let before = runtime.snapshot().expect("snapshot before grouped drag");
    let source_before = before
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source")
        .pos;
    let peer_before = before
        .graph()
        .node(&NodeId::from_u128(11))
        .expect("locked peer")
        .pos;

    let drag_start = point(px(100.0), px(100.0));
    let drag_end = point(px(150.0), px(140.0));
    cx.simulate_mouse_down(drag_start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(drag_end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let after = runtime.snapshot().expect("snapshot after grouped drag");
    assert_ne!(
        after
            .graph()
            .node(&NodeId::from_u128(10))
            .expect("moved source")
            .pos,
        source_before,
    );
    assert_eq!(
        after
            .graph()
            .node(&NodeId::from_u128(11))
            .expect("locked peer after drag")
            .pos,
        peer_before,
    );
}

#[open_gpui::test]
fn escape_cancels_node_drag_without_mutating_graph(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let before = runtime
        .snapshot()
        .expect("snapshot")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source")
        .pos;
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    cx.simulate_mouse_down(
        point(px(100.0), px(100.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(170.0), px(150.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_mouse_up(
        point(px(170.0), px(150.0)),
        MouseButton::Left,
        Default::default(),
    );

    let after = runtime
        .snapshot()
        .expect("snapshot after cancel")
        .graph()
        .node(&NodeId::from_u128(10))
        .expect("source after cancel")
        .pos;
    assert_eq!(after, before);
    cx.read_entity(&host, |host, _| {
        assert!(!host.active_scene_gesture());
    });
}

#[open_gpui::test]
fn close_in_window_cancels_active_node_drag_without_reentrant_update(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    cx.simulate_mouse_down(
        point(px(100.0), px(100.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(170.0), px(150.0)),
        Some(MouseButton::Left),
        Default::default(),
    );

    close_active_scene_gesture(&host, cx);
}

#[open_gpui::test]
fn close_in_window_cancels_active_box_selection_without_reentrant_update(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    cx.simulate_mouse_down(
        point(px(10.0), px(50.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(250.0), px(200.0)),
        Some(MouseButton::Left),
        Default::default(),
    );

    close_active_scene_gesture(&host, cx);
}

#[open_gpui::test]
fn close_in_window_cancels_active_middle_pan_without_reentrant_update(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    cx.simulate_mouse_down(
        point(px(700.0), px(500.0)),
        MouseButton::Middle,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(740.0), px(530.0)),
        Some(MouseButton::Middle),
        Default::default(),
    );

    close_active_scene_gesture(&host, cx);
}

#[open_gpui::test]
fn coalesced_remove_undo_and_payload_patch_revoke_the_retired_incarnation_without_draw(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(single_node_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let old_activity = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured_activity = old_activity.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        *captured_activity.borrow_mut() = Some(binding.read(cx).activity().clone());
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    let old_activity = old_activity
        .borrow()
        .clone()
        .expect("old incarnation activity");
    let old_instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("old retained instance")
    });
    let signal = super::HostLifecycleSignal::new();

    let removed = runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject),
        ))
        .expect("remove node")
        .applied()
        .expect("remove applies")
        .patch()
        .clone();
    signal.publish(ViewSessionEvent::RuntimePatched { patch: removed });
    let restored = runtime
        .execute(GraphCommand::undo(1))
        .expect("undo remove")
        .applied()
        .expect("undo applies")
        .patch()
        .clone();
    signal.publish(ViewSessionEvent::RuntimePatched { patch: restored });
    let patched = runtime
        .execute(GraphCommand::set_node_data(
            2,
            NodeId::from_u128(10),
            serde_json::json!({ "prompt": "reincarnated" }),
        ))
        .expect("patch reincarnated payload")
        .applied()
        .expect("payload patch applies")
        .patch()
        .clone();
    signal.publish(ViewSessionEvent::RuntimePatched { patch: patched });

    let changes = signal
        .take_node_changes()
        .expect("one coalesced node-change batch");
    let change = changes
        .get(&NodeId::from_u128(10))
        .expect("source node change");
    assert_eq!(change.revision(), 3);
    assert!(change.is_stale_for(&old_instance));
    let retired = host.update(cx, |host, cx| host.apply_node_changes(&changes, cx));
    for mounted in retired {
        mounted.retire();
    }

    assert!(old_activity.is_revoked());
    assert_eq!(old_activity.active_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
        assert_eq!(host.stats().hard_retirements(), 1);
    });
}

#[open_gpui::test]
fn blank_canvas_drag_selects_intersecting_nodes_without_graph_history(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let history_before = runtime.history_state().expect("history");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    cx.simulate_mouse_down(
        point(px(10.0), px(50.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(250.0), px(200.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    draw(cx);
    let selection_rectangle = cx
        .debug_bounds(super::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR)
        .expect("active box selection has native overlay feedback");
    assert!(selection_rectangle.size.width >= px(200.0));
    assert!(selection_rectangle.size.height >= px(100.0));
    cx.simulate_mouse_up(
        point(px(250.0), px(200.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    draw(cx);
    assert!(
        cx.debug_bounds(super::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR)
            .is_none(),
        "selection overlay must retire with the gesture owner"
    );

    let selected = cx.read_entity(&host, |host, _| {
        host.view
            .export_seed()
            .expect("view seed")
            .selection()
            .nodes()
            .to_vec()
    });
    assert_eq!(selected, [NodeId::from_u128(10)]);
    assert_eq!(
        runtime.history_state().expect("history after selection"),
        history_before,
    );
}

#[open_gpui::test]
fn dragging_a_real_output_to_a_real_input_commits_through_the_scene_host(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    let events = std::rc::Rc::new(std::cell::Cell::new(0));
    let factory_events = events.clone();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: factory_events.clone(),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let geometry = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("inspection view")
        .graph_snapshot()
        .expect("graph snapshot");
    assert!(geometry.graph().edges().is_empty());

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    assert!(!output.intersects(&input));
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    assert_eq!(events.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    let snapshot = runtime.snapshot().expect("committed graph");
    assert_eq!(snapshot.graph().edges().len(), 1);
    let edge = snapshot.graph().edges().values().next().expect("new edge");
    assert_eq!(edge.from, PortId::from_u128(20));
    assert_eq!(edge.to, PortId::from_u128(21));
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            0,
        );
    });
}

#[open_gpui::test]
fn output_connection_drag_paints_preview_from_route_anchor_to_canvas_pointer(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let pointer = point(px(700.0), px(500.0));
    let (source, target) = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("published geometry");
        (
            geometry
                .port(PortId::from_u128(20))
                .expect("fresh output geometry")
                .route_anchor(),
            host.interaction_position(pointer)
                .expect("measured interaction point")
                .canvas(),
        )
    });

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(pointer, Some(MouseButton::Left), Default::default());
    cx.read_entity(&host, |host, _| {
        let connection = host
            .interaction_presentation()
            .connection()
            .expect("move updates preview synchronously")
            .clone();
        assert_eq!(connection.pointer().canvas(), target);
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_connection_preview.get(),
            Some(OpenGpuiConnectionPreviewEvidence {
                source,
                target,
                suppressed_edge: None,
            })
        );
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1,
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
    });

    runtime
        .execute(GraphCommand::set_node_position(
            0,
            NodeId::from_u128(10),
            CanvasPoint { x: 120.0, y: 120.0 },
        ))
        .expect("move source from another runtime capability")
        .applied()
        .expect("source move applied");
    draw(cx);
    let moved_source = cx.read_entity(&host, |host, _| {
        host.view
            .geometry_snapshot()
            .expect("translated geometry")
            .port(PortId::from_u128(20))
            .expect("translated source port")
            .route_anchor()
    });
    assert_eq!(
        moved_source,
        CanvasPoint {
            x: source.x + 80.0,
            y: source.y + 40.0,
        }
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_connection_preview.get(),
            Some(OpenGpuiConnectionPreviewEvidence {
                source: moved_source,
                target,
                suppressed_edge: None,
            }),
            "the preview must follow authoritative port geometry without another pointer move",
        );
    });
}

#[open_gpui::test]
fn reconnect_preview_uses_fixed_port_anchor_and_only_suppresses_published_paint(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let runtime = GraphRuntime::builder(catalog())
        .build(occupied_input_graph())
        .expect("valid occupied graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.scene_evidence.last_planned_wire_count.get(), 1);
    });

    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("occupied input bounds");
    let pointer = point(px(700.0), px(500.0));
    let (fixed_anchor, canvas_pointer) = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("published geometry");
        (
            geometry
                .port(PortId::from_u128(20))
                .expect("fresh fixed-port geometry")
                .route_anchor(),
            host.interaction_position(pointer)
                .expect("measured interaction point")
                .canvas(),
        )
    });

    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(pointer, Some(MouseButton::Left), Default::default());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.scene_evidence.last_connection_preview.get(),
            Some(OpenGpuiConnectionPreviewEvidence {
                source: fixed_anchor,
                target: canvas_pointer,
                suppressed_edge: Some(edge),
            })
        );
        assert_eq!(host.scene_evidence.last_planned_wire_count.get(), 0);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1,
        );
    });
    assert!(
        runtime
            .snapshot()
            .expect("unchanged reconnect graph")
            .graph()
            .edge(&edge)
            .is_some(),
        "the authoritative edge remains until reconnect commit",
    );
}

#[open_gpui::test]
fn input_preview_inverts_endpoints_through_offset_transform_and_ignores_edge_budget(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    view.set_viewport(CanvasPoint { x: 12.0, y: -8.0 }, 1.5)
        .expect("set non-identity viewport");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
            .with_edge_layer_budget(0)
        });
        *scene_host.borrow_mut() = Some(host.clone());
        OffsetInteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    let pointer = point(px(700.0), px(500.0));
    let target_anchor = cx.read_entity(&host, |host, _| {
        assert_eq!(host.scene_origin, Some(point(px(37.0), px(29.0))));
        assert_eq!(
            host.scene_transform,
            ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 1.5)
        );
        host.view
            .geometry_snapshot()
            .expect("published geometry")
            .port(PortId::from_u128(21))
            .expect("fresh input geometry")
            .route_anchor()
    });

    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(pointer, Some(MouseButton::Left), Default::default());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        let evidence = host
            .scene_evidence
            .last_connection_preview
            .get()
            .expect("input connection preview");
        assert_canvas_point_close(evidence.source, CanvasPoint { x: 430.0, y: 322.0 });
        assert_eq!(evidence.target, target_anchor);
        assert_eq!(evidence.suppressed_edge, None);
        assert_eq!(host.scene_evidence.last_planned_wire_count.get(), 0);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1,
        );
        assert_eq!(host.last_edge_layer_overflow().budget_omitted(), 0);
    });
}

#[open_gpui::test]
fn dragging_an_unconnected_input_to_a_real_output_commits_in_graph_order(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });
    cx.simulate_mouse_move(output.center(), Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(output.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    let snapshot = runtime.snapshot().expect("committed graph");
    assert_eq!(snapshot.graph().edges().len(), 1);
    let edge = snapshot.graph().edges().values().next().expect("new edge");
    assert_eq!(edge.from, PortId::from_u128(20));
    assert_eq!(edge.to, PortId::from_u128(21));
}

#[open_gpui::test]
fn overlapping_port_drop_targets_the_topmost_painted_node(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(overlapping_target_graph())
        .expect("valid overlapping graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output bounds");
    let topmost = PortId::from_u128(99);
    let target = cx.read_entity(&host, |host, _| {
        let anchor = host
            .view
            .geometry_snapshot()
            .expect("fresh geometry")
            .port(topmost)
            .expect("topmost port geometry")
            .route_anchor();
        let local = host
            .scene_transform
            .expect("scene transform")
            .screen_point_for_canvas(anchor)
            .expect("visible target anchor");
        let origin = host.scene_origin.expect("scene origin");
        point(origin.x + px(local.x), origin.y + px(local.y))
    });

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(target, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let snapshot = runtime.snapshot().expect("committed graph");
    let edge = snapshot
        .graph()
        .edges()
        .values()
        .next()
        .expect("new connection");
    assert_eq!(edge.from, PortId::from_u128(20));
    assert_eq!(edge.to, topmost);
}

#[open_gpui::test]
fn deferred_release_from_a_retired_instance_cannot_target_reused_node_and_port_ids(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    let before = runtime.snapshot().expect("source snapshot");
    let source_id = NodeId::from_u128(10);
    let source = before
        .graph()
        .node(&source_id)
        .expect("source node")
        .clone();
    let ports = source
        .ports
        .iter()
        .map(|port| {
            TopologyPort::new(
                *port,
                before.graph().port(port).expect("source port").clone(),
            )
        })
        .collect::<Vec<_>>();

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_some())
    });
    let removed = runtime
        .execute(GraphCommand::topology(
            before.revision(),
            TopologyEdit::new().remove_node(source_id, IncidentEdgePolicy::Reject),
        ))
        .expect("remove old incarnation")
        .applied()
        .expect("remove applied")
        .revision();
    runtime
        .execute(GraphCommand::topology(
            removed,
            TopologyEdit::new().add_node(source_id, source, ports),
        ))
        .expect("re-add same IDs")
        .applied()
        .expect("re-add applied");

    // Release is deferred by the host listener, while the owner now identifies an old incarnation.
    cx.simulate_mouse_up(input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(
        runtime
            .snapshot()
            .expect("current graph")
            .graph()
            .edges()
            .is_empty(),
        "an event from the retired instance must not connect its reincarnated IDs"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.interaction_presentation().connection().is_none());
    });
}

#[open_gpui::test]
fn windowless_close_rejects_active_pointer_ownership_before_closing_the_view(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_some())
    });

    assert!(matches!(
        cx.update_entity(&host, |host, _| host.close()),
        Err(super::OpenGpuiSceneHostCloseError::WindowCleanupRequired)
    ));
    cx.read_entity(&host, |host, _| assert_eq!(host.close_reason(), None));
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close");
        assert!(window.captured_hitbox().is_none());
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed
        );
        assert!(host.interaction_presentation().connection().is_none());
    });
}

#[open_gpui::test]
fn rendered_idle_host_requires_window_aware_close(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    assert!(matches!(
        cx.update_entity(&host, |host, _| host.close()),
        Err(super::OpenGpuiSceneHostCloseError::WindowCleanupRequired)
    ));
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), None);
        assert!(!host.interaction_owner.terminal());
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("rendered host closes through the window-aware API");
    });
}

#[test]
fn never_rendered_host_closes_windowlessly_without_pending_cleanup() {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        viewport(),
    );

    host.close().expect("unrendered host has no window state");
    assert!(host.interaction_owner.terminal());
    assert!(!host.interaction_owner.test_terminal_cleanup_pending());
    assert!(!host.interaction_owner.requires_window_cleanup());
    assert!(!host.interaction_owner.has_focus_state());
}

#[open_gpui::test]
fn windowless_close_rejects_keyboard_connection_search_before_closing_the_view(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance")
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(instance, PortId::from_u128(20)),
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert!(host.interaction_owner.has_connection());
    });

    assert!(matches!(
        cx.update_entity(&host, |host, _| host.close()),
        Err(super::OpenGpuiSceneHostCloseError::WindowCleanupRequired)
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close");
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed
        );
        assert!(!host.interaction_owner.has_connection());
        assert_eq!(host.stats().protected_instances(), 0);
    });
}

#[open_gpui::test]
fn windowless_close_rejects_node_owned_window_activity(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(single_node_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let leases = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let factory_leases = leases.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        factory_leases.borrow_mut().push(
            binding
                .read(cx)
                .activity()
                .acquire(OpenGpuiActivityKind::Focus)
                .expect("focus activity"),
        );
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_leases(), 1)
    });

    assert!(matches!(
        cx.update_entity(&host, |host, _| host.close()),
        Err(super::OpenGpuiSceneHostCloseError::WindowCleanupRequired)
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close");
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().protected_instances(), 0);
    });
    drop(leases);
}

#[test]
fn windowless_close_rejects_queued_commands_without_executing_or_resolving_them() {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        viewport(),
    );
    let mut ticket = host
        .commands
        .enqueue(
            GraphCommand::set_node_data(
                0,
                NodeId::from_u128(10),
                serde_json::json!({ "source": "queued-close" }),
            ),
            super::OpenGpuiCommandOrigin::Binding,
        )
        .expect("queued binding command");

    assert!(matches!(
        host.close(),
        Err(super::OpenGpuiSceneHostCloseError::WindowCleanupRequired)
    ));
    assert_eq!(host.close_reason(), None);
    assert_eq!(runtime.snapshot().expect("unchanged runtime").revision(), 0);
    assert_command_ticket_pending(&mut ticket);

    drop(host);
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::OpenGpuiCommandTicketError::HostDropped)
    ));
}

#[open_gpui::test]
fn window_close_resolves_queued_ticket_only_after_the_host_borrow_is_released(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let mut ticket = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "window-close" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });
    assert_command_ticket_pending(&mut ticket);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close drains the command");
        assert_command_ticket_pending(&mut ticket);
    });
    assert_eq!(runtime.snapshot().expect("closed runtime").revision(), 1);

    cx.run_until_parked();
    assert_eq!(
        poll_command_ticket(&mut ticket)
            .expect("ticket transport")
            .applied()
            .expect("queued close command applied")
            .revision(),
        1
    );
}

struct PanicOnCloseCommand;

impl GraphRuntimeMiddleware for PanicOnCloseCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        panic!("close command middleware panic");
    }
}

#[open_gpui::test]
fn window_close_middleware_panic_does_not_strand_command_ticket(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph")
        .with_middleware(PanicOnCloseCommand);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                OpenGpuiNodeRegistry::builder()
                    .seal()
                    .expect("valid empty test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    let mut ticket = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "panicking-window-close" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });

    let close =
        cx.update(|window, cx| host.update(cx, |host, cx| host.close_in_window(window, cx)));

    assert!(matches!(
        close,
        Err(super::OpenGpuiSceneHostCloseError::PanicDuringClose)
    ));
    assert_eq!(
        runtime.snapshot().expect("uncommitted runtime").revision(),
        0
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert!(matches!(
            host.commands.begin_close(),
            Err(crate::command_journal::OpenGpuiCommandCloseError::Closed {
                reason: ViewCloseReason::HostClosed
            })
        ));
    });
    cx.run_until_parked();
    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(crate::OpenGpuiCommandTicketError::ClosePanicked {
            reason: ViewCloseReason::HostClosed
        })
    ));
}

#[open_gpui::test]
fn scene_preparation_failure_after_collection_fails_the_batch_and_reopens_capacity(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let mut failed = cx.update_entity(&host, |host, _| {
        let ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "attempt": "failed" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queue command before injected scene failure");
        host.fail_next_scene_wires_for_test(ViewSessionError::Busy);
        ticket
    });

    draw(cx);
    assert!(matches!(
        poll_command_ticket(&mut failed),
        Err(crate::OpenGpuiCommandTicketError::FramePreparation { .. })
    ));
    assert_eq!(runtime.snapshot().expect("unchanged graph").revision(), 0);
    cx.read_entity(&host, |host, _| assert!(!host.commands.has_pending()));

    let mut retry = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "attempt": "retry" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("journal capacity was released")
    });
    draw(cx);
    assert!(
        poll_command_ticket(&mut retry)
            .expect("retry ticket transport")
            .applied()
            .is_some()
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeedbackPanicPoint {
    BeforeMiddleware,
    AfterMiddleware,
    ViewSubscriber,
}

struct PanicOnceDuringFeedback {
    point: FeedbackPanicPoint,
    armed: bool,
}

impl GraphRuntimeMiddleware for PanicOnceDuringFeedback {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        if self.point == FeedbackPanicPoint::BeforeMiddleware && std::mem::take(&mut self.armed) {
            panic!("ordinary feedback before middleware panic");
        }
        Ok(())
    }

    fn after_command(&mut self, _snapshot: GraphRuntimeMiddlewareSnapshot, _patch: &RuntimePatch) {
        if self.point == FeedbackPanicPoint::AfterMiddleware && std::mem::take(&mut self.armed) {
            panic!("ordinary feedback after middleware panic");
        }
    }
}

#[open_gpui::test]
fn ordinary_frame_before_middleware_panic_keeps_the_host_usable(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_ordinary_feedback_panic_isolated(cx, FeedbackPanicPoint::BeforeMiddleware);
}

#[open_gpui::test]
fn ordinary_frame_after_middleware_panic_keeps_the_host_usable(cx: &mut open_gpui::TestAppContext) {
    assert_ordinary_feedback_panic_isolated(cx, FeedbackPanicPoint::AfterMiddleware);
}

#[open_gpui::test]
fn ordinary_frame_view_subscriber_panic_keeps_the_host_usable(cx: &mut open_gpui::TestAppContext) {
    assert_ordinary_feedback_panic_isolated(cx, FeedbackPanicPoint::ViewSubscriber);
}

fn assert_ordinary_feedback_panic_isolated(
    cx: &mut open_gpui::TestAppContext,
    point: FeedbackPanicPoint,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let runtime = match point {
        FeedbackPanicPoint::BeforeMiddleware | FeedbackPanicPoint::AfterMiddleware => {
            runtime.with_middleware(PanicOnceDuringFeedback { point, armed: true })
        }
        FeedbackPanicPoint::ViewSubscriber => runtime,
    };
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    if point == FeedbackPanicPoint::ViewSubscriber {
        let armed = std::rc::Rc::new(std::cell::Cell::new(true));
        view.subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) && armed.replace(false) {
                panic!("ordinary feedback view subscriber panic");
            }
        })
        .expect("subscribe panicking view callback");
    }
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let mut failed = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "attempt": "panicking" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queue command for panicking feedback")
    });
    draw(cx);

    assert!(matches!(
        poll_command_ticket(&mut failed),
        Err(crate::OpenGpuiCommandTicketError::FeedbackOutcomeUnknown)
    ));
    let revision_after_panic = runtime.snapshot().expect("runtime after panic").revision();
    let expected_revision = match point {
        FeedbackPanicPoint::BeforeMiddleware => 0,
        FeedbackPanicPoint::AfterMiddleware | FeedbackPanicPoint::ViewSubscriber => 1,
    };
    assert_eq!(revision_after_panic, expected_revision);
    cx.read_entity(&host, |host, _| {
        assert!(!host.commands.has_pending());
        let diagnostic = host.last_diagnostic().expect("feedback panic diagnostic");
        assert_eq!(
            diagnostic.kind(),
            super::OpenGpuiSceneHostDiagnosticKind::Feedback
        );
        assert!(diagnostic.message().contains("outcome is unknown"));
        assert!(
            diagnostic
                .message()
                .contains("may already have been committed")
        );
    });

    let mut retry = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    revision_after_panic,
                    NodeId::from_u128(10),
                    serde_json::json!({ "attempt": "retry" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("host accepts a command after feedback panic")
    });
    draw(cx);
    assert_eq!(
        poll_command_ticket(&mut retry)
            .expect("retry ticket transport")
            .applied()
            .expect("retry command applied")
            .revision(),
        revision_after_panic + 1
    );

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close after feedback panic");
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
    });
}

#[test]
fn view_handle_controls_the_exact_session_without_extending_host_lifetime() {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal()
            .expect("valid empty test registry"),
        viewport(),
    );
    let handle = host.view_handle();

    handle
        .set_viewport(CanvasPoint { x: 12.0, y: -8.0 }, 1.5)
        .expect("control the hosted view");
    assert_eq!(
        host.view
            .viewport_transform()
            .expect("same hosted session")
            .pan,
        CanvasPoint { x: 12.0, y: -8.0 }
    );

    drop(host);
    assert!(
        !handle.is_open(),
        "weak handle must not keep the view alive"
    );
}

#[open_gpui::test]
fn dropping_a_real_wire_on_empty_space_requests_search_without_mutating_the_graph(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    let events = std::rc::Rc::new(std::cell::Cell::new(0));
    let factory_events = events.clone();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: factory_events.clone(),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let empty = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    assert_eq!(events.get(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });
    cx.simulate_mouse_up(empty, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(runtime.snapshot().unwrap().graph().edges().is_empty());
    cx.read_entity(&host, |host, _| {
        let request = host.connection_search_request().expect("search request");
        assert_eq!(request.source(), PortId::from_u128(20));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });
}

#[open_gpui::test]
fn stale_native_port_generation_cannot_settle_a_reused_mount_and_owner(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let owner = cx.read_entity(&host, |host, _| {
        let instance = host
            .mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance");
        port_drag_owner(instance, PortId::from_u128(20))
    });
    let dispatch = |event: OpenGpuiPortDragEvent, cx: &mut open_gpui::VisualTestContext| {
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.handle_port_event(OpenGpuiPortEvent::Pointer(event), window, cx);
            });
        });
    };

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            owner.clone(),
            1,
            OpenGpuiPortDragPhase::Start,
            output.center(),
        ),
        cx,
    );
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        CanvasInteractionPhase::Armed,
    );
    dispatch(
        OpenGpuiPortDragEvent::for_test(
            owner.clone(),
            1,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
            output.center(),
        ),
        cx,
    );
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        CanvasInteractionPhase::Idle,
    );

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            owner.clone(),
            2,
            OpenGpuiPortDragPhase::Start,
            output.center(),
        ),
        cx,
    );
    let second_generation = cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        presentation.generation().expect("G2 semantic generation")
    });

    for phase in [
        OpenGpuiPortDragPhase::Move,
        OpenGpuiPortDragPhase::Release,
        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
    ] {
        dispatch(
            OpenGpuiPortDragEvent::for_test(owner.clone(), 1, phase, point(px(700.0), px(500.0))),
            cx,
        );
        cx.read_entity(&host, |host, _| {
            let presentation = host.interaction_presentation();
            assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
            assert_eq!(presentation.generation(), Some(second_generation));
        });
    }

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            owner.clone(),
            2,
            OpenGpuiPortDragPhase::Move,
            point(px(700.0), px(500.0)),
        ),
        cx,
    );
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        CanvasInteractionPhase::ConnectionDrag,
    );
    dispatch(
        OpenGpuiPortDragEvent::for_test(
            owner,
            2,
            OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
            point(px(700.0), px(500.0)),
        ),
        cx,
    );
}

#[open_gpui::test]
fn deferred_real_port_terminal_event_cannot_settle_the_next_native_generation(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_interactive_gesture_scene(cx);
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    let (owner, first_native_generation, first_generation) = cx.read_entity(&host, |host, _| {
        let owner = host.active_port_drag().expect("G1 native port owner");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed,
        );
        (
            owner,
            host.interaction_owner
                .test_connection_native_generation()
                .expect("G1 native generation"),
            host.interaction_presentation()
                .generation()
                .expect("G1 semantic generation"),
        )
    });

    cx.update(|window, cx| {
        assert!(window.dispatch_keystroke(Keystroke::parse("escape").unwrap(), cx));
        assert!(window.captured_hitbox().is_none());
    });
    draw(cx);
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());

    let second_generation = cx.read_entity(&host, |host, _| {
        assert_eq!(host.active_port_drag(), Some(owner.clone()));
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        let generation = presentation.generation().expect("G2 semantic generation");
        assert_ne!(generation, first_generation);
        assert_ne!(
            host.interaction_owner
                .test_connection_native_generation()
                .expect("G2 native generation"),
            first_native_generation,
        );
        generation
    });

    cx.update(|window, cx| {
        let host = host.clone();
        let owner = owner.clone();
        window.defer(cx, move |window, cx| {
            let event = OpenGpuiPortDragEvent::for_test(
                owner,
                first_native_generation,
                OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::Escape),
                output.center(),
            );
            host.update(cx, |host, cx| {
                host.handle_port_event(OpenGpuiPortEvent::Pointer(event), window, cx);
            });
        });
    });
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        assert_eq!(presentation.generation(), Some(second_generation));
        assert!(host.active_port_drag().is_some());
    });

    let outside = point(px(700.0), px(500.0));
    cx.simulate_mouse_move(outside, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_none());
        assert_ne!(
            host.interaction_presentation().terminal(),
            Some(InteractionTerminalReason::Cancelled),
        );
    });
}

#[open_gpui::test]
fn keyboard_activation_of_a_real_output_port_enters_the_same_search_flow(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance")
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(instance, PortId::from_u128(20)),
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });

    cx.read_entity(&host, |host, _| {
        let request = host
            .connection_search_request()
            .expect("activation creates a search request");
        assert_eq!(request.source(), PortId::from_u128(20));
        assert!(matches!(
            request.intent(),
            ConnectionGestureIntent::Connect { source, .. }
                if source == PortId::from_u128(20)
        ));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });
}

#[open_gpui::test]
fn high_fanout_output_pointer_and_keyboard_drag_skip_incident_edges(
    cx: &mut open_gpui::TestAppContext,
) {
    const INCIDENT_EDGES: usize = 9_999;

    let runtime = GraphRuntime::builder(catalog())
        .build(high_fanout_output_graph(INCIDENT_EDGES))
        .expect("valid fanout graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
            .with_edge_layer_budget(0)
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let pointer_before = runtime.stats().expect("stats before pointer drag");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    let pointer_after = runtime.stats().expect("stats after pointer drag");
    assert_eq!(
        pointer_after.connection_query_index_bucket_reads
            - pointer_before.connection_query_index_bucket_reads,
        0
    );
    assert_eq!(
        pointer_after.connection_query_index_entries_read
            - pointer_before.connection_query_index_entries_read,
        0
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });

    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.update_window_entity(&host, |host, window, cx| {
        let request = host
            .connection_search_request()
            .expect("pointer drag creates a search request");
        host.cancel_connection_search(request, window, cx)
            .expect("cancel pointer search");
    });

    let instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance")
    });
    let keyboard_before = runtime.stats().expect("stats before keyboard activation");
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(instance, PortId::from_u128(20)),
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    let keyboard_after = runtime.stats().expect("stats after keyboard activation");
    assert_eq!(
        keyboard_after.connection_query_index_bucket_reads
            - keyboard_before.connection_query_index_bucket_reads,
        0
    );
    assert_eq!(
        keyboard_after.connection_query_index_entries_read
            - keyboard_before.connection_query_index_entries_read,
        0
    );
    cx.read_entity(&host, |host, _| {
        let request = host
            .connection_search_request()
            .expect("keyboard activation creates a search request");
        assert!(matches!(
            request.intent(),
            ConnectionGestureIntent::Connect { source, .. }
                if source == PortId::from_u128(20)
        ));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });
}

#[open_gpui::test]
fn pointer_keyboard_and_accessibility_activation_of_an_unconnected_input_enter_connect_search(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("unconnected input bounds");
    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.update_window_entity(&host, |host, window, cx| {
        let request = host
            .connection_search_request()
            .expect("input drag creates a search request");
        assert_eq!(request.source(), PortId::from_u128(21));
        assert!(matches!(
            request.intent(),
            ConnectionGestureIntent::Connect { source, .. }
                if source == PortId::from_u128(21)
        ));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        host.cancel_connection_search(request, window, cx)
            .expect("cancel pointer search");
    });

    let instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(11))
            .cloned()
            .expect("target instance")
    });
    for source in [
        OpenGpuiPortActivationSource::Keyboard,
        OpenGpuiPortActivationSource::Accessibility,
    ] {
        let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
            port_drag_owner(instance.clone(), PortId::from_u128(21)),
            source,
        ));
        cx.update(|window, cx| {
            host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
        });

        cx.update_window_entity(&host, |host, window, cx| {
            let request = host
                .connection_search_request()
                .expect("input activation creates a search request");
            assert_eq!(request.source(), PortId::from_u128(21));
            assert!(matches!(
                request.intent(),
                ConnectionGestureIntent::Connect { source, .. }
                    if source == PortId::from_u128(21)
            ));
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::ConnectionSearch
            );
            assert!(
                !host
                    .connection_search_candidates(request, window, cx)
                    .expect("input-originated semantic candidates")
                    .is_empty()
            );
            host.cancel_connection_search(request, window, cx)
                .expect("cancel activation search");
        });
    }
}

#[open_gpui::test]
fn accessibility_activation_of_an_occupied_input_enters_reconnect_search(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(occupied_input_graph())
        .expect("valid occupied graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(11))
            .cloned()
            .expect("occupied input instance")
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(instance, PortId::from_u128(21)),
        OpenGpuiPortActivationSource::Accessibility,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });

    cx.read_entity(&host, |host, _| {
        let request = host
            .connection_search_request()
            .expect("occupied input activation creates a reconnect search");
        assert_eq!(request.source(), PortId::from_u128(20));
        assert!(matches!(
            request.intent(),
            ConnectionGestureIntent::Reconnect {
                edge,
                fixed_port,
                ..
            } if edge == EdgeId::from_u128(30) && fixed_port == PortId::from_u128(20)
        ));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });
}

#[open_gpui::test]
fn create_from_wire_query_and_selection_add_one_node_and_edge_atomically(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let history_before = runtime.history_state().expect("history state");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let empty = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(empty, MouseButton::Left, Default::default());
    cx.run_until_parked();

    let submission = cx.update_window_entity(&host, |host, window, cx| {
        let request = host.connection_search_request().expect("search request");
        let candidates = host
            .connection_search_candidates(request, window, cx)
            .expect("semantic candidates");
        assert_eq!(candidates.candidates().len(), 1);
        let submission = host
            .select_connection_search_candidate(request, &candidates, 0, window, cx)
            .expect("enqueue atomic create and connect");
        assert_eq!(host.connection_search_request(), None);
        assert!(matches!(
            host.select_connection_search_candidate(request, &candidates, 0, window, cx),
            Err(super::OpenGpuiConnectionSearchError::InactiveRequest)
        ));
        assert!(matches!(
            host.cancel_connection_search(request, window, cx),
            Err(super::OpenGpuiConnectionSearchError::InactiveRequest)
        ));
        submission
    });
    draw(cx);
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.interaction_presentation().connection().is_none());
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            0,
        );
    });

    let snapshot = runtime.snapshot().expect("committed graph");
    assert_eq!(snapshot.graph().nodes().len(), 3);
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert!(snapshot.graph().node(&submission.node()).is_some());
    assert_eq!(
        runtime.history_state().expect("history state").undo_len(),
        history_before.undo_len() + 1,
        "node and connection share one undo boundary",
    );

    runtime
        .execute(GraphCommand::undo(snapshot.revision()))
        .expect("undo command")
        .applied()
        .expect("undo applied");
    let undone = runtime.snapshot().expect("undone graph");
    assert_eq!(undone.graph().nodes().len(), 2);
    assert!(undone.graph().edges().is_empty());
}

#[open_gpui::test]
fn create_from_wire_selection_rebinds_after_an_unrelated_revision(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    let (request, candidates) = cx.update_window_entity(&host, |host, window, cx| {
        let request = host.connection_search_request().expect("search request");
        let candidates = host
            .connection_search_candidates(request, window, cx)
            .expect("semantic candidates");
        (request, candidates)
    });

    let current = runtime.snapshot().expect("current graph");
    runtime
        .execute(GraphCommand::set_node_data(
            current.revision(),
            NodeId::from_u128(10),
            serde_json::json!({"changed": true}),
        ))
        .expect("intervening command")
        .applied()
        .expect("intervening command applied");
    let history_after_intervening = runtime.history_state().expect("history state");

    let submission = cx.update_window_entity(&host, |host, window, cx| {
        let submission = host
            .select_connection_search_candidate(request, &candidates, 0, window, cx)
            .expect("selection enters prefix-barrier commit");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Committing
        );
        submission
    });
    draw(cx);
    draw(cx);

    let snapshot = runtime.snapshot().expect("committed topology");
    assert_eq!(snapshot.graph().nodes().len(), 3);
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert!(snapshot.graph().node(&submission.node()).is_some());
    assert_eq!(
        runtime.history_state().expect("history state").undo_len(),
        history_after_intervening.undo_len() + 1,
        "fresh revision binding retains the create-and-connect undo boundary",
    );
}

#[open_gpui::test]
fn rejected_port_start_cannot_cancel_an_existing_search_or_commit(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let source_instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance")
    });
    let activate = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(source_instance, PortId::from_u128(20)),
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(activate, window, cx));
    });
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    let request = cx.read_entity(&host, |host, _| {
        host.connection_search_request().expect("search request")
    });

    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.connection_search_request(), Some(request));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });

    let candidates = cx.update_window_entity(&host, |host, window, cx| {
        host.connection_search_candidates(request, window, cx)
            .expect("semantic candidates")
    });
    let mut prefix_ticket = cx.update_entity(&host, |host, _| {
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued prefix command")
    });
    cx.update_window_entity(&host, |host, window, cx| {
        host.select_connection_search_candidate(request, &candidates, 0, window, cx)
            .expect("selection waits behind prefix");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Committing
        );
    });

    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();
    let committed_early = runtime
        .snapshot()
        .expect("graph after rejected start")
        .graph()
        .edges()
        .len()
        == 1;
    cx.read_entity(&host, |host, _| {
        let phase = host.interaction_presentation().phase();
        assert!(
            (phase == CanvasInteractionPhase::Committing
                && host.interaction_owner.test_has_pending_submission())
                || (phase == CanvasInteractionPhase::Idle && committed_early),
            "a rejected native start must leave the old transaction pending or successfully settled",
        );
    });
    if committed_early {
        assert!(poll_command_ticket(&mut prefix_ticket).is_ok());
    } else {
        assert_command_ticket_pending(&mut prefix_ticket);
    }

    draw(cx);
    draw(cx);
    let snapshot = runtime.snapshot().expect("committed graph");
    assert_eq!(snapshot.graph().nodes().len(), 3);
    assert_eq!(snapshot.graph().edges().len(), 1);
    if !committed_early {
        assert!(poll_command_ticket(&mut prefix_ticket).is_ok());
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn cancelling_a_pending_interaction_releases_its_prefix_task_reservation(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let source_instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("source instance")
    });
    let activate = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        port_drag_owner(source_instance, PortId::from_u128(20)),
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(activate, window, cx));
    });
    let (request, candidates) = cx.update_window_entity(&host, |host, window, cx| {
        let request = host.connection_search_request().expect("search request");
        let candidates = host
            .connection_search_candidates(request, window, cx)
            .expect("semantic candidates");
        (request, candidates)
    });

    cx.update_window_entity(&host, |host, window, cx| {
        let _prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued prefix command");
        host.select_connection_search_candidate(request, &candidates, 0, window, cx)
            .expect("selection waits behind prefix");
        assert!(host.has_pending_interaction_prefix_task());

        let cancelled = host.interaction_owner.cancel();
        host.discard_pending_interaction_submission();
        host.apply_canvas_interaction_update(cancelled, window, cx);
        assert_eq!(host.commands.capacity_usage(), (1, 0, 0));

        for index in 1..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            host.commands
                .enqueue_detached(
                    GraphCommand::set_node_data(
                        0,
                        NodeId::from_u128(10),
                        serde_json::json!({ "queued": index }),
                    ),
                    super::OpenGpuiCommandOrigin::Binding,
                )
                .unwrap_or_else(|error| {
                    panic!("cancelled prefix task releases slot {index}: {error:?}")
                });
        }
    });
}

#[open_gpui::test]
fn node_size_updates_after_a_captured_prefix_wait_for_their_own_barrier(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let (mut prefix_ticket, mut suffix_ticket) = cx.update_entity(&host, |host, cx| {
        let source_update = issue_content_height_update(host, source, 172.0);
        assert!(host.queue_node_size_updates(vec![source_update]));
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_geometry(
                    0,
                    [NodeGeometryEdit::new(source).with_size(Some(CanvasSize {
                        width: 180.0,
                        height: 150.0,
                    }))],
                )
                .without_history(),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("authoritative prefix resize");
        host.schedule_pending_node_size_prefix(cx);
        assert!(host.has_pending_node_size_prefix_task());

        let suffix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(1, target, serde_json::json!({ "suffix": true })),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("suffix command queued after the captured prefix");
        let target_update = issue_content_height_update(host, target, 188.0);
        assert!(host.queue_node_size_updates(vec![target_update]));
        assert_eq!(host.pending_node_size_command_counts(), Some((1, 1)));
        (prefix_ticket, suffix_ticket)
    });

    for _ in 0..8 {
        draw(cx);
    }

    assert!(
        poll_command_ticket(&mut prefix_ticket)
            .expect("prefix ticket transport")
            .applied()
            .is_some()
    );
    assert!(
        poll_command_ticket(&mut suffix_ticket)
            .expect("suffix ticket transport")
            .applied()
            .is_some()
    );
    let snapshot = runtime.snapshot().expect("ordered node-size graph");
    assert_eq!(snapshot.revision(), 3);
    assert_eq!(
        snapshot.graph().node(&source).expect("source node").size,
        Some(CanvasSize {
            width: 180.0,
            height: 150.0,
        })
    );
    let target_node = snapshot.graph().node(&target).expect("target node");
    assert_eq!(target_node.data, serde_json::json!({ "suffix": true }));
    assert_eq!(
        target_node.size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.pending_node_size_command_counts(), None);
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        assert!(
            host.last_diagnostic().is_none(),
            "unexpected host diagnostic: {:?}",
            host.last_diagnostic()
        );
    });
}

#[open_gpui::test]
fn closing_with_a_pending_node_size_prefix_releases_its_task_and_reservation(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let mut prefix_ticket = cx.update_window_entity(&host, |host, window, cx| {
        let update = issue_content_height_update(host, NodeId::from_u128(10), 172.0);
        assert!(host.queue_node_size_updates(vec![update]));
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    0,
                    NodeId::from_u128(11),
                    serde_json::json!({ "close-prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued close prefix");
        host.schedule_pending_node_size_prefix(cx);
        assert!(host.has_pending_node_size_prefix_task());
        assert_eq!(host.commands.capacity_usage(), (1, 0, 1));

        host.close_in_window(window, cx)
            .expect("window close drains the prefix and cancels node sizing");
        assert_eq!(host.pending_node_size_command_counts(), None);
        assert!(!host.has_pending_node_size_prefix_task());
        assert!(host.ready_node_size_prefixes.is_empty());
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
        prefix_ticket
    });

    cx.run_until_parked();
    assert!(poll_command_ticket(&mut prefix_ticket).is_ok());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert_eq!(host.pending_node_size_command_counts(), None);
        assert!(host.ready_node_size_prefixes.is_empty());
        assert_eq!(host.commands.capacity_usage(), (0, 0, 0));
    });
}

#[open_gpui::test]
fn create_from_wire_cancel_clears_search_and_leaves_graph_unchanged(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let history_before = runtime.history_state().expect("history state");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert!(host.scene_evidence.last_connection_preview.get().is_some());
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1,
        );
    });

    cx.update_window_entity(&host, |host, window, cx| {
        let request = host.connection_search_request().expect("search request");
        host.cancel_connection_search(request, window, cx)
            .expect("cancel current search");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(host.connection_search_request(), None);
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_none());
        assert_eq!(host.scene_evidence.last_connection_preview.get(), None);
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            0,
        );
    });

    assert_eq!(
        runtime.history_state().expect("history state"),
        history_before,
    );
    let snapshot = runtime.snapshot().expect("unchanged graph");
    assert_eq!(snapshot.graph().nodes().len(), 2);
    assert!(snapshot.graph().edges().is_empty());
}

#[open_gpui::test]
fn occupied_input_replacement_is_atomic_and_one_undo_restores_the_old_edge(
    cx: &mut open_gpui::TestAppContext,
) {
    let old_edge = EdgeId::from_u128(30);
    let runtime = GraphRuntime::builder(catalog())
        .build(occupied_input_graph())
        .expect("valid occupied-input graph");
    let history_before = runtime.history_state().expect("history state");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    draw(cx);

    let replacement_output = cx
        .debug_bounds("jellyflow-test-replacement-output-port")
        .expect("replacement output bounds");
    let occupied_input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("occupied input bounds");
    cx.simulate_mouse_down(
        replacement_output.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        occupied_input.center(),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(
        occupied_input.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    let replaced = runtime.snapshot().expect("replacement snapshot");
    assert_eq!(replaced.graph().edges().len(), 1);
    assert!(replaced.graph().edge(&old_edge).is_none());
    let new_edge = replaced
        .graph()
        .edges()
        .iter()
        .next()
        .expect("replacement edge");
    assert_eq!(new_edge.1.from, PortId::from_u128(22));
    assert_eq!(new_edge.1.to, PortId::from_u128(21));
    assert_eq!(
        runtime.history_state().expect("history state").undo_len(),
        history_before.undo_len() + 1,
    );

    runtime
        .execute(GraphCommand::undo(replaced.revision()))
        .expect("undo replacement")
        .applied()
        .expect("undo applied");
    let restored = runtime.snapshot().expect("restored graph");
    assert_eq!(restored.graph().edges().len(), 1);
    let restored_edge = restored.graph().edge(&old_edge).expect("old edge restored");
    assert_eq!(restored_edge.from, PortId::from_u128(20));
    assert_eq!(restored_edge.to, PortId::from_u128(21));
}

#[open_gpui::test]
fn explicit_input_reconnect_retains_edge_identity_and_undo_restores_the_endpoint(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let runtime = GraphRuntime::builder(catalog())
        .build(reconnect_graph())
        .expect("valid reconnect graph");
    let history_before = runtime.history_state().expect("history state");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    draw(cx);

    let old_input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("old input bounds");
    let new_input = cx
        .debug_bounds("jellyflow-test-reconnect-input-port")
        .expect("new input bounds");
    cx.simulate_mouse_down(old_input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        new_input.center(),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_up(new_input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    let reconnected = runtime.snapshot().expect("reconnected graph");
    assert_eq!(reconnected.graph().edges().len(), 1);
    let reconnected_edge = reconnected
        .graph()
        .edge(&edge)
        .expect("edge identity retained");
    assert_eq!(reconnected_edge.from, PortId::from_u128(20));
    assert_eq!(reconnected_edge.to, PortId::from_u128(22));
    assert_eq!(
        runtime.history_state().expect("history state").undo_len(),
        history_before.undo_len() + 1,
    );

    runtime
        .execute(GraphCommand::undo(reconnected.revision()))
        .expect("undo reconnect")
        .applied()
        .expect("undo applied");
    let restored = runtime.snapshot().expect("restored graph");
    let restored_edge = restored.graph().edge(&edge).expect("edge restored");
    assert_eq!(restored_edge.from, PortId::from_u128(20));
    assert_eq!(restored_edge.to, PortId::from_u128(21));
}

#[open_gpui::test]
fn runtime_drop_during_input_reconnect_retires_the_preview_without_mutating_the_edge(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(reconnect_graph())
        .expect("valid reconnect graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    draw(cx);

    let occupied_input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("occupied input bounds");
    cx.simulate_mouse_down(
        occupied_input.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(
        &captured_host.borrow().clone().expect("captured host"),
        |host, _| {
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::ConnectionDrag
            );
            assert!(host.interaction_presentation().connection().is_some());
        },
    );
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    let host = captured_host.borrow().clone().expect("captured host");
    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed
        );
        assert!(host.interaction_presentation().connection().is_none());
        assert!(host.active_port_drag().is_none());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn runtime_drop_does_not_guess_third_party_pointer_ownership(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let source = NodeId::from_u128(10);
    let capture_lease = std::rc::Rc::new(std::cell::RefCell::new(None));
    let factory_lease = capture_lease.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, window, cx| {
        if binding
            .read(cx)
            .authoring()
            .expect("live node authoring")
            .node_id()
            == source
        {
            *factory_lease.borrow_mut() = Some(
                binding
                    .read(cx)
                    .activity()
                    .acquire(OpenGpuiActivityKind::PointerCapture)
                    .expect("third-party pointer capture activity"),
            );
            window.capture_pointer(HitboxId::placeholder());
        }
        cx.new(|_| RootOnlyNode)
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    assert!(capture_lease.borrow().is_some());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    drop(runtime);
    cx.run_until_parked();

    assert_eq!(
        cx.update(|window, _| window.captured_hitbox()),
        Some(HitboxId::placeholder()),
        "an activity lease keeps a component resident but does not identify its capture hitbox",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1,);
        assert!(!host.interaction_owner.has_focus_state());
    });

    cx.update(|window, _| {
        window.release_pointer();
        window.capture_pointer(HitboxId::placeholder());
    });
    draw_once(cx);
    assert!(
        cx.update(|window, _| window.captured_hitbox()).is_some(),
        "a later terminal redraw must not release another component's pointer capture",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1,);
        assert!(host.interaction_owner.terminal());
        assert!(!host.interaction_owner.has_focus_state());
    });
    cx.update(|window, _| window.release_pointer());
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    draw_once(cx);
    cx.run_until_parked();
}

fn simulate_key_up(cx: &mut open_gpui::VisualTestContext, key: &str) {
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).expect("valid test keystroke"),
    });
}

fn draw_once(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
}

#[open_gpui::test]
fn create_from_wire_overlay_capacity_failure_aborts_then_retries_after_release(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(protected_capacity_search_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let leases = std::rc::Rc::new(std::cell::RefCell::new(std::collections::BTreeMap::<
        NodeId,
        OpenGpuiActivityLease,
    >::new()));
    let factory_leases = leases.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let node = binding
            .read(cx)
            .authoring()
            .expect("live node authoring")
            .node_id();
        if node != protected_capacity_scene_node_id() {
            let lease = binding
                .read(cx)
                .activity()
                .acquire(OpenGpuiActivityKind::Focus)
                .expect("fill protected capacity");
            factory_leases.borrow_mut().insert(node, lease);
        }
        cx.new(|_| StickyPortNode {
            binding,
            handle: None,
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    assert_eq!(leases.borrow().len(), OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
    let source_instance = cx.read_entity(&host, |host, _| {
        host.mounts
            .retained_instance(protected_capacity_scene_node_id())
            .cloned()
            .expect("search source instance")
    });
    let activate = || {
        OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
            port_drag_owner(source_instance.clone(), PortId::from_u128(20)),
            OpenGpuiPortActivationSource::Keyboard,
        ))
    };
    let overflows_before = cx.read_entity(&host, |host, _| host.stats().protected_overflows());

    let event = activate();
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.connection_search_request().is_none());
        assert!(!host.interaction_owner.has_connection());
        assert_eq!(host.stats().protected_overflows(), overflows_before + 1);
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    let released = *leases.borrow().keys().next().expect("one protected owner");
    drop(leases.borrow_mut().remove(&released));
    let event = activate();
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.update_window_entity(&host, |host, window, cx| {
        let request = host
            .connection_search_request()
            .expect("retry acquires the released overlay slot");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        host.cancel_connection_search(request, window, cx)
            .expect("cancel retry search");
        assert_eq!(
            host.stats().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY - 1
        );
    });
}

fn poll_command_ticket(
    ticket: &mut crate::OpenGpuiCommandTicket,
) -> crate::OpenGpuiCommandTicketResult {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match std::future::Future::poll(std::pin::Pin::new(ticket), &mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("native command ticket is still pending"),
    }
}

fn assert_command_ticket_pending(ticket: &mut crate::OpenGpuiCommandTicket) {
    assert_command_ticket_pending_at(ticket, "while command is queued");
}

fn assert_command_ticket_pending_at(ticket: &mut crate::OpenGpuiCommandTicket, stage: &str) {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        matches!(
            std::future::Future::poll(std::pin::Pin::new(ticket), &mut context),
            std::task::Poll::Pending
        ),
        "native command ticket unexpectedly resolved {stage}"
    );
}

fn assert_canvas_point_close(actual: CanvasPoint, expected: CanvasPoint) {
    const EPSILON: f32 = 0.001;
    assert!(
        (actual.x - expected.x).abs() <= EPSILON && (actual.y - expected.y).abs() <= EPSILON,
        "canvas points differ: actual={actual:?}, expected={expected:?}",
    );
}

fn mount_gesture_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    mount_gesture_scene_for_graph(cx, unconnected_graph())
}

fn mount_interactive_gesture_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn mount_gesture_scene_for_graph(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow::core::Graph,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn mount_content_size_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(content_size_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(80.0, 320.0).expect("valid content-height policy"),
        ),
    );
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register_with_policy(RENDERER, policy, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn issue_content_height_update(
    host: &mut OpenGpuiSceneHost,
    node: NodeId,
    height: f32,
) -> (jellyflow::runtime::NodeInstanceKey, OpenGpuiNodeSizeUpdate) {
    let instance = host
        .mounts
        .retained_instance(node)
        .cloned()
        .expect("content-sized node instance");
    let environment_epoch = host.adapter_environment_epoch;
    let mounted = host
        .mounts
        .get_mut(&instance)
        .expect("mounted content node");
    let document_size = mounted.semantic_zoom.canonical_size();
    let measured_size = CanvasSize {
        width: document_size.width,
        height,
    };
    let policy = mounted.registration_policy.size_policy();
    let epoch = OpenGpuiNodeSizeEpoch::new(mounted.content_epoch, environment_epoch);
    assert!(
        mounted
            .size_state
            .observe(policy, epoch, document_size, measured_size)
            .is_idle(),
        "the first sample only establishes stability",
    );
    let update = mounted
        .size_state
        .observe(policy, epoch, document_size, measured_size);
    assert!(
        update.command().is_some(),
        "the second stable sample emits one size command",
    );
    (instance, update)
}

fn mount_connection_scene_for_graph(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow::core::Graph,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid test registry"),
                viewport(),
            )
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn close_active_scene_gesture(
    host: &Entity<OpenGpuiSceneHost>,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.read_entity(host, |host, _| assert!(host.active_scene_gesture()));
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("window-aware close");
        assert!(window.captured_hitbox().is_none());
        assert!(window.focused(cx).is_none());
    });
    cx.read_entity(host, |host, _| {
        assert!(!host.active_scene_gesture());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });

    cx.run_until_parked();
    cx.read_entity(host, |host, _| {
        assert_eq!(
            host.scene_evidence.host_closed_scene_cancellations.get(),
            0,
            "terminal Host listeners must not record late native cancellation evidence",
        );
        assert!(!host.active_scene_gesture());
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
    });
}

fn dense_edge_graph(edge_count: usize) -> (jellyflow::core::Graph, Vec<EdgeId>) {
    assert!(edge_count > 0);
    let source = NodeId::from_u128(30_000);
    let target = NodeId::from_u128(30_001);
    let outputs = (0..edge_count)
        .map(|index| PortId::from_u128(100_000 + index as u128))
        .collect::<Vec<_>>();
    let inputs = (0..edge_count)
        .map(|index| PortId::from_u128(200_000 + index as u128))
        .collect::<Vec<_>>();
    let edges = (0..edge_count)
        .map(|index| EdgeId::from_u128(300_000 + index as u128))
        .collect::<Vec<_>>();
    let mut source_node = node(CanvasPoint { x: 40.0, y: 80.0 }, outputs[0]);
    source_node.ports = outputs.clone();
    let mut target_node = node(CanvasPoint { x: 360.0, y: 80.0 }, inputs[0]);
    target_node.ports = inputs.clone();

    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, source_node);
    graph.insert_node(target, target_node);
    for index in 0..edge_count {
        graph.insert_port(
            outputs[index],
            port(source, &format!("output-{index}"), PortDirection::Out),
        );
        graph.insert_port(
            inputs[index],
            port(target, &format!("input-{index}"), PortDirection::In),
        );
        graph.insert_edge(
            edges[index],
            Edge::new(EdgeKind::Data, outputs[index], inputs[index]),
        );
    }
    (graph.build().expect("valid dense edge graph"), edges)
}

fn spatial_edge_graph(grid_side: usize, spacing: f32) -> (jellyflow::core::Graph, Vec<EdgeId>) {
    assert!(grid_side > 0);
    let edge_count = grid_side * grid_side;
    let source = NodeId::from_u128(400_000);
    let target = NodeId::from_u128(400_001);
    let outputs = (0..edge_count)
        .map(|index| PortId::from_u128(500_000 + index as u128))
        .collect::<Vec<_>>();
    let inputs = (0..edge_count)
        .map(|index| PortId::from_u128(600_000 + index as u128))
        .collect::<Vec<_>>();
    let edges = (0..edge_count)
        .map(|index| EdgeId::from_u128(700_000 + index as u128))
        .collect::<Vec<_>>();
    let extent = grid_side as f32 * spacing + 160.0;
    let mut source_node = node(CanvasPoint::default(), outputs[0]);
    source_node.size = Some(CanvasSize {
        width: extent,
        height: extent,
    });
    source_node.ports = outputs.clone();
    let mut target_node = node(CanvasPoint::default(), inputs[0]);
    target_node.size = source_node.size;
    target_node.ports = inputs.clone();

    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    graph.insert_node(source, source_node);
    graph.insert_node(target, target_node);
    for index in 0..edge_count {
        graph.insert_port(
            outputs[index],
            port(source, &format!("output-{index}"), PortDirection::Out),
        );
        graph.insert_port(
            inputs[index],
            port(target, &format!("input-{index}"), PortDirection::In),
        );
        graph.insert_edge(
            edges[index],
            Edge::new(EdgeKind::Data, outputs[index], inputs[index]),
        );
    }
    (graph.build().expect("valid spatial edge graph"), edges)
}

fn bounded_edge_graph(
    visible_edges: usize,
    offscreen_edges: usize,
) -> (jellyflow::core::Graph, Vec<EdgeId>) {
    bounded_edge_graph_with(visible_edges, offscreen_edges, |output, input| {
        Edge::new(EdgeKind::Data, output, input)
    })
}

fn bounded_edge_graph_with(
    visible_edges: usize,
    offscreen_edges: usize,
    mut make_edge: impl FnMut(PortId, PortId) -> Edge,
) -> (jellyflow::core::Graph, Vec<EdgeId>) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let mut visible = Vec::with_capacity(visible_edges);
    for index in 0..visible_edges + offscreen_edges {
        let source = NodeId::from_u128(1_000 + (index * 2) as u128);
        let target = NodeId::from_u128(1_001 + (index * 2) as u128);
        let output = PortId::from_u128(10_000 + (index * 2) as u128);
        let input = PortId::from_u128(10_001 + (index * 2) as u128);
        let edge = EdgeId::from_u128(20_000 + index as u128);
        let origin_x = if index < visible_edges {
            40.0
        } else {
            10_000.0
        };
        let origin_y = 20.0 + (index as f32 * 24.0);
        graph.insert_node(
            source,
            node(
                CanvasPoint {
                    x: origin_x,
                    y: origin_y,
                },
                output,
            ),
        );
        graph.insert_node(
            target,
            node(
                CanvasPoint {
                    x: origin_x + 320.0,
                    y: origin_y,
                },
                input,
            ),
        );
        graph.insert_port(output, port(source, "output", PortDirection::Out));
        graph.insert_port(input, port(target, "input", PortDirection::In));
        graph.insert_edge(edge, make_edge(output, input));
        if index < visible_edges {
            visible.push(edge);
        }
    }
    (graph.build().expect("valid bounded edge graph"), visible)
}

fn publish_all_edge_geometry(view: &ViewSession, selected: impl IntoIterator<Item = EdgeId>) {
    publish_edge_geometry_with(
        view,
        selected,
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 12_000.0,
                height: 2_000.0,
            },
        },
        |_, _, bounds, direction| {
            let x = match direction {
                PortDirection::In => bounds.origin.x,
                PortDirection::Out => bounds.origin.x + bounds.size.width - 12.0,
            };
            ViewRect::new(
                ViewPoint::new(x, bounds.origin.y + 44.0),
                ViewSize::new(12.0, 12.0),
            )
        },
    );
}

fn publish_spatial_edge_geometry(view: &ViewSession, grid_side: usize, spacing: f32) {
    let extent = grid_side as f32 * spacing + 160.0;
    publish_edge_geometry_with(
        view,
        [],
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: extent,
                height: extent,
            },
        },
        |_, index, bounds, direction| {
            let row = index / grid_side;
            let column = index % grid_side;
            let x = bounds.origin.x
                + column as f32 * spacing
                + match direction {
                    PortDirection::Out => 0.0,
                    PortDirection::In => 128.0,
                };
            let y = bounds.origin.y + row as f32 * spacing;
            ViewRect::new(ViewPoint::new(x, y), ViewSize::new(4.0, 4.0))
        },
    );
}

fn publish_edge_geometry_with(
    view: &ViewSession,
    selected: impl IntoIterator<Item = EdgeId>,
    frame_bounds: CanvasRect,
    mut visual_for_port: impl FnMut(NodeId, usize, CanvasRect, PortDirection) -> ViewRect,
) {
    let frame = view
        .frame(FrameRequest::new(
            frame_bounds,
            RendererStamp::new(RENDERER, 1),
        ))
        .expect("materialize all edge endpoints");
    let collector = frame.collector();
    collector
        .selection([], selected, [])
        .expect("select priority edge");
    for node in frame.nodes() {
        let bounds = node.bounds();
        let root = ViewRect::new(
            ViewPoint::new(bounds.origin.x, bounds.origin.y),
            ViewSize::new(bounds.size.width, bounds.size.height),
        );
        let mut layout = node.begin_layout(root);
        for (index, port) in node.ports().iter().enumerate() {
            let direction = port.port().expect("frame port exists").dir;
            let visual = visual_for_port(node.id(), index, bounds, direction);
            let (anchor_x, outward) = match direction {
                PortDirection::In => (visual.origin.x, HandlePosition::Left),
                PortDirection::Out => (visual.origin.x + visual.size.width, HandlePosition::Right),
            };
            layout
                .port_rendered(
                    port.layout_target(),
                    visual,
                    visual,
                    ViewPoint::new(anchor_x, visual.origin.y + visual.size.height * 0.5),
                    outward,
                )
                .expect("cover real frame port");
        }
        collector
            .layout(layout.finish().expect("complete frame-owned node layout"))
            .expect("report complete node geometry");
    }
    view.submit_feedback(collector.finish().expect("finish edge geometry"))
        .expect("publish edge geometry");
}

fn protected_capacity_scene_graph() -> jellyflow::core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x64_000));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        graph.insert_node(
            NodeId::from_u128(index as u128 + 1),
            portless_node(CanvasPoint { x: 400.0, y: 300.0 }),
        );
    }
    graph.insert_node(
        protected_capacity_scene_node_id(),
        portless_node(CanvasPoint { x: 40.0, y: 80.0 }),
    );
    graph.build().expect("valid protected-capacity graph")
}

fn protected_capacity_search_graph() -> jellyflow::core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x64_001));
    for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        graph.insert_node(
            NodeId::from_u128(index as u128 + 1),
            portless_node(CanvasPoint { x: 400.0, y: 300.0 }),
        );
    }
    let source = protected_capacity_scene_node_id();
    let output = PortId::from_u128(20);
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph
        .build()
        .expect("valid protected-capacity search graph")
}

fn protected_capacity_scene_node_id() -> NodeId {
    NodeId::from_u128(10_000)
}

fn routed_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(target, node(CanvasPoint { x: 360.0, y: 80.0 }, input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(input, port(target, "input", PortDirection::In));
    graph.insert_edge(
        EdgeId::from_u128(30),
        Edge::new(EdgeKind::Data, output, input),
    );
    graph.build().expect("valid graph")
}

fn restricted_edge_action_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let edge_id = EdgeId::from_u128(30);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(target, node(CanvasPoint { x: 360.0, y: 80.0 }, input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(input, port(target, "input", PortDirection::In));
    let mut edge = Edge::new(EdgeKind::Data, output, input);
    edge.deletable = Some(false);
    edge.reconnectable = Some(EdgeReconnectable::Endpoint(
        EdgeReconnectableEndpoint::Target,
    ));
    graph.insert_edge(edge_id, edge);
    graph.build().expect("valid restricted edge graph")
}

fn unconnected_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(target, node(CanvasPoint { x: 360.0, y: 80.0 }, input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(input, port(target, "input", PortDirection::In));
    graph.build().expect("valid graph")
}

fn content_size_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let mut source_node = node(CanvasPoint { x: 40.0, y: 80.0 }, PortId::from_u128(20));
    source_node.ports.clear();
    let mut target_node = node(CanvasPoint { x: 360.0, y: 80.0 }, PortId::from_u128(21));
    target_node.ports.clear();
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, source_node);
    graph.insert_node(target, target_node);
    graph.build().expect("valid content-size graph")
}

fn overlapping_target_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let lower_target = NodeId::from_u128(11);
    let top_target = NodeId::from_u128(12);
    let output = PortId::from_u128(20);
    let lower_input = PortId::from_u128(21);
    let top_input = PortId::from_u128(99);
    let target_position = CanvasPoint { x: 360.0, y: 80.0 };
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(lower_target, node(target_position, lower_input));
    graph.insert_node(top_target, node(target_position, top_input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(
        lower_input,
        port(lower_target, "lower-input", PortDirection::In),
    );
    graph.insert_port(top_input, port(top_target, "top-input", PortDirection::In));
    graph.build().expect("valid overlapping target graph")
}

fn high_fanout_output_graph(edge_count: usize) -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    let mut output_port = port(source, "output", PortDirection::Out);
    output_port.capacity = PortCapacity::Multi;
    graph.insert_port(output, output_port);
    let mut target_node = node(
        CanvasPoint { x: 360.0, y: 80.0 },
        PortId::from_u128(200_000),
    );
    target_node.ports.clear();
    target_node.hidden = true;
    for index in 0..edge_count {
        let input = PortId::from_u128(200_000 + index as u128);
        target_node.ports.push(input);
        graph.insert_port(input, port(target, "input", PortDirection::In));
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.hidden = true;
        graph.insert_edge(EdgeId::from_u128(100_000 + index as u128), edge);
    }
    graph.insert_node(target, target_node);
    graph.build().expect("valid high-fanout output graph")
}

fn locked_source_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let mut source_node = node(CanvasPoint { x: 40.0, y: 80.0 }, output);
    source_node.selectable = Some(false);
    source_node.draggable = Some(false);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, source_node);
    graph.insert_node(target, node(CanvasPoint { x: 360.0, y: 80.0 }, input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(input, port(target, "input", PortDirection::In));
    graph.build().expect("valid graph with locked source")
}

fn locked_peer_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let mut peer = node(CanvasPoint { x: 360.0, y: 80.0 }, input);
    peer.draggable = Some(false);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(target, peer);
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(input, port(target, "input", PortDirection::In));
    graph.build().expect("valid graph with locked peer")
}

fn single_node_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let output = PortId::from_u128(20);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.build().expect("valid single-node graph")
}

fn occupied_input_graph() -> jellyflow::core::Graph {
    let old_source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let replacement_source = NodeId::from_u128(12);
    let old_output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let replacement_output = PortId::from_u128(22);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        old_source,
        node(CanvasPoint { x: 40.0, y: 80.0 }, old_output),
    );
    graph.insert_node(
        replacement_source,
        node(CanvasPoint { x: 200.0, y: 260.0 }, replacement_output),
    );
    graph.insert_node(target, node(CanvasPoint { x: 500.0, y: 160.0 }, input));
    graph.insert_port(old_output, port(old_source, "output", PortDirection::Out));
    graph.insert_port(
        replacement_output,
        port(replacement_source, "output", PortDirection::Out),
    );
    graph.insert_port(input, port(target, "input", PortDirection::In));
    graph.insert_edge(
        EdgeId::from_u128(30),
        Edge::new(EdgeKind::Data, old_output, input),
    );
    graph.build().expect("valid occupied-input graph")
}

fn reconnect_graph() -> jellyflow::core::Graph {
    let source = NodeId::from_u128(10);
    let old_target = NodeId::from_u128(11);
    let new_target = NodeId::from_u128(12);
    let output = PortId::from_u128(20);
    let old_input = PortId::from_u128(21);
    let new_input = PortId::from_u128(22);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 160.0 }, output));
    graph.insert_node(
        old_target,
        node(CanvasPoint { x: 360.0, y: 80.0 }, old_input),
    );
    graph.insert_node(
        new_target,
        node(CanvasPoint { x: 650.0, y: 260.0 }, new_input),
    );
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(old_input, port(old_target, "input", PortDirection::In));
    graph.insert_port(new_input, port(new_target, "input", PortDirection::In));
    graph.insert_edge(
        EdgeId::from_u128(30),
        Edge::new(EdgeKind::Data, output, old_input),
    );
    graph.build().expect("valid reconnect graph")
}

fn test_port_selector(port: PortId, direction: PortDirection) -> String {
    match (port, direction) {
        (id, PortDirection::Out) if id == PortId::from_u128(20) => {
            "jellyflow-test-output-port".to_owned()
        }
        (id, PortDirection::In) if id == PortId::from_u128(21) => {
            "jellyflow-test-input-port".to_owned()
        }
        (id, PortDirection::Out) if id == PortId::from_u128(22) => {
            "jellyflow-test-replacement-output-port".to_owned()
        }
        (id, PortDirection::In) if id == PortId::from_u128(22) => {
            "jellyflow-test-reconnect-input-port".to_owned()
        }
        (_, PortDirection::In) => "jellyflow-test-extra-input-port".to_owned(),
        (_, PortDirection::Out) => "jellyflow-test-extra-output-port".to_owned(),
    }
}

fn test_node_selector(node: NodeId) -> String {
    format!("jellyflow-test-node-{node:?}")
}

fn node(position: CanvasPoint, port: PortId) -> Node {
    Node {
        kind: NodeKindKey::new(KIND),
        kind_version: 1,
        pos: position,
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 180.0,
            height: 100.0,
        }),
        hidden: false,
        collapsed: false,
        ports: vec![port],
        data: serde_json::Value::Null,
    }
}

fn portless_node(position: CanvasPoint) -> Node {
    let mut node = node(position, PortId::from_u128(u128::MAX));
    node.ports.clear();
    node
}

fn port(node: NodeId, key: &str, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Scene layer node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .port(jellyflow::runtime::schema::NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .port(jellyflow::runtime::schema::NodePortSpec::new(
                    "output",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .expect("unique node kind");
    catalog
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}

#[open_gpui::test]
fn fit_view_uses_canonical_graph_bounds_and_is_a_noop_before_surface_measurement(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured_host = captured.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                OpenGpuiNodeRegistry::builder()
                    .seal()
                    .expect("valid empty registry"),
                viewport(),
            )
        });
        *captured_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured.borrow().clone().expect("captured host");

    cx.update_entity(&host, |host, cx| {
        host.surface_size = None;
        assert!(!host.fit_view(cx).expect("unmeasured fit is a no-op"));
        host.surface_size = Some(ViewSize::new(800.0, 600.0));
        assert!(host.fit_view(cx).expect("fit routed nodes"));
    });
    let transform = weak_view.viewport_transform().expect("fit transform");
    let top_left = transform
        .screen_point_for_canvas(CanvasPoint { x: 40.0, y: 80.0 })
        .expect("project fit origin");
    let bottom_right = transform
        .screen_point_for_canvas(CanvasPoint { x: 540.0, y: 180.0 })
        .expect("project fit extent");
    assert!((top_left.x - 48.0).abs() < 0.001);
    assert!((bottom_right.x - 752.0).abs() < 0.001);
    assert!(((top_left.y + bottom_right.y) * 0.5 - 300.0).abs() < 0.001);
}

#[test]
fn measured_surface_viewport_tracks_local_pixels_and_transform() {
    let transform =
        ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 2.0).expect("valid transform");
    let viewport = super::canvas_viewport_for_surface(ViewSize::new(900.0, 720.0), transform)
        .expect("valid measured viewport");

    assert_eq!(
        viewport,
        CanvasRect {
            origin: CanvasPoint { x: -12.0, y: 8.0 },
            size: CanvasSize {
                width: 450.0,
                height: 360.0,
            },
        }
    );
}
