use std::rc::Rc;
use std::time::Duration;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphBuilder, GraphId,
    NodeGraphConnectionMode, NodeId, NodeKindKey, PortDirection, PortId, PortKind, TypeDesc,
};
use jellyflow_open_gpui::host::{InitialViewportPolicy, OpenGpuiSceneHost};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::graph::{GraphRuntime, GraphRuntimeError};
use jellyflow_runtime::interaction::CanvasInteractionPhase;
use jellyflow_runtime::io::GraphFile;
use jellyflow_runtime::view::{PublishedPortGeometry, ViewSessionOptions};
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::{
    AppContext, Bounds, Entity, MouseButton, Pixels, ScrollDelta, ScrollEvent, ScrollSource,
    TouchPhase, VisualTestContext, point, px, size,
};
use open_gpui_canvas_jellyflow::nodes::{
    NativeNodeResources, WORKFLOW_CONDITION_KIND, WORKFLOW_LLM_KIND, native_node_catalog,
    native_node_registry, workflow_condition_add_selector, workflow_condition_body_selector,
    workflow_condition_case_actions_selector, workflow_condition_case_label_selector,
    workflow_condition_case_selector, workflow_condition_case_value_selector,
    workflow_condition_diagnostic_selector, workflow_condition_else_label_selector,
    workflow_condition_else_selector, workflow_condition_field_selector,
    workflow_condition_node_id, workflow_condition_node_selector,
    workflow_condition_remove_selector, workflow_condition_scroll_selector,
    workflow_condition_up_selector, workflow_condition_value_selector, workflow_demo_graph,
    workflow_header_metadata_selector, workflow_header_title_selector, workflow_llm_body_selector,
    workflow_llm_node_id, workflow_llm_node_selector, workflow_model_conflict_selector,
    workflow_model_option_selector, workflow_model_reapply_selector, workflow_model_selector,
    workflow_node_status_selector, workflow_port_selector, workflow_prompt_accept_remote_selector,
    workflow_prompt_apply_selector, workflow_prompt_conflict_selector,
    workflow_prompt_reapply_selector, workflow_prompt_selector,
};
use open_gpui_canvas_jellyflow::product_catalog::native_product_catalog;
use open_gpui_canvas_jellyflow::product_domain::condition::{
    ConditionCase, ConditionIntent, ConditionObservation, ConditionPredicate,
};
use open_gpui_ui_components::gpui_adapter::init_text_input;

const CONCURRENT_CASE_ID: &str = "branch-concurrent";
const CONCURRENT_CASE_PORT: u128 = 0xd1fa_1104;
const CONCURRENT_CASE_VALUE_PORT: u128 = 0xd1fa_1106;
const OVERVIEW_NODE_SELECTOR: &str = "jellyflow-overview-node";

fn native_runtime(graph: Graph) -> Result<GraphRuntime, GraphRuntimeError> {
    GraphRuntime::builder(native_node_catalog().expect("native catalog")).build(graph)
}

fn native_registry(cx: &mut open_gpui::App) -> OpenGpuiNodeRegistry {
    let catalog = native_node_catalog().expect("native catalog");
    native_node_registry(NativeNodeResources::ready_default(cx), &catalog)
}

fn frozen_graph_value(graph: &Graph) -> serde_json::Value {
    serde_json::to_value(GraphFile::from_graph(graph.clone())).expect("frozen graph DTO serializes")
}

#[test]
fn exec_create_from_wire_finds_both_workflow_kinds() {
    let graph = workflow_demo_graph().expect("workflow graph");
    let llm = graph
        .node(&workflow_llm_node_id())
        .expect("workflow LLM node");
    let success = llm
        .ports
        .iter()
        .copied()
        .find(|port_id| {
            let port = graph.port(port_id).expect("LLM port");
            port.key.0 == "success" && port.dir == PortDirection::Out && port.kind == PortKind::Exec
        })
        .expect("LLM success port");
    let runtime = native_runtime(graph).expect("workflow runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let success = view
        .port_instance_key(success)
        .expect("workflow port identity query")
        .expect("workflow output is current");
    let intent = view
        .port_connection_drag_intent(success)
        .expect("connectable workflow output")
        .into_outcome()
        .ready()
        .expect("workflow output remains current");
    let binding =
        jellyflow_runtime::interaction::ConnectionInteractionStart::binding_from_port_drag(
            intent,
            EdgeId::from_u128(0xd1fa_0c01),
            NodeGraphConnectionMode::Strict,
        )
        .expect("workflow connection binding");

    let candidates = view
        .create_from_wire_candidates(&binding)
        .expect("Exec create-from-wire candidates");
    let keys = candidates
        .candidates()
        .iter()
        .map(|candidate| {
            (
                candidate.node_kind().0.as_str(),
                candidate.port().0.as_str(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();

    assert!(keys.contains(&(WORKFLOW_LLM_KIND, "input")));
    assert!(keys.contains(&(WORKFLOW_CONDITION_KIND, "input")));
}

#[open_gpui::test]
fn catalog_default_workflow_nodes_mount_without_product_side_data_repair(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let catalog = native_node_catalog().expect("native catalog");
    let llm = NodeId::from_u128(0xd1fa_4001);
    let condition = NodeId::from_u128(0xd1fa_4002);
    let llm_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_LLM_KIND),
            llm,
            native_product_bounds(WORKFLOW_LLM_KIND, CanvasPoint { x: 40.0, y: 40.0 }),
            [
                PortId::from_u128(0xd1fa_4101),
                PortId::from_u128(0xd1fa_4102),
            ],
        )
        .expect("default workflow LLM");
    let condition_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_CONDITION_KIND),
            condition,
            native_product_bounds(WORKFLOW_CONDITION_KIND, CanvasPoint { x: 460.0, y: 40.0 }),
            [
                PortId::from_u128(0xd1fa_4201),
                PortId::from_u128(0xd1fa_4202),
            ],
        )
        .expect("default workflow condition");
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xd1fa_4000));
    for creation in [llm_creation, condition_creation] {
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
    }
    let runtime = native_runtime(graph.build().expect("default workflow graph"))
        .expect("default workflow runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("default workflow view");
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    draw(cx);

    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "ready"))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "ready"))
            .is_some()
    );
    assert!(cx.debug_bounds(&workflow_prompt_selector(llm)).is_some());
    assert!(
        cx.debug_bounds(&workflow_condition_add_selector(condition))
            .is_some()
    );
}

#[open_gpui::test]
fn workflow_port_layout_anchor_and_hit_target_stay_stable_across_zoom(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = native_runtime(workflow_demo_graph().expect("workflow port profile graph"))
        .expect("workflow port profile runtime");
    let input = {
        let snapshot = runtime.snapshot().expect("workflow port profile snapshot");
        workflow_port_by_key(&snapshot, workflow_llm_node_id(), "input")
    };
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow port profile view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    for zoom in [0.5, 1.0, 2.0] {
        weak_view
            .set_viewport(
                ViewportTransform::new(CanvasPoint::default(), zoom)
                    .expect("valid workflow port profile viewport"),
            )
            .expect("set workflow port profile viewport");
        draw(cx);

        let transform = weak_view.viewport_transform().expect("workflow viewport");
        let geometry = weak_view.geometry_snapshot().expect("workflow geometry");
        let port = geometry
            .port(input)
            .expect("published workflow input port")
            .clone();
        assert_workflow_port_profile(&port, &transform, zoom >= 1.0);
        assert_native_workflow_port_hit_boundary(cx, &host, &port, &transform);
        let after_probe = weak_view
            .geometry_snapshot()
            .expect("workflow geometry after boundary probe")
            .port(input)
            .expect("workflow input after boundary probe")
            .clone();
        assert_eq!(after_probe.visual_bounds(), port.visual_bounds());
        assert_eq!(after_probe.hit_bounds(), port.hit_bounds());
        assert_eq!(after_probe.route_anchor(), port.route_anchor());
    }
}

