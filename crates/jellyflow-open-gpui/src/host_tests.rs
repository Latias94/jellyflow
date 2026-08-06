use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable,
    EdgeReconnectableEndpoint, EdgeRouteSpec, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::profile::ApplyPipelineError;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::NodeGeometryEdit,
    commands::TopologyEdit, commands::TopologyPort, geometry::HandlePosition, geometry::ViewPoint,
    geometry::ViewRect, geometry::ViewSize, graph::EdgeEndpoint, graph::EdgeInstanceKey,
    graph::GraphRuntime, graph::GraphRuntimeMiddleware, graph::GraphRuntimeMiddlewareSnapshot,
    graph::RuntimePatch, interaction::CanvasInteractionError, interaction::CanvasInteractionPhase,
    interaction::InteractionTerminalReason, interaction::InteractionTickDemand,
    io::ViewSessionSeed, io::ViewStackingSeed, view::AdapterEnvironment,
    view::FeedbackSubmissionError, view::FrameMaterializationBudget, view::FrameRequest,
    view::LayoutNodeStatus, view::PublishedPortOutcome, view::RendererStamp,
    view::StaleFrameReason, view::ViewCloseReason, view::ViewSession, view::ViewSessionError,
    view::ViewSessionEvent, view::ViewSessionOptions,
};
use open_gpui::{
    App, AppContext, Context, CursorStyle, Entity, FocusHandle, HitboxId, InteractiveElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement, PinchEvent, Render, RequestFrameOptions, ScrollDelta, ScrollWheelEvent,
    StatefulInteractiveElement, Styled, TouchPhase, VisualContext, Window, div, point, px,
};

use crate::activity::{OpenGpuiActivityKind, OpenGpuiActivityLease};
use crate::authoring::OpenGpuiPortHandle;
use crate::chrome::OpenGpuiEdgeVisualFacts;
use crate::command_journal::OPEN_GPUI_COMMAND_JOURNAL_CAPACITY;
use crate::interaction::{
    OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource, OpenGpuiPortDragCancelReason,
    OpenGpuiPortDragEvent, OpenGpuiPortDragOwner, OpenGpuiPortDragPhase, OpenGpuiPortEvent,
    OpenGpuiSceneGestureKind, port_drag_owner,
};
use crate::navigation::{OpenGpuiInputPreset, OpenGpuiNavigationConfig};
use crate::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRendererDescriptor,
};
use crate::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};

use super::connection::OpenGpuiConnectionPreviewProbeOutcome;
use super::edge_semantics::{
    OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW, open_gpui_edge_semantic_selector,
};
use super::focus_navigation::OpenGpuiGraphFocusTarget;
use super::retained_node_owner::RetainedFrameReconcileFault;
use super::scene_render::{
    edge_path_endpoints, promote_incident_routes, visit_edge_chrome_wires_in_paint_order,
};
use super::{
    LayoutReceiptAction, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY, OpenGpuiBlankCanvasContextHandler,
    OpenGpuiBlankCanvasContextRequest, OpenGpuiNodeBinding, OpenGpuiNodeRegistry,
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneHost, OpenGpuiSceneHostInitError,
    effective_frame_materialization_budget, layout_receipt_action,
};

#[path = "host/tests/interaction_characterization.rs"]
mod interaction_characterization;

#[path = "host/tests/frame_transaction.rs"]
mod frame_transaction;

#[path = "host/tests/pointer_capture_lifecycle.rs"]
mod pointer_capture_lifecycle;

const KIND: &str = "test.scene-layer-node";
const EXEC_KIND: &str = "test.scene-layer-exec-node";
const RENDERER: &str = "test.native.scene-layer-node";

#[test]
fn protected_required_nodes_raise_materialization_and_scan_limits_together() {
    let configured = FrameMaterializationBudget::new(7, 5, 3)
        .with_visible_node_scan_limit(11)
        .with_required_node_scan_limit(13)
        .with_routing_dependency_scan_limit(17)
        .with_port_scan_limit(19);

    let effective = effective_frame_materialization_budget(configured, 513);

    assert_eq!(effective.visible_nodes(), 7);
    assert_eq!(effective.required_nodes(), 513);
    assert_eq!(effective.required_node_scan_limit(), 513);
    assert_eq!(effective.routing_dependency_nodes(), 3);
    assert_eq!(effective.visible_node_scan_limit(), 11);
    assert_eq!(effective.routing_dependency_scan_limit(), 17);
    assert_eq!(effective.port_scan_limit(), 19);
}

#[test]
fn scene_host_reports_a_closed_view_instead_of_panicking() {
    let catalog = NodeCatalog::new();
    let graph = GraphBuilder::new(GraphId::from_u128(0x00C1_05ED))
        .build()
        .expect("valid empty graph");
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph)
        .expect("valid empty runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach empty view");
    view.close().expect("close view before host construction");
    let registry = OpenGpuiNodeRegistry::builder()
        .seal(&catalog)
        .expect("empty registry is valid");

    assert!(matches!(
        OpenGpuiSceneHost::new(view, registry, viewport()),
        Err(OpenGpuiSceneHostInitError::Interaction(
            CanvasInteractionError::ViewUnavailable
        ))
    ));
}

fn register_test_renderer<F, V>(registry: &mut OpenGpuiNodeRegistryBuilder, factory: F)
where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    register_test_renderer_for_kind(registry, KIND, 1, "Scene layer node", factory);
}

fn register_test_renderer_for_kind<F, V>(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    kind: &'static str,
    renderer_incarnation: u64,
    accessible_name: &'static str,
    factory: F,
) where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            kind,
            1,
            RendererStamp::new(RENDERER, renderer_incarnation),
            OpenGpuiNodeAdapterPresentation::new(accessible_name),
            move |binding, window, cx| Ok(factory(binding, window, cx)),
        ))
        .expect("unique test renderer registration");
}

fn register_test_renderer_with_policy<F, V>(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    policy: OpenGpuiNodeRegistrationPolicy,
    factory: F,
) where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                KIND,
                1,
                RendererStamp::new(RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Scene layer node"),
                move |binding, window, cx| Ok(factory(binding, window, cx)),
            )
            .with_policy(policy),
        )
        .expect("unique test renderer registration");
}

fn current_port_drag_owner(
    host: &OpenGpuiSceneHost,
    node: NodeId,
    port: PortId,
) -> OpenGpuiPortDragOwner {
    let instance = host
        .retained_nodes
        .retained_instance(node)
        .cloned()
        .expect("current mounted node instance");
    let port_instance = host
        .view
        .port_instance_key(port)
        .expect("current port identity query")
        .expect("current real port identity");
    port_drag_owner(instance, port_instance).expect("coherent node and port identities")
}

fn current_edge_instance(host: &OpenGpuiSceneHost, edge: EdgeId) -> EdgeInstanceKey {
    host.view
        .edge_instance_key(edge)
        .expect("current edge identity query")
        .expect("current edge identity")
}

fn set_selection_by_id(
    view: &ViewSession,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
) -> Result<(), ViewSessionError> {
    let snapshot = view.graph_snapshot()?;
    let nodes = nodes
        .into_iter()
        .map(|node| {
            snapshot
                .node_lifetime_key(node)
                .expect("selected test node is current")
        })
        .collect();
    let edges = edges
        .into_iter()
        .map(|edge| {
            snapshot
                .edge_instance_key(edge)
                .expect("selected test edge is current")
        })
        .collect();
    view.set_selection(nodes, edges, Vec::new())
}

fn node_drag_start_by_id(
    view: &ViewSession,
    node: NodeId,
    selection_mode: jellyflow_runtime::interaction::SelectionMode,
) -> Result<Option<jellyflow_runtime::interaction::NodeDragStart>, ViewSessionError> {
    let lifetime = view
        .node_lifetime_key(node)?
        .expect("dragged test node is current");
    view.node_drag_start(lifetime, selection_mode)
}

fn pointer_sample_canvas(
    sample: jellyflow_runtime::viewport::ViewportPointerSample,
) -> CanvasPoint {
    let raw = sample.raw();
    sample
        .snapshot()
        .transform()
        .canvas_point_at_screen(raw)
        .expect("valid pointer sample has a canvas projection")
}

fn assert_connect_search_binding(host: &OpenGpuiSceneHost, source: PortId) {
    let binding = host
        .interaction_presentation()
        .connection_intent()
        .expect("active connection-search binding");
    assert_eq!(binding.fixed_port_instance().port(), source);
    assert!(binding.reconnect_edge().is_none());
}

fn assert_reconnect_search_binding(host: &OpenGpuiSceneHost, edge: EdgeId, fixed_port: PortId) {
    let binding = host
        .interaction_presentation()
        .connection_intent()
        .expect("active reconnect-search binding");
    assert_eq!(binding.fixed_port_instance().port(), fixed_port);
    assert_eq!(
        binding.reconnect_edge().map(EdgeInstanceKey::edge),
        Some(edge)
    );
}

struct RoutedNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

struct RootOnlyNode;

struct NativeTextControlNode {
    focus: FocusHandle,
    id: String,
}

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

impl Render for NativeTextControlNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().relative().size_full().child(
            div()
                .id(self.id.clone())
                .debug_selector(|| "jellyflow-test-native-text-control".to_owned())
                .role(open_gpui::accesskit::Role::TextInput)
                .aria_label("Native node text control")
                .track_focus(&self.focus)
                .tab_stop(true)
                .w(px(120.0))
                .h(px(28.0)),
        )
    }
}

struct RecordGraphCommands {
    labels: Rc<std::cell::RefCell<Vec<Option<String>>>>,
}

impl GraphRuntimeMiddleware for RecordGraphCommands {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.labels
            .borrow_mut()
            .push(command.label().map(str::to_owned));
        Ok(())
    }
}

struct InteractiveRoutedNode {
    binding: Entity<OpenGpuiNodeBinding>,
    events: std::rc::Rc<std::cell::Cell<u32>>,
}

struct InteractiveScene {
    host: Entity<OpenGpuiSceneHost>,
}

struct ReplaceableInteractiveScene {
    host: Option<Entity<OpenGpuiSceneHost>>,
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
        div()
            .size_full()
            .on_key_down(|_, _, _| {})
            .child(self.host.clone())
    }
}

impl Render for ReplaceableInteractiveScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().children(self.host.clone())
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
        let ports = authoring
            .ports()
            .map(|port| (port.id(), port.facts().dir, port.handle().clone()))
            .collect::<Vec<_>>();
        let drag_handle = binding.drag_handle();
        let mut input_index = 0usize;
        let mut output_index = 0usize;
        let mut root = div().relative().size_full();
        for (port, direction, handle) in ports {
            let row = match direction {
                PortDirection::In => {
                    let row = input_index;
                    input_index = input_index.saturating_add(1);
                    row
                }
                PortDirection::Out => {
                    let row = output_index;
                    output_index = output_index.saturating_add(1);
                    row
                }
            };
            let selector = test_port_selector(port, direction);
            let marker = div()
                .debug_selector(move || selector.clone())
                .w(px(14.0))
                .h(px(14.0));
            let positioned = match direction {
                PortDirection::In => div().absolute().left(px(0.0)),
                PortDirection::Out => div().absolute().right(px(0.0)),
            }
            .top(px(43.0 + row as f32 * 22.0));
            let events = self.events.clone();
            let interactive = handle.interactive_with_observer(marker, move |_, _, _| {
                events.set(events.get() + 1);
            });
            root = root.child(positioned.child(interactive));
        }
        drag_handle.wrap(root)
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            jellyflow_runtime::view::FrameRequest::new(viewport(), RendererStamp::new(RENDERER, 1))
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
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");

    draw(cx);
    cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("incomplete native measurement is explicit");
        assert!(diagnostic.message().contains("missing real ports"));
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
        weak_view
            .geometry_snapshot()
            .expect("geometry after native work settles")
            .layout_revision(),
        authoritative.layout_revision(),
        "incomplete coverage must preserve the last complete geometry",
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
    register_test_renderer(&mut registry, |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("snapshot before replacing source ports")
                .version(),
            TopologyEdit::new().replace_node_ports(
                NodeId::from_u128(10),
                std::iter::empty::<TopologyPort>(),
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("replace source ports")
        .applied()
        .expect("port replacement applies");
    cx.run_until_parked();
    draw(cx);

    let diagnostic = cx.read_entity(&host, |host, _| {
        let diagnostic = host
            .last_diagnostic()
            .expect("prepaint measurement diagnostic");
        assert_eq!(diagnostic.node(), Some(NodeId::from_u128(10)));
        assert!(!diagnostic.message().is_empty());
        diagnostic.clone()
    });
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&host, |host, _| host.last_diagnostic().cloned()),
        Some(diagnostic),
        "a deterministic prepaint error must settle without replacing its diagnosis",
    );
}

#[open_gpui::test]
fn fresh_route_is_planned_from_published_geometry(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let weak_view = view.downgrade();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });

    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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

    cx.update_entity(&host, |host, _| {
        let wires = host.scene_wires().expect("fresh route plan");
        assert_eq!(wires.published.len(), 1);
        assert_eq!(wires.published[0].edge, EdgeId::from_u128(30));
        assert_eq!(
            wires.published[0].geometry_revision,
            geometry.route_geometry_revision(EdgeId::from_u128(30)),
        );
    });
}

#[open_gpui::test]
fn focused_edge_is_resolved_inside_the_ordinary_wire_plan(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let edge = EdgeId::from_u128(30);
        let focused = host
            .view
            .edge_instance_key(edge)
            .expect("read current focused edge lifetime")
            .expect("current focused edge lifetime");
        let wires = host
            .scene_wires_with_offsets(&BTreeMap::new(), Some(focused))
            .expect("focused ordinary wire plan");
        let wire = wires
            .published
            .iter()
            .find(|wire| wire.edge == edge)
            .expect("focused edge remains in runtime paint order");

        assert_eq!(
            wire.visual_facts,
            OpenGpuiEdgeVisualFacts::default().with_focused(true)
        );
        assert!(
            wire.halo_paint_key.is_some(),
            "focus contributes one ordinary dominant halo",
        );
    });
}

#[open_gpui::test]
fn edge_chrome_keeps_hover_style_in_the_authoritative_runtime_position(
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
        set_selection_by_id(&host.view, Vec::new(), vec![selected])
            .expect("select emphasized edge");
    });
    cx.run_until_parked();
    draw(cx);
    let before_hover = cx.update_entity(&host, |host, _| {
        host.scene_wires()
            .expect("ordered edge plan before hover")
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>()
    });

    let pointer = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("edge geometry");
        let sample = geometry
            .route(hovered)
            .expect("hover route")
            .facts()
            .path
            .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
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

    let paint_order = cx.update_entity(&host, |host, _| {
        let wires = host.scene_wires().expect("ordered edge chrome plan");
        assert_eq!(
            wires
                .published
                .iter()
                .map(|wire| wire.edge)
                .collect::<Vec<_>>(),
            before_hover,
            "hover must invalidate style without moving the route to an adapter overlay",
        );
        let mut order = Vec::new();
        visit_edge_chrome_wires_in_paint_order(&wires, |wire| {
            order.push((wire.edge, wire.visual_facts));
        });
        order
    });
    assert_eq!(
        paint_order,
        vec![
            (ordinary, OpenGpuiEdgeVisualFacts::default()),
            (
                hovered,
                OpenGpuiEdgeVisualFacts::default().with_hovered(true),
            ),
            (
                selected,
                OpenGpuiEdgeVisualFacts::default().with_selected(true),
            ),
        ],
        "paths, endpoint markers, labels, and hit visitation share runtime route order",
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
            .seal(&catalog())
            .expect("valid empty registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host")
    .with_edge_layer_budget(usize::MAX);

    assert_eq!(
        host.edge_layer_budget(),
        super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET,
    );
}

#[test]
fn endpoint_geometry_diagnostics_deduplicate_and_report_the_latest_group() {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal(&catalog())
            .expect("valid empty registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 3 endpoint fallback selected PublishedPort".to_owned()],
        1,
    );
    assert_eq!(
        host.last_diagnostic().map(|diagnostic| diagnostic.kind()),
        Some(super::OpenGpuiSceneHostDiagnosticKind::EndpointGeometry)
    );
    let first = host.last_diagnostic().cloned();

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 3 endpoint fallback selected PublishedPort".to_owned()],
        1,
    );
    assert_eq!(
        host.last_diagnostic(),
        first.as_ref(),
        "the same retained fallback must preserve the existing diagnosis",
    );

    host.record_endpoint_geometry_diagnostics(
        vec!["layout revision 4 endpoint fallback selected ExplicitFallback".to_owned()],
        2,
    );
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
            .seal(&catalog())
            .expect("valid empty registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");

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
    assert!(host.last_diagnostic().is_none());
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
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });

    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    cx.update_entity(&host, |host, _| {
        assert!(
            host.scene_wires()
                .expect("empty route plan without fresh endpoints")
                .published
                .is_empty()
        );
    });
}

#[test]
fn edge_layer_culls_offscreen_routes_and_keeps_the_frontmost_runtime_suffix() {
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
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host")
    .with_edge_layer_budget(EDGE_BUDGET);
    let wires = host.scene_wires().expect("bounded edge plan");

    assert_eq!(
        wires
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
        vec![visible_edges[2], selected],
        "the adapter budget keeps the frontmost suffix without classifying route styles",
    );
    assert_eq!(
        wires.published[0].visual_facts,
        OpenGpuiEdgeVisualFacts::default()
    );
    assert_eq!(
        wires.published[1].visual_facts,
        OpenGpuiEdgeVisualFacts::default().with_selected(true)
    );

    let overflow = host.last_edge_layer_overflow();
    assert!(!overflow.query_pending());
    assert!(!overflow.query_budget_exhausted());
    assert!(!overflow.reused_previous_plan());
    assert_eq!(overflow.query_candidates_matched(), VISIBLE_EDGES as u64);
    assert_eq!(overflow.budget_omitted(), VISIBLE_EDGES - EDGE_BUDGET);
    assert!(!overflow.is_empty());

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

    let previous_geometry_revisions = cached
        .published
        .iter()
        .map(|wire| (wire.edge, wire.geometry_revision))
        .collect::<BTreeMap<_, _>>();
    publish_all_edge_geometry_with_offset(&host.view, [selected], 18.0);
    let reselected = visible_edges[2];
    set_selection_by_id(&host.view, Vec::new(), vec![reselected])
        .expect("change paint order before a bounded refresh");
    host.visible_route_query_budget = 1;
    let resumed = host
        .scene_wires()
        .expect("incomplete query retains the complete paint plan");
    assert_eq!(resumed.published.len(), EDGE_BUDGET);
    assert_eq!(
        resumed
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
        vec![visible_edges[2], visible_edges[3]],
        "an incomplete refresh retains the previous complete runtime order without adapter elevation",
    );
    assert!(
        resumed
            .published
            .iter()
            .find(|wire| wire.edge == reselected)
            .expect("reselected edge remains in the retained route publication")
            .visual_facts
            .selected()
    );
    assert!(
        !resumed
            .published
            .iter()
            .find(|wire| wire.edge == selected)
            .expect("previously selected edge remains in the retained route publication")
            .visual_facts
            .selected()
    );
    let overflow = host.last_edge_layer_overflow();
    assert!(overflow.query_pending());
    assert!(overflow.reused_previous_plan());
    assert!(overflow.query_entries_examined() <= 1);
    let current_geometry = host
        .view
        .geometry_snapshot()
        .expect("current geometry after bounded route refresh");
    for wire in resumed.published.iter() {
        let current_revision = current_geometry
            .route_geometry_revision(wire.edge)
            .expect("current published route has a geometry revision");
        assert_eq!(wire.geometry_revision, Some(current_revision));
        assert_ne!(
            previous_geometry_revisions
                .get(&wire.edge)
                .copied()
                .flatten(),
            Some(current_revision),
            "pending membership must not reuse its old tessellation revision",
        );
    }
}

#[test]
fn manual_frame_bounds_remain_the_route_query_authority_after_surface_measurement() {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach routed view");
    publish_all_edge_geometry(&view, []);
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");
    host.surface_size = Some(ViewSize::new(800.0, 600.0));
    host.set_frame_bounds(CanvasRect {
        origin: CanvasPoint {
            x: 10_000.0,
            y: 10_000.0,
        },
        size: viewport().size,
    });

    let wires = host.scene_wires().expect("manual offscreen route plan");

    assert!(wires.published.is_empty());
    assert_eq!(
        host.last_edge_layer_overflow().query_candidates_matched(),
        0,
        "a measured surface must not replace explicit manual query bounds",
    );
}

