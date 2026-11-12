use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll, Waker};

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphBuilder, GraphId,
    NodeGraphConnectionMode, NodeId, NodeKindKey, PortId,
};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::commands::{ConnectionEdit, GraphCommand, IncidentEdgePolicy, TopologyEdit};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::graph::{
    CreateFromWireCandidates, Diagnostic, DiagnosticTarget, GraphRuntime, GraphRuntimeError,
};
use jellyflow_runtime::interaction::{
    CanvasInteractionPhase, ConnectionSearchRequest, ConnectionSearchStatus,
    InteractionTerminalReason,
};
use jellyflow_runtime::profile::GraphProfile;
use jellyflow_runtime::view::{PublishedPortGeometry, ViewSessionOptions, WeakViewSession};
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::{
    AppContext, Bounds, Context, Entity, MouseButton, ParentElement, Pixels, Render, Styled,
    VisualTestContext, Window, div, point, px, size,
};
use open_gpui_canvas_jellyflow::connection_search::{
    NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR, NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR,
    NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR, NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR,
    NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR, NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR,
    NATIVE_CONNECTION_SEARCH_SELECTOR, NATIVE_GRAPH_SURFACE_SELECTOR, NativeConnectionSearchFuture,
    NativeConnectionSearchSource, NativeConnectionSearchSourceError, NativeGraphSurface,
    native_connection_search_candidate_selector, native_connection_search_input_selector,
};
use open_gpui_canvas_jellyflow::nodes::{
    BLUEPRINT_ADD_INPUT_SELECTOR, BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND,
    BLUEPRINT_DISCARD_SELECTOR, BLUEPRINT_NODE_SELECTOR, NativeNodeResources, SHADER_MULTIPLY_KIND,
    SHADER_MULTIPLY_RENDERER, SHADER_NODE_SELECTOR, SHADER_PREVIEW_CONTROL_SELECTOR,
    SHADER_PREVIEW_VALUE_SELECTOR, blueprint_body_selector, blueprint_header_metadata_selector,
    blueprint_header_title_selector, blueprint_pin_down_selector, blueprint_pin_label_selector,
    blueprint_pin_remove_selector, blueprint_pin_up_selector, blueprint_port_label_selector,
    blueprint_port_selector, native_node_catalog, native_node_registry, shader_body_selector,
    shader_connected_input_selector, shader_header_metadata_selector, shader_header_title_selector,
    shader_inline_editor_selector, shader_inline_value_selector, shader_port_label_selector,
    shader_port_selector,
};
use open_gpui_canvas_jellyflow::product_catalog::native_product_catalog;
use open_gpui_canvas_jellyflow::product_domain::blueprint::{
    BlueprintError, BlueprintIntent, BlueprintObservation, BlueprintPin,
};
use open_gpui_canvas_jellyflow::product_theme::NativeProductTheme;
use open_gpui_ui_components::gpui_adapter::init_text_input;

const SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-shader-factor-command-diagnostic";
const BLUEPRINT_COMMAND_PENDING_SELECTOR: &str = "jellyflow-native-blueprint-command-pending";
const BLUEPRINT_COMMAND_DIAGNOSTIC_SELECTOR: &str = "jellyflow-native-blueprint-command-diagnostic";
const BLUEPRINT_RETRY_SELECTOR: &str = "jellyflow-native-blueprint-retry";

fn native_runtime(graph: Graph) -> Result<GraphRuntime, GraphRuntimeError> {
    GraphRuntime::builder(native_node_catalog().expect("native node catalog")).build(graph)
}

fn native_registry(cx: &mut open_gpui::App) -> OpenGpuiNodeRegistry {
    let catalog = native_node_catalog().expect("native node catalog");
    native_node_registry(
        NativeNodeResources::ready_default(cx)
            .with_product_theme(NativeProductTheme::shader_blueprint()),
        &catalog,
    )
}

fn native_profiled_runtime(
    graph: Graph,
    profile: Box<dyn GraphProfile>,
) -> Result<GraphRuntime, GraphRuntimeError> {
    GraphRuntime::builder(native_node_catalog().expect("native node catalog"))
        .with_profile(profile)
        .build(graph)
}

#[open_gpui::test]
fn native_shader_entity_mounts_through_scene_host_and_submits_real_port_feedback(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(shader_graph()).expect("shader runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader view");
    let weak_view = view.downgrade();
    let registry = cx.update(native_registry);
    assert_eq!(
        registry
            .renderer_stamp(&NodeKindKey::new(SHADER_MULTIPLY_KIND), 1)
            .map(|renderer| renderer.key()),
        Some(SHADER_MULTIPLY_RENDERER),
    );

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(view, registry, viewport()).expect("valid Open GPUI scene host")
    });
    draw(cx);

    assert!(cx.debug_bounds(SHADER_NODE_SELECTOR).is_some());
    let preview_control = cx
        .debug_bounds(SHADER_PREVIEW_CONTROL_SELECTOR)
        .expect("native shader preview control");
    assert!(cx.debug_bounds(SHADER_PREVIEW_VALUE_SELECTOR).is_none());
    let input_port = cx
        .debug_bounds(&shader_port_selector(input_a_id()))
        .expect("native shader input port");
    let output_port = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output port");
    let inline_editor = cx
        .debug_bounds(&shader_inline_editor_selector(node_id(), "a"))
        .expect("native shader inline editor");
    assert!(!input_port.intersects(&inline_editor));
    assert!(!output_port.intersects(&inline_editor));
    assert!(input_port.center().y >= inline_editor.origin.y);
    assert!(input_port.center().y <= inline_editor.origin.y + inline_editor.size.height);

    let geometry = weak_view.geometry_snapshot().expect("shader geometry");
    let node = geometry.node(node_id()).expect("shader node geometry");
    assert!(node.is_fresh());
    for port in [input_a_id(), input_b_id(), output_id()] {
        let layout = geometry.port(port).expect("shader port geometry");
        assert_eq!(layout.node(), node_id());
        assert!(layout.visual_bounds().size.is_positive_finite());
    }

    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(host.last_diagnostic().is_none());
    });

    cx.simulate_click(preview_control.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(SHADER_PREVIEW_VALUE_SELECTOR).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
    });
}

#[open_gpui::test]
fn native_shader_surface_tab_moves_from_graph_focus_into_the_graph_focus_domain(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(shader_graph()).expect("shader keyboard runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader keyboard view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let title = cx
        .debug_bounds(&shader_header_title_selector(node_id()))
        .expect("shader node title");
    cx.simulate_click(title.center(), Default::default());
    draw(cx);
    let before = cx
        .update(|window, cx| window.focused(cx))
        .expect("graph focus before Tab");
    cx.update(|window, cx| {
        assert!(surface.read(cx).graph_has_focus(window, cx));
    });

    cx.simulate_keystrokes("tab");
    draw(cx);

    let forward = cx
        .update(|window, cx| window.focused(cx))
        .expect("graph focus after Tab");
    assert_ne!(forward, before);
    cx.update(|window, cx| {
        assert!(surface.read(cx).graph_has_focus(window, cx));
    });

    cx.simulate_keystrokes("shift-tab");
    draw(cx);

    cx.update(|window, cx| {
        assert_ne!(window.focused(cx), Some(forward));
        assert!(surface.read(cx).graph_has_focus(window, cx));
    });
}

#[open_gpui::test]
fn shader_port_layout_anchor_and_hit_target_stay_stable_across_zoom_and_hover(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(shader_graph()).expect("shader port profile runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader port profile view");
    let weak_view = view.downgrade();
    let registry = cx.update(native_registry);
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(view, registry, viewport()).expect("valid Open GPUI scene host")
    });

    for zoom in [0.5, 1.0, 2.0] {
        weak_view
            .set_viewport(
                ViewportTransform::new(CanvasPoint::default(), zoom)
                    .expect("valid shader port profile viewport"),
            )
            .expect("set shader port profile viewport");
        draw(cx);

        let transform = weak_view.viewport_transform().expect("shader viewport");
        let geometry = weak_view.geometry_snapshot().expect("shader geometry");
        for port_id in [input_a_id(), output_id()] {
            let port = geometry
                .port(port_id)
                .expect("published shader port")
                .clone();
            assert_product_port_profile(&port, &transform, zoom >= 1.0);
            assert_native_port_hit_boundary(cx, &host, &port, &transform);
            let after_probe = weak_view
                .geometry_snapshot()
                .expect("shader geometry after boundary probe")
                .port(port_id)
                .expect("shader port after boundary probe")
                .clone();
            assert_eq!(after_probe.visual_bounds(), port.visual_bounds());
            assert_eq!(after_probe.hit_bounds(), port.hit_bounds());
            assert_eq!(after_probe.route_anchor(), port.route_anchor());
        }

        if zoom == 1.0 {
            let before = geometry
                .port(output_id())
                .expect("output before hover")
                .clone();
            let marker = cx
                .debug_bounds(&shader_port_selector(output_id()))
                .expect("detail output marker");
            cx.simulate_mouse_move(marker.center(), None, Default::default());
            draw(cx);
            let after = weak_view
                .geometry_snapshot()
                .expect("shader geometry after hover")
                .port(output_id())
                .expect("output after hover")
                .clone();
            assert_eq!(after.visual_bounds(), before.visual_bounds());
            assert_eq!(after.route_anchor(), before.route_anchor());
        }
    }
}