fn assert_native_workflow_port_hit_boundary(
    cx: &mut VisualTestContext,
    host: &Entity<OpenGpuiSceneHost>,
    port: &PublishedPortGeometry,
    transform: &ViewportTransform,
) {
    assert!(matches!(
        port.outward(),
        HandlePosition::Left | HandlePosition::Right
    ));
    let hit = port.hit_bounds().expect("visible workflow port hit bounds");
    let min = transform
        .screen_point_for_canvas(hit.origin)
        .expect("finite workflow hit origin");
    let max = transform
        .screen_point_for_canvas(CanvasPoint {
            x: hit.origin.x + hit.size.width,
            y: hit.origin.y + hit.size.height,
        })
        .expect("finite workflow hit extent");
    let center_y = (min.y + max.y) * 0.5;
    let (inside_x, outside_x) = match port.outward() {
        HandlePosition::Left => (min.x + 0.5, min.x - 0.5),
        HandlePosition::Right => (max.x - 0.5, max.x + 0.5),
        HandlePosition::Top | HandlePosition::Bottom => unreachable!("workflow ports are lateral"),
    };
    let outside = point(px(outside_x), px(center_y));
    cx.simulate_mouse_down(outside, MouseButton::Left, Default::default());
    cx.read_entity(host, |host, _| {
        assert!(
            host.active_port_drag().is_none(),
            "a point 0.5 px beyond the workflow hit boundary must not acquire a port",
        );
    });
    cx.simulate_mouse_up(outside, MouseButton::Left, Default::default());

    let inside = point(px(inside_x), px(center_y));
    cx.simulate_mouse_down(inside, MouseButton::Left, Default::default());
    cx.read_entity(host, |host, _| {
        assert_eq!(
            host.active_port_drag().map(|owner| owner.port()),
            Some(port.port()),
            "a point 0.5 px inside the workflow hit boundary must acquire the exact port",
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(inside, MouseButton::Left, Default::default());
    draw(cx);
}

#[open_gpui::test]
fn condition_async_provider_projects_loading_then_publishes_without_retention_protection(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    draw_once(cx);
    let condition = workflow_condition_node_id();
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "draft"))
            .is_some(),
        "an unresolved asynchronous field provider is not Ready"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });

    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    draw_once(cx);
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "ready"))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_condition_field_selector(condition))
            .is_some()
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn condition_async_provider_is_revoked_when_the_node_retires_before_completion(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    draw_once(cx);
    let condition = workflow_condition_node_id();
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "draft"))
            .is_some()
    );
    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(condition, IncidentEdgePolicy::Cascade),
        ))
        .applied_revision()
        .expect("condition retires before its provider publishes");

    draw(cx);
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&condition)
            .is_none()
    );
    assert!(
        cx.debug_bounds(&workflow_condition_node_selector(condition))
            .is_none()
    );
    assert!(
        cx.debug_bounds(&workflow_condition_field_selector(condition))
            .is_none()
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn workflow_layout_contract_holds_in_compact_window(cx: &mut open_gpui::TestAppContext) {
    assert_workflow_layout_contract(cx, 960.0, 640.0);
}

#[open_gpui::test]
fn workflow_layout_contract_holds_in_normal_window(cx: &mut open_gpui::TestAppContext) {
    assert_workflow_layout_contract(cx, 1_440.0, 900.0);
}

#[open_gpui::test]
fn workflow_measured_surface_fit_publishes_every_intrinsic_size_without_diagnostics(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new_measured_surface(view, native_registry(cx))
            .expect("valid Open GPUI scene host")
            .with_initial_viewport_policy(InitialViewportPolicy::FitOnFirstMeasuredGeometry)
    });

    for _ in 0..12 {
        draw(cx);
        cx.read_entity(&host, |host, _| {
            assert!(
                host.last_diagnostic().is_none(),
                "measured-surface convergence emitted a host diagnostic: {:?}",
                host.last_diagnostic()
            );
        });
    }

    let snapshot = runtime.snapshot().unwrap();
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert_eq!(geometry.nodes().len(), snapshot.graph().nodes().len());
    for (node_id, _) in snapshot.graph().nodes() {
        let published = geometry
            .node(*node_id)
            .unwrap_or_else(|| panic!("workflow node {node_id:?} has no published geometry"));
        assert!(published.is_fresh());
        assert!(published.bounds().is_positive_finite());
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn workflow_llm_content_height_converges_with_conflict_content(cx: &mut open_gpui::TestAppContext) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let llm = workflow_llm_node_id();
    let initial_size = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&llm)
        .and_then(|node| node.size)
        .expect("catalog LLM size");
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = host_view.downgrade();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    draw(cx);
    let ready_size = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&llm)
        .and_then(|node| node.size)
        .expect("converged ready LLM size");
    assert_eq!(ready_size.width, initial_size.width);
    assert!(
        ready_size.height + 100.0 < initial_size.height,
        "ready LLM content should remove the old document-sized blank area: \
         initial={initial_size:?}, ready={ready_size:?}",
    );
    assert!(
        (weak_view
            .geometry_snapshot()
            .unwrap()
            .node(llm)
            .expect("ready LLM geometry")
            .bounds()
            .size
            .height
            - ready_size.height)
            .abs()
            <= 1.0,
        "published ready geometry must match the converged document size",
    );

    let model = cx
        .debug_bounds(&workflow_model_selector(llm))
        .expect("native model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    let claude = cx
        .debug_bounds(&workflow_model_option_selector(llm, "claude-3.7-sonnet"))
        .expect("native model option");
    apply_remote_model(&remote_view, "remote-provider-model");
    cx.simulate_click(claude.center(), Default::default());
    draw(cx);

    assert!(
        cx.debug_bounds(&workflow_model_conflict_selector(llm))
            .is_some()
    );
    let conflict_size = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&llm)
        .and_then(|node| node.size)
        .expect("converged conflict LLM size");
    assert!(
        conflict_size.height > ready_size.height,
        "visible conflict controls must grow the content-authoritative LLM node: \
         ready={ready_size:?}, conflict={conflict_size:?}",
    );

    let reapply = cx
        .debug_bounds(&workflow_model_reapply_selector(llm))
        .expect("model conflict reapply action");
    cx.simulate_click(reapply.center(), Default::default());
    draw(cx);
    let restored_size = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&llm)
        .and_then(|node| node.size)
        .expect("restored ready LLM size");
    assert!(
        (restored_size.height - ready_size.height).abs() <= 1.0,
        "resolving the conflict must restore the compact ready height: \
         ready={ready_size:?}, restored={restored_size:?}",
    );
}