#[test]
fn tracked_surface_routes_follow_the_current_transform_before_prepaint_caches_bounds() {
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid routed graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach routed view");
    publish_all_edge_geometry(&view, []);
    let mut host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");
    host.surface_size = Some(ViewSize::new(800.0, 600.0));
    host.track_surface_viewport();
    let cached_bounds = host.frame_bounds();
    host.view
        .set_viewport(
            ViewportTransform::new(
                CanvasPoint {
                    x: -10_000.0,
                    y: -10_000.0,
                },
                1.0,
            )
            .expect("valid panned viewport"),
        )
        .expect("pan routed view");

    let wires = host.scene_wires().expect("current-transform route plan");

    assert_eq!(host.frame_bounds(), cached_bounds);
    assert!(wires.published.is_empty());
    assert_eq!(
        host.last_edge_layer_overflow().query_candidates_matched(),
        0,
        "route query and culling must use the current transform without waiting for prepaint",
    );
}

#[test]
fn incident_route_promotion_reserves_a_saturated_handoff_budget() {
    let (graph, incident, ordinary) = overlapping_edge_graph();
    let runtime = GraphRuntime::builder(catalog())
        .build(graph.clone())
        .expect("valid promotion graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach promotion view");
    let mut routes = vec![ordinary];

    let promoted = promote_incident_routes(
        &view,
        Some(&graph),
        &BTreeSet::from([NodeId::from_u128(10)]),
        &mut routes,
        1,
    )
    .expect("promote incident route");

    assert_eq!(promoted, BTreeSet::from([incident]));
    assert_eq!(
        routes,
        vec![ordinary, incident],
        "the incident route must occupy the retained frontmost budget slot",
    );
}

#[test]
fn dense_routes_complete_through_bounded_query_pages_and_reuse_the_plan() {
    const ROUTE_SCAN_LIMIT: usize = 64;
    const EDGE_COUNT: usize = ROUTE_SCAN_LIMIT * 4;
    const EDGE_BUDGET: usize = ROUTE_SCAN_LIMIT * 2;

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
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");
    assert_eq!(
        host.visible_route_query_budget(),
        super::OPEN_GPUI_DEFAULT_VISIBLE_ROUTE_QUERY_BUDGET,
    );
    let mut host = host
        .with_edge_layer_budget(EDGE_BUDGET)
        .with_visible_route_query_budget(ROUTE_SCAN_LIMIT);

    let mut pending_pages = 0_u64;
    let terminal_wires = loop {
        let wires = host.scene_wires().expect("bounded dense edge plan");
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
        assert!(!overflow.query_budget_exhausted());
        assert_eq!(
            wires.published.len(),
            EDGE_BUDGET,
            "the edge layer budget must retain a deterministic frontmost suffix",
        );
        assert_eq!(overflow.budget_omitted(), EDGE_COUNT - EDGE_BUDGET);
        break wires;
    };

    let terminal = host
        .scene_wires()
        .expect("a completed paged query is reusable for an unchanged query key");
    assert_eq!(
        terminal
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
        terminal_wires
            .published
            .iter()
            .map(|wire| wire.edge)
            .collect::<Vec<_>>(),
    );
    let overflow = host.last_edge_layer_overflow();
    assert!(!overflow.query_budget_exhausted());
    assert!(overflow.reused_previous_plan());
    assert_eq!(overflow.query_entries_examined(), 0);
    assert_eq!(overflow.query_candidates_matched(), 0);
}

#[test]
fn ten_thousand_spatial_routes_materialize_only_the_local_hundred() {
    const GRID_SIDE: usize = 100;
    const SPACING: f32 = 512.0;
    const QUERY_BUDGET: usize = 512;

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
            .seal(&catalog())
            .expect("valid empty test registry"),
        query_viewport,
    )
    .expect("valid Open GPUI scene host")
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
            .map(|wire| wire.base_paint_key)
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
}

#[open_gpui::test]
fn retired_edge_actions_cannot_target_a_same_id_successor(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let retired = cx.read_entity(&host, |host, _| current_edge_instance(host, edge));
    let removed = runtime
        .execute(GraphCommand::disconnect(
            runtime
                .snapshot()
                .expect("snapshot before removal")
                .version(),
            edge,
        ))
        .into_result()
        .expect("remove edge");
    let removed_version = removed.applied().expect("edge removal applies").version();
    runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .expect("restore edge")
        .applied()
        .expect("edge restoration applies");
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let successor = cx.read_entity(&host, |host, _| current_edge_instance(host, edge));
    assert_ne!(retired, successor);
    let before = runtime.snapshot().expect("snapshot before stale actions");
    cx.update_window_entity(&host, |host, window, cx| {
        assert!(!host.activate_edge_action(
            retired,
            jellyflow_runtime::interaction::SelectionMode::Replace,
            window,
            cx,
        ));
        assert!(
            host.edge_reconnect_press_target(retired, EdgeEndpoint::To)
                .is_none()
        );
        assert!(!host.activate_edge_reconnect_action(retired, EdgeEndpoint::To, window, cx,));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(!host.interaction_owner.has_connection());
    });

    let after = runtime.snapshot().expect("snapshot after stale actions");
    assert_eq!(after.revision(), before.revision());
    assert!(after.graph().edge(&edge).is_some());
}

#[open_gpui::test]
fn edge_activation_failure_is_diagnosed_without_replacing_active_interaction(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let edge = cx.read_entity(&host, |host, _| {
        current_edge_instance(host, EdgeId::from_u128(30))
    });
    cx.simulate_mouse_down(
        point(px(700.0), px(500.0)),
        MouseButton::Middle,
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(720.0), px(520.0)),
        Some(MouseButton::Middle),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Pan
        );
    });

    cx.update_window_entity(&host, |host, window, cx| {
        assert!(!host.activate_edge_action(
            edge,
            jellyflow_runtime::interaction::SelectionMode::Replace,
            window,
            cx,
        ));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Pan
        );
        assert!(
            host.last_diagnostic()
                .is_some_and(|diagnostic| diagnostic.message().contains("interaction is active"))
        );
    });

    close_active_scene_gesture(&host, cx);
}

#[open_gpui::test]
fn retired_node_action_cannot_select_a_same_id_successor(cx: &mut open_gpui::TestAppContext) {
    let node = NodeId::from_u128(10);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let retired = cx.read_entity(&host, |host, _| {
        host.retained_nodes
            .retained_instance(node)
            .expect("retired node mount")
            .lifetime()
    });
    let removed = runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("snapshot before removal")
                .version(),
            TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Cascade),
        ))
        .into_result()
        .expect("remove node");
    let removed_version = removed.applied().expect("node removal applies").version();
    runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .expect("restore node")
        .applied()
        .expect("node restoration applies");
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let successor = cx.read_entity(&host, |host, _| {
        host.retained_nodes
            .retained_instance(node)
            .expect("successor node mount")
            .lifetime()
    });
    assert_ne!(retired, successor);
    cx.update_window_entity(&host, |host, window, cx| {
        assert!(!host.activate_node_action(
            retired,
            jellyflow_runtime::interaction::SelectionMode::Replace,
            window,
            cx,
        ));
        assert!(
            host.view
                .selection()
                .expect("selection after stale action")
                .nodes()
                .is_empty()
        );
    });
}

#[open_gpui::test]
fn every_route_uses_one_exact_pointer_path_away_from_and_through_the_focus_proxy(
    cx: &mut open_gpui::TestAppContext,
) {
    let routes = [
        EdgeRouteSpec::straight(),
        EdgeRouteSpec::default(),
        EdgeRouteSpec::default_smooth_step(),
        EdgeRouteSpec::default_orthogonal(),
    ];
    let next_route = Cell::new(0usize);
    let (graph, edges) = bounded_edge_graph_with_gap(routes.len(), 0, 520.0, |output, input| {
        let index = next_route.get();
        next_route.set(index + 1);
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.view.route = Some(routes[index]);
        edge
    });
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);

    for zoom in [0.5, 1.0, 2.0] {
        cx.read_entity(&host, |host, _| {
            let pan = if zoom > 1.0 {
                CanvasPoint { x: -200.0, y: 0.0 }
            } else {
                CanvasPoint::default()
            };
            host.view
                .set_viewport(ViewportTransform::new(pan, zoom).unwrap())
                .expect("set route-matrix viewport");
        });
        draw(cx);
        draw(cx);

        for edge in edges.iter().copied() {
            // Keep the off-center probes outside the 22-pixel endpoint targets at overview zoom.
            for progress in [0.35, 0.5, 0.65] {
                cx.read_entity(&host, |host, _| {
                    set_selection_by_id(&host.view, Vec::new(), Vec::new())
                        .expect("clear route-matrix selection");
                });
                let pointer = cx.read_entity(&host, |host, _| {
                    let geometry = host.view.geometry_snapshot().expect("edge geometry");
                    let sample = geometry
                        .route(edge)
                        .expect("published edge")
                        .facts()
                        .path
                        .sample(jellyflow_runtime::geometry::EdgePathAnchor::Progress(
                            progress,
                        ))
                        .expect("edge progress");
                    let transform = host.view.viewport_transform().expect("viewport transform");
                    let position = transform
                        .screen_point_for_canvas(sample.point)
                        .expect("projected edge progress");
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
                        [edge],
                        "route {edge:?}, progress {progress}, zoom {zoom}",
                    );
                    assert_eq!(
                        host.interaction_presentation().phase(),
                        CanvasInteractionPhase::Idle
                    );
                });
            }
        }
    }
}

#[open_gpui::test]
fn overlapping_routes_pick_the_same_front_edge_that_the_batch_layer_paints_last(
    cx: &mut open_gpui::TestAppContext,
) {
    let (graph, back, front) = overlapping_edge_graph();
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);
    draw(cx);
    draw(cx);

    for (selected, expected) in [(None, front), (Some(back), back)] {
        cx.read_entity(&host, |host, _| {
            set_selection_by_id(&host.view, Vec::new(), selected)
                .expect("seed overlapping edge selection");
        });
        draw(cx);
        let pointer = cx.read_entity(&host, |host, _| {
            let sample = host
                .view
                .geometry_snapshot()
                .expect("overlapping geometry")
                .route(expected)
                .expect("overlapping route")
                .facts()
                .path
                .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
                .expect("overlapping route center");
            let screen = host
                .view
                .viewport_transform()
                .expect("overlapping transform")
                .screen_point_for_canvas(sample.point)
                .expect("project overlapping route");
            let origin = host.scene_origin.expect("measured scene origin");
            point(origin.x + px(screen.x), origin.y + px(screen.y))
        });

        cx.simulate_click(pointer, Default::default());
        cx.run_until_parked();
        draw(cx);
        cx.read_entity(&host, |host, _| {
            assert_eq!(
                host.view
                    .selection()
                    .expect("overlapping pointer selection")
                    .edges(),
                [expected],
            );
        });
    }
}

#[open_gpui::test]
fn blank_canvas_context_hook_is_blank_only_and_does_not_mutate_the_graph(
    cx: &mut open_gpui::TestAppContext,
) {
    let requests = Rc::new(std::cell::RefCell::new(Vec::<
        OpenGpuiBlankCanvasContextRequest,
    >::new()));
    let captured_requests = requests.clone();
    let handler: OpenGpuiBlankCanvasContextHandler =
        Rc::new(move |request, _window, _cx| captured_requests.borrow_mut().push(request));
    let (runtime, host, cx) =
        mount_connection_scene_for_graph_with_context_handler(cx, routed_graph(), Some(handler));
    draw(cx);
    draw(cx);

    let before = runtime.snapshot().expect("graph before context requests");
    let history_before = runtime
        .history_state()
        .expect("history before context requests");
    let (blank, expected_position, node, edge) = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("context geometry");
        let transform = host.view.viewport_transform().expect("context transform");
        let origin = host.scene_origin.expect("context scene origin");
        let project = |canvas: CanvasPoint| {
            let screen = transform
                .screen_point_for_canvas(canvas)
                .expect("project context probe");
            point(origin.x + px(screen.x), origin.y + px(screen.y))
        };
        let blank = project(CanvasPoint { x: 20.0, y: 20.0 });
        let node_bounds = geometry
            .node(NodeId::from_u128(10))
            .expect("source node geometry")
            .bounds();
        let node = project(CanvasPoint {
            x: node_bounds.origin.x + node_bounds.size.width * 0.5,
            y: node_bounds.origin.y + node_bounds.size.height * 0.5,
        });
        let edge_sample = geometry
            .route(EdgeId::from_u128(30))
            .expect("context edge geometry")
            .facts()
            .path
            .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
            .expect("context edge center");
        (
            blank,
            pointer_sample_canvas(
                host.viewport_pointer_sample(blank)
                    .expect("blank pointer sample"),
            ),
            node,
            project(edge_sample.point),
        )
    });
    let modifiers = Modifiers {
        shift: true,
        ..Default::default()
    };

    cx.simulate_mouse_down(blank, MouseButton::Right, modifiers);
    cx.simulate_mouse_up(blank, MouseButton::Right, modifiers);
    cx.run_until_parked();
    draw(cx);
    let request = requests.borrow()[0];
    assert_eq!(request.position(), expected_position);
    assert_eq!(request.button(), MouseButton::Right);
    assert_eq!(request.modifiers(), modifiers);

    for occupied in [node, edge] {
        cx.simulate_mouse_down(occupied, MouseButton::Right, Default::default());
        cx.simulate_mouse_up(occupied, MouseButton::Right, Default::default());
        cx.run_until_parked();
        draw(cx);
    }

    assert_eq!(requests.borrow().len(), 1);
    assert!(before.ptr_eq(&runtime.snapshot().expect("graph after context requests")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after context requests"),
        history_before
    );
}

#[open_gpui::test]
fn blueprint_right_click_opens_blank_context_while_right_drag_pans(
    cx: &mut open_gpui::TestAppContext,
) {
    let requests = Rc::new(std::cell::RefCell::new(Vec::<
        OpenGpuiBlankCanvasContextRequest,
    >::new()));
    let captured_requests = requests.clone();
    let handler: OpenGpuiBlankCanvasContextHandler =
        Rc::new(move |request, _window, _cx| captured_requests.borrow_mut().push(request));
    let (_runtime, host, cx) =
        mount_connection_scene_for_graph_with_context_handler(cx, routed_graph(), Some(handler));
    draw(cx);
    draw(cx);
    cx.update_entity(&host, |host, cx| {
        host.set_navigation_config(
            OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Blueprint),
            cx,
        )
        .expect("idle host accepts Blueprint navigation");
    });
    draw(cx);

    let (click, drag_end, expected_position, transform_before) =
        cx.read_entity(&host, |host, _| {
            let transform = host.view.viewport_transform().expect("context transform");
            let origin = host.scene_origin.expect("context scene origin");
            let canvas = CanvasPoint { x: 20.0, y: 20.0 };
            let screen = transform
                .screen_point_for_canvas(canvas)
                .expect("project Blueprint context probe");
            let click = point(origin.x + px(screen.x), origin.y + px(screen.y));
            (
                click,
                point(click.x + px(40.0), click.y + px(30.0)),
                canvas,
                transform,
            )
        });

    cx.simulate_mouse_down(click, MouseButton::Right, Default::default());
    cx.simulate_mouse_up(click, MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(requests.borrow()[0].position(), expected_position);
    let transform_after_click = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("post-click transform")
    });
    assert_eq!(transform_after_click, transform_before);

    cx.simulate_mouse_down(click, MouseButton::Right, Default::default());
    cx.simulate_mouse_move(drag_end, Some(MouseButton::Right), Default::default());
    cx.simulate_mouse_up(drag_end, MouseButton::Right, Default::default());
    cx.run_until_parked();
    draw(cx);

    assert_eq!(requests.borrow().len(), 1);
    cx.read_entity(&host, |host, _| {
        assert_ne!(
            host.view
                .viewport_transform()
                .expect("post-drag transform")
                .pan,
            transform_after_click.pan
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.pending_blank_canvas_context.is_none());
    });

    let (node, edge, transform_before_occupied_clicks) = cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("context geometry");
        let transform = host.view.viewport_transform().expect("context transform");
        let origin = host.scene_origin.expect("context scene origin");
        let project = |canvas: CanvasPoint| {
            let screen = transform
                .screen_point_for_canvas(canvas)
                .expect("project occupied context probe");
            point(origin.x + px(screen.x), origin.y + px(screen.y))
        };
        let node_bounds = geometry
            .node(NodeId::from_u128(10))
            .expect("source node geometry")
            .bounds();
        let edge_sample = geometry
            .route(EdgeId::from_u128(30))
            .expect("context edge geometry")
            .facts()
            .path
            .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
            .expect("context edge center");
        (
            project(CanvasPoint {
                x: node_bounds.origin.x + node_bounds.size.width * 0.5,
                y: node_bounds.origin.y + node_bounds.size.height * 0.5,
            }),
            project(edge_sample.point),
            transform,
        )
    });
    for occupied in [node, edge] {
        cx.simulate_mouse_down(occupied, MouseButton::Right, Default::default());
        cx.simulate_mouse_up(occupied, MouseButton::Right, Default::default());
        cx.run_until_parked();
        draw(cx);
    }
    assert_eq!(requests.borrow().len(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .viewport_transform()
                .expect("post-occupied-click transform"),
            transform_before_occupied_clicks
        );
    });
}

#[open_gpui::test]
fn blueprint_pending_context_is_cancelled_when_pointer_capture_is_replaced(
    cx: &mut open_gpui::TestAppContext,
) {
    let requests = Rc::new(std::cell::RefCell::new(Vec::<
        OpenGpuiBlankCanvasContextRequest,
    >::new()));
    let captured_requests = requests.clone();
    let handler: OpenGpuiBlankCanvasContextHandler =
        Rc::new(move |request, _window, _cx| captured_requests.borrow_mut().push(request));
    let (_runtime, host, cx) =
        mount_connection_scene_for_graph_with_context_handler(cx, routed_graph(), Some(handler));
    draw(cx);
    draw(cx);
    cx.update_entity(&host, |host, cx| {
        host.set_navigation_config(
            OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Blueprint),
            cx,
        )
        .expect("idle host accepts Blueprint navigation");
    });
    draw(cx);

    let blank = cx.read_entity(&host, |host, _| {
        let transform = host.view.viewport_transform().expect("context transform");
        let origin = host.scene_origin.expect("context scene origin");
        let screen = transform
            .screen_point_for_canvas(CanvasPoint { x: 20.0, y: 20.0 })
            .expect("project Blueprint context probe");
        point(origin.x + px(screen.x), origin.y + px(screen.y))
    });
    cx.simulate_mouse_down(blank, MouseButton::Right, Default::default());
    let jellyflow_capture = cx
        .update(|window, _| window.captured_hitbox())
        .expect("right-pan arbitration owns pointer capture");
    cx.read_entity(&host, |host, _| {
        assert!(host.pending_blank_canvas_context.is_some());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });

    let successor = HitboxId::placeholder();
    assert_ne!(successor, jellyflow_capture);
    cx.update(|window, _| window.capture_pointer(successor));
    draw_once(cx);
    cx.run_until_parked();

    assert!(requests.borrow().is_empty());
    cx.read_entity(&host, |host, _| {
        assert!(host.pending_blank_canvas_context.is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
    cx.update(|window, _| window.release_pointer());
}

#[open_gpui::test]
fn forbidden_endpoint_reconnect_has_zero_side_effects(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, restricted_edge_action_graph());
    draw(cx);
    draw(cx);

    cx.read_entity(&host, |host, _| {
        let instance = current_edge_instance(host, edge);
        assert!(
            host.edge_reconnect_press_target(instance, EdgeEndpoint::From)
                .is_none()
        );
        assert!(
            host.edge_reconnect_press_target(instance, EdgeEndpoint::To)
                .is_some()
        );
        assert!(!host.interaction_owner.has_connection());
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
            let plan = host
                .edge_reconnect_press_target(current_edge_instance(host, edge), EdgeEndpoint::To)
                .expect("reconnect target");
            assert!(
                !host.interaction_owner.has_connection(),
                "pure reconnect resolution must not install native ownership",
            );
            let sample = host
                .viewport_pointer_sample(pointer)
                .expect("reconnect pointer sample");
            let (start, admission) = plan.into_parts();
            let pressed = host
                .interaction_owner
                .press_connection(start, sample, admission)
                .expect("arm reconnect handle")
                .expect("idle reconnect is admitted");
            host.apply_canvas_interaction_update(pressed, window, cx);
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Armed
            );
            assert!(host.interaction_owner.has_connection());

            let moved = host
                .interaction_owner
                .move_to(
                    host.viewport_pointer_sample(point(pointer.x + px(24.0), pointer.y))
                        .expect("moved reconnect pointer sample"),
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
fn busy_edge_reconnect_admission_neither_installs_nor_replaces_native_context(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, restricted_edge_action_graph());
    draw(cx);
    draw(cx);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let sample = host
                .viewport_pointer_sample(point(px(700.0), px(500.0)))
                .expect("pan pointer sample");
            let update = host
                .interaction_owner
                .press(
                    jellyflow_runtime::interaction::CanvasPressTarget::Pan,
                    sample,
                )
                .expect("arm existing pan");
            host.apply_canvas_interaction_update(update, window, cx);
            let generation = host
                .interaction_presentation()
                .generation()
                .expect("pan generation");
            let protected = host.resource_snapshot().protected_instances();
            let focus = host.interaction_owner.has_focus_state();

            assert!(!host.activate_edge_reconnect_action(
                current_edge_instance(host, edge),
                EdgeEndpoint::To,
                window,
                cx,
            ));
            assert!(!host.interaction_owner.has_connection());
            assert_eq!(
                host.interaction_presentation().generation(),
                Some(generation),
            );
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Armed
            );
            assert_eq!(host.resource_snapshot().protected_instances(), protected);
            assert_eq!(host.interaction_owner.has_focus_state(), focus);

            let update = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(update, window, cx));
        });
    });

    let owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        owner,
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    draw(cx);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let (owner_before, mount_before) = host
                .interaction_owner
                .connection_identity()
                .expect("existing native connection context");
            let presentation_before = host.interaction_presentation();
            let phase_before = presentation_before.phase();
            assert_eq!(phase_before, CanvasInteractionPhase::ConnectionDrag);
            let generation_before = presentation_before.generation();
            assert!(host.connection_search_request().is_none());
            let protected_before = host.resource_snapshot().protected_instances();
            let focus_before = host.interaction_owner.has_focus_state();

            assert!(!host.activate_edge_reconnect_action(
                current_edge_instance(host, edge),
                EdgeEndpoint::To,
                window,
                cx,
            ));

            let (owner_after, mount_after) = host
                .interaction_owner
                .connection_identity()
                .expect("busy admission preserves existing context");
            assert_eq!(owner_after, owner_before);
            assert!(mount_after.is_same_mount(&mount_before));
            assert_eq!(host.interaction_presentation().phase(), phase_before);
            assert_eq!(
                host.interaction_presentation().generation(),
                generation_before,
            );
            assert!(host.connection_search_request().is_none());
            assert_eq!(
                host.resource_snapshot().protected_instances(),
                protected_before
            );
            assert_eq!(host.interaction_owner.has_focus_state(), focus_before);
            let update = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(update, window, cx));
        });
    });
}