fn assert_native_port_hit_boundary(
    cx: &mut VisualTestContext,
    host: &Entity<OpenGpuiSceneHost>,
    port: &PublishedPortGeometry,
    transform: &ViewportTransform,
) {
    let hit = port.hit_bounds().expect("visible port hit bounds");
    let min = transform
        .screen_point_for_canvas(hit.origin)
        .expect("finite hit origin");
    let max = transform
        .screen_point_for_canvas(CanvasPoint {
            x: hit.origin.x + hit.size.width,
            y: hit.origin.y + hit.size.height,
        })
        .expect("finite hit extent");
    let center_y = (min.y + max.y) * 0.5;
    let (inside_x, outside_x) = match port.outward() {
        HandlePosition::Left => (min.x + 0.5, min.x - 0.5),
        HandlePosition::Right => (max.x - 0.5, max.x + 0.5),
        HandlePosition::Top => (min.x + (max.x - min.x) * 0.5, min.x),
        HandlePosition::Bottom => (min.x + (max.x - min.x) * 0.5, max.x),
    };
    assert!(matches!(
        port.outward(),
        HandlePosition::Left | HandlePosition::Right
    ));
    let outside = point(px(outside_x), px(center_y));
    cx.simulate_mouse_down(outside, MouseButton::Left, Default::default());
    cx.read_entity(host, |host, _| {
        assert!(
            host.active_port_drag().is_none(),
            "a point 0.5 px beyond the published hit boundary must not acquire a port",
        );
    });
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());

    let inside = point(px(inside_x), px(center_y));
    cx.simulate_mouse_down(inside, MouseButton::Left, Default::default());
    cx.read_entity(host, |host, _| {
        assert_eq!(
            host.active_port_drag().map(|owner| owner.port()),
            Some(port.port()),
            "a point 0.5 px inside the published hit boundary must acquire the exact port",
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(inside, MouseButton::Left, Default::default());
    draw(cx);
}

#[open_gpui::test]
fn shader_and_blueprint_layout_contract_holds_in_compact_window(
    cx: &mut open_gpui::TestAppContext,
) {
    assert_shader_layout_contract(cx, 960.0, 640.0);
}

#[open_gpui::test]
fn shader_and_blueprint_layout_contract_holds_in_normal_window(cx: &mut open_gpui::TestAppContext) {
    assert_shader_layout_contract(cx, 1_440.0, 900.0);
}

#[open_gpui::test]
fn connected_input_hides_and_restores_retained_inline_state_across_history(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(shader_graph()).expect("shader runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let inline_editor = shader_inline_editor_selector(node_id(), "a");
    let initial_value = shader_inline_value_selector(node_id(), "a", 0.75);
    let edited_value = shader_inline_value_selector(node_id(), "a", 0.80);
    let connected_status = shader_connected_input_selector(node_id(), "a");
    let editor_bounds = cx
        .debug_bounds(&inline_editor)
        .expect("disconnected input inline editor");
    assert!(cx.debug_bounds(&initial_value).is_some());
    assert!(cx.debug_bounds(&connected_status).is_none());

    cx.simulate_click(editor_bounds.center(), Default::default());
    cx.simulate_keystrokes("up");
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds(SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR)
            .is_none()
    );
    assert!(cx.debug_bounds(&edited_value).is_some());
    let edited = runtime.snapshot().expect("revisioned inline factor");
    assert_eq!(edited.revision(), 1);
    let persisted_factor = edited.graph().node(&node_id()).unwrap().data["factor"]
        .as_f64()
        .expect("persisted numeric factor");
    assert!((persisted_factor - 0.8).abs() < 0.000_01);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    runtime
        .execute(
            GraphCommand::topology(
                edited.version(),
                TopologyEdit::new().connection(ConnectionEdit::connect(
                    edge_id(),
                    source_output_id(),
                    input_a_id(),
                    NodeGraphConnectionMode::Strict,
                )),
            )
            .with_label("connect shader input"),
        )
        .applied_revision()
        .expect("connect applies");
    draw(cx);

    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&edge_id())
            .is_some()
    );
    assert!(cx.debug_bounds(&inline_editor).is_none());
    assert!(cx.debug_bounds(&connected_status).is_some());
    assert_fresh_input_geometry(&weak_view);
    assert_single_retained_mount(cx, &host);

    let connected = runtime.snapshot().expect("connected shader snapshot");
    runtime
        .execute(
            GraphCommand::topology(
                connected.version(),
                TopologyEdit::new().connection(ConnectionEdit::disconnect(edge_id())),
            )
            .with_label("disconnect shader input"),
        )
        .applied_revision()
        .expect("disconnect applies");
    draw(cx);

    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&edge_id())
            .is_none()
    );
    assert!(cx.debug_bounds(&inline_editor).is_some());
    assert!(cx.debug_bounds(&edited_value).is_some());
    assert_fresh_input_geometry(&weak_view);
    assert_single_retained_mount(cx, &host);

    let disconnected = runtime.snapshot().expect("disconnected shader snapshot");
    runtime
        .execute(GraphCommand::undo(disconnected.version()))
        .applied_revision()
        .expect("undo applies");
    draw(cx);
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&edge_id())
            .is_some()
    );
    assert!(cx.debug_bounds(&inline_editor).is_none());
    assert!(cx.debug_bounds(&connected_status).is_some());
    assert_fresh_input_geometry(&weak_view);
    assert_single_retained_mount(cx, &host);

    let undone = runtime.snapshot().expect("undone shader snapshot");
    runtime
        .execute(GraphCommand::redo(undone.version()))
        .applied_revision()
        .expect("redo applies");
    draw(cx);
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&edge_id())
            .is_none()
    );
    assert!(cx.debug_bounds(&inline_editor).is_some());
    assert!(cx.debug_bounds(&edited_value).is_some());
    assert_fresh_input_geometry(&weak_view);
    assert_single_retained_mount(cx, &host);
}

#[open_gpui::test]
fn shader_wire_drop_filters_native_search_and_commits_one_atomic_topology_command(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader search runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader search view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let graph_bounds = cx
        .debug_bounds(NATIVE_GRAPH_SURFACE_SELECTOR)
        .expect("native graph surface bounds");
    cx.read_entity(&surface, |surface, cx| {
        let host = surface.host().read(cx);
        let measured = host.surface_size().expect("measured native surface");
        assert_eq!(measured.width, graph_bounds.size.width.as_f32());
        assert_eq!(measured.height, graph_bounds.size.height.as_f32());
        assert_eq!(host.frame_bounds().size.width, measured.width);
        assert_eq!(host.frame_bounds().size.height, measured.height);
    });

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    let drop_point = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(drop_point, MouseButton::Left, Default::default());
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native create-from-wire filter");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("Blueprint");
    draw(cx);

    let candidate = cx
        .debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .expect("compatible Blueprint candidate");
    assert!(
        candidate.center().y <= px(720.0),
        "native search must stay inside the graph surface: {candidate:?}"
    );
    cx.simulate_click(candidate.center(), Default::default());
    draw(cx);

    let snapshot = runtime.snapshot().expect("created shader topology");
    assert_eq!(
        snapshot.graph().nodes().len(),
        3,
        "candidate click must reach the native button"
    );
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let created = snapshot
        .graph()
        .nodes()
        .values()
        .find(|node| node.kind.0 == BLUEPRINT_CALL_KIND)
        .expect("Blueprint node created from native search");
    assert_eq!(created.pos, CanvasPoint { x: 700.0, y: 500.0 });

    runtime
        .execute(GraphCommand::undo(snapshot.version()))
        .applied_revision()
        .expect("undo applies");
    let undone = runtime.snapshot().expect("undone shader topology");
    assert_eq!(undone.graph().nodes().len(), 2);
    assert!(undone.graph().edges().is_empty());

    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_none()
        );
    });
}

#[open_gpui::test]
fn shader_native_search_enter_submits_the_active_filtered_identity_once(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader keyboard-search runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader keyboard-search view");
    let (_surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);

    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native create-from-wire filter");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("Blueprint");
    draw(cx);
    assert!(
        cx.debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .is_some(),
    );

    cx.simulate_keystrokes("enter");
    draw(cx);

    let snapshot = runtime
        .snapshot()
        .expect("keyboard-created shader topology");
    assert_eq!(snapshot.graph().nodes().len(), 3);
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert!(
        snapshot
            .graph()
            .nodes()
            .values()
            .any(|node| node.kind.0 == BLUEPRINT_CALL_KIND),
    );
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
}

#[open_gpui::test]
fn shader_native_search_roving_focus_keeps_active_and_enter_target_aligned(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader roving-search runtime");
    let before = runtime
        .snapshot()
        .expect("shader graph before roving search");
    let multiply_count_before = before
        .graph()
        .nodes()
        .values()
        .filter(|node| node.kind.0 == SHADER_MULTIPLY_KIND)
        .count();
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader roving-search view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    open_shader_output_search(cx);
    draw(cx);
    let first = native_connection_search_candidate_selector(BLUEPRINT_CALL_KIND, "value");
    let second = native_connection_search_candidate_selector(SHADER_MULTIPLY_KIND, "a");
    assert!(cx.debug_bounds(&first).is_some());
    assert!(cx.debug_bounds(&second).is_some());
    assert!(cx.update(|window, cx| { surface.read(cx).connection_search_has_focus(window, cx) }));

    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(cx.focused_debug_selector().as_deref(), Some(first.as_str()));

    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        cx.focused_debug_selector().as_deref(),
        Some(second.as_str())
    );

    cx.simulate_keystrokes("enter");
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    let after = runtime
        .snapshot()
        .expect("shader graph after roving commit");
    assert_eq!(
        after
            .graph()
            .nodes()
            .values()
            .filter(|node| node.kind.0 == SHADER_MULTIPLY_KIND)
            .count(),
        multiply_count_before + 1,
    );
    assert_eq!(after.graph().edges().len(), 1);
}

#[open_gpui::test]
fn ordinary_shader_wire_rejection_is_owned_by_the_shared_interaction_session(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(two_catalog_shader_nodes(TypeDesc::String))
        .expect("typed shader rejection runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("typed shader rejection view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let rejected_source = cx
        .debug_bounds(&shader_port_selector(typed_source_output_id()))
        .expect("string shader output");
    let rejected_target = cx
        .debug_bounds(&shader_port_selector(typed_target_input_id()))
        .expect("float shader input");
    let before = runtime.snapshot().expect("before rejected native wire");
    let history_before = runtime
        .history_state()
        .expect("history before rejected native wire");

    cx.simulate_mouse_down(
        rejected_source.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_up(
        rejected_target.center(),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);

    let rejected = runtime.snapshot().expect("after rejected native wire");
    assert!(before.ptr_eq(&rejected));
    assert_eq!(rejected.revision(), before.revision());
    assert!(rejected.graph().edges().is_empty());
    assert_eq!(runtime.history_state().unwrap(), history_before);
    cx.read_entity(&surface, |surface, cx| {
        let presentation = surface.host().read(cx).interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            presentation.terminal(),
            Some(InteractionTerminalReason::InvalidTarget)
        );
    });

    let valid_source = cx
        .debug_bounds(&shader_port_selector(typed_target_output_id()))
        .expect("float shader output");
    let valid_target = cx
        .debug_bounds(&shader_port_selector(typed_source_input_a_id()))
        .expect("float shader input");
    cx.simulate_mouse_down(valid_source.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(valid_target.center(), MouseButton::Left, Default::default());
    draw(cx);

    assert_eq!(
        runtime
            .snapshot()
            .expect("recovered native wire")
            .graph()
            .edges()
            .len(),
        1
    );
    cx.read_entity(&surface, |surface, cx| {
        let presentation = surface.host().read(cx).interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            presentation.terminal(),
            Some(InteractionTerminalReason::Applied)
        );
    });
}

#[open_gpui::test]
fn shader_native_search_escape_cancels_without_graph_mutation(cx: &mut open_gpui::TestAppContext) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader cancel runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader cancel view");
    let (_surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());

    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native create-from-wire filter");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("not-a-compatible-node");
    draw(cx);
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR)
            .is_some()
    );

    cx.simulate_keystrokes("escape");
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    let snapshot = runtime.snapshot().expect("cancelled shader search");
    assert_eq!(snapshot.revision(), 0);
    assert_eq!(snapshot.graph().nodes().len(), 2);
    assert!(snapshot.graph().edges().is_empty());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[open_gpui::test]