#[open_gpui::test]
fn condition_header_input_port_stays_clear_of_the_first_branch_row(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime =
        native_runtime(workflow_demo_graph().expect("workflow graph")).expect("workflow runtime");
    let condition = workflow_condition_node_id();
    let snapshot = runtime.snapshot().expect("workflow snapshot");
    let observation =
        ConditionObservation::from_snapshot(&snapshot, condition).expect("condition observation");
    let input = observation.input_port();
    let first_branch = observation
        .cases()
        .iter()
        .next()
        .cloned()
        .expect("first condition branch");
    let else_branch = observation.else_branch().clone();
    drop(snapshot);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });

    draw(cx);

    let input_marker = cx
        .debug_bounds(&workflow_port_selector(input))
        .expect("condition header input marker");
    let first_branch_marker = cx
        .debug_bounds(&workflow_condition_case_selector(
            condition,
            first_branch.port_id(),
        ))
        .expect("first condition branch marker");
    let else_marker = cx
        .debug_bounds(&workflow_condition_else_selector(
            condition,
            else_branch.port_id(),
        ))
        .expect("immutable Else marker");
    assert!(
        input_marker.origin.y + input_marker.size.height + px(12.0) <= first_branch_marker.origin.y,
        "the fixed input port must remain in the header instead of sharing the first branch row"
    );
    assert!(
        else_marker.origin.y > first_branch_marker.origin.y,
        "Else must render after every editable case"
    );

    let geometry = weak_view.geometry_snapshot().expect("workflow geometry");
    let node_bounds = geometry
        .node(condition)
        .expect("condition node geometry")
        .bounds();
    let input_bounds = geometry
        .port(input)
        .expect("condition input geometry")
        .visual_bounds();
    let first_branch_bounds = geometry
        .port(first_branch.port_id())
        .expect("first condition branch geometry")
        .visual_bounds();
    let input_center_x = input_bounds.origin.x + input_bounds.size.width / 2.0;
    let first_branch_center_x = first_branch_bounds.origin.x + first_branch_bounds.size.width / 2.0;
    assert!(
        (input_center_x - node_bounds.origin.x).abs() <= 1.0,
        "the input marker center {input_center_x} must stay on the node's left boundary {}",
        node_bounds.origin.x,
    );
    let node_right = node_bounds.origin.x + node_bounds.size.width;
    assert!(
        (first_branch_center_x - node_right).abs() <= 1.0,
        "the branch marker center {first_branch_center_x} must stay on the node's right boundary \
         {node_right}"
    );
}

#[open_gpui::test]
fn native_workflow_controls_commit_graph_facts_and_publish_real_case_ports(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let host_runtime = runtime.clone();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "ready"))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "ready"))
            .is_some()
    );
    let prompt = cx
        .debug_bounds(&workflow_prompt_selector(llm))
        .expect("native prompt input");
    let model = cx
        .debug_bounds(&workflow_model_selector(llm))
        .expect("native model select");
    let initial = host_runtime.snapshot().unwrap();
    let initial_branches = ConditionObservation::from_snapshot(&initial, condition)
        .unwrap()
        .cases()
        .to_vec();
    let initial_condition_size = initial
        .graph()
        .node(&condition)
        .and_then(|node| node.size)
        .expect("content-height condition has a converged persisted size");
    assert_eq!(initial_branches.len(), 2);
    for branch in &initial_branches {
        assert!(
            cx.debug_bounds(&workflow_condition_case_selector(
                condition,
                branch.port_id()
            ))
            .is_some(),
            "every condition row must expose its real output port"
        );
    }

    cx.simulate_click(prompt.center(), Default::default());
    cx.simulate_input("Route urgent ");
    draw(cx);
    assert_eq!(prompt_value(&host_runtime), "");
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "draft"))
            .is_some()
    );
    cx.simulate_input("customer tickets");
    draw(cx);
    assert_eq!(prompt_value(&host_runtime), "");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert!(host.resource_snapshot().protected_leases() >= 2);
    });
    let apply_prompt = cx
        .debug_bounds(&workflow_prompt_apply_selector(llm))
        .expect("native prompt apply command");
    cx.simulate_click(apply_prompt.center(), Default::default());
    assert_eq!(
        prompt_value(&host_runtime),
        "Route urgent customer tickets",
        "the TestApp event loop may submit the real host frame before simulate_click returns"
    );
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "ready"))
            .is_some()
    );

    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    let claude = cx
        .debug_bounds(&workflow_model_option_selector(llm, "claude-3.7-sonnet"))
        .expect("native model option");
    cx.simulate_click(claude.center(), Default::default());
    assert_eq!(model_value(&host_runtime), "claude-3.7-sonnet");
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "ready"))
            .is_some()
    );

    let field = cx
        .debug_bounds(&workflow_condition_field_selector(condition))
        .expect("native condition field selector after async provider completion");
    cx.simulate_click(field.center(), Default::default());
    draw(cx);
    let risk_score = cx
        .debug_bounds(&condition_field_option_selector(condition, "risk_score"))
        .expect("async condition field option");
    cx.simulate_click(risk_score.center(), Default::default());
    draw(cx);
    let value = cx
        .debug_bounds(&workflow_condition_value_selector(condition))
        .expect("native condition value input");
    cx.simulate_click(value.center(), Default::default());
    cx.simulate_input("0.8");
    draw(cx);
    let add = cx
        .debug_bounds(&workflow_condition_add_selector(condition))
        .expect("native add-branch button");
    cx.simulate_click(add.center(), Default::default());
    for _ in 0..4 {
        draw(cx);
        if ConditionObservation::from_snapshot(&host_runtime.snapshot().unwrap(), condition)
            .is_ok_and(|observation| observation.cases().len() == 3)
        {
            break;
        }
    }
    let updated = host_runtime.snapshot().unwrap();
    let updated_condition = ConditionObservation::from_snapshot(&updated, condition).unwrap();
    let branches = updated_condition.cases();
    assert_eq!(branches.len(), 3);
    let added = branches.last().expect("native Add creates one case");
    let updated_condition_size = updated
        .graph()
        .node(&condition)
        .and_then(|node| node.size)
        .expect("resized condition keeps a persisted size");
    assert!(
        updated_condition_size.height > initial_condition_size.height,
        "adding a visible branch must grow the content-authoritative condition node"
    );
    let geometry = weak_view.geometry_snapshot().unwrap();
    let condition_geometry = geometry
        .node(condition)
        .expect("resized condition geometry remains published");
    assert!(
        (condition_geometry.bounds().size.height - updated_condition_size.height).abs() <= 1.0,
        "published node geometry must agree with the converged graph size"
    );
    for port in [added.port_id(), added.value_port_id()] {
        assert_eq!(
            updated
                .graph()
                .port(&port)
                .expect("new real graph port")
                .node,
            condition
        );
        assert_eq!(
            geometry
                .port(port)
                .expect("new published port geometry")
                .node(),
            condition
        );
    }
    assert!(
        cx.debug_bounds(&workflow_condition_case_value_selector(
            condition,
            added.value_port_id(),
        ))
        .is_some(),
        "the dynamic typed value port has one native marker"
    );

    let before_invalid = host_runtime.snapshot().unwrap();
    let before_invalid_case_count = ConditionObservation::from_snapshot(&before_invalid, condition)
        .unwrap()
        .cases()
        .len();
    enter_condition_case_value(cx, condition, "invalid");
    let add = cx
        .debug_bounds(&workflow_condition_add_selector(condition))
        .expect("native add-branch button for invalid typed value");
    cx.simulate_click(add.center(), Default::default());
    draw(cx);
    draw(cx);

    let after_invalid = host_runtime.snapshot().unwrap();
    assert_eq!(
        ConditionObservation::from_snapshot(&after_invalid, condition)
            .unwrap()
            .cases()
            .len(),
        before_invalid_case_count,
    );
    assert!(
        cx.debug_bounds(&workflow_condition_diagnostic_selector(condition))
            .is_some(),
        "invalid typed draft remains local and its diagnostic survives reconciliation frames"
    );
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "error"))
            .is_some()
    );
    let value = cx
        .debug_bounds(&workflow_condition_value_selector(condition))
        .expect("invalid native condition value input remains editable");
    cx.simulate_click(value.center(), Default::default());
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("0.9");
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_condition_diagnostic_selector(condition))
            .is_none(),
        "correcting a typed literal clears only its validation diagnostic"
    );
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "ready"))
            .is_some()
    );
    cx.read_entity(&host, |host, _| {
        assert!(host.last_diagnostic().is_none());
    });
}