#[open_gpui::test]
fn removing_the_authoritative_edge_retires_an_active_reconnect(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    cx.update_window_entity(&host, |host, window, cx| {
        assert!(host.activate_edge_reconnect_action(
            current_edge_instance(host, edge),
            EdgeEndpoint::To,
            window,
            cx,
        ));
    });
    draw(cx);
    cx.update_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert_eq!(
            host.connection_preview_plan()
                .and_then(|preview| preview.suppressed_edge)
                .map(EdgeInstanceKey::edge),
            Some(edge),
        );
    });

    let version = runtime
        .snapshot()
        .expect("snapshot before disconnect")
        .version();
    runtime
        .execute(GraphCommand::disconnect(version, edge))
        .into_result()
        .expect("external edge removal")
        .applied()
        .expect("disconnect applies");
    cx.run_until_parked();
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            presentation.terminal(),
            Some(InteractionTerminalReason::OwnerRetired)
        );
        assert!(presentation.connection().is_none());
        assert!(!host.interaction_owner.has_connection());
        assert!(host.connection_preview_plan().is_none());
    });
}

#[open_gpui::test]
fn binding_presentation_projects_real_node_and_port_focus(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let (root_focus, binding) = cx.read_entity(&host, |host, _| {
        let binding = host
            .retained_nodes
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
fn dynamic_port_reconciliation_preserves_order_index_and_retained_capabilities(
    cx: &mut open_gpui::TestAppContext,
) {
    const INITIAL_PORT_COUNT: usize = 16;
    const REPLACEMENT_PORT_COUNT: usize = 8;

    let node = NodeId::from_u128(80_000);
    let initial_ports = (0..INITIAL_PORT_COUNT)
        .map(|index| PortId::from_u128(90_000 + index as u128))
        .collect::<Vec<_>>();
    let runtime = GraphRuntime::builder(portless_catalog())
        .build(dynamic_port_graph(node, &initial_ports))
        .expect("valid many-port graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach many-port view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&catalog()).expect("valid many-port registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let binding = cx.read_entity(&host, |host, _| {
        host.retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == node)
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("many-port binding")
    });
    let retained = initial_ports[5];
    let removed = initial_ports[0];
    let (retained_focus, retained_handle) = cx.read_entity(&binding, |binding, _| {
        let authoring = binding.authoring().expect("initial authoring");
        assert_eq!(
            authoring.ports().map(|port| port.id()).collect::<Vec<_>>(),
            initial_ports
        );
        for id in initial_ports.iter().rev().copied() {
            assert_eq!(authoring.port(id).map(|port| port.id()), Some(id));
        }
        (
            binding
                .port_focus_handle(retained)
                .expect("retained focus")
                .clone(),
            binding
                .cached_port_handle(retained)
                .expect("retained handle"),
        )
    });

    let mut replacement_ports = initial_ports[4..10]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>();
    replacement_ports.extend([PortId::from_u128(100_000), PortId::from_u128(100_001)]);
    assert_eq!(replacement_ports.len(), REPLACEMENT_PORT_COUNT);
    let version = runtime
        .snapshot()
        .expect("snapshot before topology")
        .version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().replace_node_ports(
                node,
                replacement_ports
                    .iter()
                    .copied()
                    .map(|id| TopologyPort::new(id, dynamic_port(node, id))),
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("replace many-port topology")
        .applied()
        .expect("many-port topology applies");
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&binding, |binding, _| {
        let authoring = binding.authoring().expect("replacement authoring");
        assert_eq!(
            authoring.ports().map(|port| port.id()).collect::<Vec<_>>(),
            replacement_ports
        );
        for id in replacement_ports.iter().rev().copied() {
            assert_eq!(authoring.port(id).map(|port| port.id()), Some(id));
        }
        assert!(authoring.port(removed).is_none());
        assert_eq!(
            binding.port_focus_handle(retained),
            Some(&retained_focus),
            "reordering a retained port must preserve its native focus identity"
        );
        assert!(
            binding
                .cached_port_handle(retained)
                .expect("retained handle after topology")
                .shares_core_with(&retained_handle),
            "reordering a retained port must preserve its stable handle core"
        );
    });

    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    cx.read_entity(&binding, |binding, _| {
        let authoring = binding.authoring().expect("stable authoring");
        assert_eq!(authoring.ports().len(), REPLACEMENT_PORT_COUNT);
        assert!(authoring.port(retained).is_some());
        assert_eq!(binding.port_focus_handle(retained), Some(&retained_focus));
        assert!(
            binding
                .cached_port_handle(retained)
                .expect("stable retained handle")
                .shares_core_with(&retained_handle)
        );
    });
}

#[open_gpui::test]
fn coalesced_port_reincarnation_replaces_focus_and_drops_retired_hover(
    cx: &mut open_gpui::TestAppContext,
) {
    let node = NodeId::from_u128(80_100);
    let port = PortId::from_u128(90_100);
    let runtime = GraphRuntime::builder(portless_catalog())
        .build(dynamic_port_graph(node, &[port]))
        .expect("valid single-port graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach single-port view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&catalog())
                .expect("valid single-port registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);
    let binding = cx.read_entity(&host, |host, _| {
        host.retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == node)
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("single-port binding")
    });
    let (retired_instance, retired_focus) = cx.read_entity(&binding, |binding, _| {
        (
            binding.port_instance(port).expect("retired port identity"),
            binding
                .port_focus_handle(port)
                .expect("retired port focus")
                .clone(),
        )
    });
    let node_instance = cx.read_entity(&host, |host, _| {
        host.retained_nodes
            .retained_instance(node)
            .cloned()
            .expect("retained node identity")
    });
    cx.update_entity(&host, |host, _| {
        assert!(
            host.retained_nodes
                .set_hover(Some(node_instance), Some(retired_instance))
        );
    });
    let removed = runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("snapshot before removal")
                .version(),
            TopologyEdit::new().replace_node_ports(
                node,
                std::iter::empty::<TopologyPort>(),
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("remove port");
    let removed_version = removed.applied().expect("port removal applies").version();
    let restored = runtime
        .execute(GraphCommand::undo(removed_version))
        .into_result()
        .expect("restore port");
    let restored_revision = restored
        .applied()
        .expect("port restoration applies")
        .revision();

    cx.update_entity(&binding, |binding, cx| {
        binding.advance(restored_revision, cx)
    })
    .expect("advance binding to restored port incarnation");
    cx.read_entity(&binding, |binding, _| {
        let successor_instance = binding
            .port_instance(port)
            .expect("successor port identity");
        let successor_focus = binding
            .port_focus_handle(port)
            .expect("successor port focus");
        assert_ne!(retired_instance, successor_instance);
        assert_ne!(&retired_focus, successor_focus);
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.retained_nodes.hovered_port().is_none());
    });
}

#[open_gpui::test]
fn max_capacity_node_drag_builds_one_index_with_equivalent_offsets(
    cx: &mut open_gpui::TestAppContext,
) {
    const DRAGGED_NODE_COUNT: usize = jellyflow_runtime::interaction::MAX_NODE_MOVE_COHORT_SIZE;
    let nodes = (0..DRAGGED_NODE_COUNT)
        .map(|index| NodeId::from_u128(110_000 + index as u128))
        .collect::<Vec<_>>();
    let runtime = GraphRuntime::builder(portless_catalog())
        .build(portless_nodes_graph(&nodes))
        .expect("valid maximum-capacity graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach maximum-capacity view");
    set_selection_by_id(&view, nodes.clone(), Vec::new()).expect("select every draggable node");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&catalog())
                .expect("valid maximum-capacity registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let clicked = nodes[0];
        let start = node_drag_start_by_id(
            &host.view,
            clicked,
            jellyflow_runtime::interaction::SelectionMode::Replace,
        )
        .expect("drag admission")
        .expect("selected node is draggable");
        let start_sample = host
            .view
            .viewport_pointer_sample(ViewPoint::new(10.0, 10.0))
            .expect("finite start sample");
        host.interaction_owner
            .press(
                jellyflow_runtime::interaction::CanvasPressTarget::NodeDrag(start),
                start_sample,
            )
            .expect("arm node drag");
        let end_sample = host
            .view
            .viewport_pointer_sample(ViewPoint::new(30.0, 34.0))
            .expect("finite end sample");
        host.interaction_owner
            .move_to(end_sample)
            .expect("activate node drag");
        let presentation = host.interaction_owner.presentation();
        let drag = presentation.node_drag().expect("active multi-node drag");
        assert_eq!(drag.nodes(), nodes);
        let expected_delta = drag.delta();

        let offsets = host.interaction_node_drag_offsets();
        assert_eq!(offsets.len(), DRAGGED_NODE_COUNT);
        for node in &nodes {
            assert_eq!(offsets.get(node), Some(&expected_delta));
        }
    });
}

#[open_gpui::test]
fn node_drag_cohort_materializes_never_mounted_and_parked_peers_until_applied_frame(
    cx: &mut open_gpui::TestAppContext,
) {
    let visible = NodeId::from_u128(120_000);
    let offscreen = NodeId::from_u128(120_001);
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x81_001));
    graph.insert_node(visible, portless_node(CanvasPoint { x: 40.0, y: 40.0 }));
    graph.insert_node(
        offscreen,
        portless_node(CanvasPoint {
            x: 4_000.0,
            y: 40.0,
        }),
    );
    let runtime = GraphRuntime::builder(portless_catalog())
        .build(graph.build().expect("valid culled-peer graph"))
        .expect("valid culled-peer runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach culled-peer view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry
                .seal(&catalog())
                .expect("valid culled-peer registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
        .with_navigation_config(
            crate::navigation::OpenGpuiNavigationConfig::default()
                .with_alignment_snap_tolerance(None),
        )
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert!(host.retained_nodes.retained_instance(offscreen).is_none());
    });

    let arm = |host: &mut OpenGpuiSceneHost| {
        set_selection_by_id(&host.view, vec![visible, offscreen], Vec::new())
            .expect("select complete drag cohort");
        let start = node_drag_start_by_id(
            &host.view,
            visible,
            jellyflow_runtime::interaction::SelectionMode::Replace,
        )
        .expect("drag admission")
        .expect("two-node drag is admitted");
        let sample = host
            .view
            .viewport_pointer_sample(ViewPoint::new(10.0, 10.0))
            .expect("finite start sample");
        host.interaction_owner
            .press(
                jellyflow_runtime::interaction::CanvasPressTarget::NodeDrag(start),
                sample,
            )
            .expect("arm two-node drag");
    };

    cx.update_entity(&host, |host, cx| {
        arm(host);
        cx.notify();
    });
    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 2);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert!(host.retained_nodes.retained_instance(offscreen).is_some());
        assert!(host.interaction_owner.has_node_drag_residency());
    });

    cx.update_entity(&host, |host, cx| {
        host.interaction_owner
            .cancel()
            .expect("cancel protected node drag");
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        arm(host);
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 2);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert!(host.interaction_owner.has_node_drag_residency());
    });

    let (ticket, submission) = cx.update_entity(&host, |host, _| {
        let end = host
            .view
            .viewport_pointer_sample(ViewPoint::new(40.0, 24.0))
            .expect("finite end sample");
        host.interaction_owner
            .move_to(end)
            .expect("activate two-node drag");
        host.interaction_owner
            .release(end)
            .expect("release two-node drag")
            .into_dispatch()
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                jellyflow_runtime::interaction::CanvasInteractionEffect::SubmitGraph {
                    submission,
                } => Some((submission.ticket(), submission)),
                _ => None,
            })
            .expect("node drag submits one graph mutation")
    });
    let command = cx
        .update_entity(&host, |host, _| {
            host.interaction_owner.bind_submission(submission)
        })
        .expect("fresh drag cohort")
        .expect("non-zero drag mutation");
    let completion = runtime.execute(command);
    let applied_revision = completion.applied_revision().expect("node drag applies");
    let visible_lifetime = runtime
        .snapshot()
        .expect("applied snapshot")
        .node_lifetime_key(visible)
        .expect("visible node lifetime");

    cx.update_entity(&host, |host, cx| {
        host.interaction_owner
            .settle_test_evidence(
                ticket,
                jellyflow_runtime::interaction::InteractionSubmissionSettlementEvidence::Command(
                    completion,
                ),
            )
            .expect("settle node drag receipt");
        assert!(host.interaction_owner.has_node_drag_residency());
        assert!(
            !host
                .interaction_owner
                .observe_materialized_frame(applied_revision, [visible_lifetime])
        );
        assert!(host.interaction_owner.has_node_drag_residency());
        cx.notify();
    });
    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        let resources = host.resource_snapshot();
        assert_eq!(resources.active_nodes() + resources.parked_nodes(), 2);
        assert!(host.retained_nodes.retained_instance(offscreen).is_some());
        assert!(host.interaction_owner.has_node_drag_residency());
        assert!(
            host.interaction_owner
                .node_drag_route_handoff_token()
                .is_some()
        );
    });

    assert!(cx.simulate_frame(RequestFrameOptions {
        require_presentation: true,
        ..Default::default()
    }));
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.has_node_drag_residency());
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
    });
}

#[open_gpui::test]
fn binding_presentation_tracks_real_port_hit_bounds_hover(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let binding = cx.read_entity(&host, |host, _| {
        host.retained_nodes
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
fn settled_routed_scene_remains_quiescent_across_an_explicit_redraw(
    cx: &mut open_gpui::TestAppContext,
) {
    let selected = NodeId::from_u128(10);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);

    let (baseline_nodes, baseline_resources, baseline_layout_revision) =
        cx.read_entity(&host, |host, _| {
            assert_settled_host_has_no_pending_work(host);
            let nodes = host
                .retained_nodes
                .active_in_order()
                .map(|(instance, _)| instance.node())
                .collect::<Vec<_>>();
            (
                nodes,
                host.resource_snapshot(),
                host.view
                    .geometry_snapshot()
                    .expect("settled routed geometry")
                    .layout_revision(),
            )
        });
    let baseline_graph_revision = runtime.snapshot().expect("settled routed graph").revision();

    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_settled_host_has_no_pending_work(host);
        assert_eq!(
            host.view
                .geometry_snapshot()
                .expect("quiescent routed geometry")
                .layout_revision(),
            baseline_layout_revision,
            "an explicit redraw must not publish new geometry",
        );
        assert_eq!(host.resource_snapshot(), baseline_resources);
    });
    assert_eq!(
        runtime
            .snapshot()
            .expect("graph after quiescence")
            .revision(),
        baseline_graph_revision,
    );

    cx.read_entity(&host, |host, _| {
        set_selection_by_id(&host.view, vec![selected], Vec::new())
            .expect("change frame-local selection rank");
    });
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&host, |host, _| {
        let nodes = host
            .retained_nodes
            .active_in_order()
            .map(|(instance, _)| instance.node())
            .collect::<Vec<_>>();
        assert_eq!(nodes.last().copied(), Some(selected));
        assert_eq!(nodes.len(), baseline_nodes.len());
        assert!(baseline_nodes.iter().all(|node| nodes.contains(node)));
        assert_eq!(host.resource_snapshot(), baseline_resources);
    });
}

#[open_gpui::test]
fn overview_node_hover_uses_the_published_lod_footprint(cx: &mut open_gpui::TestAppContext) {
    let source = NodeId::from_u128(10);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    cx.read_entity(&host, |host, _| {
        host.view
            .set_viewport(ViewportTransform::new(CanvasPoint::default(), 0.5).unwrap())
            .expect("set overview viewport");
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let (pointer, binding) = cx.read_entity(&host, |host, _| {
        let transform = host.scene_transform.expect("overview scene transform");
        let geometry = host.view.geometry_snapshot().expect("overview geometry");
        let published = geometry
            .node(source)
            .expect("published overview node")
            .bounds();
        let mounted = host
            .retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == source)
            .map(|(_, mounted)| mounted)
            .expect("mounted overview node");
        assert!(published.size.height > mounted.canonical_bounds().size.height);

        let canvas = CanvasPoint {
            x: published.origin.x + published.size.width * 0.5,
            y: published.origin.y + published.size.height - 2.0 / transform.zoom,
        };
        assert!(test_canvas_rect_contains(published, canvas));
        assert!(
            !test_canvas_rect_contains(mounted.canonical_bounds(), canvas),
            "the probe must exercise the LOD-only part of the visible node",
        );
        let screen = transform
            .screen_point_for_canvas(canvas)
            .expect("project overview hover probe");
        let origin = host.scene_origin.expect("measured scene origin");
        let binding = mounted.binding.clone().expect("source node binding");
        (
            point(origin.x + px(screen.x), origin.y + px(screen.y)),
            binding,
        )
    });

    cx.simulate_mouse_move(pointer, None, Default::default());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.retained_nodes.hovered_node().map(|node| node.node()),
            Some(source),
        );
        assert_eq!(host.retained_nodes.hovered_port(), None);
    });
    cx.read_entity(&binding, |binding, _| {
        assert!(binding.presentation().node().hovered());
    });
}

#[open_gpui::test]
fn frontmost_node_hover_clears_a_covered_node_port(cx: &mut open_gpui::TestAppContext) {
    let source = NodeId::from_u128(10);
    let back = NodeId::from_u128(11);
    let front = NodeId::from_u128(12);
    let graph = overlapping_hover_graph();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid overlapping hover graph");
    let view = runtime
        .attach_view(ViewSessionOptions::with_seed(
            ViewSessionSeed::new(graph_id).with_stacking(ViewStackingSeed::new(
                vec![source, back, front],
                Vec::new(),
                Vec::new(),
            )),
        ))
        .expect("attach explicitly stacked hover view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: Rc::new(Cell::new(0)),
        })
    });
    let captured_host = Rc::new(std::cell::RefCell::new(None));
    let scene_host = Rc::clone(&captured_host);
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.retained_nodes
                .active_in_order()
                .last()
                .map(|(instance, _)| instance.node()),
            Some(front),
        );
    });
    let covered_port = cx
        .debug_bounds("jellyflow-test-reconnect-input-port")
        .expect("covered back-node port bounds");

    cx.simulate_mouse_move(covered_port.center(), None, Default::default());
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.retained_nodes.hovered_node().map(|node| node.node()),
            Some(front),
        );
        assert_eq!(
            host.retained_nodes.hovered_port(),
            None,
            "a covered back-node port must not be combined with the frontmost node",
        );
    });
}