fn shader_native_search_outside_click_restores_focus_without_graph_mutation(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader outside-click runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader outside-click view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let inline_editor = cx
        .debug_bounds(&shader_inline_editor_selector(node_id(), "a"))
        .expect("shader inline editor");
    cx.simulate_click(inline_editor.center(), Default::default());
    draw(cx);
    let previous_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("inline editor focus");
    let before = runtime.snapshot().expect("before outside-click search");
    let history_before = runtime
        .history_state()
        .expect("before outside-click history");

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
    cx.update(|window, cx| {
        assert_ne!(window.focused(cx), Some(previous_focus.clone()));
    });
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_some()
        );
    });

    let backdrop = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR)
        .expect("application-owned search backdrop");
    cx.simulate_click(
        point(backdrop.origin.x + px(2.0), backdrop.origin.y + px(2.0)),
        Default::default(),
    );
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.update(|window, cx| {
        assert_eq!(window.focused(cx), Some(previous_focus.clone()));
    });
    let after = runtime.snapshot().expect("after outside-click search");
    assert!(before.ptr_eq(&after));
    assert_eq!(after.revision(), before.revision());
    assert_eq!(after.graph().nodes().len(), before.graph().nodes().len());
    assert_eq!(after.graph().edges().len(), before.graph().edges().len());
    assert_eq!(runtime.history_state().unwrap(), history_before);
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_none()
        );
    });
}

#[open_gpui::test]
fn terminal_host_cleanup_moves_focus_off_the_removed_search_input(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader focus runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader focus view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native create-from-wire filter");
    cx.simulate_click(input.center(), Default::default());
    let search_focus = cx
        .update(|window, cx| window.focused(cx))
        .expect("search input focus");

    cx.update(|window, cx| {
        surface.update(cx, |surface, cx| {
            surface.host().update(cx, |host, cx| {
                host.close_in_window(window, cx)
                    .expect("close native graph host");
                cx.notify();
            });
        });
    });
    cx.run_until_parked();
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.update(|window, cx| {
        assert_ne!(
            window.focused(cx),
            Some(search_focus.clone()),
            "a removed search controller must not keep hidden focus"
        );
    });
}

#[open_gpui::test]
fn shader_native_search_rebinds_a_stale_candidate_before_one_atomic_commit(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader stale-search runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader stale-search view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);

    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native create-from-wire filter");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("Blueprint");
    draw(cx);
    let candidate = cx
        .debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .expect("compatible Blueprint candidate");

    let current = runtime.snapshot().expect("current shader snapshot");
    runtime
        .execute(GraphCommand::set_node_data(
            current.version(),
            node_id(),
            serde_json::json!({"changed": true}),
        ))
        .applied_revision()
        .expect("intervening shader edit applies");

    cx.simulate_click(candidate.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    let snapshot = runtime.snapshot().expect("freshly rebound shader snapshot");
    assert_eq!(snapshot.graph().nodes().len(), 3);
    assert_eq!(snapshot.graph().edges().len(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    cx.read_entity(&surface, |surface, cx| {
        let host = surface.host().read(cx);
        assert!(host.connection_search_request().is_none());
        assert_eq!(
            host.interaction_presentation().terminal(),
            Some(InteractionTerminalReason::Applied)
        );
    });
}

struct FailOnceConnectionSearchSource {
    attempts: AtomicU8,
}

impl FailOnceConnectionSearchSource {
    fn new() -> Self {
        Self {
            attempts: AtomicU8::new(0),
        }
    }

    fn attempts(&self) -> u8 {
        self.attempts.load(Ordering::SeqCst)
    }
}

impl NativeConnectionSearchSource for FailOnceConnectionSearchSource {
    fn candidates(
        &self,
        _request: ConnectionSearchRequest,
        authoritative: CreateFromWireCandidates,
    ) -> NativeConnectionSearchFuture {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if attempt == 0 {
                Err(NativeConnectionSearchSourceError::new(
                    "application-owned candidate query failed",
                ))
            } else {
                Ok(authoritative)
            }
        })
    }
}

#[derive(Default)]
struct DeferredConnectionSearchSource {
    attempts: Mutex<Vec<Arc<DeferredConnectionSearchAttempt>>>,
}

impl DeferredConnectionSearchSource {
    fn attempts(&self) -> usize {
        self.attempts
            .lock()
            .expect("deferred search attempts lock")
            .len()
    }

    fn attempt(&self, index: usize) -> Arc<DeferredConnectionSearchAttempt> {
        Arc::clone(
            self.attempts
                .lock()
                .expect("deferred search attempts lock")
                .get(index)
                .expect("deferred search attempt"),
        )
    }
}

impl NativeConnectionSearchSource for DeferredConnectionSearchSource {
    fn candidates(
        &self,
        _request: ConnectionSearchRequest,
        authoritative: CreateFromWireCandidates,
    ) -> NativeConnectionSearchFuture {
        let attempt = Arc::new(DeferredConnectionSearchAttempt::new(authoritative));
        self.attempts
            .lock()
            .expect("deferred search attempts lock")
            .push(Arc::clone(&attempt));
        Box::pin(DeferredConnectionSearchFuture { attempt })
    }
}

struct DeferredConnectionSearchAttempt {
    state: Mutex<DeferredConnectionSearchAttemptState>,
    dropped: AtomicBool,
}

struct DeferredConnectionSearchAttemptState {
    authoritative: Option<CreateFromWireCandidates>,
    result: Option<Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>>,
    waker: Option<Waker>,
}

impl DeferredConnectionSearchAttempt {
    fn new(authoritative: CreateFromWireCandidates) -> Self {
        Self {
            state: Mutex::new(DeferredConnectionSearchAttemptState {
                authoritative: Some(authoritative),
                result: None,
                waker: None,
            }),
            dropped: AtomicBool::new(false),
        }
    }

    fn complete_successfully(&self) {
        let waker = {
            let mut state = self.state.lock().expect("deferred search attempt lock");
            let authoritative = state
                .authoritative
                .take()
                .expect("deferred search authoritative candidates");
            state.result = Some(Ok(authoritative));
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn complete_with_error(&self, message: impl Into<String>) {
        let waker = {
            let mut state = self.state.lock().expect("deferred search attempt lock");
            state.authoritative = None;
            state.result = Some(Err(NativeConnectionSearchSourceError::new(message)));
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    fn was_dropped(&self) -> bool {
        self.dropped.load(Ordering::SeqCst)
    }
}

struct DeferredConnectionSearchFuture {
    attempt: Arc<DeferredConnectionSearchAttempt>,
}

impl Future for DeferredConnectionSearchFuture {
    type Output = Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>;

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let mut state = self
            .attempt
            .state
            .lock()
            .expect("deferred search attempt lock");
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for DeferredConnectionSearchFuture {
    fn drop(&mut self) {
        self.attempt.dropped.store(true, Ordering::SeqCst);
    }
}

struct CompactGraphSurface {
    graph: Entity<NativeGraphSurface>,
}

impl Render for CompactGraphSurface {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().w(px(680.0)).h(px(640.0)).child(self.graph.clone())
    }
}

#[open_gpui::test]
fn shader_native_search_recovers_after_candidate_query_failure_without_mutation(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader query-recovery runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader query-recovery view");
    let search_source = Arc::new(FailOnceConnectionSearchSource::new());
    let source_observer = Arc::clone(&search_source);
    let (surface, cx) = cx.add_window_view(move |_, cx| {
        NativeGraphSurface::new_with_search_source(view, native_registry(cx), search_source, cx)
    });
    draw(cx);
    assert_eq!(source_observer.attempts(), 0);

    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    assert_eq!(source_observer.attempts(), 1);
    draw(cx);
    let candidate_selector =
        native_connection_search_candidate_selector(BLUEPRINT_CALL_KIND, "value");
    let before_failed_query = runtime.snapshot().expect("before failed query");
    let history_before_failed_query = runtime
        .history_state()
        .expect("history before failed query");
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR)
            .is_some()
    );
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR)
            .is_some()
    );
    assert!(cx.debug_bounds(&candidate_selector).is_none());
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_some()
        );
    });
    let after_failed_query = runtime.snapshot().expect("after failed query");
    assert!(before_failed_query.ptr_eq(&after_failed_query));
    assert_eq!(
        after_failed_query.revision(),
        before_failed_query.revision()
    );
    assert_eq!(
        runtime.history_state().expect("history after failed query"),
        history_before_failed_query,
    );

    let retry = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR)
        .expect("native failed-query retry");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert_eq!(source_observer.attempts(), 2);

    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR)
            .is_none()
    );
    assert!(cx.debug_bounds(&candidate_selector).is_some());
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_some()
        );
    });
    let after_recovered_query = runtime.snapshot().expect("after recovered query");
    assert!(before_failed_query.ptr_eq(&after_recovered_query));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after recovered query"),
        history_before_failed_query,
    );
}

#[open_gpui::test]
fn slow_connection_search_source_keeps_native_controls_responsive(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("slow-search shader runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("slow-search shader view");
    let search_source = Arc::new(DeferredConnectionSearchSource::default());
    let source_observer = Arc::clone(&search_source);
    let (_surface, cx) = cx.add_window_view(move |_, cx| {
        NativeGraphSurface::new_with_search_source(view, native_registry(cx), search_source, cx)
    });
    draw(cx);

    open_shader_output_search(cx);
    assert_eq!(source_observer.attempts(), 1);
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
            .is_some(),
        "a pending provider must leave a paintable loading surface",
    );

    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("slow-search input remains interactive");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("Blueprint");
    draw(cx);
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
            .is_some(),
        "typing while the provider is pending must not block the UI executor",
    );

    source_observer.attempt(0).complete_successfully();
    draw(cx);
    assert!(
        cx.debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .is_some(),
    );
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
            .is_none(),
    );
}