#[open_gpui::test]
fn native_condition_case_list_scrolls_without_changing_the_canvas_viewport(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    for index in 0..5 {
        enter_condition_case_value(cx, condition, &format!("scroll-{index}"));
        let add = cx
            .debug_bounds(&workflow_condition_add_selector(condition))
            .expect("native add-branch button");
        cx.simulate_click(add.center(), Default::default());
        draw(cx);
    }

    let snapshot = runtime.snapshot().unwrap();
    let observation = ConditionObservation::from_snapshot(&snapshot, condition).unwrap();
    let branches = observation.cases();
    let first_branch = branches.first().expect("first condition branch");
    let first_port = first_branch.port_id();
    let first_selector = workflow_condition_case_selector(condition, first_branch.port_id());
    let first_before = cx
        .debug_bounds(&first_selector)
        .expect("first branch before scrolling");
    let scroll = cx
        .debug_bounds(&workflow_condition_scroll_selector(condition))
        .expect("native condition scroll region");
    let viewport_before = weak_view.viewport_transform().unwrap();
    let runtime_snapshot = runtime.snapshot().unwrap();
    let graph_before = frozen_graph_value(runtime_snapshot.graph());
    let before_geometry = weak_view.geometry_snapshot().unwrap();
    let first_before_geometry = before_geometry
        .port(first_port)
        .expect("first branch geometry before scrolling");
    assert!(first_before_geometry.hit_bounds().is_some());

    cx.simulate_event(ScrollEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -3.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    draw(cx);

    let first_after = cx
        .debug_bounds(&first_selector)
        .expect("first branch after scrolling");
    assert!(first_after.origin.y < first_before.origin.y);
    assert_eq!(weak_view.viewport_transform().unwrap(), viewport_before);
    let hidden_geometry = weak_view.geometry_snapshot().unwrap();
    let hidden_port = hidden_geometry
        .port(first_port)
        .expect("fully clipped branch keeps route geometry");
    assert_eq!(hidden_port.port(), first_port);
    assert_eq!(hidden_port.hit_bounds(), None);
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        graph_before,
        "native scrolling must not mutate ports or existing edges",
    );
    for edge in runtime.snapshot().unwrap().graph().edges().keys() {
        assert!(
            hidden_geometry.route(*edge).is_some(),
            "existing edge {edge:?} keeps a published route while branch ports are clipped",
        );
    }

    cx.simulate_event(ScrollEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, 3.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    draw(cx);

    let restored_geometry = weak_view.geometry_snapshot().unwrap();
    let restored_port = restored_geometry
        .port(first_port)
        .expect("scrolled-back branch keeps the same port id");
    assert!(restored_port.hit_bounds().is_some());
}

#[open_gpui::test]
fn native_condition_reorder_and_remove_controls_republish_dynamic_port_geometry(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    let initial = runtime.snapshot().unwrap();
    let initial_condition = ConditionObservation::from_snapshot(&initial, condition).unwrap();
    let initial_branches = initial_condition.cases();
    let first = initial_branches[0].clone();
    let connected = initial_branches[1].clone();
    assert!(
        cx.debug_bounds(&workflow_condition_up_selector(condition, first.port_id()))
            .is_none(),
        "the first branch must not expose an upward mutation"
    );

    let up = cx
        .debug_bounds(&workflow_condition_up_selector(
            condition,
            connected.port_id(),
        ))
        .expect("second branch Up button");
    cx.simulate_click(up.center(), Default::default());
    draw(cx);

    let after_up = runtime.snapshot().unwrap();
    let after_up_condition = ConditionObservation::from_snapshot(&after_up, condition).unwrap();
    assert_eq!(
        after_up_condition.cases(),
        [connected.clone(), first.clone()]
    );
    let geometry = weak_view.geometry_snapshot().unwrap();
    for branch in [&first, &connected] {
        let layout = geometry
            .port(branch.port_id())
            .expect("reordered branch keeps port geometry");
        assert_eq!(layout.node(), condition);
        assert!(layout.visual_bounds().size.is_positive_finite());
    }

    let remove = cx
        .debug_bounds(&workflow_condition_remove_selector(
            condition,
            connected.port_id(),
        ))
        .expect("connected branch Remove button");
    cx.simulate_click(remove.center(), Default::default());
    draw(cx);

    let after_remove = runtime.snapshot().unwrap();
    assert!(
        after_remove
            .graph()
            .node(&condition)
            .and_then(|node| node.size)
            .expect("condition size after remove")
            .height
            < after_up
                .graph()
                .node(&condition)
                .and_then(|node| node.size)
                .expect("condition size before remove")
                .height,
        "removing a visible branch must shrink the content-authoritative condition node"
    );
    assert_eq!(
        ConditionObservation::from_snapshot(&after_remove, condition)
            .unwrap()
            .cases(),
        std::slice::from_ref(&first)
    );
    assert!(after_remove.graph().port(&connected.port_id()).is_none());
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.port(connected.port_id()).is_none());
    assert_eq!(
        geometry
            .port(first.port_id())
            .expect("surviving branch geometry")
            .node(),
        condition
    );
}

#[open_gpui::test]
fn prompt_conflict_can_explicitly_reapply_local_text_after_another_view_wins(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let fixture_node_count = runtime.snapshot().unwrap().graph().nodes().len();
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);
    let llm = workflow_llm_node_id();
    let prompt = cx
        .debug_bounds(&workflow_prompt_selector(llm))
        .expect("native prompt input");
    cx.simulate_click(prompt.center(), Default::default());
    cx.simulate_input("local prompt");
    apply_remote_prompt(&remote_view, "remote prompt");
    draw(cx);

    assert_eq!(prompt_value(&runtime), "remote prompt");
    assert!(
        cx.debug_bounds(&workflow_prompt_conflict_selector(llm))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_prompt_accept_remote_selector(llm))
            .is_some()
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let reapply = cx
        .debug_bounds(&workflow_prompt_reapply_selector(llm))
        .expect("explicit local reapply action");
    let before_reapply_revision = runtime.snapshot().unwrap().revision();
    cx.simulate_click(reapply.center(), Default::default());

    assert_eq!(prompt_value(&runtime), "local prompt");
    assert!(runtime.snapshot().unwrap().revision() > before_reapply_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_prompt_conflict_selector(llm))
            .is_none()
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        )
    });
}

#[open_gpui::test]
fn prompt_conflict_can_discard_local_text_without_writing_the_graph(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let fixture_node_count = runtime.snapshot().unwrap().graph().nodes().len();
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);
    let llm = workflow_llm_node_id();
    let prompt = cx
        .debug_bounds(&workflow_prompt_selector(llm))
        .expect("native prompt input");
    cx.simulate_click(prompt.center(), Default::default());
    cx.simulate_input("discard me");
    apply_remote_prompt(&remote_view, "authoritative prompt");
    draw(cx);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let data_before_accept = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&llm)
        .unwrap()
        .data
        .clone();
    let accept = cx
        .debug_bounds(&workflow_prompt_accept_remote_selector(llm))
        .expect("explicit remote acceptance action");
    cx.simulate_click(accept.center(), Default::default());
    draw(cx);

    assert_eq!(prompt_value(&runtime), "authoritative prompt");
    assert_eq!(
        runtime.snapshot().unwrap().graph().node(&llm).unwrap().data,
        data_before_accept
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert!(
        cx.debug_bounds(&workflow_prompt_conflict_selector(llm))
            .is_none()
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        )
    });
}