#[open_gpui::test]
fn detail_edge_hover_passes_through_canonical_node_blank_space(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    cx.read_entity(&host, |host, _| {
        host.view
            .set_viewport(ViewportTransform::new(CanvasPoint::default(), 2.0).unwrap())
            .expect("set detail viewport");
    });
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let pointer = cx.read_entity(&host, |host, _| {
        let transform = host.scene_transform.expect("detail scene transform");
        let geometry = host.view.geometry_snapshot().expect("detail geometry");
        let canonical_bounds = host
            .retained_nodes
            .active_in_order()
            .map(|(_, mounted)| mounted.canonical_bounds())
            .collect::<Vec<_>>();
        assert!(
            host.retained_nodes
                .active_in_order()
                .any(|(instance, mounted)| {
                    geometry.node(instance.node()).is_some_and(|published| {
                        published.bounds().size.width < mounted.canonical_bounds().size.width
                            && published.bounds().size.height
                                < mounted.canonical_bounds().size.height
                    })
                })
        );
        let path = &geometry
            .route(edge)
            .expect("published routed edge")
            .facts()
            .path;
        let canvas = (1..100)
            .find_map(|step| {
                let sample = path
                    .sample(jellyflow_runtime::geometry::EdgePathAnchor::Progress(
                        step as f32 / 100.0,
                    ))
                    .ok()?;
                let point = sample.point;
                let inside_canonical = canonical_bounds
                    .iter()
                    .copied()
                    .any(|bounds| test_canvas_rect_contains(bounds, point));
                let inside_published = geometry
                    .nodes()
                    .values()
                    .any(|node| test_canvas_rect_contains(node.bounds(), point));
                let inside_port = geometry.ports().values().any(|port| {
                    port.hit_bounds()
                        .is_some_and(|bounds| test_canvas_rect_contains(bounds, point))
                });
                (inside_canonical && !inside_published && !inside_port).then_some(point)
            })
            .expect("edge crosses canonical-only node space at detail zoom");
        let screen = transform
            .screen_point_for_canvas(canvas)
            .expect("project detail edge probe");
        let origin = host.scene_origin.expect("measured scene origin");
        point(origin.x + px(screen.x), origin.y + px(screen.y))
    });

    cx.simulate_mouse_move(pointer, None, Default::default());
    draw(cx);
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.retained_nodes.hovered_node(), None);
        assert_eq!(host.retained_nodes.hovered_port(), None);
        assert_eq!(
            host.interaction_presentation().hovered_edge(),
            host.view.edge_instance_key(edge).unwrap()
        );
    });
}

#[open_gpui::test]
fn measured_port_wrapper_tracks_the_same_hover_presentation(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let binding = cx.read_entity(&host, |host, _| {
        host.retained_nodes
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
fn edge_semantic_bounds_are_screen_stable_and_pointer_inert(cx: &mut open_gpui::TestAppContext) {
    let edge = EdgeId::from_u128(30);
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, routed_graph());
    draw(cx);
    draw(cx);
    let selector = open_gpui_edge_semantic_selector(edge);

    let mut pointer = None;
    for zoom in [0.5, 1.0, 2.0] {
        cx.read_entity(&host, |host, _| {
            host.view
                .set_viewport(ViewportTransform::new(CanvasPoint::default(), zoom).unwrap())
                .expect("set semantic-bound viewport");
        });
        cx.run_until_parked();
        draw(cx);
        draw(cx);

        let expected_center = cx.read_entity(&host, |host, _| {
            let geometry = host.view.geometry_snapshot().expect("edge geometry");
            let sample = geometry
                .route(edge)
                .expect("published edge")
                .facts()
                .path
                .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
                .expect("edge center");
            let projected = host
                .view
                .viewport_transform()
                .expect("viewport transform")
                .screen_point_for_canvas(sample.point)
                .expect("project edge center");
            let origin = host.scene_origin.expect("scene origin");
            point(origin.x + px(projected.x), origin.y + px(projected.y))
        });
        let bounds = cx.debug_bounds(&selector).expect("edge semantic bounds");
        assert_eq!(bounds.size.width, px(OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW));
        assert_eq!(bounds.size.height, px(OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW));
        assert!((bounds.center().x - expected_center.x).abs() <= px(0.5));
        assert!((bounds.center().y - expected_center.y).abs() <= px(0.5));
        pointer = Some(bounds.center());
    }

    let pointer = pointer.expect("last semantic center");
    cx.simulate_mouse_down(pointer, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .selection()
                .expect("runtime edge-pick selection")
                .edges(),
            [edge],
            "the semantic rectangle must not intercept runtime edge picking",
        );
    });
}

#[open_gpui::test]
fn active_scene_gestures_keep_the_window_cursor_over_port_hitboxes(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_interactive_gesture_scene(cx);
    cx.set_platform_hovered_window(Some(cx.window_handle()));
    draw(cx);
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output port bounds");
    cx.simulate_mouse_move(output.center(), None, Default::default());
    draw(cx);

    let owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        owner,
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    draw(cx);

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
    });
    assert_eq!(cx.platform_cursor_style(), CursorStyle::DragLink);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let cancelled = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(cancelled, window, cx));
        });
    });
    draw(cx);
    cx.simulate_mouse_move(output.center(), None, Default::default());
    draw(cx);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let start = host
                .viewport_pointer_sample(output.center())
                .expect("pan start sample");
            let pressed = host
                .interaction_owner
                .press(
                    jellyflow_runtime::interaction::CanvasPressTarget::Pan,
                    start,
                )
                .expect("arm pan");
            host.apply_canvas_interaction_update(pressed, window, cx);
            host.interaction_owner
                .move_to(
                    host.viewport_pointer_sample(point(
                        output.center().x + px(24.0),
                        output.center().y,
                    ))
                    .expect("pan move sample"),
                )
                .expect("promote pan without applying its viewport delta");
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Pan
            );
        });
    });
    draw(cx);

    assert_eq!(cx.platform_cursor_style(), CursorStyle::ClosedHand);
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

    let edge_instance = cx.read_entity(&host, |host, _| {
        host.view
            .geometry_snapshot()
            .expect("edge geometry")
            .route(edge)
            .expect("published edge")
            .instance()
    });
    let mut edge_focus = None;
    for _ in 0..16 {
        cx.simulate_keystrokes("tab");
        let (focused_after, target_after) = cx.update(|window, cx| {
            (
                window.focused(cx),
                host.read(cx)
                    .interaction_owner
                    .exact_focused_graph_target(window),
            )
        });
        if target_after == Some(OpenGpuiGraphFocusTarget::Edge(edge_instance)) {
            edge_focus = focused_after;
            break;
        }
    }
    let edge_focus = edge_focus.expect("edge focus after nodes and ports");

    simulate_key_press(cx, "enter");
    cx.run_until_parked();
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view.selection().expect("edge selection").edges(),
            [edge]
        );
    });
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(edge_focus.clone())));

    simulate_key_down(cx, "delete", false);
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
    cx.update(|window, cx| {
        assert_ne!(window.focused(cx), Some(edge_focus));
        assert!(matches!(
            host.read(cx)
                .interaction_owner
                .exact_focused_graph_target(window),
            Some(OpenGpuiGraphFocusTarget::Port(_))
        ));
    });
}

#[open_gpui::test]
fn tab_advances_focus_then_reveals_the_focused_node_without_changing_zoom(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));
    let offscreen_pan = CanvasPoint {
        x: 5_000.0,
        y: 5_000.0,
    };
    cx.read_entity(&host, |host, _| {
        host.view
            .set_viewport(ViewportTransform::new(offscreen_pan, 2.0).expect("valid viewport"))
            .expect("move viewport away from materialized nodes");
    });

    let mut revealed = None;
    for _ in 0..16 {
        let focused_before = cx.update(|window, cx| window.focused(cx));
        cx.simulate_keystrokes("tab");
        let (focused_after, target, transform) = cx.update(|window, cx| {
            (
                window.focused(cx),
                host.read(cx)
                    .interaction_owner
                    .exact_focused_graph_target(window),
                host.read(cx)
                    .view
                    .viewport_transform()
                    .expect("revealed viewport"),
            )
        });
        assert_ne!(
            focused_after, focused_before,
            "Tab reveal must not consume focus traversal",
        );
        if matches!(target, Some(OpenGpuiGraphFocusTarget::Node { .. })) {
            revealed = Some(transform);
            break;
        }
    }

    let revealed = revealed.expect("Tab traversal reaches an exact node target");
    assert_eq!(revealed.zoom, 2.0, "node reveal must preserve zoom exactly");
    assert_ne!(
        revealed.pan, offscreen_pan,
        "an offscreen focused node must minimally pan into view",
    );
}

#[open_gpui::test]
fn graph_root_delete_removes_mixed_selection_as_one_command(cx: &mut open_gpui::TestAppContext) {
    assert_graph_root_selection_delete(cx, "delete");
}

#[open_gpui::test]
fn graph_root_backspace_removes_mixed_selection_as_one_command(cx: &mut open_gpui::TestAppContext) {
    assert_graph_root_selection_delete(cx, "backspace");
}

#[open_gpui::test]
fn focused_node_delete_removes_mixed_selection_as_one_command(cx: &mut open_gpui::TestAppContext) {
    assert_exact_graph_focus_selection_delete(cx, ExactDeleteFocus::Node);
}

#[open_gpui::test]
fn focused_port_delete_removes_mixed_selection_as_one_command(cx: &mut open_gpui::TestAppContext) {
    assert_exact_graph_focus_selection_delete(cx, ExactDeleteFocus::Port);
}

#[open_gpui::test]
fn focused_edge_delete_removes_mixed_selection_as_one_command(cx: &mut open_gpui::TestAppContext) {
    assert_exact_graph_focus_selection_delete(cx, ExactDeleteFocus::Edge);
}

#[open_gpui::test]
fn focused_reconnect_delete_removes_mixed_selection_as_one_command(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_exact_graph_focus_selection_delete(cx, ExactDeleteFocus::Reconnect);
}

#[open_gpui::test]
fn native_text_control_descendant_focus_ignores_graph_delete_keys(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, control_focus, command_labels, cx) = mount_selection_delete_scene(cx);
    draw(cx);
    draw(cx);
    let control_focus = control_focus
        .borrow()
        .clone()
        .expect("mounted native text control focus");
    cx.update(|window, cx| window.focus(&control_focus, cx));
    let before = runtime
        .snapshot()
        .expect("graph before descendant delete keys");
    let history_before = runtime
        .history_state()
        .expect("history before descendant delete keys");

    for key in ["delete", "backspace"] {
        simulate_key_down(cx, key, false);
        cx.run_until_parked();
        draw(cx);
    }

    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));
    assert!(command_labels.borrow().is_empty());
    assert!(
        before.ptr_eq(
            &runtime
                .snapshot()
                .expect("graph after descendant delete keys")
        )
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after descendant delete keys"),
        history_before,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .selection()
                .expect("preserved mixed selection")
                .nodes(),
            [NodeId::from_u128(10)],
        );
    });
}

#[open_gpui::test]
fn graph_root_arrows_nudge_the_selected_cohort_once_and_shift_accelerates(
    cx: &mut open_gpui::TestAppContext,
) {
    let nodes = [NodeId::from_u128(10), NodeId::from_u128(11)];
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    cx.read_entity(&host, |host, _| {
        set_selection_by_id(&host.view, nodes, Vec::new()).expect("seed nudge selection");
    });
    draw(cx);
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));
    let before = runtime.snapshot().expect("graph before nudge");
    let positions_before = nodes.map(|node| before.graph().node(&node).expect("selected node").pos);
    let history_before = runtime.history_state().expect("history before nudge");

    simulate_key_down(cx, "right", false);
    for _ in 0..4 {
        draw(cx);
    }
    let after_regular = runtime.snapshot().expect("graph after regular nudge");
    for (index, node) in nodes.into_iter().enumerate() {
        let position = after_regular.graph().node(&node).expect("nudged node").pos;
        assert_eq!(position.x, positions_before[index].x + 10.0);
        assert_eq!(position.y, positions_before[index].y);
    }
    assert_eq!(
        runtime
            .history_state()
            .expect("history after regular nudge")
            .undo_len(),
        history_before.undo_len() + 1,
    );

    simulate_key_down(cx, "shift-down", false);
    for _ in 0..4 {
        draw(cx);
    }
    let after_accelerated = runtime.snapshot().expect("graph after accelerated nudge");
    for (index, node) in nodes.into_iter().enumerate() {
        let position = after_accelerated
            .graph()
            .node(&node)
            .expect("accelerated node")
            .pos;
        assert_eq!(position.x, positions_before[index].x + 10.0);
        assert_eq!(position.y, positions_before[index].y + 50.0);
    }
    assert_eq!(
        runtime
            .history_state()
            .expect("history after accelerated nudge")
            .undo_len(),
        history_before.undo_len() + 2,
    );
}

#[open_gpui::test]
fn native_text_control_descendant_focus_ignores_graph_nudge_keys(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, control_focus, command_labels, cx) = mount_selection_delete_scene(cx);
    draw(cx);
    draw(cx);
    let control_focus = control_focus
        .borrow()
        .clone()
        .expect("mounted native text control focus");
    cx.update(|window, cx| window.focus(&control_focus, cx));
    let before = runtime
        .snapshot()
        .expect("graph before descendant nudge keys");
    let history_before = runtime
        .history_state()
        .expect("history before descendant nudge keys");

    for key in ["right", "shift-down"] {
        simulate_key_down(cx, key, false);
        cx.run_until_parked();
        draw(cx);
    }

    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(control_focus)));
    assert!(command_labels.borrow().is_empty());
    assert!(
        before.ptr_eq(
            &runtime
                .snapshot()
                .expect("graph after descendant nudge keys")
        )
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after descendant nudge keys"),
        history_before,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn graph_root_repeated_key_and_active_session_do_not_submit_selection_delete(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, _control_focus, command_labels, cx) = mount_selection_delete_scene(cx);
    draw(cx);
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));
    let before = runtime
        .snapshot()
        .expect("graph before ignored delete keys");
    let history_before = runtime
        .history_state()
        .expect("history before ignored delete keys");

    for key in ["delete", "backspace"] {
        simulate_key_down(cx, key, true);
    }
    cx.run_until_parked();
    assert!(command_labels.borrow().is_empty());

    let blank = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(blank, MouseButton::Left, Default::default());
    cx.read_entity(&host, |host, _| {
        assert_ne!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
    });
    simulate_key_down(cx, "delete", false);
    assert!(command_labels.borrow().is_empty());
    cx.simulate_mouse_up(blank, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);

    assert!(command_labels.borrow().is_empty());
    assert!(before.ptr_eq(&runtime.snapshot().expect("graph after ignored delete keys")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after ignored delete keys"),
        history_before,
    );
}

fn assert_graph_root_selection_delete(cx: &mut open_gpui::TestAppContext, key: &str) {
    let (runtime, host, _control_focus, command_labels, cx) = mount_selection_delete_scene(cx);
    draw(cx);
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));
    let before = runtime.snapshot().expect("graph before selection delete");
    let history_before = runtime
        .history_state()
        .expect("history before selection delete");

    simulate_key_down(cx, key, false);
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    assert_eq!(
        command_labels.borrow().as_slice(),
        [Some("Delete selection".to_owned())],
        "one graph-root key press must enqueue one selection topology command",
    );
    let deleted = runtime.snapshot().expect("graph after selection delete");
    assert_eq!(deleted.revision(), before.revision() + 1);
    assert!(deleted.graph().node(&NodeId::from_u128(10)).is_none());
    assert!(deleted.graph().edge(&EdgeId::from_u128(30)).is_none());
    assert!(deleted.graph().node(&NodeId::from_u128(11)).is_some());
    assert_eq!(
        runtime
            .history_state()
            .expect("history after selection delete")
            .undo_len(),
        history_before.undo_len() + 1,
    );
    assert_eq!(
        cx.read_entity(&host, |host, _| host.interaction_presentation().phase()),
        CanvasInteractionPhase::Idle,
    );

    runtime
        .execute(GraphCommand::undo(deleted.version()))
        .into_result()
        .expect("undo selection delete")
        .applied()
        .expect("selection delete undo applies");
    let restored = runtime
        .snapshot()
        .expect("graph after selection-delete undo");
    assert!(restored.graph().node(&NodeId::from_u128(10)).is_some());
    assert!(restored.graph().edge(&EdgeId::from_u128(30)).is_some());
}

#[derive(Debug, Clone, Copy)]
enum ExactDeleteFocus {
    Node,
    Port,
    Edge,
    Reconnect,
}

fn assert_exact_graph_focus_selection_delete(
    cx: &mut open_gpui::TestAppContext,
    target: ExactDeleteFocus,
) {
    let source = NodeId::from_u128(10);
    let port = PortId::from_u128(20);
    let edge = EdgeId::from_u128(30);
    let graph = if matches!(target, ExactDeleteFocus::Reconnect) {
        reconnect_graph()
    } else {
        routed_graph()
    };
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, graph);
    cx.read_entity(&host, |host, _| {
        set_selection_by_id(&host.view, vec![source], vec![edge])
            .expect("seed exact-focus mixed selection");
    });
    draw(cx);
    draw(cx);

    let focus = cx.update_window_entity(&host, |host, _window, cx| match target {
        ExactDeleteFocus::Node => host
            .retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == source)
            .map(|(_, mounted)| mounted.focus.clone())
            .expect("source node focus"),
        ExactDeleteFocus::Port => {
            let binding = host
                .retained_nodes
                .active_in_order()
                .find(|(instance, _)| instance.node() == source)
                .and_then(|(_, mounted)| mounted.binding.clone())
                .expect("source node binding");
            binding
                .read(cx)
                .port_focus_handle(port)
                .cloned()
                .expect("source port focus")
        }
        ExactDeleteFocus::Edge => {
            let instance = current_edge_instance(host, edge);
            host.interaction_owner
                .graph_focus_action_handle(OpenGpuiGraphFocusTarget::Edge(instance), cx)
        }
        ExactDeleteFocus::Reconnect => {
            let instance = current_edge_instance(host, edge);
            host.interaction_owner.graph_focus_action_handle(
                OpenGpuiGraphFocusTarget::reconnect(instance, EdgeEndpoint::To),
                cx,
            )
        }
    });
    cx.update(|window, cx| window.focus(&focus, cx));
    let before = runtime.snapshot().expect("graph before exact-focus delete");
    let history_before = runtime
        .history_state()
        .expect("history before exact-focus delete");

    simulate_key_down(cx, "delete", false);
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let deleted = runtime.snapshot().expect("graph after exact-focus delete");
    assert_eq!(deleted.revision(), before.revision() + 1, "{target:?}");
    assert!(deleted.graph().node(&source).is_none(), "{target:?}");
    assert!(deleted.graph().edge(&edge).is_none(), "{target:?}");
    assert!(
        deleted.graph().node(&NodeId::from_u128(11)).is_some(),
        "{target:?}",
    );
    assert_eq!(
        runtime
            .history_state()
            .expect("history after exact-focus delete")
            .undo_len(),
        history_before.undo_len() + 1,
        "{target:?}",
    );
}

#[open_gpui::test]
fn space_plus_left_drag_pans_only_while_the_graph_root_owns_focus(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let root_focus = cx.read_entity(&host, |host, _| {
        host.interaction_owner.focus_handle().expect("canvas focus")
    });
    cx.update(|window, cx| window.focus(&root_focus, cx));
    simulate_key_down(cx, "space", false);
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_owner.space_pan_held())
    });

    let before = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("viewport before Space pan")
    });
    let start = point(px(10.0), px(20.0));
    let end = point(px(90.0), px(70.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    let after = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("viewport after Space pan")
    });
    assert_ne!(after.pan, before.pan);

    simulate_key_up(cx, "space");
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.space_pan_held())
    });

    simulate_key_down(cx, "space", false);
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_owner.space_pan_held())
    });
    cx.update(|window, _| window.blur());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.space_pan_held())
    });
}