#[open_gpui::test]
fn cancelled_search_drops_its_provider_and_rejects_its_result_from_the_next_generation(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("cancelled-search shader runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("cancelled-search shader view");
    let search_source = Arc::new(DeferredConnectionSearchSource::default());
    let source_observer = Arc::clone(&search_source);
    let (surface, cx) = cx.add_window_view(move |_, cx| {
        NativeGraphSurface::new_with_search_source(view, native_registry(cx), search_source, cx)
    });
    draw(cx);

    open_shader_output_search(cx);
    let first_request = cx.read_entity(&surface, |surface, cx| {
        surface
            .host()
            .read(cx)
            .connection_search_request()
            .expect("first deferred search request")
    });
    let first_attempt = source_observer.attempt(0);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(first_attempt.was_dropped());
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());

    open_shader_output_search(cx);
    let second_request = cx.read_entity(&surface, |surface, cx| {
        surface
            .host()
            .read(cx)
            .connection_search_request()
            .expect("second deferred search request")
    });
    assert_ne!(first_request.generation(), second_request.generation());
    assert_eq!(source_observer.attempts(), 2);

    first_attempt.complete_successfully();
    draw(cx);
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
            .is_some(),
        "the cancelled generation must not publish into the active generation",
    );
    assert!(
        cx.debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .is_none(),
    );

    source_observer.attempt(1).complete_successfully();
    draw(cx);
    assert!(
        cx.debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .is_some(),
    );
}

#[open_gpui::test]
fn closing_the_host_cancels_a_pending_connection_search_provider(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("closing-search shader runtime");
    let before = runtime.snapshot().expect("pre-close shader snapshot");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("closing-search shader view");
    let search_source = Arc::new(DeferredConnectionSearchSource::default());
    let source_observer = Arc::clone(&search_source);
    let (surface, cx) = cx.add_window_view(move |_, cx| {
        NativeGraphSurface::new_with_search_source(view, native_registry(cx), search_source, cx)
    });
    draw(cx);

    open_shader_output_search(cx);
    let attempt = source_observer.attempt(0);
    let host = cx.read_entity(&surface, |surface, _| surface.host().clone());
    cx.update(|window, cx| {
        host.update(cx, |host, cx| host.close_in_window(window, cx))
            .expect("close host with a pending product search");
    });
    draw(cx);

    assert!(attempt.was_dropped());
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.read_entity(&surface, |surface, cx| {
        let host = surface.host().read(cx);
        assert_eq!(
            host.close_reason(),
            Some(jellyflow_runtime::view::ViewCloseReason::HostClosed)
        );
        assert!(host.connection_search_request().is_none());
    });

    attempt.complete_successfully();
    draw(cx);
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    assert!(before.ptr_eq(&runtime.snapshot().expect("post-close shader snapshot")));
}

#[open_gpui::test]
fn compact_product_canvas_keeps_search_error_retry_ready_and_empty_states_in_bounds(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("compact shader search runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("compact shader search view");
    let search_source = Arc::new(DeferredConnectionSearchSource::default());
    let source_observer = Arc::clone(&search_source);
    let captured_surface = Rc::new(RefCell::new(None));
    let surface_capture = Rc::clone(&captured_surface);
    let (_root, cx) = cx.add_window_view(move |_, cx| {
        let graph = cx.new(|cx| {
            NativeGraphSurface::new_with_search_source(view, native_registry(cx), search_source, cx)
        });
        *surface_capture.borrow_mut() = Some(graph.clone());
        CompactGraphSurface { graph }
    });
    let surface = captured_surface
        .borrow()
        .clone()
        .expect("captured compact graph surface");
    draw(cx);

    let graph_bounds = cx
        .debug_bounds(NATIVE_GRAPH_SURFACE_SELECTOR)
        .expect("compact graph bounds");
    assert_eq!(graph_bounds.size.width, px(680.0));
    assert_eq!(graph_bounds.size.height, px(640.0));
    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("compact shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(
            graph_bounds.origin.x + px(660.0),
            graph_bounds.origin.y + px(620.0),
        ),
        MouseButton::Left,
        Default::default(),
    );
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let loading = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
        .expect("compact loading state paints before candidate resolution");
    assert_bounds_inside_safe_area(loading, graph_bounds, 12.0);
    cx.run_until_parked();
    draw(cx);
    assert_eq!(source_observer.attempts(), 1);
    source_observer
        .attempt(0)
        .complete_with_error("application-owned candidate query failed");
    draw(cx);

    for selector in [
        NATIVE_CONNECTION_SEARCH_SELECTOR,
        NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR,
        NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR,
        NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR,
    ] {
        let bounds = cx.debug_bounds(selector).expect("compact search state");
        assert_bounds_inside_safe_area(bounds, graph_bounds, 12.0);
    }

    let retry = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR)
        .expect("compact retry control");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert_eq!(source_observer.attempts(), 2);
    let retry_loading = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR)
        .expect("compact retry loading state");
    assert_bounds_inside_safe_area(retry_loading, graph_bounds, 12.0);
    source_observer.attempt(1).complete_successfully();
    draw(cx);
    for selector in [
        NATIVE_CONNECTION_SEARCH_SELECTOR,
        NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR,
    ] {
        let bounds = cx.debug_bounds(selector).expect("compact ready state");
        assert_bounds_inside_safe_area(bounds, graph_bounds, 12.0);
    }
    let candidate = cx
        .debug_bounds(&native_connection_search_candidate_selector(
            BLUEPRINT_CALL_KIND,
            "value",
        ))
        .expect("compact ready candidate");
    assert_bounds_inside_safe_area(candidate, graph_bounds, 12.0);

    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("compact search input");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("no-such-compatible-node");
    draw(cx);
    let empty = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR)
        .expect("compact empty state");
    let results = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR)
        .expect("compact bounded results viewport");
    assert!(results.contains(&empty.center()));
    assert_bounds_inside_safe_area(results, graph_bounds, 12.0);
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_some()
        );
    });
}

#[open_gpui::test]
fn occupied_shader_input_search_reconnects_atomically_and_undo_restores_the_edge(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("shader reconnect-search runtime");
    connect_shader_fixture_edge(&runtime);
    let history_before = runtime
        .history_state()
        .expect("history before reconnect search");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader reconnect-search view");
    let weak_view = view.downgrade();
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let previous_focus = focus_shader_preview_control(cx);
    open_occupied_shader_input_search(cx, &surface);
    let candidate = select_blueprint_value_candidate(cx);
    cx.simulate_click(candidate.center(), Default::default());
    draw(cx);
    wait_for_connection_search_close(cx, &surface, &weak_view);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    let reconnected = runtime.snapshot().expect("reconnected shader graph");
    assert_eq!(reconnected.graph().nodes().len(), 3);
    assert_eq!(reconnected.graph().edges().len(), 1);
    let edge = reconnected
        .graph()
        .edge(&edge_id())
        .expect("stable reconnected edge");
    assert_eq!(edge.from, source_output_id());
    assert_ne!(edge.to, input_a_id());
    let new_target = reconnected
        .graph()
        .port(&edge.to)
        .expect("reconnected Blueprint input");
    assert_eq!(
        reconnected
            .graph()
            .node(&new_target.node)
            .expect("created Blueprint node")
            .kind
            .0,
        BLUEPRINT_CALL_KIND,
    );
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        history_before.undo_len() + 1,
        "node creation and reconnect must share one history boundary",
    );

    runtime
        .execute(GraphCommand::undo(reconnected.version()))
        .applied_revision()
        .expect("undo reconnect search applies");
    let undone = runtime.snapshot().expect("undone reconnect search");
    assert_eq!(undone.graph().nodes().len(), 2);
    assert_eq!(
        undone.graph().edge(&edge_id()).expect("restored edge").to,
        input_a_id(),
    );
}

#[open_gpui::test]
fn occupied_shader_reconnect_search_rebinds_stale_candidate_atomically(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(shader_graph()).expect("stale reconnect-search runtime");
    connect_shader_fixture_edge(&runtime);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("stale reconnect-search view");
    let weak_view = view.downgrade();
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let previous_focus = focus_shader_preview_control(cx);
    open_occupied_shader_input_search(cx, &surface);
    let candidate = select_blueprint_value_candidate(cx);
    let current = runtime.snapshot().expect("current reconnect graph");
    runtime
        .execute(GraphCommand::set_node_data(
            current.version(),
            node_id(),
            serde_json::json!({"changed": true}),
        ))
        .applied_revision()
        .expect("intervening reconnect edit applies");
    let history_after_intervening = runtime.history_state().unwrap();

    cx.simulate_click(candidate.center(), Default::default());
    draw(cx);
    wait_for_connection_search_close(cx, &surface, &weak_view);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    let reconnected = runtime.snapshot().expect("fresh reconnect snapshot");
    assert_eq!(reconnected.graph().nodes().len(), 3);
    assert_eq!(reconnected.graph().edges().len(), 1);
    let edge = reconnected
        .graph()
        .edge(&edge_id())
        .expect("stable reconnected edge");
    assert_eq!(edge.from, source_output_id());
    assert_ne!(edge.to, input_a_id());
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        history_after_intervening.undo_len() + 1,
    );
}

#[open_gpui::test]
fn occupied_shader_reconnect_search_rejection_then_outside_cancel_preserves_edge_and_focus(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_profiled_runtime(shader_graph(), Box::new(RejectBlueprintCreationProfile))
        .expect("rejecting reconnect-search runtime");
    connect_shader_fixture_edge(&runtime);
    let history_before = runtime
        .history_state()
        .expect("history before rejected reconnect");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("rejecting reconnect-search view");
    let (surface, cx) =
        cx.add_window_view(move |_, cx| NativeGraphSurface::new(view, native_registry(cx), cx));
    draw(cx);

    let previous_focus = focus_shader_preview_control(cx);
    open_occupied_shader_input_search(cx, &surface);
    let candidate = select_blueprint_value_candidate(cx);
    cx.simulate_click(candidate.center(), Default::default());
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR)
            .is_some()
    );
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR)
            .is_some(),
        "a rejected commit must require an explicit retry",
    );
    assert_original_shader_edge(&runtime);
    assert_eq!(runtime.snapshot().unwrap().graph().nodes().len(), 2);
    assert_eq!(runtime.history_state().unwrap(), history_before);
    cx.read_entity(&surface, |surface, cx| {
        let host = surface.host().read(cx);
        assert!(host.connection_search_request().is_some());
        assert_eq!(
            host.interaction_presentation()
                .connection()
                .and_then(|connection| connection.search()),
            Some(ConnectionSearchStatus::CommitRejected),
        );
    });

    let backdrop = cx
        .debug_bounds(NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR)
        .expect("reconnect search backdrop");
    cx.simulate_click(
        point(backdrop.origin.x + px(2.0), backdrop.origin.y + px(2.0)),
        Default::default(),
    );
    draw(cx);

    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    assert_original_shader_edge(&runtime);
    assert_eq!(runtime.history_state().unwrap(), history_before);
    cx.read_entity(&surface, |surface, cx| {
        assert!(
            surface
                .host()
                .read(cx)
                .connection_search_request()
                .is_none()
        );
    });
}