#[open_gpui::test]
fn model_selection_requires_explicit_reapply_after_a_remote_change_arrives_while_open(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);
    let llm = workflow_llm_node_id();
    let model = cx
        .debug_bounds(&workflow_model_selector(llm))
        .expect("native model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    let claude = cx
        .debug_bounds(&workflow_model_option_selector(llm, "claude-3.7-sonnet"))
        .expect("native model option");
    apply_remote_model(&remote_view, "remote-provider-model");
    cx.simulate_click(claude.center(), Default::default());
    draw(cx);

    assert_eq!(model_value(&runtime), "remote-provider-model");
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert!(
        cx.debug_bounds(&workflow_model_conflict_selector(llm))
            .is_some()
    );
    let reapply = cx
        .debug_bounds(&workflow_model_reapply_selector(llm))
        .expect("explicit model reapply action");
    let before_reapply_revision = runtime.snapshot().unwrap().revision();
    cx.simulate_click(reapply.center(), Default::default());

    assert_eq!(model_value(&runtime), "claude-3.7-sonnet");
    assert!(runtime.snapshot().unwrap().revision() > before_reapply_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_conflict_selector(llm))
            .is_none()
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
}

#[open_gpui::test]
fn stale_condition_add_waits_for_refresh_and_explicit_retry_without_render_resubmission(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = host_view.downgrade();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);
    let mounted_revision = runtime.snapshot().unwrap().revision();

    let condition = workflow_condition_node_id();
    enter_condition_case_value(cx, condition, "local retry");
    cx.update(|window, cx| window.blur(cx));
    draw(cx);
    let remote_snapshot = remote_view.graph_snapshot().unwrap();
    let concurrent = ConditionCase::new(
        CONCURRENT_CASE_ID,
        "Needs review",
        PortId::from_u128(CONCURRENT_CASE_PORT),
        PortId::from_u128(CONCURRENT_CASE_VALUE_PORT),
        test_condition_predicate("concurrent"),
    )
    .unwrap();
    let concurrent_intent = ConditionIntent::add(concurrent.clone());
    let remote_condition =
        ConditionObservation::from_snapshot(&remote_snapshot, condition).unwrap();
    let remote_add = remote_condition.plan(&concurrent_intent).unwrap();
    let _interleave_remote_command = cx.update(|_, cx| {
        let mut remote_add = Some(remote_add);
        cx.observe(&host, move |_, _| {
            let Some(remote_add) = remote_add.take() else {
                return;
            };
            remote_view
                .execute(remote_add)
                .unwrap()
                .applied_revision()
                .expect("remote add applies before the queued native command");
        })
    });

    let add = cx
        .debug_bounds(&workflow_condition_add_selector(condition))
        .expect("native add-branch button");
    cx.simulate_click(add.center(), Default::default());

    draw(cx);
    let conflict_revision = runtime.snapshot().unwrap().revision();
    assert!(conflict_revision > mounted_revision);
    draw(cx);
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_some(),
        "stale condition intent exposes explicit Retry after refresh",
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let refreshed_snapshot = runtime.snapshot().unwrap();
    let refreshed_condition =
        ConditionObservation::from_snapshot(&refreshed_snapshot, condition).unwrap();
    let refreshed = refreshed_condition.cases();
    assert_eq!(refreshed.len(), 3);
    assert_eq!(refreshed.last(), Some(&concurrent));
    assert!(refreshed_condition.satisfies(&concurrent_intent));
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert!(host.resource_snapshot().parked_nodes() >= 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(viewport());
        cx.notify();
    });
    draw(cx);
    let retry = cx
        .debug_bounds(&condition_retry_selector(condition))
        .expect("recoverable topology intent survives parking and remounting");

    draw(cx);
    draw(cx);
    assert_eq!(runtime.snapshot().unwrap().revision(), conflict_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let generation_before_retry = weak_view
        .geometry_snapshot()
        .unwrap()
        .node(condition)
        .unwrap()
        .layout_generation();

    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert!(runtime.snapshot().unwrap().revision() > conflict_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_none()
    );
    let generation_after_retry = weak_view
        .geometry_snapshot()
        .unwrap()
        .node(condition)
        .unwrap()
        .layout_generation();
    assert_eq!(
        generation_after_retry,
        generation_before_retry + 3,
        "retry publishes one topology patch, one managed invalidation, and one converged size patch"
    );
    draw(cx);
    draw(cx);
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .node(condition)
            .unwrap()
            .layout_generation(),
        generation_after_retry,
        "settled retry frames cannot replay layout invalidation"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    let committed_snapshot = runtime.snapshot().unwrap();
    let committed_condition =
        ConditionObservation::from_snapshot(&committed_snapshot, condition).unwrap();
    let committed = committed_condition.cases();
    assert_eq!(committed.len(), 4);
    assert_eq!(committed[2], concurrent);
    assert!(committed[3].id().starts_with("case-"));
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .port(&committed[3].port_id())
            .is_some()
    );
}