#[open_gpui::test]
fn explicit_node_drag_and_dify_wheel_pan_and_pinch_zoom_stay_view_local(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
            .with_navigation_config(OpenGpuiNavigationConfig::for_preset(
                OpenGpuiInputPreset::Dify,
            ))
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
    let transform_after_wheel = cx.read_entity(&host, |host, _| {
        host.scene_transform.expect("wheel-panned transform")
    });
    assert_ne!(transform_after_wheel.pan, transform_after_pan.pan);
    assert_eq!(transform_after_wheel.zoom, transform_after_pan.zoom);

    cx.simulate_event(PinchEvent {
        position: point(px(420.0), px(280.0)),
        delta: 0.1,
        modifiers: Default::default(),
        phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
    draw(cx);
    let transform_after_pinch = cx.read_entity(&host, |host, _| {
        host.scene_transform.expect("pinch-zoomed transform")
    });
    assert_ne!(transform_after_pinch.zoom, transform_after_wheel.zoom);

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
    view.set_viewport(ViewportTransform::new(CanvasPoint::default(), 0.5).unwrap())
        .expect("overview zoom");
    let binding_capture = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured_binding = binding_capture.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    cx.update_entity(&host, |host, cx| {
        host.set_navigation_config(
            OpenGpuiNavigationConfig::default().with_alignment_snap_tolerance(Some(6.0)),
            cx,
        )
        .expect("idle host accepts opt-in alignment snapping");
    });
    draw(cx);

    let start = point(px(100.0), px(100.0));
    let near_alignment = point(px(236.0), px(100.0));
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(near_alignment, Some(MouseButton::Left), Default::default());
    draw(cx);

    cx.update_entity(&host, |host, _| {
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
fn routed_node_drag_keeps_release_position_and_published_route_after_commit(
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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

    cx.simulate_mouse_up(drag_end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);
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
fn idle_tick_schedule_does_not_notify_or_register_a_frame(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let notifications = Rc::new(Cell::new(0_u64));
    let observed_notifications = Rc::clone(&notifications);
    let _subscription = cx.update(|_, cx| {
        cx.observe(&host, move |_, _| {
            observed_notifications.set(observed_notifications.get().saturating_add(1));
        })
    });
    cx.run_until_parked();
    notifications.set(0);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.schedule_interaction_tick(window, cx);
            host.schedule_interaction_tick(window, cx);
        });
    });
    cx.run_until_parked();

    assert_eq!(notifications.get(), 0, "idle scheduling must not notify");
    cx.read_entity(&host, |host, _| {
        let resources = host.resource_snapshot();
        assert!(!resources.interaction_tick_scheduled());
        assert!(!resources.interaction_tick_frame_requested());
    });
}

#[open_gpui::test]
fn while_active_empty_update_preserves_the_scheduled_tick_and_frame_request(
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
        point(px(150.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    draw(cx);

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::NodeDrag,
            );
            let resources = host.resource_snapshot();
            assert!(resources.interaction_tick_scheduled());
            assert!(resources.interaction_tick_frame_requested());

            let update = host
                .interaction_owner
                .tick(1.0 / 60.0, host.surface_size.expect("measured surface"))
                .expect("active no-op tick");
            assert_eq!(update.tick_demand(), InteractionTickDemand::WhileActive);
            assert!(!update.query_work_pending());
            assert!(update.effects().is_empty());
            assert_eq!(update.damage(), Default::default());

            host.apply_canvas_interaction_update(update, window, cx);
            let resources = host.resource_snapshot();
            assert!(
                resources.interaction_tick_scheduled(),
                "WhileActive remains the sole authority for retaining the pending callback",
            );
            assert!(resources.interaction_tick_frame_requested());
        });
    });

    cx.simulate_mouse_up(
        point(px(150.0), px(140.0)),
        MouseButton::Left,
        Default::default(),
    );
}

#[open_gpui::test]
fn active_node_drag_requests_successor_animation_frames_for_auto_pan(
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
        point(px(150.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(5.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.handle_scene_pointer_exit(window, cx);
        });
    });
    draw(cx);

    let mut previous = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        assert!(host.resource_snapshot().interaction_tick_scheduled());
        host.view
            .viewport_transform()
            .expect("viewport before auto-pan")
    });

    for frame in 1..=2 {
        std::thread::sleep(std::time::Duration::from_millis(35));
        assert!(
            cx.simulate_frame(open_gpui::RequestFrameOptions::default()),
            "frame {frame}: the test platform must deliver the requested frame",
        );
        let (current, scheduled) = cx.read_entity(&host, |host, _| {
            (
                host.view
                    .viewport_transform()
                    .expect("viewport during auto-pan"),
                host.resource_snapshot().interaction_tick_scheduled(),
            )
        });
        assert!(
            scheduled,
            "frame {frame}: the successor tick must remain scheduled"
        );
        assert!(
            current.pan.x > previous.pan.x,
            "frame {frame}: the left-edge drag must advance viewport pan; previous={previous:?}, current={current:?}",
        );
        previous = current;
    }

    cx.simulate_mouse_up(
        point(px(5.0), px(140.0)),
        MouseButton::Left,
        Default::default(),
    );
}

#[open_gpui::test]
fn node_drag_release_before_redraw_commits_the_last_presented_auto_pan_projection(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let node = NodeId::from_u128(10);
    let initial_position = runtime
        .snapshot()
        .expect("graph before node drag")
        .graph()
        .node(&node)
        .expect("dragged node")
        .pos;

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
    let release = point(px(5.0), px(140.0));
    cx.simulate_mouse_move(release, Some(MouseButton::Left), Default::default());
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.handle_scene_pointer_exit(window, cx);
        });
    });
    draw(cx);

    let (presented_viewport, presented_delta) = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        assert!(host.resource_snapshot().interaction_tick_scheduled());
        (
            host.view
                .viewport_transform()
                .expect("presented viewport before callback"),
            host.interaction_node_drag_delta(node)
                .expect("presented node-drag delta"),
        )
    });

    let callback_observation = Rc::new(Cell::new(None));
    let observed = Rc::clone(&callback_observation);
    let callback_host = host.clone();
    cx.update(|window, _cx| {
        window.on_next_frame(move |window, cx| {
            let (before_viewport, before_delta) = {
                let host = callback_host.read(cx);
                (
                    host.view.viewport_transform().ok(),
                    host.interaction_node_drag_delta(node),
                )
            };
            window.dispatch_event(
                open_gpui::PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: release,
                    modifiers: Default::default(),
                    click_count: 1,
                }),
                cx,
            );
            let (after_viewport, after_delta) = {
                let host = callback_host.read(cx);
                (
                    host.view.viewport_transform().ok(),
                    host.interaction_node_drag_delta(node),
                )
            };
            observed.set(Some((
                before_viewport,
                before_delta,
                after_viewport,
                after_delta,
            )));
            window.refresh();
        });
    });
    assert!(
        cx.simulate_frame(open_gpui::RequestFrameOptions::default()),
        "the test platform must deliver the queued tick and release callbacks",
    );
    let (
        callback_viewport_before_release,
        callback_delta_before_release,
        callback_viewport_after_release,
        callback_delta_after_release,
    ) = callback_observation
        .take()
        .expect("release callback observation");
    assert_eq!(callback_viewport_before_release, Some(presented_viewport));
    assert_eq!(callback_delta_before_release, Some(presented_delta));
    assert_eq!(callback_viewport_after_release, Some(presented_viewport));
    assert_eq!(callback_delta_after_release, Some(presented_delta));

    cx.run_until_parked();
    draw(cx);
    cx.read_entity(&host, |host, _| {
        let resources = host.resource_snapshot();
        assert!(!resources.interaction_tick_scheduled());
        assert!(!resources.interaction_tick_frame_requested());
    });
    let final_position = runtime
        .snapshot()
        .expect("graph after node drag")
        .graph()
        .node(&node)
        .expect("dragged node after release")
        .pos;

    assert_eq!(
        final_position,
        CanvasPoint {
            x: initial_position.x + presented_delta.x,
            y: initial_position.y + presented_delta.y,
        },
    );
}

#[open_gpui::test]
fn external_frame_clock_cannot_let_stale_native_callback_consume_successor_tick(
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
        point(px(150.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.simulate_mouse_move(
        point(px(5.0), px(140.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.handle_scene_pointer_exit(window, cx);
        });
    });
    draw(cx);

    let before_external_tick = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        assert!(host.resource_snapshot().interaction_tick_scheduled());
        host.view
            .viewport_transform()
            .expect("viewport before externally driven tick")
    });

    std::thread::sleep(std::time::Duration::from_millis(35));
    let externally_advanced = cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.advance_interaction_frame(std::time::Instant::now(), window, cx)
        })
    });
    assert!(
        externally_advanced,
        "the external frame clock must consume the scheduled tick"
    );
    draw(cx);
    let after_external_tick = cx.read_entity(&host, |host, _| {
        assert!(host.resource_snapshot().interaction_tick_scheduled());
        host.view
            .viewport_transform()
            .expect("viewport after externally driven tick")
    });
    assert!(
        after_external_tick.pan.x > before_external_tick.pan.x,
        "the external frame clock must advance left-edge auto-pan"
    );

    assert!(
        cx.simulate_frame(open_gpui::RequestFrameOptions::default()),
        "the queued native frame callbacks must be delivered"
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.resource_snapshot().interaction_tick_scheduled());
        let transform = host
            .view
            .viewport_transform()
            .expect("viewport after queued native callbacks");
        assert!(
            transform.pan.x > after_external_tick.pan.x,
            "the successor callback must advance auto-pan exactly once",
        );
    });
}

#[open_gpui::test]
fn close_before_render_discards_ready_tick_without_mutating_node_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);

    let graph_before = runtime.snapshot().expect("graph before close");
    let history_before = runtime.history_state().expect("history before close");
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
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::NodeDrag,
        );
        let resources = host.resource_snapshot();
        assert!(resources.interaction_tick_scheduled());
        assert!(resources.interaction_tick_frame_requested());
    });

    let viewport_before_callback = cx.read_entity(&host, |host, _| {
        host.view
            .viewport_transform()
            .expect("viewport before callback entry")
    });
    let callback_host = host.clone();
    let callback_observation = Rc::new(Cell::new(None));
    let observed = Rc::clone(&callback_observation);
    cx.update(|window, _| {
        window.on_next_frame(move |window, cx| {
            let before = callback_host.read(cx).view.viewport_transform().ok();
            callback_host.update(cx, |host, cx| {
                host.close_in_window(window, cx)
                    .expect("close after the scheduled tick enters but before render");
            });
            let after = callback_host.read(cx).view.viewport_transform().ok();
            observed.set(Some((before, after)));
            window.refresh();
        });
    });
    assert!(
        cx.simulate_frame(open_gpui::RequestFrameOptions::default()),
        "the test platform must deliver the queued tick and close callbacks",
    );
    let (viewport_before_close, viewport_after_close) = callback_observation
        .take()
        .expect("close callback observation");
    assert_eq!(viewport_before_close, Some(viewport_before_callback));
    assert!(viewport_after_close.is_none());
    cx.read_entity(&host, |host, _| {
        let resources = host.resource_snapshot();
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Closed,
        );
        assert!(host.interaction_owner.test_pending_work_empty());
        assert!(
            !resources.interaction_tick_scheduled(),
            "close must cancel the ready semantic tick before render",
        );
        assert!(!resources.interaction_tick_frame_requested());
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1);
        assert!(!host.interaction_owner.has_focus_state());
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after close")));
    assert_eq!(
        runtime.history_state().expect("history after close"),
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    let pointer = point(px(100.0), px(100.0));

    cx.simulate_mouse_down(pointer, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus.clone())));
    cx.read_entity(&host, |host, _| {
        assert!(!host.active_scene_gesture());
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
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
        assert!(host.active_scene_gesture());
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
    });

    cx.simulate_mouse_up(pointer, MouseButton::Left, Default::default());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    cx.read_entity(&host, |host, _| {
        assert!(!host.active_scene_gesture());
        assert_eq!(
            host.resource_snapshot().protected_instances(),
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
            set_selection_by_id(&host.view, vec![target], Vec::new())
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
        set_selection_by_id(&host.view, vec![selected], Vec::new()).expect("select native node");
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
            set_selection_by_id(&host.view, vec![target], Vec::new())
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
        set_selection_by_id(
            &host.view,
            vec![NodeId::from_u128(10), NodeId::from_u128(11)],
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
fn detach_all_instances_restores_active_box_selection_baseline(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    cx.read_entity(&host, |host, _| {
        set_selection_by_id(&host.view, vec![target], Vec::new())
            .expect("seed box selection baseline");
    });
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
    cx.run_until_parked();

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Marquee,
        );
        assert_eq!(
            host.view
                .selection()
                .expect("transient box selection")
                .nodes(),
            [source],
        );
    });

    cx.update_entity(&host, |host, _| {
        drop(host.detach_all_instances());
    });

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.view
                .selection()
                .expect("restored box selection baseline")
                .nodes(),
            [target],
        );
    });
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
fn releasing_last_host_entity_cancels_active_port_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output port bounds");

    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_some());
    });

    release_last_host_entity(&scene, host, cx);
    assert_eq!(runtime.live_view_count(), 0);
}

#[open_gpui::test]
fn releasing_last_host_entity_cancels_active_node_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
    draw(cx);
    let start = point(px(100.0), px(100.0));
    let end = point(px(170.0), px(150.0));

    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    cx.read_entity(&host, |host, _| {
        assert!(host.active_scene_gesture());
    });

    release_last_host_entity(&scene, host, cx);
    assert_eq!(runtime.live_view_count(), 0);
}

#[open_gpui::test]
fn releasing_last_host_entity_cancels_active_scene_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
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
    cx.read_entity(&host, |host, _| {
        assert!(host.active_scene_gesture());
    });

    release_last_host_entity(&scene, host, cx);
    assert_eq!(runtime.live_view_count(), 0);
}

#[open_gpui::test]
fn runtime_drop_cancels_detached_active_port_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output port bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(px(700.0), px(500.0)),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.active_port_drag().is_some());
    });

    detach_host_but_retain_entity(&scene, &host, cx);
    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(host.active_port_drag().is_none());
    });
}

#[open_gpui::test]
fn runtime_drop_cancels_detached_active_node_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
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
    cx.read_entity(&host, |host, _| assert!(host.active_scene_gesture()));

    detach_host_but_retain_entity(&scene, &host, cx);
    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(!host.active_scene_gesture());
    });
}

#[open_gpui::test]
fn runtime_drop_cancels_detached_active_scene_capture(cx: &mut open_gpui::TestAppContext) {
    let (runtime, scene, host, cx) = mount_replaceable_gesture_scene(cx);
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
    cx.read_entity(&host, |host, _| assert!(host.active_scene_gesture()));

    detach_host_but_retain_entity(&scene, &host, cx);
    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(!host.active_scene_gesture());
    });
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        *captured_activity.borrow_mut() = Some(binding.read(cx).activity().clone());
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
        host.retained_nodes
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("old retained instance")
    });
    cx.update_entity(&host, |host, _| {
        assert!(
            host.retained_nodes
                .set_hover(Some(old_instance.clone()), None)
        );
    });
    let signal = super::HostLifecycleSignal::new();

    let removed = runtime
        .execute(GraphCommand::topology(
            runtime
                .snapshot()
                .expect("snapshot before node removal")
                .version(),
            TopologyEdit::new().remove_node(NodeId::from_u128(10), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("remove node")
        .applied()
        .expect("remove applies")
        .patch()
        .clone();
    signal.publish_runtime_patch_for_test(removed);
    let restored = runtime
        .execute(GraphCommand::undo(
            runtime
                .snapshot()
                .expect("snapshot before restoring node")
                .version(),
        ))
        .into_result()
        .expect("undo remove")
        .applied()
        .expect("undo applies")
        .patch()
        .clone();
    signal.publish_runtime_patch_for_test(restored);
    let patched = runtime
        .execute(GraphCommand::set_node_data(
            runtime
                .snapshot()
                .expect("snapshot before reincarnated payload patch")
                .version(),
            NodeId::from_u128(10),
            serde_json::json!({ "prompt": "reincarnated" }),
        ))
        .into_result()
        .expect("patch reincarnated payload")
        .applied()
        .expect("payload patch applies")
        .patch()
        .clone();
    signal.publish_runtime_patch_for_test(patched);

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
        assert!(host.retained_nodes.hovered_node().is_none());
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn reconcile_panic_revokes_inserted_mount_authority(cx: &mut open_gpui::TestAppContext) {
    let runtime = GraphRuntime::builder(catalog())
        .build(single_node_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let captured_activity = Rc::new(std::cell::RefCell::new(None));
    let factory_activity = captured_activity.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        *factory_activity.borrow_mut() = Some(binding.read(cx).activity().clone());
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                CanvasRect {
                    origin: CanvasPoint {
                        x: 10_000.0,
                        y: 10_000.0,
                    },
                    size: viewport().size,
                },
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(viewport());
            host.retained_nodes
                .inject_reconcile_fault(RetainedFrameReconcileFault::AfterNewMountInsert);
            cx.notify();
        });
        draw_once(cx);
    }));

    assert!(
        panic.is_err(),
        "the injected reconcile panic remains observable"
    );
    let activity = captured_activity
        .borrow()
        .clone()
        .expect("factory produced one mount-scoped activity registry");
    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    cx.update_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_none());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
        assert!(host.connection_preview_plan().is_none());
    });
}

#[open_gpui::test]
fn output_connection_drag_keeps_press_time_anchor_when_current_geometry_moves(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            pointer_sample_canvas(
                host.viewport_pointer_sample(pointer)
                    .expect("measured pointer sample"),
            ),
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
        assert_eq!(connection.pointer_canvas(), target);
    });
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let preview = host
            .connection_preview_plan()
            .expect("output connection preview");
        let (preview_source, preview_target) =
            edge_path_endpoints(&preview.path).expect("preview endpoints");
        assert_eq!(preview_source, source);
        assert_eq!(preview_target, target);
        assert!(preview.suppressed_edge.is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
    });

    runtime
        .execute(GraphCommand::set_node_position(
            runtime
                .snapshot()
                .expect("snapshot before external source move")
                .version(),
            NodeId::from_u128(10),
            CanvasPoint { x: 120.0, y: 120.0 },
        ))
        .into_result()
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
    cx.update_entity(&host, |host, _| {
        let preview = host
            .connection_preview_plan()
            .expect("fixed-endpoint preview after source geometry moves");
        let (preview_source, preview_target) =
            edge_path_endpoints(&preview.path).expect("fixed preview endpoints");
        assert_eq!(
            (preview_source, preview_target),
            (source, target),
            "the preview must not rebuild its fixed endpoint from later port geometry",
        );
        assert!(preview.suppressed_edge.is_none());
    });
}

#[open_gpui::test]
fn reconnect_preview_uses_fixed_port_anchor_and_suppresses_the_published_route_plan(
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.update_entity(&host, |host, _| {
        assert_eq!(
            host.scene_wires()
                .expect("initial published route plan")
                .published
                .len(),
            1,
        );
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
            pointer_sample_canvas(
                host.viewport_pointer_sample(pointer)
                    .expect("measured pointer sample"),
            ),
        )
    });

    cx.simulate_mouse_down(input.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(pointer, Some(MouseButton::Left), Default::default());
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let preview = host
            .connection_preview_plan()
            .expect("reconnect preview plan");
        let endpoints = edge_path_endpoints(&preview.path).expect("reconnect preview endpoints");
        assert_eq!(endpoints, (fixed_anchor, canvas_pointer));
        assert_eq!(
            preview.suppressed_edge.map(EdgeInstanceKey::edge),
            Some(edge),
        );
        assert!(
            host.scene_wires()
                .expect("suppressed reconnect route plan")
                .published
                .is_empty()
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
    view.set_viewport(ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 1.5).unwrap())
        .expect("set non-identity viewport");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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

    cx.update_entity(&host, |host, _| {
        let preview = host
            .connection_preview_plan()
            .expect("input connection preview");
        let (source, target) = edge_path_endpoints(&preview.path).expect("input preview endpoints");
        assert_canvas_point_close(source, CanvasPoint { x: 430.0, y: 322.0 });
        assert_eq!(target, target_anchor);
        assert!(preview.suppressed_edge.is_none());
        assert!(
            host.scene_wires()
                .expect("edge-budget-independent preview route plan")
                .published
                .is_empty()
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            before.version(),
            TopologyEdit::new().remove_node(source_id, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("remove old incarnation")
        .applied()
        .expect("remove applied")
        .version();
    runtime
        .execute(GraphCommand::topology(
            removed,
            TopologyEdit::new().add_node(source_id, source, ports),
        ))
        .into_result()
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");

    host.close().expect("unrendered host has no window state");
    assert!(host.interaction_owner.terminal());
    assert!(!host.interaction_owner.test_terminal_cleanup_pending());
    assert!(!host.interaction_owner.requires_window_cleanup());
    assert!(!host.interaction_owner.has_focus_state());
}

#[open_gpui::test]
fn windowless_close_rejects_keyboard_connection_before_closing_the_view(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        owner,
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert!(host.connection_search_request().is_none());
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
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_leases(), 1)
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
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
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
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");
    let mut ticket = host
        .commands
        .enqueue(
            GraphCommand::set_node_data(
                runtime
                    .snapshot()
                    .expect("queued-close graph version")
                    .version(),
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
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                crate::command_journal::OpenGpuiCommandTicketError::HostDropped
            )
        )
    ));
}

#[open_gpui::test]
fn window_close_resolves_queued_ticket_only_after_the_host_borrow_is_released(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("window-close graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
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
            .applied_revision()
            .expect("queued close command applied"),
        1
    );
}

#[open_gpui::test]
fn window_close_revocation_panic_finishes_window_cleanup_and_does_not_strand_ticket(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let _lease = cx.update_entity(&host, |host, _| {
        let instance = host
            .retained_nodes
            .retained_instance(NodeId::from_u128(10))
            .cloned()
            .expect("retained node instance");
        host.retained_nodes
            .get(&instance)
            .expect("active node mount")
            .activity
            .acquire_with_revocation(OpenGpuiActivityKind::Task, || {
                panic!("injected window-close revocation panic");
            })
            .expect("revocation activity")
    });
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("revocation-panic graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "revocation-panic-window-close" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });

    let close = cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let close = host.close_in_window(window, cx);
            assert_command_ticket_pending_at(&mut ticket, "inside terminal host borrow");
            close
        })
    });

    assert!(matches!(
        close,
        Err(super::OpenGpuiSceneHostCloseError::PanicDuringClose)
    ));
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert!(!host.interaction_owner.requires_window_cleanup());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
    });
    cx.run_until_parked();
    assert_eq!(
        poll_command_ticket(&mut ticket)
            .expect("ticket transport")
            .applied_revision()
            .expect("queued close command applied"),
        1
    );
    assert_eq!(runtime.snapshot().expect("closed runtime").revision(), 1);
}