#[open_gpui::test]
fn blueprint_add_input_control_mounts_one_real_dynamic_port_without_remounting(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(blueprint_graph()).expect("blueprint runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("blueprint view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    assert!(cx.debug_bounds(BLUEPRINT_NODE_SELECTOR).is_some());
    for port in [
        blueprint_exec_input_id(),
        blueprint_exec_output_id(),
        blueprint_value_input_id(),
    ] {
        assert!(cx.debug_bounds(&blueprint_port_selector(port)).is_some());
    }
    let add_input = cx
        .debug_bounds(BLUEPRINT_ADD_INPUT_SELECTOR)
        .expect("native add-input control");

    cx.simulate_click(add_input.center(), Default::default());
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds(BLUEPRINT_COMMAND_PENDING_SELECTOR)
            .is_none()
    );

    let snapshot = runtime.snapshot().expect("dynamic blueprint snapshot");
    let node = snapshot
        .graph()
        .node(&blueprint_node_id())
        .expect("blueprint node");
    assert_eq!(node.ports.len(), 4);
    let dynamic = *node.ports.last().expect("dynamic port id");
    let dynamic_port = snapshot.graph().port(&dynamic).expect("dynamic real port");
    assert_eq!(dynamic_port.node, blueprint_node_id());
    assert!(cx.debug_bounds(&blueprint_port_selector(dynamic)).is_some());

    let geometry = weak_view.geometry_snapshot().expect("blueprint geometry");
    assert!(geometry.node(blueprint_node_id()).unwrap().is_fresh());
    assert_eq!(geometry.port(dynamic).unwrap().node(), blueprint_node_id());
    assert_single_retained_mount(cx, &host);
}

#[open_gpui::test]
fn blueprint_native_pin_rows_reorder_remove_and_republish_real_geometry(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(blueprint_graph()).expect("blueprint row runtime");
    let first = BlueprintPin::new("first", dynamic_pin_id(), "First", TypeDesc::Float).unwrap();
    let second =
        BlueprintPin::new("second", sparse_input_1_id(), "Second", TypeDesc::Float).unwrap();
    apply_blueprint_intent(&runtime, BlueprintIntent::add(first.clone()));
    apply_blueprint_intent(&runtime, BlueprintIntent::add(second.clone()));
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("blueprint row view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    for pin in [&first, &second] {
        assert!(
            cx.debug_bounds(&blueprint_port_selector(pin.port_id()))
                .is_some()
        );
        let geometry = weak_view
            .geometry_snapshot()
            .unwrap()
            .port(pin.port_id())
            .expect("dynamic pin geometry")
            .clone();
        assert_eq!(geometry.node(), blueprint_node_id());
        assert!(geometry.visual_bounds().is_positive_finite());
        assert!(
            geometry
                .hit_bounds()
                .is_some_and(CanvasRect::is_positive_finite),
            "dynamic pin {:?} published invalid hit geometry: {geometry:?}",
            pin.port_id(),
        );
    }

    let up = cx
        .debug_bounds(&blueprint_pin_up_selector(
            blueprint_node_id(),
            second.port_id(),
        ))
        .expect("second pin up control");
    cx.simulate_click(up.center(), Default::default());
    draw(cx);
    draw(cx);
    assert_eq!(
        blueprint_pin_snapshot(&runtime.snapshot().unwrap()).unwrap(),
        vec![second.clone(), first.clone()]
    );

    let down = cx
        .debug_bounds(&blueprint_pin_down_selector(
            blueprint_node_id(),
            second.port_id(),
        ))
        .expect("first displayed pin down control");
    cx.simulate_click(down.center(), Default::default());
    draw(cx);
    draw(cx);
    assert_eq!(
        blueprint_pin_snapshot(&runtime.snapshot().unwrap()).unwrap(),
        vec![first.clone(), second.clone()]
    );

    let remove = cx
        .debug_bounds(&blueprint_pin_remove_selector(
            blueprint_node_id(),
            first.port_id(),
        ))
        .expect("first pin remove control");
    cx.simulate_click(remove.center(), Default::default());
    draw(cx);
    draw(cx);
    let removed = runtime.snapshot().unwrap();
    assert_eq!(
        blueprint_pin_snapshot(&removed).unwrap(),
        vec![second.clone()]
    );
    assert!(removed.graph().port(&first.port_id()).is_none());
    assert!(
        cx.debug_bounds(&blueprint_port_selector(first.port_id()))
            .is_none()
    );
    assert!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(first.port_id())
            .is_none()
    );
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(second.port_id())
            .unwrap()
            .node(),
        blueprint_node_id(),
    );
    assert_single_retained_mount(cx, &host);
}