#[open_gpui::test]
fn condition_retry_is_abandoned_when_remote_topology_invalidates_its_intent(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak_view = host_view.downgrade();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    let remote_snapshot = remote_view.graph_snapshot().unwrap();
    let remote_condition =
        ConditionObservation::from_snapshot(&remote_snapshot, condition).unwrap();
    let removed = remote_condition.cases()[1].clone();
    let remove_intent = ConditionIntent::remove(removed.id(), IncidentEdgePolicy::Cascade);
    let remote_remove = remote_condition.plan(&remove_intent).unwrap();
    let _interleave_remote_command = cx.update(|_, cx| {
        let mut remote_remove = Some(remote_remove);
        cx.observe(&host, move |_, _| {
            let Some(remote_remove) = remote_remove.take() else {
                return;
            };
            remote_view
                .execute(remote_remove)
                .unwrap()
                .applied_revision()
                .expect("remote removal applies before the queued native reorder");
        })
    });

    let up = cx
        .debug_bounds(&workflow_condition_up_selector(
            condition,
            removed.port_id(),
        ))
        .expect("native reorder action for the soon-to-be-removed case");
    cx.simulate_click(up.center(), Default::default());
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_some(),
        "stale local reorder first reaches explicit Retryable state"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });
    let before_retry = runtime.snapshot().unwrap();
    let before_retry_history = runtime.history_state().unwrap();
    let generation_before_retry = weak_view
        .geometry_snapshot()
        .unwrap()
        .node(condition)
        .unwrap()
        .layout_generation();

    let retry = cx
        .debug_bounds(&condition_retry_selector(condition))
        .expect("invalidated retry action");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    draw(cx);

    let after_retry = runtime.snapshot().unwrap();
    let after_condition = ConditionObservation::from_snapshot(&after_retry, condition).unwrap();
    assert!(after_condition.satisfies(&remove_intent));
    assert_eq!(
        after_retry.graph().node(&condition).unwrap().data,
        before_retry.graph().node(&condition).unwrap().data
    );
    assert_eq!(runtime.history_state().unwrap(), before_retry_history);
    assert_eq!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .node(condition)
            .unwrap()
            .layout_generation(),
        generation_before_retry + 1,
        "abandoning an invalidated retry publishes only its diagnostic size convergence"
    );
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_none(),
        "invalid product intent cannot remain permanently retryable"
    );
    assert!(
        cx.debug_bounds(&workflow_condition_diagnostic_selector(condition))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(condition, "error"))
            .is_some()
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn condition_unparseable_observation_drains_completion_and_allows_explicit_discard(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    enter_condition_case_value(cx, condition, "unparseable remote update");
    let remote_snapshot = remote_view.graph_snapshot().unwrap();
    let mut malformed_data = remote_snapshot
        .graph()
        .node(&condition)
        .unwrap()
        .data
        .clone();
    malformed_data["cases"] = serde_json::json!("not-an-array");
    let remote_update =
        GraphCommand::set_node_data(remote_snapshot.version(), condition, malformed_data.clone());
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
                .expect("remote malformed Condition data applies before the local command");
        })
    });

    let add = cx
        .debug_bounds(&workflow_condition_add_selector(condition))
        .expect("local add before malformed remote update");
    cx.simulate_click(add.center(), Default::default());
    draw(cx);
    draw(cx);

    assert!(
        cx.debug_bounds(&condition_discard_selector(condition))
            .is_some(),
        "completed local mutation remains explicitly discardable when Condition facts cannot parse"
    );
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_none()
    );
    let before_discard = runtime.snapshot().unwrap();
    let before_discard_history = runtime.history_state().unwrap();
    assert_eq!(
        before_discard.graph().node(&condition).unwrap().data,
        malformed_data
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    let discard = cx
        .debug_bounds(&condition_discard_selector(condition))
        .expect("discard unavailable Condition mutation");
    cx.simulate_click(discard.center(), Default::default());
    draw(cx);
    draw(cx);

    assert!(
        cx.debug_bounds(&condition_discard_selector(condition))
            .is_none()
    );
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&condition)
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
fn controlled_model_select_escape_restores_focus_releases_lease_and_is_graph_view_inert(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let graph_before = runtime
        .snapshot()
        .expect("graph before opening model select");
    let history_before = runtime
        .history_state()
        .expect("history before opening model select");
    let selection_before = weak_view
        .selection()
        .expect("selection before opening model select");
    let viewport_before = weak_view
        .viewport_transform()
        .expect("viewport before opening model select");
    let llm = workflow_llm_node_id();
    let model_selector = workflow_model_selector(llm);
    let option_selector = workflow_model_option_selector(llm, "claude-3.7-sonnet");

    let model = cx
        .debug_bounds(&model_selector)
        .expect("native model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);

    assert!(cx.debug_bounds(&option_selector).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.simulate_keystrokes("escape");
    draw(cx);

    assert!(cx.debug_bounds(&option_selector).is_none());
    assert!(
        cx.debug_selector_is_focused(&model_selector),
        "Escape returns focus to the controlled select trigger",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after Escape")));
    assert_eq!(
        runtime.history_state().expect("history after Escape"),
        history_before,
    );
    assert_eq!(
        weak_view.selection().expect("selection after Escape"),
        selection_before,
    );
    assert_eq!(
        weak_view
            .viewport_transform()
            .expect("viewport after Escape"),
        viewport_before,
    );
}

#[open_gpui::test]
fn controlled_model_select_outside_press_dismisses_without_reaching_the_graph_underlay(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let condition = workflow_condition_node_id();
    let condition_input = {
        let snapshot = runtime.snapshot().expect("workflow snapshot");
        ConditionObservation::from_snapshot(&snapshot, condition)
            .expect("condition observation")
            .input_port()
    };
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let graph_before = runtime.snapshot().expect("graph before outside presses");
    let history_before = runtime
        .history_state()
        .expect("history before outside presses");
    let selection_before = weak_view
        .selection()
        .expect("selection before outside presses");
    let viewport_before = weak_view
        .viewport_transform()
        .expect("viewport before outside presses");
    let llm = workflow_llm_node_id();
    let model_selector = workflow_model_selector(llm);
    let option_selector = workflow_model_option_selector(llm, "claude-3.7-sonnet");
    let condition_bounds = cx
        .debug_bounds(&workflow_condition_node_selector(condition))
        .expect("condition node bounds");
    let condition_input_bounds = cx
        .debug_bounds(&workflow_port_selector(condition_input))
        .expect("condition input bounds");
    let outside_positions = [
        point(px(1_030.0), px(680.0)),
        condition_bounds.center(),
        condition_input_bounds.center(),
    ];

    for position in outside_positions {
        let model = cx
            .debug_bounds(&model_selector)
            .expect("native model select");
        cx.simulate_click(model.center(), Default::default());
        draw(cx);
        assert!(cx.debug_bounds(&option_selector).is_some());
        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().protected_instances(), 1);
            assert_eq!(host.resource_snapshot().protected_leases(), 1);
        });

        cx.simulate_mouse_down(position, MouseButton::Left, Default::default());

        assert!(cx.update(|window, _| window.captured_pointer()).is_none());
        cx.read_entity(&host, |host, _| {
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Idle,
            );
            assert!(host.active_port_drag().is_none());
        });
        assert_eq!(
            weak_view.selection().expect("selection after MouseDown"),
            selection_before,
        );
        assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after MouseDown")));
        assert_eq!(
            runtime.history_state().expect("history after MouseDown"),
            history_before,
        );
        assert_eq!(
            weak_view
                .viewport_transform()
                .expect("viewport after MouseDown"),
            viewport_before,
        );

        cx.simulate_mouse_up(position, MouseButton::Left, Default::default());
        draw(cx);

        assert!(cx.debug_bounds(&option_selector).is_none());
        cx.read_entity(&host, |host, _| {
            assert_eq!(host.resource_snapshot().protected_instances(), 0);
            assert_eq!(host.resource_snapshot().protected_leases(), 0);
        });
        assert_eq!(
            weak_view
                .selection()
                .expect("selection after outside press"),
            selection_before,
        );
        assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after outside press")));
        assert_eq!(
            runtime
                .history_state()
                .expect("history after outside press"),
            history_before,
        );
        assert_eq!(
            weak_view
                .viewport_transform()
                .expect("viewport after outside press"),
            viewport_before,
        );
    }
}

#[open_gpui::test]
fn model_overlay_lease_protects_culling_and_releases_on_every_close_path(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let fixture_node_count = runtime.snapshot().unwrap().graph().nodes().len();
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let llm = workflow_llm_node_id();
    let model_selector = workflow_model_selector(llm);
    let option_selector = workflow_model_option_selector(llm, "claude-3.7-sonnet");
    let model = cx
        .debug_bounds(&model_selector)
        .expect("native model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(&option_selector).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(
            host.resource_snapshot().parked_nodes(),
            fixture_node_count - 1
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(viewport());
        cx.notify();
    });
    draw(cx);
    let model = cx
        .debug_bounds(&model_selector)
        .expect("restored model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(&option_selector).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), fixture_node_count);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(viewport());
        cx.notify();
    });
    draw(cx);
    let model = cx
        .debug_bounds(&model_selector)
        .expect("restored model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    let claude = cx
        .debug_bounds(&option_selector)
        .expect("native model option");
    cx.simulate_click(claude.center(), Default::default());
    draw(cx);
    assert_eq!(model_value(&runtime), "claude-3.7-sonnet");
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });

    let model = cx
        .debug_bounds(&model_selector)
        .expect("model select before retirement");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });
    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(llm, IncidentEdgePolicy::Cascade),
        ))
        .applied_revision()
        .expect("LLM retirement applies");
    draw(cx);

    assert!(cx.debug_bounds(&model_selector).is_none());
    assert!(cx.debug_bounds(&option_selector).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn model_overlay_lease_preserves_native_workflow_across_semantic_zoom(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let fixture_node_count = runtime.snapshot().unwrap().graph().nodes().len();
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    let model_selector = workflow_model_selector(llm);
    let option_selector = workflow_model_option_selector(llm, "claude-3.7-sonnet");
    let prompt_selector = workflow_prompt_selector(llm);
    let baseline = runtime.snapshot().unwrap();
    let condition_branch = ConditionObservation::from_snapshot(&baseline, condition)
        .unwrap()
        .cases()[0]
        .port_id();
    let baseline_revision = baseline.revision();
    let baseline_graph = frozen_graph_value(baseline.graph());
    let baseline_history = runtime.history_state().unwrap();
    let expected_ports = baseline
        .graph()
        .ports()
        .into_iter()
        .map(|(port, _)| *port)
        .collect::<Vec<_>>();
    let assert_graph_unchanged = || {
        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.revision(), baseline_revision);
        assert_eq!(frozen_graph_value(snapshot.graph()), baseline_graph);
        assert_eq!(runtime.history_state().unwrap(), baseline_history);
    };

    let model = cx
        .debug_bounds(&model_selector)
        .expect("native model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(&option_selector).is_some());
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 0.5)
                .expect("valid workflow overview viewport"),
        )
        .unwrap();
    draw(cx);
    assert!(cx.debug_bounds(&model_selector).is_some());
    assert!(cx.debug_bounds(&prompt_selector).is_some());
    assert!(cx.debug_bounds(&option_selector).is_some());
    assert!(
        cx.debug_bounds(&workflow_condition_case_selector(
            condition,
            condition_branch,
        ))
        .is_none(),
        "an unprotected condition node may use overview while the LLM overlay stays native"
    );
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some());
    assert_graph_unchanged();
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        );
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });

    let model = cx
        .debug_bounds(&model_selector)
        .expect("protected low-zoom model select");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    assert!(cx.debug_bounds(&model_selector).is_none());
    assert!(cx.debug_bounds(&prompt_selector).is_none());
    assert!(cx.debug_bounds(&option_selector).is_none());
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_some());
    assert_graph_unchanged();
    let overview_geometry = weak_view.geometry_snapshot().unwrap();
    assert_eq!(overview_geometry.ports().len(), expected_ports.len());
    for port in &expected_ports {
        let geometry = overview_geometry
            .port(*port)
            .expect("overview keeps every workflow port in published geometry");
        assert!(geometry.visual_bounds().size.is_positive_finite());
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        );
    });

    weak_view
        .set_viewport(
            ViewportTransform::new(CanvasPoint::default(), 1.0)
                .expect("valid workflow detail viewport"),
        )
        .unwrap();
    draw(cx);
    assert!(cx.debug_bounds(&model_selector).is_some());
    assert!(cx.debug_bounds(&prompt_selector).is_some());
    assert!(cx.debug_bounds(&option_selector).is_none());
    assert!(
        cx.debug_bounds(&workflow_node_status_selector(llm, "ready"))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_condition_case_selector(
            condition,
            condition_branch,
        ))
        .is_some()
    );
    assert!(cx.debug_bounds(OVERVIEW_NODE_SELECTOR).is_none());
    assert_eq!(prompt_value(&runtime), "");
    assert_eq!(model_value(&runtime), "gpt-4.1");
    assert_graph_unchanged();
    let restored_geometry = weak_view.geometry_snapshot().unwrap();
    assert_eq!(restored_geometry.ports().len(), expected_ports.len());
    for port in expected_ports {
        assert!(restored_geometry.port(port).is_some());
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
        assert_eq!(
            host.resource_snapshot().strong_instances(),
            fixture_node_count
        );
        assert!(host.last_diagnostic().is_none());
    });
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 1_100.0,
            height: 720.0,
        },
    }
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