#[open_gpui::test]
fn runtime_drop_bridge_terminalizes_a_queued_ticket_and_finishes_window_cleanup(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("runtime-drop graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "runtime-drop" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });
    assert_command_ticket_pending(&mut ticket);

    drop(runtime);
    cx.run_until_parked();

    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                crate::command_journal::OpenGpuiCommandTicketError::HostClosed {
                    reason: ViewCloseReason::RuntimeDropped
                }
            )
        )
    ));
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(!host.interaction_owner.requires_window_cleanup());
        assert!(!host.commands.has_pending());
    });
}

#[open_gpui::test]
fn prepaint_detected_close_terminalizes_the_collected_ticket(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_gesture_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("prepaint-close graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "prepaint-runtime-close" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });

    cx.update_entity(&host, |host, _| {
        host.prepaint_close_fault = true;
    });
    draw(cx);

    let ticket_result = poll_command_ticket(&mut ticket);
    assert!(
        matches!(
            ticket_result,
            Err(
                crate::command_journal::OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                    crate::command_journal::OpenGpuiCommandTicketError::HostClosed {
                        reason: ViewCloseReason::HostClosed
                    }
                )
            )
        ),
        "unexpected prepaint-close ticket result: {ticket_result:?}",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert!(!host.commands.has_pending());
        assert!(!host.interaction_owner.requires_window_cleanup());
    });
}

#[open_gpui::test]
fn first_render_of_an_already_closed_view_terminalizes_a_queued_ticket(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
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
                    .seal(&catalog())
                    .expect("valid empty test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("closed-before-subscribe graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "source": "closed-before-subscribe" }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued binding command")
    });

    drop(runtime);
    draw_once(cx);

    assert!(matches!(
        poll_command_ticket(&mut ticket),
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                crate::command_journal::OpenGpuiCommandTicketError::HostClosed {
                    reason: ViewCloseReason::RuntimeDropped
                }
            )
        )
    ));
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert!(!host.commands.has_pending());
    });
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
                    .seal(&catalog())
                    .expect("valid empty test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    let mut ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("panicking-close graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
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
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::ExecutionUnknown(
                crate::command_journal::OpenGpuiCommandTicketError::ClosePanicked {
                    reason: ViewCloseReason::HostClosed
                }
            )
        )
    ));
}

#[open_gpui::test]
fn scene_preparation_failure_after_collection_fails_the_batch_and_reopens_capacity(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_gesture_scene(cx);
    draw(cx);
    let mut failed = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("preparation-failure graph version")
            .version();
        let ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
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
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::DefinitelyNotExecuted(
                crate::command_journal::OpenGpuiCommandTicketError::FramePreparation { .. }
            )
        )
    ));
    assert_eq!(runtime.snapshot().expect("unchanged graph").revision(), 0);
    cx.read_entity(&host, |host, _| assert!(!host.commands.has_pending()));

    let mut retry = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("preparation retry graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
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
            .applied_revision()
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let mut failed = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("panicking feedback graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
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
        Err(
            crate::command_journal::OpenGpuiCommandTicketFailure::ExecutionUnknown(
                crate::command_journal::OpenGpuiCommandTicketError::FeedbackOutcomeUnknown
            )
        )
    ));
    let snapshot_after_panic = runtime.snapshot().expect("runtime after panic");
    let version_after_panic = snapshot_after_panic.version();
    let revision_after_panic = snapshot_after_panic.revision();
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
                    version_after_panic,
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
            .applied_revision()
            .expect("retry command applied"),
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
fn dropping_host_closes_the_exact_view_session() {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let handle = view.downgrade();
    let host = OpenGpuiSceneHost::new(
        view,
        OpenGpuiNodeRegistry::builder()
            .seal(&catalog())
            .expect("valid empty test registry"),
        viewport(),
    )
    .expect("valid Open GPUI scene host");

    handle
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 1.5).unwrap())
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
        assert!(host.connection_search_request().is_some());
        assert_connect_search_binding(host, PortId::from_u128(20));
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
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
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
fn keyboard_activation_arms_an_exact_source_and_commits_through_an_exact_target(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    let history_before = runtime
        .history_state()
        .expect("history before keyboard connection");

    let source = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    let activate = |owner| {
        OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
            owner,
            OpenGpuiPortActivationSource::Keyboard,
        ))
    };
    let event = activate(source.clone());
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });

    let generation = cx.read_entity(&host, |host, _| {
        assert!(host.connection_search_request().is_none());
        assert_connect_search_binding(host, PortId::from_u128(20));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert_eq!(
            host.interaction_owner
                .armed_keyboard_connection_source()
                .map(|source| source.port()),
            Some(PortId::from_u128(20)),
        );
        assert!(host.interaction_owner.has_connection());
        host.interaction_presentation()
            .generation()
            .expect("keyboard connection generation")
    });

    let event = activate(source);
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().generation(),
            Some(generation)
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
    });

    let target = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    let event = activate(target);
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Committing,
        );
    });
    for _ in 0..4 {
        draw(cx);
    }

    let snapshot = runtime.snapshot().expect("keyboard connection graph");
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert_eq!(
        runtime
            .history_state()
            .expect("history after keyboard connection")
            .undo_len(),
        history_before.undo_len() + 1,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(!host.interaction_owner.has_connection());
    });
}

#[open_gpui::test]
fn mouse_click_connect_uses_the_same_source_target_authority_as_keyboard_activation(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    draw(cx);
    let source = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    let target = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    let source_position = point(px(200.0), px(160.0));
    let target_position = point(px(520.0), px(160.0));
    let dispatch = |event, cx: &mut open_gpui::VisualTestContext| {
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.handle_port_event(OpenGpuiPortEvent::Pointer(event), window, cx);
            });
        });
    };

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            source.clone(),
            1,
            OpenGpuiPortDragPhase::Start,
            source_position,
        ),
        cx,
    );
    dispatch(
        OpenGpuiPortDragEvent::for_test(source, 1, OpenGpuiPortDragPhase::Release, source_position),
        cx,
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_owner
                .armed_keyboard_connection_source()
                .map(|source| source.port()),
            Some(PortId::from_u128(20)),
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
    });

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            target.clone(),
            2,
            OpenGpuiPortDragPhase::Start,
            target_position,
        ),
        cx,
    );
    dispatch(
        OpenGpuiPortDragEvent::for_test(target, 2, OpenGpuiPortDragPhase::Release, target_position),
        cx,
    );
    for _ in 0..4 {
        draw(cx);
    }

    assert_eq!(runtime.snapshot().unwrap().graph().edges().len(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
    });
}

#[open_gpui::test]
fn blueprint_pointer_click_does_not_arm_click_to_connect(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    draw(cx);
    cx.update_entity(&host, |host, cx| {
        host.set_navigation_config(
            OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Blueprint),
            cx,
        )
        .expect("idle host accepts Blueprint navigation");
    });
    let source = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    let position = point(px(200.0), px(160.0));

    for phase in [OpenGpuiPortDragPhase::Start, OpenGpuiPortDragPhase::Release] {
        let event = OpenGpuiPortDragEvent::for_test(source.clone(), 1, phase, position);
        cx.update(|window, cx| {
            host.update(cx, |host, cx| {
                host.handle_port_event(OpenGpuiPortEvent::Pointer(event), window, cx);
            });
        });
    }

    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(
            host.interaction_owner
                .armed_keyboard_connection_source()
                .is_none()
        );
    });
}

#[open_gpui::test]
fn escape_from_the_exact_source_focus_cancels_keyboard_connection_without_history(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    draw(cx);
    let source = PortId::from_u128(20);
    let (owner, focus) = cx.update_entity(&host, |host, cx| {
        let owner = current_port_drag_owner(host, NodeId::from_u128(10), source);
        let binding = host
            .retained_nodes
            .active_in_order()
            .find(|(instance, _)| instance.node() == NodeId::from_u128(10))
            .and_then(|(_, mounted)| mounted.binding.clone())
            .expect("source binding");
        let focus = binding
            .read(cx)
            .port_focus_handle(source)
            .cloned()
            .expect("source focus");
        (owner, focus)
    });
    cx.update(|window, cx| window.focus(&focus, cx));
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        owner,
        OpenGpuiPortActivationSource::Keyboard,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
    });

    simulate_key_down(cx, "escape", false);
    cx.run_until_parked();
    draw(cx);

    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(focus)));
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(!host.interaction_owner.has_connection());
        assert!(host.connection_search_request().is_none());
    });
    assert_eq!(runtime.snapshot().expect("unchanged graph").revision(), 0);
    assert_eq!(
        runtime
            .history_state()
            .expect("unchanged history")
            .undo_len(),
        0
    );
}

#[open_gpui::test]
fn pointer_empty_release_enters_search_while_semantic_activation_only_arms_the_source(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(unconnected_graph())
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
        assert_connect_search_binding(host, PortId::from_u128(21));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        host.cancel_connection_search(&request, window, cx)
            .expect("cancel pointer search");
    });

    let owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    for source in [
        OpenGpuiPortActivationSource::Keyboard,
        OpenGpuiPortActivationSource::Accessibility,
    ] {
        let event =
            OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(owner.clone(), source));
        cx.update(|window, cx| {
            host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
        });

        cx.update_window_entity(&host, |host, window, cx| {
            assert!(host.connection_search_request().is_none());
            assert_connect_search_binding(host, PortId::from_u128(21));
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::ConnectionDrag
            );
            assert_eq!(
                host.interaction_owner
                    .armed_keyboard_connection_source()
                    .map(|source| source.port()),
                Some(PortId::from_u128(21)),
            );
            let update = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(update, window, cx));
        });
    }
}

#[open_gpui::test]
fn retried_connection_search_rejects_a_stale_cancel_capability(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
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

    let first = cx.update_window_entity(&host, |host, window, cx| {
        let request = host
            .connection_search_request()
            .expect("initial search request");
        let failed = host
            .interaction_owner
            .search_failed(&request)
            .expect("current request can publish failure");
        host.apply_canvas_interaction_update(failed, window, cx);
        host.retry_connection_search(&request, window, cx)
            .expect("failed search can retry");
        request
    });
    let successor = cx.read_entity(&host, |host, _| {
        host.connection_search_request()
            .expect("retry publishes a successor request")
    });
    assert_ne!(successor, first);

    cx.update_window_entity(&host, |host, window, cx| {
        assert!(matches!(
            host.cancel_connection_search(&first, window, cx),
            Err(super::OpenGpuiConnectionSearchError::StaleGeneration)
        ));
        assert_eq!(
            host.connection_search_request().as_ref(),
            Some(&successor),
            "a stale capability cannot cancel its successor",
        );
        host.cancel_connection_search(&successor, window, cx)
            .expect("the current capability can cancel its search");
        assert_eq!(host.connection_search_request(), None);
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
        );
    });
}

#[open_gpui::test]
fn rejected_keyboard_reconnect_target_retains_the_exact_source_and_feedback(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog())
        .build(occupied_input_graph())
        .expect("valid occupied graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);

    let source = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(11), PortId::from_u128(21))
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        source,
        OpenGpuiPortActivationSource::Accessibility,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });

    cx.read_entity(&host, |host, _| {
        assert!(host.connection_search_request().is_none());
        assert_reconnect_search_binding(host, EdgeId::from_u128(30), PortId::from_u128(20));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
    });

    let invalid_target = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(12), PortId::from_u128(22))
    });
    let event = OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
        invalid_target,
        OpenGpuiPortActivationSource::Accessibility,
    ));
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.handle_port_event(event, window, cx));
    });
    draw(cx);

    cx.update_window_entity(&host, |host, window, cx| {
        assert_eq!(
            host.interaction_owner
                .armed_keyboard_connection_source()
                .map(|source| source.port()),
            Some(PortId::from_u128(21)),
        );
        let feedback = host
            .interaction_owner
            .keyboard_connection_feedback()
            .expect("structured invalid-target feedback");
        assert_eq!(feedback.source(), PortId::from_u128(21));
        assert_eq!(feedback.target(), PortId::from_u128(22));
        assert!(!feedback.diagnostics().is_empty());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
        let update = host.interaction_owner.cancel();
        assert!(host.apply_canvas_interaction_result(update, window, cx));
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            .connection_search_candidates(&request, window, cx)
            .expect("semantic candidates");
        assert_eq!(candidates.candidates().len(), 1);
        let creation = connection_search_creation(&request, &candidates, 0);
        let submission = host
            .select_connection_search_candidate(&request, &candidates, 0, creation, window, cx)
            .expect("enqueue atomic create and connect");
        assert_eq!(host.connection_search_request(), None);
        let repeated_creation = connection_search_creation(&request, &candidates, 0);
        assert!(matches!(
            host.select_connection_search_candidate(
                &request,
                &candidates,
                0,
                repeated_creation,
                window,
                cx,
            ),
            Err(super::OpenGpuiConnectionSearchError::InactiveRequest)
        ));
        assert!(matches!(
            host.cancel_connection_search(&request, window, cx),
            Err(super::OpenGpuiConnectionSearchError::InactiveRequest)
        ));
        submission
    });
    for _ in 0..8 {
        draw(cx);
        let settled = cx.read_entity(&host, |host, _| {
            let presentation = host.interaction_presentation();
            presentation.phase() == CanvasInteractionPhase::Idle
        });
        if settled {
            break;
        }
    }
    cx.update_entity(&host, |host, _| {
        let graph = runtime.snapshot().expect("create-from-wire graph evidence");
        let (edge_id, edge) = graph
            .graph()
            .edges()
            .iter()
            .next()
            .expect("created connection edge");
        let geometry = host
            .view
            .geometry_snapshot()
            .expect("create-from-wire geometry evidence");
        let settlement_evidence = format!(
            "created_node={}, target_port={}, final_route={}, diagnostic={:?}",
            geometry.node(submission.node()).is_some(),
            geometry.port(edge.to).is_some(),
            geometry.route(*edge_id).is_some(),
            host.last_diagnostic(),
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle,
            "{settlement_evidence}",
        );
        assert!(host.interaction_presentation().connection().is_none());
        assert!(host.connection_preview_plan().is_none());
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
        .execute(GraphCommand::undo(snapshot.version()))
        .into_result()
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
            .connection_search_candidates(&request, window, cx)
            .expect("semantic candidates");
        (request, candidates)
    });

    let current = runtime.snapshot().expect("current graph");
    runtime
        .execute(GraphCommand::set_node_data(
            current.version(),
            NodeId::from_u128(10),
            serde_json::json!({"changed": true}),
        ))
        .into_result()
        .expect("intervening command")
        .applied()
        .expect("intervening command applied");
    let history_after_intervening = runtime.history_state().expect("history state");

    let submission = cx.update_window_entity(&host, |host, window, cx| {
        let creation = connection_search_creation(&request, &candidates, 0);
        let submission = host
            .select_connection_search_candidate(&request, &candidates, 0, creation, window, cx)
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
        assert_eq!(host.connection_search_request().as_ref(), Some(&request));
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
    });

    let candidates = cx.update_window_entity(&host, |host, window, cx| {
        host.connection_search_candidates(&request, window, cx)
            .expect("semantic candidates")
    });
    let mut prefix_ticket = cx.update_entity(&host, |host, _| {
        let version = host
            .view
            .graph_snapshot()
            .expect("connection-search prefix graph version")
            .version();
        host.commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued prefix command")
    });
    cx.update_window_entity(&host, |host, window, cx| {
        let creation = connection_search_creation(&request, &candidates, 0);
        host.select_connection_search_candidate(&request, &candidates, 0, creation, window, cx)
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
            .connection_search_candidates(&request, window, cx)
            .expect("semantic candidates");
        (request, candidates)
    });

    cx.update_window_entity(&host, |host, window, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("cancellation prefix graph version")
            .version();
        let _prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(10),
                    serde_json::json!({ "prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued prefix command");
        let creation = connection_search_creation(&request, &candidates, 0);
        host.select_connection_search_candidate(&request, &candidates, 0, creation, window, cx)
            .expect("selection waits behind prefix");
        assert!(host.has_pending_interaction_prefix_task());
        assert!(
            host.interaction_owner
                .test_prefix_task_attach_transition_matrix(),
            "prefix tasks attach only to an exact preparing submission",
        );
        assert!(
            host.interaction_owner
                .test_stale_prefix_task_attach_is_isolated(),
            "a stale prefix task must not replace the current submission task",
        );
        assert!(
            host.interaction_owner
                .test_duplicate_prefix_task_attach_is_rejected(),
            "a second task cannot attach after the submission entered its waiting stage",
        );

        let cancelled = host.interaction_owner.cancel();
        host.discard_pending_interaction_submission();
        assert!(host.apply_canvas_interaction_result(cancelled, window, cx));

        for index in 1..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            drop(
                host.commands
                    .enqueue(
                        GraphCommand::set_node_data(
                            version,
                            NodeId::from_u128(10),
                            serde_json::json!({ "queued": index }),
                        ),
                        super::OpenGpuiCommandOrigin::Binding,
                    )
                    .unwrap_or_else(|error| {
                        panic!("cancelled prefix task releases slot {index}: {error:?}")
                    }),
            );
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
        let initial_version = host
            .view
            .graph_snapshot()
            .expect("node-size ordering graph version")
            .version();
        issue_content_height_update(host, source, 172.0);
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_geometry(
                    initial_version,
                    [NodeGeometryEdit::new(source).with_size(Some(CanvasSize {
                        width: 180.0,
                        height: 150.0,
                    }))],
                )
                .without_history(),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("authoritative prefix resize");
        host.schedule_content_size_prefix(cx);
        assert!(host.content_sizes.has_prefix_task());

        let suffix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    initial_version,
                    target,
                    serde_json::json!({ "suffix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("suffix command queued after the captured prefix");
        issue_content_height_update(host, target, 188.0);
        assert_eq!(host.content_sizes.pending_command_counts(), Some((1, 1)));
        (prefix_ticket, suffix_ticket)
    });

    for _ in 0..8 {
        draw(cx);
    }

    assert!(
        poll_command_ticket(&mut prefix_ticket)
            .expect("prefix ticket transport")
            .applied_revision()
            .is_some()
    );
    assert_eq!(
        poll_command_ticket(&mut suffix_ticket)
            .expect("suffix ticket transport")
            .kind(),
        jellyflow_runtime::commands::CommandExecutionKind::Conflict
    );
    let snapshot = runtime.snapshot().expect("ordered node-size graph");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(
        snapshot.graph().node(&source).expect("source node").size,
        Some(CanvasSize {
            width: 180.0,
            height: 150.0,
        })
    );
    let target_node = snapshot.graph().node(&target).expect("target node");
    assert_eq!(target_node.data, serde_json::Value::Null);
    assert_eq!(
        target_node.size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
        assert!(
            host.last_diagnostic().is_none(),
            "unexpected host diagnostic: {:?}",
            host.last_diagnostic()
        );
    });
}