#[open_gpui::test]
fn blueprint_rejected_add_requires_explicit_retry_and_never_resubmits_from_render(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_profiled_runtime(
        blueprint_graph(),
        Box::new(RejectBlueprintDynamicPinProfile),
    )
    .expect("blueprint rejection runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("blueprint rejection view");
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let baseline = runtime.snapshot().expect("baseline rejected blueprint");
    let baseline_node = baseline.graph().node(&blueprint_node_id()).unwrap();
    let baseline_ports = baseline_node.ports.clone();
    let baseline_data = baseline_node.data.clone();
    let baseline_history = runtime.history_state().unwrap();
    let add_input = cx
        .debug_bounds(BLUEPRINT_ADD_INPUT_SELECTOR)
        .expect("native add-input control");
    cx.simulate_click(add_input.center(), Default::default());
    draw(cx);
    draw(cx);

    let rejected = runtime.snapshot().expect("rejected blueprint snapshot");
    assert_eq!(
        rejected
            .graph()
            .node(&blueprint_node_id())
            .expect("rejected blueprint node")
            .ports,
        baseline_ports,
    );
    assert_eq!(
        rejected.graph().node(&blueprint_node_id()).unwrap().data,
        baseline_data,
    );
    assert!(blueprint_pin_snapshot(&rejected).unwrap().is_empty());
    assert_eq!(runtime.history_state().unwrap(), baseline_history);
    assert!(
        cx.debug_bounds(BLUEPRINT_COMMAND_DIAGNOSTIC_SELECTOR)
            .is_some()
    );
    let retry = cx
        .debug_bounds(BLUEPRINT_RETRY_SELECTOR)
        .expect("explicit blueprint retry");

    draw(cx);
    draw(cx);
    let stable = runtime.snapshot().unwrap();
    assert_eq!(
        stable.graph().node(&blueprint_node_id()).unwrap().ports,
        baseline_ports
    );
    assert!(blueprint_pin_snapshot(&stable).unwrap().is_empty());
    assert_eq!(runtime.history_state().unwrap(), baseline_history);

    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    draw(cx);
    let retried = runtime.snapshot().unwrap();
    assert_eq!(
        retried.graph().node(&blueprint_node_id()).unwrap().ports,
        baseline_ports
    );
    assert!(blueprint_pin_snapshot(&retried).unwrap().is_empty());
    assert_eq!(runtime.history_state().unwrap(), baseline_history);
    assert!(cx.debug_bounds(BLUEPRINT_RETRY_SELECTOR).is_some());
}

#[open_gpui::test]
fn blueprint_unparseable_observation_drains_completion_and_allows_explicit_discard(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = Rc::new(native_runtime(blueprint_graph()).expect("blueprint runtime"));
    let host_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("Blueprint host view");
    let remote_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("Blueprint remote view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let remote_snapshot = remote_view.graph_snapshot().unwrap();
    let mut malformed_data = remote_snapshot
        .graph()
        .node(&blueprint_node_id())
        .unwrap()
        .data
        .clone();
    malformed_data["pins"] = serde_json::json!("not-an-array");
    let remote_update = GraphCommand::set_node_data(
        remote_snapshot.version(),
        blueprint_node_id(),
        malformed_data.clone(),
    );
    let _interleave_remote_command = cx.update(|_, cx| {
        let mut remote_update = Some(remote_update);
        cx.observe(&host, move |_, _| {
            let Some(remote_update) = remote_update.take() else {
                return;
            };
            remote_view
                .execute(remote_update)
                .unwrap()
                .applied_revision()
                .expect("remote malformed Blueprint data applies before the local command");
        })
    });

    let add = cx
        .debug_bounds(BLUEPRINT_ADD_INPUT_SELECTOR)
        .expect("local add before malformed remote update");
    cx.simulate_click(add.center(), Default::default());
    draw(cx);
    draw(cx);

    assert!(
        cx.debug_bounds(BLUEPRINT_DISCARD_SELECTOR).is_some(),
        "definitive completion remains explicitly discardable when Blueprint facts cannot parse"
    );
    assert!(cx.debug_bounds(BLUEPRINT_RETRY_SELECTOR).is_none());
    let before_discard = runtime.snapshot().unwrap();
    let before_discard_history = runtime.history_state().unwrap();
    assert_eq!(
        before_discard
            .graph()
            .node(&blueprint_node_id())
            .unwrap()
            .data,
        malformed_data
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    let discard = cx
        .debug_bounds(BLUEPRINT_DISCARD_SELECTOR)
        .expect("discard unavailable Blueprint mutation");
    cx.simulate_click(discard.center(), Default::default());
    draw(cx);
    draw(cx);

    assert!(cx.debug_bounds(BLUEPRINT_DISCARD_SELECTOR).is_none());
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&blueprint_node_id())
            .unwrap()
            .data,
        malformed_data,
        "discard only releases local mutation supervision"
    );
    assert_eq!(runtime.history_state().unwrap(), before_discard_history);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn blueprint_add_input_fills_a_sparse_name_without_replacing_stable_ports(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(blueprint_graph()).expect("blueprint runtime");
    prepare_sparse_blueprint_inputs(&runtime);
    let sparse = runtime.snapshot().expect("sparse blueprint snapshot");
    assert!(sparse.graph().port(&sparse_input_1_id()).is_none());
    assert_eq!(
        sparse
            .graph()
            .port(&sparse_input_2_id())
            .expect("retained sparse pin")
            .data["label"],
        "input_2"
    );

    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("sparse blueprint view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let add_input = cx
        .debug_bounds(BLUEPRINT_ADD_INPUT_SELECTOR)
        .expect("native add-input control");
    cx.simulate_click(add_input.center(), Default::default());
    draw(cx);
    draw(cx);

    let added = runtime
        .snapshot()
        .expect("filled sparse blueprint snapshot");
    let node = added
        .graph()
        .node(&blueprint_node_id())
        .expect("blueprint node");
    let added_input_1 = node
        .ports
        .iter()
        .copied()
        .find(|port_id| {
            added
                .graph()
                .port(port_id)
                .is_some_and(|port| port.data["label"] == "input_1")
        })
        .expect("first unoccupied sparse name is filled");
    assert_ne!(added_input_1, sparse_input_1_id());
    assert_eq!(
        added
            .graph()
            .port(&sparse_input_2_id())
            .expect("retained sparse pin")
            .data["label"],
        "input_2"
    );
    assert!(
        cx.debug_bounds(&blueprint_port_selector(added_input_1))
            .is_some()
    );
    assert_single_retained_mount(cx, &host);
}

#[open_gpui::test]
fn blueprint_semantic_zoom_preserves_sparse_dynamic_port_geometry_without_remounting(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = native_runtime(blueprint_graph()).expect("blueprint semantic zoom runtime");
    prepare_sparse_blueprint_inputs(&runtime);
    apply_blueprint_intent(
        &runtime,
        BlueprintIntent::add(
            BlueprintPin::new("roughness", dynamic_pin_id(), "roughness", TypeDesc::Float)
                .expect("valid named dynamic pin"),
        ),
    );
    let before = runtime.snapshot().expect("before blueprint semantic zoom");
    let history_before = runtime
        .history_state()
        .expect("before blueprint semantic zoom history");
    let node_ports = before
        .graph()
        .node(&blueprint_node_id())
        .expect("blueprint node")
        .ports
        .clone();
    let dynamic_ports = node_ports
        .iter()
        .copied()
        .filter(|port_id| {
            before
                .graph()
                .port(port_id)
                .is_some_and(|port| port.key.0.starts_with("arg."))
        })
        .collect::<Vec<_>>();
    assert_eq!(dynamic_ports, vec![sparse_input_2_id(), dynamic_pin_id()]);
    assert_eq!(history_before.undo_len(), 4);

    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("blueprint semantic zoom view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), blueprint_viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    assert!(cx.debug_bounds(BLUEPRINT_NODE_SELECTOR).is_some());
    for port_id in &dynamic_ports {
        assert!(
            cx.debug_bounds(&blueprint_port_selector(*port_id))
                .is_some()
        );
    }
    assert_single_retained_mount(cx, &host);

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 0.05)
                .expect("valid blueprint overview viewport"),
        )
        .expect("set blueprint overview zoom");
    draw(cx);

    assert!(cx.debug_bounds(BLUEPRINT_NODE_SELECTOR).is_none());
    assert!(cx.debug_bounds(BLUEPRINT_ADD_INPUT_SELECTOR).is_none());
    let overview = weak_view
        .geometry_snapshot()
        .expect("blueprint overview geometry");
    let overview_node = overview
        .node(blueprint_node_id())
        .expect("blueprint overview node geometry");
    assert!(overview_node.is_fresh());
    let expected_ports = node_ports
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        overview_node
            .ports()
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        expected_ports,
    );
    let overview_dynamic_geometry = dynamic_ports
        .iter()
        .map(|port_id| {
            let geometry = overview
                .port(*port_id)
                .expect("dynamic overview port geometry");
            assert_eq!(geometry.port(), *port_id);
            assert_eq!(geometry.node(), blueprint_node_id());
            assert!(geometry.visual_bounds().is_positive_finite());
            assert!(
                geometry
                    .hit_bounds()
                    .is_some_and(CanvasRect::is_positive_finite)
            );
            assert!(geometry.route_anchor().is_finite());
            geometry.clone()
        })
        .collect::<Vec<_>>();

    draw(cx);
    let repeated_overview = weak_view
        .geometry_snapshot()
        .expect("repeated blueprint overview geometry");
    for (port_id, expected) in dynamic_ports.iter().zip(&overview_dynamic_geometry) {
        assert_eq!(repeated_overview.port(*port_id), Some(expected));
    }

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 1.0)
                .expect("valid blueprint detail viewport"),
        )
        .expect("restore blueprint detail zoom");
    draw(cx);

    assert!(cx.debug_bounds(BLUEPRINT_NODE_SELECTOR).is_some());
    let restored_geometry = weak_view
        .geometry_snapshot()
        .expect("restored blueprint geometry");
    for port_id in &dynamic_ports {
        assert!(
            cx.debug_bounds(&blueprint_port_selector(*port_id))
                .is_some()
        );
        assert_eq!(
            restored_geometry
                .port(*port_id)
                .expect("restored dynamic port geometry")
                .port(),
            *port_id,
        );
    }
    assert_single_retained_mount(cx, &host);
    let after = runtime.snapshot().expect("after blueprint semantic zoom");
    assert_eq!(
        after
            .graph()
            .node(&blueprint_node_id())
            .expect("restored blueprint node")
            .data,
        before
            .graph()
            .node(&blueprint_node_id())
            .expect("pre-zoom blueprint node")
            .data,
    );
    assert_eq!(
        after
            .graph()
            .node(&blueprint_node_id())
            .expect("restored blueprint node")
            .ports,
        node_ports,
    );
    assert_eq!(runtime.history_state().unwrap(), history_before);
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn wait_for_connection_search_close(
    cx: &mut open_gpui::VisualTestContext,
    surface: &Entity<NativeGraphSurface>,
    view: &WeakViewSession,
) {
    for _ in 0..8 {
        if cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_none() {
            return;
        }
        draw(cx);
    }
    let presentation = cx.read_entity(surface, |surface, cx| {
        surface.host().read(cx).interaction_presentation()
    });
    let snapshot = view.graph_snapshot().ok();
    let geometry = view.geometry_snapshot().ok();
    let edge = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.graph().edge(&edge_id()));
    let route = geometry
        .as_ref()
        .and_then(|geometry| geometry.route(edge_id()).cloned());
    let ports = edge.map(|edge| {
        (
            edge.from,
            geometry
                .as_ref()
                .and_then(|geometry| geometry.port(edge.from).cloned()),
            edge.to,
            geometry
                .as_ref()
                .and_then(|geometry| geometry.port(edge.to).cloned()),
        )
    });
    panic!(
        "connection search did not close after authoritative publication: {presentation:?}; graph={snapshot:?}; route={route:?}; ports={ports:?}"
    );
}

fn assert_product_port_profile(
    port: &PublishedPortGeometry,
    transform: &ViewportTransform,
    detail: bool,
) {
    let visual = projected_canvas_rect_size(port.visual_bounds(), transform);
    let hit = projected_canvas_rect_size(
        port.hit_bounds().expect("visible product port hit target"),
        transform,
    );
    if detail {
        assert!(
            (visual.width - 11.0).abs() <= 0.1,
            "detail port visual width must keep the 11px layout slot, got {}px",
            visual.width
        );
        assert!(
            (visual.height - 11.0).abs() <= 0.1,
            "detail port visual height must keep the 11px layout slot, got {}px",
            visual.height
        );
    } else {
        assert!(visual.width > 0.0 && visual.width <= 11.0);
        assert!(visual.height > 0.0 && visual.height <= 11.0);
    }
    assert!((hit.width - 22.0).abs() <= 0.1);
    assert!((hit.height - 22.0).abs() <= 0.1);

    let bounds = port.visual_bounds();
    let center = CanvasPoint {
        x: bounds.origin.x + bounds.size.width * 0.5,
        y: bounds.origin.y + bounds.size.height * 0.5,
    };
    assert!((port.route_anchor().x - center.x).abs() <= 0.01);
    assert!((port.route_anchor().y - center.y).abs() <= 0.01);
}

fn projected_canvas_rect_size(bounds: CanvasRect, transform: &ViewportTransform) -> CanvasSize {
    let origin = transform
        .screen_point_for_canvas(bounds.origin)
        .expect("finite projected bounds origin");
    let far = transform
        .screen_point_for_canvas(CanvasPoint {
            x: bounds.origin.x + bounds.size.width,
            y: bounds.origin.y + bounds.size.height,
        })
        .expect("finite projected bounds extent");
    CanvasSize {
        width: (far.x - origin.x).abs(),
        height: (far.y - origin.y).abs(),
    }
}