fn condition_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-retry-{}", node.0)
}

fn condition_discard_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-discard-{}", node.0)
}

fn condition_field_option_selector(node: NodeId, value: &str) -> String {
    format!(
        "listbox:jellyflow-workflow-condition-field-{}-listbox:option:{value}",
        node.0
    )
}

fn enter_condition_case_value(cx: &mut VisualTestContext, node: NodeId, value: &str) {
    let input = cx
        .debug_bounds(&workflow_condition_value_selector(node))
        .expect("native condition value input");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input(value);
    draw(cx);
}

fn test_condition_predicate(literal: &str) -> ConditionPredicate {
    ConditionPredicate::equals(
        "customer_intent",
        serde_json::json!(literal),
        TypeDesc::String,
    )
    .expect("valid test condition predicate")
}

fn offscreen_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: 10_000.0,
            y: 10_000.0,
        },
        size: CanvasSize {
            width: 1_100.0,
            height: 720.0,
        },
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    draw_once(cx);
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
}

fn draw_once(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn assert_workflow_port_profile(
    port: &PublishedPortGeometry,
    transform: &ViewportTransform,
    detail: bool,
) {
    let visual = projected_workflow_rect_size(port.visual_bounds(), transform);
    let hit = projected_workflow_rect_size(
        port.hit_bounds().expect("visible workflow port hit target"),
        transform,
    );
    if detail {
        assert!((visual.width - 12.0).abs() <= 0.1);
        assert!((visual.height - 12.0).abs() <= 0.1);
    } else {
        assert!(visual.width > 0.0 && visual.width <= 10.0);
        assert!(visual.height > 0.0 && visual.height <= 10.0);
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

fn projected_workflow_rect_size(bounds: CanvasRect, transform: &ViewportTransform) -> CanvasSize {
    let origin = transform
        .screen_point_for_canvas(bounds.origin)
        .expect("finite workflow bounds origin");
    let far = transform
        .screen_point_for_canvas(CanvasPoint {
            x: bounds.origin.x + bounds.size.width,
            y: bounds.origin.y + bounds.size.height,
        })
        .expect("finite workflow bounds extent");
    CanvasSize {
        width: (far.x - origin.x).abs(),
        height: (far.y - origin.y).abs(),
    }
}

fn assert_workflow_layout_contract(
    cx: &mut open_gpui::TestAppContext,
    window_width: f32,
    window_height: f32,
) {
    let runtime = native_runtime(workflow_layout_graph()).expect("workflow layout runtime");
    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    let snapshot = runtime.snapshot().expect("workflow layout snapshot");
    let llm_input = workflow_port_by_key(&snapshot, llm, "input");
    let llm_output = workflow_port_by_key(&snapshot, llm, "success");
    let condition_input = workflow_port_by_key(&snapshot, condition, "input");
    let condition_observation =
        ConditionObservation::from_snapshot(&snapshot, condition).expect("layout condition");
    let cases = condition_observation.cases().to_vec();
    let else_port = condition_observation.else_branch().port_id();
    drop(snapshot);

    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow layout view");
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

    assert_llm_layout(&mut cx, llm, [llm_input, llm_output]);
    assert_condition_layout(&mut cx, condition, condition_input, &cases, else_port);
}

fn workflow_layout_graph() -> Graph {
    let graph = workflow_demo_graph().expect("workflow layout graph");
    let mut builder = GraphBuilder::from_detached_graph(graph);
    let label = "Customer intent matches an intentionally long production routing condition label";
    let (first_case_port, first_case_value_port) = {
        let condition = builder
            .node_mut(&workflow_condition_node_id())
            .expect("workflow layout condition");
        let port = serde_json::from_value(condition.data["cases"][0]["port_id"].clone())
            .expect("workflow layout case port id");
        let value_port =
            serde_json::from_value(condition.data["cases"][0]["value_port_id"].clone())
                .expect("workflow layout case value port id");
        condition.data["cases"][0]["label"] = serde_json::json!(label);
        (port, value_port)
    };
    builder
        .port_mut(&first_case_port)
        .expect("workflow layout case port")
        .data["label"] = serde_json::json!(label);
    builder
        .port_mut(&first_case_value_port)
        .expect("workflow layout case value port")
        .data["label"] = serde_json::json!(label);
    builder.build().expect("valid workflow layout graph")
}

fn assert_llm_layout(cx: &mut VisualTestContext, node_id: NodeId, ports: [PortId; 2]) {
    let node = workflow_layout_bounds(cx, workflow_llm_node_selector(node_id), "LLM node");
    let title = workflow_layout_bounds(
        cx,
        workflow_header_title_selector(node_id),
        "LLM header title",
    );
    let metadata = workflow_layout_bounds(
        cx,
        workflow_header_metadata_selector(node_id),
        "LLM header metadata",
    );
    let body = workflow_layout_bounds(cx, workflow_llm_body_selector(node_id), "LLM body");
    let prompt = workflow_layout_bounds(cx, workflow_prompt_selector(node_id), "prompt control");
    let apply = workflow_layout_bounds(
        cx,
        workflow_prompt_apply_selector(node_id),
        "Apply prompt control",
    );
    let model = workflow_layout_bounds(cx, workflow_model_selector(node_id), "model control");
    let markers = ports.map(|port| {
        workflow_layout_bounds(cx, workflow_port_selector(port), "LLM header port marker")
    });

    for (name, bounds) in [
        ("title", title),
        ("metadata", metadata),
        ("body", body),
        ("prompt", prompt),
        ("Apply prompt", apply),
        ("model", model),
    ] {
        assert_workflow_layout_contained(bounds, node, &format!("LLM {name}"));
    }
    assert_workflow_layout_disjoint(title, metadata, "LLM title and metadata");
    assert_workflow_layout_disjoint(title, body, "LLM header title and body");
    assert_workflow_layout_disjoint(metadata, body, "LLM header metadata and body");
    assert_workflow_layout_disjoint(prompt, apply, "LLM prompt and Apply prompt");
    assert_workflow_layout_disjoint(prompt, model, "LLM prompt and model");
    assert_workflow_layout_disjoint(apply, model, "LLM Apply prompt and model");
    for (index, marker) in markers.into_iter().enumerate() {
        assert_workflow_layout_disjoint(marker, title, &format!("LLM port {index} and title"));
        assert_workflow_layout_disjoint(
            marker,
            metadata,
            &format!("LLM port {index} and metadata"),
        );
        assert_workflow_layout_disjoint(marker, body, &format!("LLM port {index} and body"));
    }
}

fn assert_condition_layout(
    cx: &mut VisualTestContext,
    node_id: NodeId,
    input: PortId,
    cases: &[ConditionCase],
    else_port: PortId,
) {
    let node = workflow_layout_bounds(
        cx,
        workflow_condition_node_selector(node_id),
        "Condition node",
    );
    let title = workflow_layout_bounds(
        cx,
        workflow_header_title_selector(node_id),
        "Condition header title",
    );
    let metadata = workflow_layout_bounds(
        cx,
        workflow_header_metadata_selector(node_id),
        "Condition header metadata",
    );
    let body = workflow_layout_bounds(
        cx,
        workflow_condition_body_selector(node_id),
        "Condition body",
    );
    let input_marker =
        workflow_layout_bounds(cx, workflow_port_selector(input), "Condition input marker");
    let add_case = workflow_layout_bounds(
        cx,
        workflow_condition_add_selector(node_id),
        "Add case control",
    );

    for (name, bounds) in [
        ("title", title),
        ("metadata", metadata),
        ("body", body),
        ("Add case", add_case),
    ] {
        assert_workflow_layout_contained(bounds, node, &format!("Condition {name}"));
    }
    assert_workflow_layout_disjoint(title, metadata, "Condition title and metadata");
    assert_workflow_layout_disjoint(title, body, "Condition header title and body");
    assert_workflow_layout_disjoint(metadata, body, "Condition header metadata and body");
    assert_workflow_layout_disjoint(input_marker, title, "Condition input and title");
    assert_workflow_layout_disjoint(input_marker, metadata, "Condition input and metadata");
    assert_workflow_layout_disjoint(input_marker, body, "Condition input and body");

    let mut previous_label = None;
    for case in cases {
        let route_marker = workflow_layout_bounds(
            cx,
            workflow_condition_case_selector(node_id, case.port_id()),
            "Condition case route marker",
        );
        let value_marker = workflow_layout_bounds(
            cx,
            workflow_condition_case_value_selector(node_id, case.value_port_id()),
            "Condition case value marker",
        );
        let label = workflow_layout_bounds(
            cx,
            workflow_condition_case_label_selector(node_id, case.port_id()),
            "Condition case label",
        );
        let actions = workflow_layout_bounds(
            cx,
            workflow_condition_case_actions_selector(node_id, case.port_id()),
            "Condition case actions",
        );
        assert_workflow_layout_contained(label, body, "Condition case label");
        assert_workflow_layout_contained(actions, body, "Condition case actions");
        assert_workflow_layout_disjoint(route_marker, label, "Condition route marker and label");
        assert_workflow_layout_disjoint(
            route_marker,
            actions,
            "Condition route marker and actions",
        );
        assert_workflow_layout_disjoint(value_marker, label, "Condition value marker and label");
        assert_workflow_layout_disjoint(
            value_marker,
            actions,
            "Condition value marker and actions",
        );
        assert_workflow_layout_disjoint(label, actions, "Condition case label and actions");
        assert_workflow_layout_disjoint(label, add_case, "Condition case label and Add case");
        assert_workflow_layout_disjoint(actions, add_case, "Condition case actions and Add case");
        if let Some(previous) = previous_label {
            assert_workflow_layout_disjoint(previous, label, "adjacent Condition case labels");
        }
        previous_label = Some(label);
    }

    let else_marker = workflow_layout_bounds(
        cx,
        workflow_condition_else_selector(node_id, else_port),
        "Condition Else marker",
    );
    let else_label = workflow_layout_bounds(
        cx,
        workflow_condition_else_label_selector(node_id, else_port),
        "Condition Else label",
    );
    assert_workflow_layout_contained(else_label, body, "Condition Else label");
    assert_workflow_layout_disjoint(else_marker, else_label, "Condition Else marker and label");
    assert_workflow_layout_disjoint(else_label, add_case, "Condition Else label and Add case");
    if let Some(previous) = previous_label {
        assert_workflow_layout_disjoint(previous, else_label, "last case and Else labels");
    }
}

fn workflow_port_by_key(
    snapshot: &jellyflow_runtime::graph::GraphSnapshot,
    node: NodeId,
    key: &str,
) -> PortId {
    snapshot
        .graph()
        .node(&node)
        .expect("layout node")
        .ports
        .iter()
        .copied()
        .find(|port| {
            snapshot
                .graph()
                .port(port)
                .is_some_and(|port| port.key.0 == key)
        })
        .unwrap_or_else(|| panic!("missing {key} port on {node:?}"))
}

fn workflow_layout_bounds(
    cx: &mut VisualTestContext,
    selector: impl AsRef<str>,
    name: &str,
) -> Bounds<Pixels> {
    cx.debug_bounds(selector.as_ref())
        .unwrap_or_else(|| panic!("missing layout bounds for {name}"))
}

fn assert_workflow_layout_contained(child: Bounds<Pixels>, parent: Bounds<Pixels>, context: &str) {
    assert!(
        child.is_contained_within(&parent),
        "{context} escaped its owner: child={child:?}, parent={parent:?}",
    );
}

fn assert_workflow_layout_disjoint(left: Bounds<Pixels>, right: Bounds<Pixels>, context: &str) {
    assert!(
        !left.intersects(&right),
        "{context} overlapped: left={left:?}, right={right:?}",
    );
}

fn apply_remote_prompt(view: &jellyflow_runtime::view::ViewSession, value: &str) {
    let snapshot = view.graph_snapshot().unwrap();
    let llm = workflow_llm_node_id();
    let mut data = snapshot.graph().node(&llm).unwrap().data.clone();
    data["prompt"] = value.into();
    view.execute(GraphCommand::set_node_data(snapshot.version(), llm, data))
        .unwrap()
        .applied_revision()
        .expect("remote prompt update applies");
}

fn apply_remote_model(view: &jellyflow_runtime::view::ViewSession, value: &str) {
    let snapshot = view.graph_snapshot().unwrap();
    let llm = workflow_llm_node_id();
    let mut data = snapshot.graph().node(&llm).unwrap().data.clone();
    data["model"] = value.into();
    view.execute(GraphCommand::set_node_data(snapshot.version(), llm, data))
        .unwrap()
        .applied_revision()
        .expect("remote model update applies");
}

fn prompt_value(runtime: &GraphRuntime) -> String {
    runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&workflow_llm_node_id())
        .unwrap()
        .data["prompt"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn model_value(runtime: &GraphRuntime) -> String {
    runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&workflow_llm_node_id())
        .unwrap()
        .data["model"]
        .as_str()
        .unwrap()
        .to_owned()
}