#[open_gpui::test]
fn content_size_capacity_prefers_already_protected_targets_and_parks_until_woken(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene_for_graph(cx, content_size_capacity_graph());
    draw(cx);

    let first_unprotected = NodeId::from_u128(1);
    let protected_target = NodeId::from_u128((OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1) as u128);
    let leases = cx.update_entity(&host, |host, cx| {
        (2..=OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY + 1)
            .map(|index| {
                let node = NodeId::from_u128(index as u128);
                let instance = host
                    .retained_nodes
                    .retained_instance(node)
                    .cloned()
                    .expect("protected content-size instance");
                let mount = host
                    .mount_capability(&instance)
                    .cloned()
                    .expect("protected content-size mount");
                host.retained_nodes
                    .binding_for_mount(&instance, &mount)
                    .expect("protected content-size binding")
                    .read(cx)
                    .activity()
                    .acquire(OpenGpuiActivityKind::Focus)
                    .expect("fill protected capacity")
            })
            .collect::<Vec<_>>()
    });

    cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, first_unprotected, 172.0);
        issue_content_height_update(host, protected_target, 188.0);
        host.schedule_content_size_prefix(cx);
        assert!(
            host.content_sizes.has_prefix_task(),
            "an already-protected target must bypass an unprotected queue head"
        );
    });

    for _ in 0..12 {
        draw(cx);
    }
    let snapshot = runtime
        .snapshot()
        .expect("partially settled capacity graph");
    assert_eq!(
        snapshot
            .graph()
            .node(&protected_target)
            .expect("already-protected content-size target")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    assert_eq!(
        snapshot
            .graph()
            .node(&first_unprotected)
            .expect("capacity-blocked content-size target")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 100.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.content_sizes.pending_command_counts().is_some());
        assert!(host.content_sizes.has_capacity_wait_task());
        assert!(
            !host.content_sizes.needs_frame(),
            "full protected capacity must park instead of spinning animation frames"
        );
    });

    drop(leases);
    cx.run_until_parked();
    for _ in 0..12 {
        draw(cx);
    }
    let snapshot = runtime.snapshot().expect("capacity-woken content graph");
    assert_eq!(
        snapshot
            .graph()
            .node(&first_unprotected)
            .expect("capacity-woken content-size target")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 172.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn content_size_target_survives_prefix_capture_queue_backpressure(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, source, 172.0);
        let version = host
            .view
            .graph_snapshot()
            .expect("queue-backpressure graph version")
            .version();
        for _ in 0..OPEN_GPUI_COMMAND_JOURNAL_CAPACITY {
            drop(
                host.commands
                    .enqueue(
                        GraphCommand::set_node_data(version, target, serde_json::Value::Null),
                        super::OpenGpuiCommandOrigin::Binding,
                    )
                    .expect("fill command journal with deterministic noops"),
            );
        }

        assert!(host.schedule_content_size_prefix(cx));
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 1)));
        assert!(!host.content_sizes.has_prefix_task());
        assert!(
            host.last_diagnostic().is_none(),
            "pre-admission backpressure is not a semantic size failure"
        );
    });

    for _ in 0..12 {
        draw(cx);
    }

    assert_eq!(
        runtime
            .snapshot()
            .expect("queue-backpressure graph")
            .graph()
            .node(&source)
            .expect("content-sized source")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 172.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn content_size_mutation_coalesces_the_latest_target_while_pending(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, source, 172.0);
        host.schedule_content_size_prefix(cx);
        assert!(host.content_sizes.has_prefix_task());

        let instance = host
            .retained_nodes
            .retained_instance(source)
            .cloned()
            .expect("content-sized source instance");
        host.content_sizes
            .coalesce_stable_height_for_test(&instance, 188.0);
        assert_eq!(host.content_sizes.pending_command_counts(), Some((1, 0)));
    });

    for _ in 0..12 {
        draw(cx);
    }

    let snapshot = runtime.snapshot().expect("coalesced content-size graph");
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(
        snapshot
            .graph()
            .node(&source)
            .expect("coalesced content-sized node")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn content_size_batch_primary_retirement_preserves_the_surviving_latest_target(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_content_size_batch_retirement_preserves_the_surviving_latest_target(
        cx,
        NodeId::from_u128(10),
        NodeId::from_u128(11),
    );
}

#[open_gpui::test]
fn content_size_batch_non_primary_retirement_preserves_the_surviving_latest_target(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_content_size_batch_retirement_preserves_the_surviving_latest_target(
        cx,
        NodeId::from_u128(11),
        NodeId::from_u128(10),
    );
}

#[open_gpui::test]
fn content_size_batch_secondary_close_overrides_primary_authoring(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let primary = NodeId::from_u128(10);
    let secondary = NodeId::from_u128(11);
    let (primary_activity, secondary_activity, secondary_mount) =
        cx.update_entity(&host, |host, cx| {
            issue_content_height_update(host, primary, 172.0);
            issue_content_height_update(host, secondary, 172.0);
            let effects = host.content_sizes.dispatch_queued_for_test(
                Rc::clone(&host.commands),
                &host.retained_nodes,
                cx,
            );
            host.apply_content_size_effects(effects);
            assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 2)));

            let primary_instance = host
                .retained_nodes
                .retained_instance(primary)
                .cloned()
                .expect("primary content-sized node instance");
            let secondary_instance = host
                .retained_nodes
                .retained_instance(secondary)
                .cloned()
                .expect("secondary content-sized node instance");
            let primary_node = host
                .retained_nodes
                .get(&primary_instance)
                .expect("primary content-sized node mount");
            let secondary_node = host
                .retained_nodes
                .get(&secondary_instance)
                .expect("secondary content-sized node mount");
            (
                primary_node.activity.clone(),
                secondary_node.activity.clone(),
                secondary_node.mount.clone(),
            )
        });

    // The primary binding can still open authoring facts, but terminal state belongs to the whole
    // admitted mount set. Closing the later owner must therefore win before command observation.
    secondary_mount.close(ViewCloseReason::HostClosed);
    cx.update_entity(&host, |host, cx| {
        host.observe_content_size_mutation(cx);
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });

    assert_eq!(primary_activity.active_count(), 0);
    assert_eq!(secondary_activity.active_count(), 0);
    for _ in 0..8 {
        draw(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(!host.commands.has_pending());
    });
}

#[open_gpui::test]
fn content_size_batch_runtime_primary_removal_preserves_the_surviving_latest_target(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let removed_node = NodeId::from_u128(10);
    let surviving_node = NodeId::from_u128(11);
    cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, removed_node, 172.0);
        issue_content_height_update(host, surviving_node, 172.0);
        let effects = host.content_sizes.dispatch_queued_for_test(
            Rc::clone(&host.commands),
            &host.retained_nodes,
            cx,
        );
        host.apply_content_size_effects(effects);

        let surviving_instance = host
            .retained_nodes
            .retained_instance(surviving_node)
            .cloned()
            .expect("surviving content-sized node instance");
        host.content_sizes
            .coalesce_stable_height_for_test(&surviving_instance, 188.0);
    });
    cx.run_until_parked();

    cx.update_entity(&host, |host, cx| {
        runtime
            .execute(GraphCommand::topology(
                runtime
                    .snapshot()
                    .expect("content-size command applied before runtime removal")
                    .version(),
                TopologyEdit::new().remove_node(removed_node, IncidentEdgePolicy::Cascade),
            ))
            .into_result()
            .expect("remove the primary content-size node")
            .applied()
            .expect("primary content-size node removal applies");

        // Observe before the lifecycle bridge drains the removal. The retained store still contains
        // the retired primary binding, while the same authoring snapshot can be opened through the
        // surviving batch member.
        host.observe_content_size_mutation(cx);
    });

    for _ in 0..20 {
        draw(cx);
    }

    assert_eq!(
        runtime
            .snapshot()
            .expect("partial-removal content-size graph")
            .graph()
            .node(&surviving_node)
            .expect("surviving content-sized node")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

fn assert_content_size_batch_retirement_preserves_the_surviving_latest_target(
    cx: &mut open_gpui::TestAppContext,
    retired_node: NodeId,
    surviving_node: NodeId,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let retired_mount = cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, retired_node, 172.0);
        issue_content_height_update(host, surviving_node, 172.0);
        let effects = host.content_sizes.dispatch_queued_for_test(
            Rc::clone(&host.commands),
            &host.retained_nodes,
            cx,
        );
        host.apply_content_size_effects(effects);
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 2)));

        let surviving_instance = host
            .retained_nodes
            .retained_instance(surviving_node)
            .cloned()
            .expect("surviving content-sized node instance");
        host.content_sizes
            .coalesce_stable_height_for_test(&surviving_instance, 188.0);

        let retired_instance = host
            .retained_nodes
            .retained_instance(retired_node)
            .cloned()
            .expect("retired content-sized node instance");
        host.detach_instance(&retired_instance)
            .expect("detach one admitted content-size batch mount")
    });
    retired_mount.retire();

    for _ in 0..20 {
        draw(cx);
    }

    assert_eq!(
        runtime
            .snapshot()
            .expect("partial-retirement content-size graph")
            .graph()
            .node(&surviving_node)
            .expect("surviving content-sized node")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn content_size_publication_settles_against_layout_generation_at_semantic_zoom(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    cx.update_entity(&host, |host, cx| {
        host.view
            .set_viewport(ViewportTransform::new(CanvasPoint::default(), 2.0).unwrap())
            .expect("set non-unit semantic zoom");
        issue_content_height_update(host, source, 172.0);
        host.schedule_content_size_prefix(cx);
    });

    for _ in 0..12 {
        draw(cx);
    }

    assert_eq!(
        runtime
            .snapshot()
            .expect("semantic-zoom content-size graph")
            .graph()
            .node(&source)
            .expect("content-sized source")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 172.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        let geometry = host.view.geometry_snapshot().expect("published geometry");
        let published = geometry.node(source).expect("published source geometry");
        assert_eq!(
            published.bounds().size,
            CanvasSize {
                width: 90.0,
                height: 86.0,
            },
            "semantic detail stays at native pixels while runtime geometry remains in canvas space"
        );
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn content_size_settlement_restores_a_parked_node_for_geometry_publication(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, source, 172.0);
        host.schedule_content_size_prefix(cx);
        host.set_frame_bounds(CanvasRect {
            origin: CanvasPoint {
                x: 10_000.0,
                y: 10_000.0,
            },
            size: CanvasSize {
                width: 100.0,
                height: 100.0,
            },
        });
    });

    for _ in 0..12 {
        draw(cx);
    }

    assert_eq!(
        runtime
            .snapshot()
            .expect("parked content-size graph")
            .graph()
            .node(&source)
            .expect("parked content-sized node")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 172.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn unavailable_geometry_parks_content_size_publication_after_one_recovery_frame(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let source = NodeId::from_u128(10);
    cx.update_entity(&host, |host, _| {
        issue_content_height_update(host, source, 172.0);
        let instance = host
            .retained_nodes
            .retained_instance(source)
            .cloned()
            .expect("content-sized source instance");
        host.content_sizes
            .force_queued_publication_for_test(&instance);
        host.view.close().expect("close content-size view");

        assert!(host.observe_content_size_publication_after_feedback(&BTreeSet::new()));
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 1)));
        assert!(!host.observe_content_size_publication_after_feedback(&BTreeSet::new()));
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 1)));
        assert!(!host.content_sizes.needs_frame());
        assert!(host.last_diagnostic().is_some_and(|diagnostic| {
            diagnostic
                .message()
                .contains("authoritative geometry snapshot was unavailable")
        }));
    });
}

#[open_gpui::test]
fn content_epoch_invalidation_isolates_a_pending_prefix_callback(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);
    let source = NodeId::from_u128(10);

    let (binding, mut prefix_ticket) = cx.update_entity(&host, |host, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("content epoch prefix graph version")
            .version();
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(11),
                    serde_json::json!({ "content-epoch-prefix": true }),
                )
                .without_history(),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queue content epoch prefix");
        issue_content_height_update(host, source, 172.0);
        host.schedule_content_size_prefix(cx);
        assert!(host.content_sizes.has_prefix_task());
        let instance = host
            .retained_nodes
            .retained_instance(source)
            .cloned()
            .expect("content-sized source instance");
        let mount = host
            .mount_capability(&instance)
            .cloned()
            .expect("content-sized source mount");
        host.retained_nodes
            .binding_for_mount(&instance, &mount)
            .map(|binding| (binding, prefix_ticket))
            .expect("content-sized source binding")
    });
    assert!(
        binding
            .update(cx, |binding, cx| binding.invalidate_layout(cx))
            .expect("invalidate content epoch")
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
        assert!(!host.content_sizes.has_prefix_task());
    });

    cx.run_until_parked();
    for _ in 0..4 {
        draw(cx);
    }
    assert!(
        poll_command_ticket(&mut prefix_ticket)
            .expect("content epoch prefix transport")
            .applied_revision()
            .is_some()
    );
    assert_eq!(
        runtime
            .snapshot()
            .expect("content-invalidated graph")
            .graph()
            .node(&source)
            .expect("content-invalidated source")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 100.0,
        })
    );
}

#[open_gpui::test]
fn content_epoch_invalidation_drains_an_admitted_command_before_issuing_the_successor(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);
    let source = NodeId::from_u128(10);

    let binding = cx.update_entity(&host, |host, cx| {
        issue_content_height_update(host, source, 172.0);
        let effects = host.content_sizes.dispatch_queued_for_test(
            Rc::clone(&host.commands),
            &host.retained_nodes,
            cx,
        );
        host.apply_content_size_effects(effects);
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 1)));
        let instance = host
            .retained_nodes
            .retained_instance(source)
            .cloned()
            .expect("content-sized source instance");
        let mount = host
            .mount_capability(&instance)
            .cloned()
            .expect("content-sized source mount");
        host.retained_nodes
            .binding_for_mount(&instance, &mount)
            .expect("content-sized source binding")
    });
    assert!(
        binding
            .update(cx, |binding, cx| binding.invalidate_layout(cx))
            .expect("invalidate content epoch after admission")
    );
    cx.update_entity(&host, |host, _| {
        let instance = host
            .retained_nodes
            .retained_instance(source)
            .cloned()
            .expect("content-sized source instance");
        assert_eq!(host.content_sizes.pending_command_counts(), Some((0, 1)));
        host.content_sizes
            .coalesce_stable_height_for_test(&instance, 188.0);
    });

    for _ in 0..20 {
        draw(cx);
    }
    assert_eq!(
        runtime
            .snapshot()
            .expect("drained content-size graph")
            .graph()
            .node(&source)
            .expect("content-sized source")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 188.0,
        })
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.content_sizes.pending_command_counts(), None);
    });
}

#[open_gpui::test]
fn environment_invalidation_isolates_a_pending_prefix_callback(cx: &mut open_gpui::TestAppContext) {
    let (runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);
    let source = NodeId::from_u128(10);

    let mut prefix_ticket = cx.update_entity(&host, |host, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("environment prefix graph version")
            .version();
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(11),
                    serde_json::json!({ "environment-prefix": true }),
                )
                .without_history(),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queue environment prefix");
        issue_content_height_update(host, source, 172.0);
        host.schedule_content_size_prefix(cx);
        assert!(host.content_sizes.has_prefix_task());
        assert!(
            host.set_adapter_environment(AdapterEnvironment::new(2, 3, 5, 7), cx)
                .expect("advance adapter environment")
        );
        assert_eq!(host.content_sizes.pending_command_counts(), None);
        assert!(!host.content_sizes.has_prefix_task());
        prefix_ticket
    });

    cx.run_until_parked();
    for _ in 0..4 {
        draw(cx);
    }
    assert!(
        poll_command_ticket(&mut prefix_ticket)
            .expect("environment prefix transport")
            .applied_revision()
            .is_some()
    );
    assert_eq!(
        runtime
            .snapshot()
            .expect("environment-invalidated graph")
            .graph()
            .node(&source)
            .expect("environment-invalidated source")
            .size,
        Some(CanvasSize {
            width: 180.0,
            height: 100.0,
        })
    );
}

#[open_gpui::test]
fn closing_with_a_pending_content_size_prefix_releases_its_task_and_reservation(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_content_size_scene(cx);
    draw(cx);

    let mut prefix_ticket = cx.update_window_entity(&host, |host, window, cx| {
        let version = host
            .view
            .graph_snapshot()
            .expect("close-prefix graph version")
            .version();
        issue_content_height_update(host, NodeId::from_u128(10), 172.0);
        let prefix_ticket = host
            .commands
            .enqueue(
                GraphCommand::set_node_data(
                    version,
                    NodeId::from_u128(11),
                    serde_json::json!({ "close-prefix": true }),
                ),
                super::OpenGpuiCommandOrigin::Binding,
            )
            .expect("queued close prefix");
        host.schedule_content_size_prefix(cx);
        assert!(host.content_sizes.has_prefix_task());

        host.close_in_window(window, cx)
            .expect("window close drains the prefix and cancels node sizing");
        assert_eq!(host.content_sizes.pending_command_counts(), None);
        assert!(!host.content_sizes.has_prefix_task());
        assert!(host.content_sizes.ready_is_empty());
        assert!(!host.commands.has_pending());
        prefix_ticket
    });

    cx.run_until_parked();
    assert!(poll_command_ticket(&mut prefix_ticket).is_ok());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::HostClosed));
        assert_eq!(host.content_sizes.pending_command_counts(), None);
        assert!(host.content_sizes.ready_is_empty());
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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

    cx.update_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionSearch
        );
        assert!(host.connection_preview_plan().is_some());
    });

    cx.update_window_entity(&host, |host, window, cx| {
        let request = host.connection_search_request().expect("search request");
        host.cancel_connection_search(&request, window, cx)
            .expect("cancel current search");
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(host.connection_search_request(), None);
    });
    draw(cx);
    cx.update_entity(&host, |host, _| {
        assert!(host.interaction_presentation().connection().is_none());
        assert!(host.connection_preview_plan().is_none());
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: std::rc::Rc::new(std::cell::Cell::new(0)),
        })
    });
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
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
    draw(cx);

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
        .execute(GraphCommand::undo(replaced.version()))
        .into_result()
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
fn selected_edge_reconnect_hit_stays_outside_the_real_port_while_the_cap_remains_attached(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, port_events, cx) = mount_selected_reconnect_scene(cx);
    draw(cx);
    draw(cx);

    let edge = EdgeId::from_u128(30);
    let port = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("connected input bounds");
    let reconnect = cx
        .debug_bounds(&super::scene_render::open_gpui_reconnect_hit_selector(
            edge,
            EdgeEndpoint::To,
        ))
        .expect("selected edge reconnect hit bounds");
    let reconnect_chrome = cx
        .debug_bounds(&super::scene_render::open_gpui_reconnect_chrome_selector(
            edge,
            EdgeEndpoint::To,
        ))
        .expect("selected edge reconnect chrome bounds");
    assert_eq!(reconnect.size.width, px(22.0));
    assert_eq!(reconnect.size.height, px(22.0));
    assert_eq!(reconnect_chrome.size.width, px(3.0));
    assert_eq!(reconnect_chrome.size.height, px(9.0));
    assert_eq!(reconnect_chrome.center(), port.center());
    assert_eq!(reconnect.center().y, port.center().y);
    assert!(reconnect.origin.x + reconnect.size.width < port.origin.x);

    cx.simulate_mouse_move(reconnect.center(), None, Default::default());
    draw_once(cx);
    assert_eq!(
        cx.debug_bounds(&super::scene_render::open_gpui_reconnect_hit_selector(
            edge,
            EdgeEndpoint::To,
        )),
        Some(reconnect),
        "hover emphasis must not resize the reconnect hit target",
    );
    assert_eq!(
        cx.debug_bounds(&super::scene_render::open_gpui_reconnect_chrome_selector(
            edge,
            EdgeEndpoint::To,
        )),
        Some(reconnect_chrome),
        "hover emphasis must not move or resize the endpoint cap",
    );

    let port_target = point(port.origin.x + px(1.0), port.center().y);
    assert!(port_target.x > reconnect.origin.x + reconnect.size.width);
    cx.simulate_mouse_down(port_target, MouseButton::Left, Default::default());

    assert!(
        port_events.get() > 0,
        "the native port remains reachable independently from reconnect chrome",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(port_target, MouseButton::Left, Default::default());
}