fn assert_shader_layout_contract(
    cx: &mut open_gpui::TestAppContext,
    window_width: f32,
    window_height: f32,
) {
    let runtime = native_runtime(shader_layout_graph()).expect("shader layout runtime");
    let long_pin = BlueprintPin::new(
        "production-material-parameter",
        dynamic_pin_id(),
        "An intentionally long production material parameter name that must stay inside its row",
        TypeDesc::Float,
    )
    .expect("valid long Blueprint layout pin");
    apply_blueprint_intent(&runtime, BlueprintIntent::add(long_pin.clone()));
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("shader layout view");
    let registry = cx.update(native_registry);
    let viewport = CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: window_width,
            height: window_height,
        },
    };
    let window = cx.open_window(size(px(window_width), px(window_height)), move |_, _| {
        OpenGpuiSceneHost::new(view, registry, viewport).expect("valid Open GPUI scene host")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    for _ in 0..3 {
        draw(&mut cx);
    }

    assert_multiply_layout(&mut cx);
    assert_blueprint_layout(&mut cx, long_pin.port_id());
}

fn assert_multiply_layout(cx: &mut VisualTestContext) {
    let node = layout_bounds(cx, SHADER_NODE_SELECTOR, "Multiply node");
    let title = layout_bounds(
        cx,
        shader_header_title_selector(node_id()),
        "Multiply title",
    );
    let metadata = layout_bounds(
        cx,
        shader_header_metadata_selector(node_id()),
        "Multiply metadata",
    );
    let body = layout_bounds(cx, shader_body_selector(node_id()), "Multiply body");
    let preview = layout_bounds(cx, SHADER_PREVIEW_CONTROL_SELECTOR, "preview control");
    let inline_editor = layout_bounds(
        cx,
        shader_inline_editor_selector(node_id(), "a"),
        "factor editor",
    );
    let labels = [
        layout_bounds(cx, shader_port_label_selector(node_id(), "a"), "A label"),
        layout_bounds(cx, shader_port_label_selector(node_id(), "b"), "B label"),
        layout_bounds(
            cx,
            shader_port_label_selector(node_id(), "result"),
            "Result label",
        ),
    ];
    let markers = [
        layout_bounds(cx, shader_port_selector(input_a_id()), "A marker"),
        layout_bounds(cx, shader_port_selector(input_b_id()), "B marker"),
        layout_bounds(cx, shader_port_selector(output_id()), "Result marker"),
    ];

    for (name, bounds) in [
        ("title", title),
        ("metadata", metadata),
        ("body", body),
        ("preview", preview),
        ("factor editor", inline_editor),
    ] {
        assert_layout_contained(bounds, node, &format!("Multiply {name}"));
    }
    assert_layout_disjoint(title, metadata, "Multiply title and metadata");
    assert_layout_disjoint(title, body, "Multiply header title and body");
    assert_layout_disjoint(metadata, body, "Multiply header metadata and body");
    assert_layout_disjoint(title, preview, "Multiply title and preview control");
    assert_layout_disjoint(labels[0], inline_editor, "A label and factor editor");
    assert_layout_disjoint(labels[2], inline_editor, "Result label and factor editor");
    for (marker_index, marker) in markers.into_iter().enumerate() {
        for (label_index, label) in labels.into_iter().enumerate() {
            assert_layout_disjoint(
                marker,
                label,
                &format!("Multiply marker {marker_index} and label {label_index}"),
            );
        }
        assert_layout_disjoint(
            marker,
            inline_editor,
            &format!("Multiply marker {marker_index} and factor editor"),
        );
    }
}

fn assert_blueprint_layout(cx: &mut VisualTestContext, dynamic_pin: PortId) {
    let node = layout_bounds(cx, BLUEPRINT_NODE_SELECTOR, "Blueprint node");
    let title = layout_bounds(
        cx,
        blueprint_header_title_selector(blueprint_node_id()),
        "Blueprint title",
    );
    let metadata = layout_bounds(
        cx,
        blueprint_header_metadata_selector(blueprint_node_id()),
        "Blueprint metadata",
    );
    let body = layout_bounds(
        cx,
        blueprint_body_selector(blueprint_node_id()),
        "Blueprint body",
    );
    let add_input = layout_bounds(cx, BLUEPRINT_ADD_INPUT_SELECTOR, "Add input control");
    let labels = [
        layout_bounds(
            cx,
            blueprint_port_label_selector(blueprint_node_id(), "exec_in"),
            "Blueprint In label",
        ),
        layout_bounds(
            cx,
            blueprint_port_label_selector(blueprint_node_id(), "exec_out"),
            "Blueprint Out label",
        ),
        layout_bounds(
            cx,
            blueprint_port_label_selector(blueprint_node_id(), "value"),
            "Blueprint Value label",
        ),
    ];
    let markers = [
        layout_bounds(
            cx,
            blueprint_port_selector(blueprint_exec_input_id()),
            "Blueprint In marker",
        ),
        layout_bounds(
            cx,
            blueprint_port_selector(blueprint_exec_output_id()),
            "Blueprint Out marker",
        ),
        layout_bounds(
            cx,
            blueprint_port_selector(blueprint_value_input_id()),
            "Blueprint Value marker",
        ),
    ];
    let dynamic_label = layout_bounds(
        cx,
        blueprint_pin_label_selector(blueprint_node_id(), dynamic_pin),
        "Blueprint dynamic pin label",
    );
    let dynamic_actions = layout_bounds(
        cx,
        blueprint_pin_remove_selector(blueprint_node_id(), dynamic_pin),
        "Blueprint dynamic pin actions",
    );
    let dynamic_marker = layout_bounds(
        cx,
        blueprint_port_selector(dynamic_pin),
        "Blueprint dynamic pin marker",
    );

    for (name, bounds) in [
        ("title", title),
        ("metadata", metadata),
        ("body", body),
        ("Add input", add_input),
    ] {
        assert_layout_contained(bounds, node, &format!("Blueprint {name}"));
    }
    assert_layout_disjoint(title, metadata, "Blueprint title and metadata");
    assert_layout_disjoint(title, body, "Blueprint header title and body");
    assert_layout_disjoint(metadata, body, "Blueprint header metadata and body");
    assert_layout_contained(dynamic_label, body, "Blueprint dynamic pin label");
    assert_layout_contained(dynamic_actions, body, "Blueprint dynamic pin actions");
    assert_layout_disjoint(
        dynamic_label,
        dynamic_actions,
        "Blueprint dynamic pin label and actions",
    );
    assert_layout_disjoint(
        dynamic_marker,
        dynamic_label,
        "Blueprint dynamic pin marker and label",
    );
    assert_layout_disjoint(
        dynamic_marker,
        dynamic_actions,
        "Blueprint dynamic pin marker and actions",
    );
    for (label_index, label) in labels.into_iter().enumerate() {
        assert_layout_contained(label, body, &format!("Blueprint label {label_index}"));
        assert_layout_disjoint(
            label,
            add_input,
            &format!("Blueprint label {label_index} and Add input"),
        );
    }
    for (marker_index, marker) in markers.into_iter().enumerate() {
        for (label_index, label) in labels.into_iter().enumerate() {
            assert_layout_disjoint(
                marker,
                label,
                &format!("Blueprint marker {marker_index} and label {label_index}"),
            );
        }
        assert_layout_disjoint(
            marker,
            add_input,
            &format!("Blueprint marker {marker_index} and Add input"),
        );
    }
}

fn layout_bounds(
    cx: &mut VisualTestContext,
    selector: impl AsRef<str>,
    name: &str,
) -> Bounds<Pixels> {
    cx.debug_bounds(selector.as_ref())
        .unwrap_or_else(|| panic!("missing layout bounds for {name}"))
}

fn assert_layout_contained(child: Bounds<Pixels>, parent: Bounds<Pixels>, context: &str) {
    assert!(
        child.is_contained_within(&parent),
        "{context} escaped its owner: child={child:?}, parent={parent:?}",
    );
}

fn assert_layout_disjoint(left: Bounds<Pixels>, right: Bounds<Pixels>, context: &str) {
    assert!(
        !left.intersects(&right),
        "{context} overlapped: left={left:?}, right={right:?}",
    );
}

fn assert_bounds_inside_safe_area(
    bounds: open_gpui::Bounds<open_gpui::Pixels>,
    parent: open_gpui::Bounds<open_gpui::Pixels>,
    inset: f32,
) {
    let inset = px(inset);
    assert!(bounds.origin.x >= parent.origin.x + inset, "{bounds:?}");
    assert!(bounds.origin.y >= parent.origin.y + inset, "{bounds:?}");
    assert!(
        bounds.origin.x + bounds.size.width <= parent.origin.x + parent.size.width - inset,
        "{bounds:?}",
    );
    assert!(
        bounds.origin.y + bounds.size.height <= parent.origin.y + parent.size.height - inset,
        "{bounds:?}",
    );
}

struct RejectBlueprintDynamicPinProfile;

impl GraphProfile for RejectBlueprintDynamicPinProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        graph
            .nodes()
            .iter()
            .find_map(|(id, node)| {
                (node.kind.0 == BLUEPRINT_CALL_KIND && node.ports.len() > 3).then(|| {
                    Diagnostic::error(
                        "test.shader.blueprint_dynamic_pin_rejected",
                        DiagnosticTarget::Node { id: *id },
                        "Dynamic Blueprint pins are disabled for this test profile",
                    )
                })
            })
            .into_iter()
            .collect()
    }
}

struct RejectBlueprintCreationProfile;

impl GraphProfile for RejectBlueprintCreationProfile {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        graph
            .nodes()
            .values()
            .any(|node| node.kind.0 == BLUEPRINT_CALL_KIND)
            .then(|| {
                Diagnostic::error(
                    "test.shader.blueprint_creation_rejected",
                    DiagnosticTarget::Graph,
                    "Blueprint creation is disabled for this test profile",
                )
            })
            .into_iter()
            .collect()
    }
}

fn connect_shader_fixture_edge(runtime: &GraphRuntime) {
    let version = runtime.snapshot().expect("shader graph snapshot").version();
    runtime
        .execute(GraphCommand::connect(
            version,
            edge_id(),
            source_output_id(),
            input_a_id(),
            NodeGraphConnectionMode::Strict,
        ))
        .applied_revision()
        .expect("occupied shader input connection applies");
}

fn focus_shader_preview_control(cx: &mut open_gpui::VisualTestContext) -> open_gpui::FocusHandle {
    let preview = cx
        .debug_bounds(SHADER_PREVIEW_CONTROL_SELECTOR)
        .expect("shader preview control");
    cx.simulate_click(preview.center(), Default::default());
    draw(cx);
    cx.update(|window, cx| window.focused(cx))
        .expect("shader preview focus")
}

fn open_shader_output_search(cx: &mut open_gpui::VisualTestContext) {
    let output = cx
        .debug_bounds(&shader_port_selector(output_id()))
        .expect("native shader output");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.0), px(500.0)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    assert!(cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some());
}

fn open_occupied_shader_input_search(
    cx: &mut open_gpui::VisualTestContext,
    surface: &open_gpui::Entity<NativeGraphSurface>,
) {
    let occupied_input = cx
        .debug_bounds(&shader_port_selector(input_a_id()))
        .expect("occupied shader input");
    let drop_point = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(
        occupied_input.center(),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(drop_point, Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(drop_point, MouseButton::Left, Default::default());
    draw(cx);
    let state = cx.read_entity(surface, |surface, cx| {
        let host = surface.host().read(cx);
        (
            host.interaction_presentation(),
            host.connection_search_request(),
            host.active_port_drag(),
        )
    });
    assert!(
        cx.debug_bounds(NATIVE_CONNECTION_SEARCH_SELECTOR).is_some(),
        "occupied-input activation must open reconnect search: {state:?}"
    );
    assert!(
        state
            .0
            .connection()
            .and_then(|connection| connection.binding().reconnect_edge())
            .is_some(),
        "occupied-input activation must preserve reconnect intent: {state:?}"
    );
}

fn select_blueprint_value_candidate(
    cx: &mut open_gpui::VisualTestContext,
) -> open_gpui::Bounds<open_gpui::Pixels> {
    let input = cx
        .debug_bounds(native_connection_search_input_selector())
        .expect("native reconnect filter");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("Blueprint");
    draw(cx);
    cx.debug_bounds(&native_connection_search_candidate_selector(
        BLUEPRINT_CALL_KIND,
        "value",
    ))
    .expect("compatible Blueprint reconnect candidate")
}

fn assert_original_shader_edge(runtime: &GraphRuntime) {
    let snapshot = runtime.snapshot().expect("shader graph snapshot");
    assert_eq!(snapshot.graph().edges().len(), 1);
    let edge = snapshot
        .graph()
        .edge(&edge_id())
        .expect("stable shader edge");
    assert_eq!(edge.from, source_output_id());
    assert_eq!(edge.to, input_a_id());
}

fn shader_layout_graph() -> jellyflow_core::Graph {
    let catalog = native_node_catalog().expect("native catalog");
    let multiply = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(SHADER_MULTIPLY_KIND),
            node_id(),
            native_product_bounds(SHADER_MULTIPLY_KIND, CanvasPoint { x: 40.0, y: 40.0 }),
            [input_a_id(), input_b_id(), output_id()],
        )
        .expect("layout Multiply node");
    let blueprint = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(BLUEPRINT_CALL_KIND),
            blueprint_node_id(),
            native_product_bounds(BLUEPRINT_CALL_KIND, CanvasPoint { x: 380.0, y: 40.0 }),
            [
                blueprint_exec_input_id(),
                blueprint_exec_output_id(),
                blueprint_value_input_id(),
            ],
        )
        .expect("layout Blueprint node");
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x61f0));
    for creation in [multiply, blueprint] {
        let (node_id, mut node, ports) = creation.into_parts();
        node.data["title"] = if node.kind.0 == SHADER_MULTIPLY_KIND {
            "Multiply with an intentionally long operation title".into()
        } else {
            "Blueprint function call with an intentionally long title".into()
        };
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
    }
    graph.build().expect("shader layout graph")
}

fn shader_graph() -> jellyflow_core::Graph {
    let catalog = native_node_catalog().expect("native catalog");
    let node = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(SHADER_MULTIPLY_KIND),
            node_id(),
            native_product_bounds(SHADER_MULTIPLY_KIND, CanvasPoint { x: 40.0, y: 36.0 }),
            [input_a_id(), input_b_id(), output_id()],
        )
        .expect("shader fixture node");
    let source = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(SHADER_MULTIPLY_KIND),
            source_node_id(),
            native_product_bounds(SHADER_MULTIPLY_KIND, CanvasPoint { x: -320.0, y: 36.0 }),
            [source_input_a_id(), source_input_b_id(), source_output_id()],
        )
        .expect("upstream shader fixture node");
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x5100));
    let (node_id, mut node, ports) = node.into_parts();
    node.data = shader_node_data();
    graph.insert_node(node_id, node);
    for (port_id, port) in ports {
        graph.insert_port(port_id, port);
    }
    let (source_id, mut source, source_ports) = source.into_parts();
    source.hidden = true;
    source.data = serde_json::json!({
        "title": "Upstream Multiply",
        "factor": 1.0,
        "preview": 1.0,
    });
    graph.insert_node(source_id, source);
    for (port_id, port) in source_ports {
        graph.insert_port(port_id, port);
    }
    graph.build().expect("shader graph")
}

fn two_catalog_shader_nodes(source_output_type: TypeDesc) -> jellyflow_core::Graph {
    let catalog = native_node_catalog().expect("native catalog");
    let source = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(SHADER_MULTIPLY_KIND),
            typed_source_node_id(),
            native_product_bounds(SHADER_MULTIPLY_KIND, CanvasPoint { x: 0.0, y: 0.0 }),
            [
                typed_source_input_a_id(),
                typed_source_input_b_id(),
                typed_source_output_id(),
            ],
        )
        .expect("source shader");
    let target = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(SHADER_MULTIPLY_KIND),
            typed_target_node_id(),
            native_product_bounds(SHADER_MULTIPLY_KIND, CanvasPoint { x: 360.0, y: 0.0 }),
            [
                typed_target_input_id(),
                typed_target_input_b_id(),
                typed_target_output_id(),
            ],
        )
        .expect("target shader");
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x6100));
    for creation in [source, target] {
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, mut port) in ports {
            if port_id == typed_source_output_id() {
                port.ty = Some(source_output_type.clone());
            }
            graph.insert_port(port_id, port);
        }
    }
    graph.build().expect("catalog shader graph")
}

fn blueprint_graph() -> jellyflow_core::Graph {
    let catalog = native_node_catalog().expect("native catalog");
    let blueprint = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(BLUEPRINT_CALL_KIND),
            blueprint_node_id(),
            native_product_bounds(BLUEPRINT_CALL_KIND, CanvasPoint { x: 32.0, y: 40.0 }),
            [
                blueprint_exec_input_id(),
                blueprint_exec_output_id(),
                blueprint_value_input_id(),
            ],
        )
        .expect("blueprint node");
    graph_from_creations(GraphId::from_u128(0x6200), [blueprint])
}

fn graph_from_creations<const N: usize>(
    graph_id: GraphId,
    creations: [jellyflow_runtime::schema::NodeCreation; N],
) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(graph_id);
    for creation in creations {
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
    }
    graph.build().expect("catalog graph")
}

fn native_product_bounds(kind: &str, origin: CanvasPoint) -> CanvasRect {
    let product = native_product_catalog().expect("native product catalog");
    let presentation = product
        .get(&NodeKindKey::new(kind), 1)
        .expect("native product presentation");
    CanvasRect {
        origin,
        size: presentation.initial_size(),
    }
}

fn node_id() -> NodeId {
    NodeId::from_u128(0x5100)
}

fn input_a_id() -> PortId {
    PortId::from_u128(0x5101)
}

fn input_b_id() -> PortId {
    PortId::from_u128(0x5102)
}

fn output_id() -> PortId {
    PortId::from_u128(0x5103)
}

fn source_node_id() -> NodeId {
    NodeId::from_u128(0x5200)
}

fn source_input_a_id() -> PortId {
    PortId::from_u128(0x5202)
}

fn source_input_b_id() -> PortId {
    PortId::from_u128(0x5203)
}

fn source_output_id() -> PortId {
    PortId::from_u128(0x5201)
}

fn edge_id() -> EdgeId {
    EdgeId::from_u128(0x5300)
}

fn typed_source_node_id() -> NodeId {
    NodeId::from_u128(0x6110)
}

fn typed_source_input_a_id() -> PortId {
    PortId::from_u128(0x6111)
}

fn typed_source_input_b_id() -> PortId {
    PortId::from_u128(0x6112)
}

fn typed_source_output_id() -> PortId {
    PortId::from_u128(0x6113)
}

fn typed_target_node_id() -> NodeId {
    NodeId::from_u128(0x6120)
}

fn typed_target_input_id() -> PortId {
    PortId::from_u128(0x6121)
}

fn typed_target_input_b_id() -> PortId {
    PortId::from_u128(0x6122)
}

fn typed_target_output_id() -> PortId {
    PortId::from_u128(0x6123)
}

fn blueprint_node_id() -> NodeId {
    NodeId::from_u128(0x6210)
}

fn blueprint_exec_input_id() -> PortId {
    PortId::from_u128(0x6211)
}

fn blueprint_exec_output_id() -> PortId {
    PortId::from_u128(0x6212)
}

fn blueprint_value_input_id() -> PortId {
    PortId::from_u128(0x6213)
}

fn dynamic_pin_id() -> PortId {
    PortId::from_u128(0x6214)
}

fn sparse_input_1_id() -> PortId {
    PortId::from_u128(0x6215)
}

fn sparse_input_2_id() -> PortId {
    PortId::from_u128(0x6216)
}

fn apply_blueprint_intent(runtime: &GraphRuntime, intent: BlueprintIntent) {
    let snapshot = runtime.snapshot().expect("blueprint action snapshot");
    let command = plan_blueprint_intent(&snapshot, intent).expect("blueprint pin intent command");
    runtime
        .execute(command)
        .applied_revision()
        .expect("blueprint pin intent applies");
}

fn prepare_sparse_blueprint_inputs(runtime: &GraphRuntime) {
    apply_blueprint_intent(
        runtime,
        BlueprintIntent::add(
            BlueprintPin::new(
                "sparse-input-1",
                sparse_input_1_id(),
                "input_1",
                TypeDesc::Float,
            )
            .expect("valid first sparse pin"),
        ),
    );
    apply_blueprint_intent(
        runtime,
        BlueprintIntent::add(
            BlueprintPin::new(
                "sparse-input-2",
                sparse_input_2_id(),
                "input_2",
                TypeDesc::Float,
            )
            .expect("valid second sparse pin"),
        ),
    );
    apply_blueprint_intent(
        runtime,
        BlueprintIntent::remove("sparse-input-1", IncidentEdgePolicy::Reject),
    );
}

fn blueprint_pin_snapshot(
    snapshot: &jellyflow_runtime::graph::GraphSnapshot,
) -> Result<Vec<BlueprintPin>, BlueprintError> {
    BlueprintObservation::from_snapshot(snapshot, blueprint_node_id())
        .map(|observation| observation.pins().to_vec())
}

fn plan_blueprint_intent(
    snapshot: &jellyflow_runtime::graph::GraphSnapshot,
    intent: BlueprintIntent,
) -> Result<GraphCommand, BlueprintError> {
    BlueprintObservation::from_snapshot(snapshot, blueprint_node_id())?.plan(&intent)
}

fn shader_node_data() -> serde_json::Value {
    serde_json::json!({
        "title": "Multiply",
        "factor": 0.75,
        "preview": 1.5,
    })
}

fn assert_fresh_input_geometry(view: &jellyflow_runtime::view::WeakViewSession) {
    let geometry = view.geometry_snapshot().expect("shader geometry");
    let input = geometry.port(input_a_id()).expect("typed input geometry");
    assert_eq!(input.node(), node_id());
    assert!(input.visual_bounds().size.is_positive_finite());
    assert!(geometry.node(node_id()).expect("shader node").is_fresh());
}

fn assert_single_retained_mount(
    cx: &mut open_gpui::VisualTestContext,
    host: &open_gpui::Entity<OpenGpuiSceneHost>,
) {
    cx.read_entity(host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert!(host.last_diagnostic().is_none());
    });
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 960.0,
            height: 720.0,
        },
    }
}

fn blueprint_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: BLUEPRINT_CALL_DEFAULT_SIZE.width + 160.0,
            height: BLUEPRINT_CALL_DEFAULT_SIZE.height + 160.0,
        },
    }
}