#[open_gpui::test]
fn selected_edge_reconnect_hit_remains_reachable_outside_the_port(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, port_events, cx) = mount_selected_reconnect_scene(cx);
    draw(cx);
    draw(cx);

    let edge = EdgeId::from_u128(30);
    let old_input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("connected input bounds");
    let new_input = cx
        .debug_bounds("jellyflow-test-reconnect-input-port")
        .expect("replacement input bounds");
    let reconnect = cx
        .debug_bounds(&super::scene_render::open_gpui_reconnect_hit_selector(
            edge,
            EdgeEndpoint::To,
        ))
        .expect("selected edge reconnect hit bounds");
    let outward = reconnect.center();
    assert!(outward.x < old_input.origin.x);
    let events_before = port_events.get();

    cx.simulate_mouse_down(outward, MouseButton::Left, Default::default());

    assert_eq!(
        port_events.get(),
        events_before,
        "the outward grip belongs to explicit edge reconnect chrome",
    );
    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        assert!(presentation.connection().is_none());
    });

    cx.simulate_mouse_move(
        new_input.center(),
        Some(MouseButton::Left),
        Default::default(),
    );
    cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::ConnectionDrag);
        let binding = presentation
            .connection()
            .expect("active reconnect presentation")
            .binding();
        assert_eq!(
            binding.reconnect_edge().map(EdgeInstanceKey::edge),
            Some(edge)
        );
        assert_eq!(binding.fixed_endpoint(), EdgeEndpoint::From);
    });
    cx.simulate_mouse_up(new_input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();

    let snapshot = runtime.snapshot().expect("reconnected graph");
    let reconnected = snapshot
        .graph()
        .edge(&edge)
        .expect("edge identity retained");
    assert_eq!(reconnected.to, PortId::from_u128(22));
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
        .execute(GraphCommand::undo(reconnected.version()))
        .into_result()
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

#[open_gpui::test]
fn runtime_drop_releases_registered_third_party_capture_while_host_is_detached(
    cx: &mut open_gpui::TestAppContext,
) {
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
    register_test_renderer(&mut registry, move |binding, window, cx| {
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
                    .capture_pointer(HitboxId::placeholder(), window)
                    .expect("registered third-party pointer capture"),
            );
        }
        cx.new(|_| RootOnlyNode)
    });
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        ReplaceableInteractiveScene { host: Some(host) }
    });
    let host = cx.read_entity(&scene, |scene, _| {
        scene.host.clone().expect("mounted scene host")
    });
    draw(cx);

    assert!(capture_lease.borrow().is_some());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    detach_host_but_retain_entity(&scene, &host, cx);
    drop(runtime);
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.close_reason(), Some(ViewCloseReason::RuntimeDropped));
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1,);
        assert!(!host.interaction_owner.has_focus_state());
    });

    cx.update(|window, _| {
        window.capture_pointer(HitboxId::placeholder());
    });
    cx.run_until_parked();
    assert!(
        cx.update(|window, _| window.captured_hitbox()).is_some(),
        "completed terminal cleanup must not release a successor capture",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.interaction_owner.terminal_window_transition_count(), 1,);
        assert!(host.interaction_owner.terminal());
        assert!(!host.interaction_owner.has_focus_state());
    });
    cx.update(|window, _| window.release_pointer());
}

fn assert_settled_host_has_no_pending_work(host: &OpenGpuiSceneHost) {
    let resources = host.resource_snapshot();
    assert!(!host.commands.has_pending());
    assert!(host.frame_transaction.is_none());
    assert!(host.content_sizes.pending_command_counts().is_none());
    assert!(!host.content_sizes.has_prefix_task());
    assert!(host.content_sizes.ready_is_empty());
    assert!(host.pending_blank_canvas_context.is_none());
    assert!(host.lifecycle_bridge.is_some());
    assert!(host.window_owner.is_some());
    assert!(host.terminal_reason.is_none());
    assert_eq!(
        host.interaction_presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(resources.interaction_pending_queries(), 0);
    assert!(!resources.interaction_submission_pending());
    assert_eq!(resources.interaction_ready_prefixes(), 0);
    assert!(!resources.interaction_tick_scheduled());
    assert!(!resources.interaction_tick_frame_requested());
    assert!(!resources.interaction_query_frame_scheduled());
    assert!(!resources.interaction_query_resolution_active());
    assert!(!resources.interaction_native_tick_wake_active());
    assert!(!resources.interaction_native_query_wake_active());
    assert_eq!(resources.protected_instances(), 0);
    assert_eq!(resources.protected_leases(), 0);
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    draw_once(cx);
    cx.run_until_parked();
}

fn test_canvas_rect_contains(bounds: CanvasRect, point: CanvasPoint) -> bool {
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y <= bounds.origin.y + bounds.size.height
}

fn simulate_key_down(cx: &mut open_gpui::VisualTestContext, key: &str, is_held: bool) {
    cx.simulate_event(KeyDownEvent {
        keystroke: Keystroke::parse(key)
            .expect("valid test keystroke")
            .with_simulated_ime(),
        is_held,
        prefer_character_input: false,
    });
}

fn simulate_key_up(cx: &mut open_gpui::VisualTestContext, key: &str) {
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key)
            .expect("valid test keystroke")
            .with_simulated_ime(),
    });
}

fn simulate_key_press(cx: &mut open_gpui::VisualTestContext, key: &str) {
    cx.simulate_keystrokes(key);
    simulate_key_up(cx, key);
}

fn draw_once(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
}

#[open_gpui::test]
fn keyboard_connection_capacity_failure_aborts_then_retries_after_release(
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    draw(cx);
    assert_eq!(leases.borrow().len(), OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY);
    let source_owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(
            host,
            protected_capacity_scene_node_id(),
            PortId::from_u128(20),
        )
    });
    let activate = || {
        OpenGpuiPortEvent::Activation(OpenGpuiPortActivationEvent::new(
            source_owner.clone(),
            OpenGpuiPortActivationSource::Keyboard,
        ))
    };
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
        assert_eq!(
            host.resource_snapshot().protected_instances(),
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
        assert!(host.connection_search_request().is_none());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
        );
        let update = host.interaction_owner.cancel();
        assert!(host.apply_canvas_interaction_result(update, window, cx));
        assert_eq!(
            host.resource_snapshot().protected_instances(),
            OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY - 1
        );
    });
}

fn poll_command_ticket(
    ticket: &mut crate::command_journal::OpenGpuiCommandTicket,
) -> crate::command_journal::OpenGpuiCommandTicketResult {
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match std::future::Future::poll(std::pin::Pin::new(ticket), &mut context) {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("native command ticket is still pending"),
    }
}

fn assert_command_ticket_pending(ticket: &mut crate::command_journal::OpenGpuiCommandTicket) {
    assert_command_ticket_pending_at(ticket, "while command is queued");
}

fn assert_command_ticket_pending_at(
    ticket: &mut crate::command_journal::OpenGpuiCommandTicket,
    stage: &str,
) {
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn mount_replaceable_gesture_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<ReplaceableInteractiveScene>,
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
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| InteractiveRoutedNode {
            binding,
            events: Rc::new(Cell::new(0)),
        })
    });
    let (scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        ReplaceableInteractiveScene { host: Some(host) }
    });
    let host = cx.read_entity(&scene, |scene, _| {
        scene.host.clone().expect("mounted scene host")
    });
    (runtime, scene, host, cx)
}

fn release_last_host_entity(
    scene: &Entity<ReplaceableInteractiveScene>,
    host: Entity<OpenGpuiSceneHost>,
    cx: &mut open_gpui::VisualTestContext,
) {
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    let weak_host = host.downgrade();
    cx.update_entity(scene, |scene, cx| {
        scene.host = None;
        cx.notify();
    });
    draw(cx);
    assert!(weak_host.upgrade().is_some());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());

    drop(host);
    cx.update(|_, _| {});
    cx.run_until_parked();

    assert!(weak_host.upgrade().is_none());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
}

fn detach_host_but_retain_entity(
    scene: &Entity<ReplaceableInteractiveScene>,
    host: &Entity<OpenGpuiSceneHost>,
    cx: &mut open_gpui::VisualTestContext,
) {
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
    cx.update_entity(scene, |scene, cx| {
        scene.host = None;
        cx.notify();
    });
    draw(cx);
    assert!(host.downgrade().upgrade().is_some());
    assert!(cx.update(|window, _| window.captured_hitbox()).is_some());
}

fn mount_gesture_scene_for_graph(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow_core::Graph,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let catalog = catalog();
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph)
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        cx.new(|_| RoutedNode { binding })
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
    mount_content_size_scene_for_graph(cx, content_size_graph())
}

fn mount_content_size_scene_for_graph(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow_core::Graph,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let catalog = catalog();
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph)
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
    register_test_renderer_with_policy(&mut registry, policy, |_binding, _window, cx| {
        cx.new(|_| RootOnlyNode)
    });
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn issue_content_height_update(host: &mut OpenGpuiSceneHost, node: NodeId, height: f32) {
    let instance = host
        .retained_nodes
        .retained_instance(node)
        .cloned()
        .expect("content-sized node instance");
    let effects = host
        .content_sizes
        .issue_stable_height_for_test(&instance, height);
    assert!(host.apply_content_size_effects(effects));
}

fn mount_connection_scene_for_graph(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow_core::Graph,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    mount_connection_scene_for_graph_with_context_handler(cx, graph, None)
}

fn mount_connection_scene_for_graph_with_context_handler(
    cx: &mut open_gpui::TestAppContext,
    graph: jellyflow_core::Graph,
    context_handler: Option<OpenGpuiBlankCanvasContextHandler>,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    &mut open_gpui::VisualTestContext,
) {
    let renderer_kind = graph
        .nodes()
        .values()
        .next()
        .map(|node| node.kind.clone())
        .expect("connection scene graph contains at least one node");
    assert!(
        graph
            .nodes()
            .values()
            .all(|node| node.kind == renderer_kind),
        "connection scene helper expects one semantic node kind",
    );
    let runtime = GraphRuntime::builder(catalog())
        .build(graph)
        .expect("valid graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let mut registry = OpenGpuiNodeRegistry::builder();
    let (kind, incarnation, accessible_name) = if renderer_kind == NodeKindKey::new(EXEC_KIND) {
        (EXEC_KIND, 2, "Scene layer exec node")
    } else {
        (KIND, 1, "Scene layer node")
    };
    register_test_renderer_for_kind(
        &mut registry,
        kind,
        incarnation,
        accessible_name,
        move |binding, _window, cx| {
            cx.new(|_| InteractiveRoutedNode {
                binding,
                events: std::rc::Rc::new(std::cell::Cell::new(0)),
            })
        },
    );
    let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            let mut host = OpenGpuiSceneHost::new(
                view,
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host");
            host.blank_canvas_context_handler = context_handler;
            host
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, cx)
}

fn mount_selection_delete_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    Rc<std::cell::RefCell<Option<FocusHandle>>>,
    Rc<std::cell::RefCell<Vec<Option<String>>>>,
    &mut open_gpui::VisualTestContext,
) {
    let command_labels = Rc::new(std::cell::RefCell::new(Vec::new()));
    let runtime = GraphRuntime::builder(catalog())
        .build(routed_graph())
        .expect("valid selection-delete graph")
        .with_middleware(RecordGraphCommands {
            labels: command_labels.clone(),
        });
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach selection-delete view");
    set_selection_by_id(
        &view,
        vec![NodeId::from_u128(10)],
        vec![EdgeId::from_u128(30)],
    )
    .expect("seed mixed node and edge selection");

    let control_focus = Rc::new(std::cell::RefCell::new(None));
    let captured_control_focus = control_focus.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
        let node = binding
            .read(cx)
            .authoring()
            .expect("selection-delete node authoring")
            .node_id();
        let focus = cx.focus_handle();
        let mut captured = captured_control_focus.borrow_mut();
        if captured.is_none() {
            *captured = Some(focus.clone());
        }
        drop(captured);
        cx.new(move |_| NativeTextControlNode {
            focus,
            id: format!("jellyflow-test-native-text-control-{node:?}"),
        })
    });
    let captured_host = Rc::new(std::cell::RefCell::new(None));
    let scene_host = captured_host.clone();
    let (_scene, cx) = cx.add_window_view(move |_, cx| {
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal(&catalog())
                    .expect("valid selection-delete registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, control_focus, command_labels, cx)
}

fn mount_selected_reconnect_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    GraphRuntime,
    Entity<OpenGpuiSceneHost>,
    std::rc::Rc<std::cell::Cell<u32>>,
    &mut open_gpui::VisualTestContext,
) {
    let edge = EdgeId::from_u128(30);
    let runtime = GraphRuntime::builder(catalog())
        .build(reconnect_graph())
        .expect("valid reconnect graph");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    set_selection_by_id(&view, Vec::new(), vec![edge]).expect("select reconnectable edge");
    let port_events = std::rc::Rc::new(std::cell::Cell::new(0));
    let factory_events = port_events.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_test_renderer(&mut registry, move |binding, _window, cx| {
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
                registry.seal(&catalog()).expect("valid test registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        *scene_host.borrow_mut() = Some(host.clone());
        InteractiveScene { host }
    });
    let host = captured_host.borrow().clone().expect("captured host");
    (runtime, host, port_events, cx)
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
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });

    cx.run_until_parked();
    cx.read_entity(host, |host, _| {
        assert!(!host.active_scene_gesture());
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
    });
}

fn dense_edge_graph(edge_count: usize) -> (jellyflow_core::Graph, Vec<EdgeId>) {
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

fn spatial_edge_graph(grid_side: usize, spacing: f32) -> (jellyflow_core::Graph, Vec<EdgeId>) {
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
) -> (jellyflow_core::Graph, Vec<EdgeId>) {
    bounded_edge_graph_with(visible_edges, offscreen_edges, |output, input| {
        Edge::new(EdgeKind::Data, output, input)
    })
}

fn bounded_edge_graph_with(
    visible_edges: usize,
    offscreen_edges: usize,
    make_edge: impl FnMut(PortId, PortId) -> Edge,
) -> (jellyflow_core::Graph, Vec<EdgeId>) {
    bounded_edge_graph_with_gap(visible_edges, offscreen_edges, 320.0, make_edge)
}

fn bounded_edge_graph_with_gap(
    visible_edges: usize,
    offscreen_edges: usize,
    edge_span: f32,
    mut make_edge: impl FnMut(PortId, PortId) -> Edge,
) -> (jellyflow_core::Graph, Vec<EdgeId>) {
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
                    x: origin_x + edge_span,
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
    publish_all_edge_geometry_with_offset(view, selected, 0.0);
}

fn publish_all_edge_geometry_with_offset(
    view: &ViewSession,
    selected: impl IntoIterator<Item = EdgeId>,
    port_y_offset: f32,
) {
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
                ViewPoint::new(x, bounds.origin.y + 44.0 + port_y_offset),
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
    set_selection_by_id(view, Vec::new(), selected).expect("select priority edge");
    let port_count = view
        .graph_snapshot()
        .expect("edge geometry graph snapshot")
        .graph()
        .ports()
        .len();
    let frame = view
        .frame(
            FrameRequest::new(frame_bounds, RendererStamp::new(RENDERER, 1))
                .with_materialization_budget(
                    FrameMaterializationBudget::default().with_port_scan_limit(port_count),
                ),
        )
        .expect("materialize all edge endpoints");
    let collector = frame.collector();
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

fn protected_capacity_scene_graph() -> jellyflow_core::Graph {
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

fn protected_capacity_search_graph() -> jellyflow_core::Graph {
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

fn routed_graph() -> jellyflow_core::Graph {
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

fn overlapping_edge_graph() -> (jellyflow_core::Graph, EdgeId, EdgeId) {
    let back_source = NodeId::from_u128(10);
    let back_target = NodeId::from_u128(11);
    let front_source = NodeId::from_u128(12);
    let front_target = NodeId::from_u128(13);
    let back_output = PortId::from_u128(20);
    let back_input = PortId::from_u128(21);
    let front_output = PortId::from_u128(22);
    let front_input = PortId::from_u128(23);
    let back = EdgeId::from_u128(30);
    let front = EdgeId::from_u128(31);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        back_source,
        node(CanvasPoint { x: 40.0, y: 80.0 }, back_output),
    );
    graph.insert_node(
        back_target,
        node(CanvasPoint { x: 360.0, y: 80.0 }, back_input),
    );
    graph.insert_node(
        front_source,
        node(CanvasPoint { x: 40.0, y: 80.0 }, front_output),
    );
    graph.insert_node(
        front_target,
        node(CanvasPoint { x: 360.0, y: 80.0 }, front_input),
    );
    graph.insert_port(
        back_output,
        port(back_source, "back-output", PortDirection::Out),
    );
    graph.insert_port(
        back_input,
        port(back_target, "back-input", PortDirection::In),
    );
    graph.insert_port(
        front_output,
        port(front_source, "front-output", PortDirection::Out),
    );
    graph.insert_port(
        front_input,
        port(front_target, "front-input", PortDirection::In),
    );
    graph.insert_edge(back, Edge::new(EdgeKind::Data, back_output, back_input));
    graph.insert_edge(front, Edge::new(EdgeKind::Data, front_output, front_input));
    (
        graph.build().expect("valid overlapping edge graph"),
        back,
        front,
    )
}

fn restricted_edge_action_graph() -> jellyflow_core::Graph {
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

fn unconnected_graph() -> jellyflow_core::Graph {
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

fn content_size_graph() -> jellyflow_core::Graph {
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

fn content_size_capacity_graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x64_002));
    for index in 0..=OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
        graph.insert_node(
            NodeId::from_u128(index as u128 + 1),
            portless_node(CanvasPoint { x: 400.0, y: 300.0 }),
        );
    }
    graph.insert_node(
        protected_capacity_scene_node_id(),
        portless_node(CanvasPoint { x: 40.0, y: 80.0 }),
    );
    graph.build().expect("valid content-size capacity graph")
}

fn overlapping_target_graph() -> jellyflow_core::Graph {
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

fn overlapping_hover_graph() -> jellyflow_core::Graph {
    let source = NodeId::from_u128(10);
    let back = NodeId::from_u128(11);
    let front = NodeId::from_u128(12);
    let output = PortId::from_u128(20);
    let back_first = PortId::from_u128(21);
    let back_middle = PortId::from_u128(23);
    let covered_back_port = PortId::from_u128(22);
    let front_input = PortId::from_u128(99);
    let target_position = CanvasPoint { x: 360.0, y: 80.0 };
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x90_000));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    let mut back_node = node(target_position, back_first);
    back_node.ports = vec![back_first, back_middle, covered_back_port];
    graph.insert_node(back, back_node);
    graph.insert_node(front, node(target_position, front_input));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.insert_port(back_first, port(back, "back-first", PortDirection::In));
    graph.insert_port(back_middle, port(back, "back-middle", PortDirection::In));
    graph.insert_port(
        covered_back_port,
        port(back, "back-covered", PortDirection::In),
    );
    graph.insert_port(front_input, port(front, "front-input", PortDirection::In));
    graph.build().expect("valid overlapping hover graph")
}

fn locked_source_graph() -> jellyflow_core::Graph {
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

fn locked_peer_graph() -> jellyflow_core::Graph {
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

fn single_node_graph() -> jellyflow_core::Graph {
    let source = NodeId::from_u128(10);
    let output = PortId::from_u128(20);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    graph.build().expect("valid single-node graph")
}

fn occupied_input_graph() -> jellyflow_core::Graph {
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

fn reconnect_graph() -> jellyflow_core::Graph {
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

fn dynamic_port(node: NodeId, id: PortId) -> Port {
    port(node, &format!("dynamic-{id:?}"), PortDirection::In)
}

fn dynamic_port_graph(node_id: NodeId, ports: &[PortId]) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x80_000));
    let mut graph_node = portless_node(CanvasPoint { x: 40.0, y: 40.0 });
    graph_node.ports = ports.to_vec();
    graph.insert_node(node_id, graph_node);
    for id in ports.iter().copied() {
        graph.insert_port(id, dynamic_port(node_id, id));
    }
    graph.build().expect("valid dynamic-port graph")
}

fn portless_nodes_graph(nodes: &[NodeId]) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x81_000));
    for (index, node_id) in nodes.iter().copied().enumerate() {
        graph.insert_node(
            node_id,
            portless_node(CanvasPoint {
                x: index as f32 * 220.0,
                y: 40.0,
            }),
        );
    }
    graph.build().expect("valid portless node graph")
}

fn portless_catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .expect("unique portless node kind");
    catalog
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND)
                .port(jellyflow_runtime::schema::NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .port(jellyflow_runtime::schema::NodePortSpec::new(
                    "output",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .expect("unique node kind");
    catalog
        .register(
            NodeKindSpec::builder(EXEC_KIND)
                .port(jellyflow_runtime::schema::NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Exec,
                    PortCapacity::Single,
                ))
                .port(jellyflow_runtime::schema::NodePortSpec::new(
                    "output",
                    PortDirection::Out,
                    PortKind::Exec,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .expect("unique exec node kind");
    catalog
}

fn connection_search_creation(
    request: &jellyflow_runtime::interaction::ConnectionSearchRequest,
    candidates: &jellyflow_runtime::graph::CreateFromWireCandidates,
    candidate_index: usize,
) -> jellyflow_runtime::schema::NodeCreation {
    let candidate = &candidates.candidates()[candidate_index];
    let creation = catalog()
        .instantiate_node(
            candidate.node_kind(),
            CanvasRect {
                origin: request.canvas_position(),
                size: CanvasSize {
                    width: 180.0,
                    height: 100.0,
                },
            },
        )
        .expect("product supplies valid initial geometry");
    assert_eq!(creation.node().kind_version, candidate.kind_version());
    creation
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
                    .seal(&catalog())
                    .expect("valid empty registry"),
                viewport(),
            )
            .expect("valid Open GPUI scene host")
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
