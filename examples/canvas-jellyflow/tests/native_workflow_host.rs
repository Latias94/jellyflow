use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphBuilder, GraphId, NodeGraphConnectionMode,
    NodeId, NodeKindKey, PortDirection, PortId, PortKind,
};
use jellyflow_open_gpui::{OpenGpuiNodeRegistry, OpenGpuiSceneHost};
use jellyflow_runtime::io::GraphFile;
use jellyflow_runtime::{
    GraphCommand, GraphRuntime, GraphRuntimeError, IncidentEdgePolicy, TopologyEdit, TopologyPort,
    ViewSessionOptions,
};
use open_gpui::{AppContext, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px};
use open_gpui_canvas_jellyflow::nodes::{
    ConditionCaseAction, NativeNodeResources, WORKFLOW_CONDITION_KIND, WORKFLOW_LLM_KIND,
    WorkflowCase, WorkflowNodeError, condition_cases, condition_outputs, native_node_catalog,
    native_node_registry, plan_condition_case_action, workflow_condition_add_selector,
    workflow_condition_case_selector, workflow_condition_else_selector, workflow_condition_node_id,
    workflow_condition_remove_selector, workflow_condition_scroll_selector,
    workflow_condition_up_selector, workflow_demo_graph, workflow_llm_node_id,
    workflow_model_conflict_selector, workflow_model_option_selector,
    workflow_model_reapply_selector, workflow_model_selector, workflow_node_status_selector,
    workflow_port_selector, workflow_prompt_accept_remote_selector, workflow_prompt_apply_selector,
    workflow_prompt_conflict_selector, workflow_prompt_reapply_selector, workflow_prompt_selector,
};
use open_gpui_ui_components::gpui_adapter::init_text_input;

const ADDED_CASE_ID: &str = "branch-fallback";
const ADDED_CASE_PORT: u128 = 0xd1fa_1103;
const CONCURRENT_CASE_ID: &str = "branch-concurrent";
const CONCURRENT_CASE_PORT: u128 = 0xd1fa_1104;
const OVERVIEW_NODE_SELECTOR: &str = "jellyflow-overview-node";

fn native_runtime(graph: Graph) -> Result<GraphRuntime, GraphRuntimeError> {
    GraphRuntime::builder(native_node_catalog().expect("native catalog")).build(graph)
}

fn native_registry(cx: &mut open_gpui::App) -> OpenGpuiNodeRegistry {
    native_node_registry(NativeNodeResources::ready_default(cx))
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

    let candidates = view
        .create_from_wire_candidates(success, NodeGraphConnectionMode::Strict)
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
            CanvasPoint { x: 40.0, y: 40.0 },
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
            CanvasPoint { x: 460.0, y: 40.0 },
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
fn condition_header_input_port_stays_clear_of_the_first_branch_row(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime =
        native_runtime(workflow_demo_graph().expect("workflow graph")).expect("workflow runtime");
    let condition = workflow_condition_node_id();
    let snapshot = runtime.snapshot().expect("workflow snapshot");
    let input = snapshot
        .graph()
        .node(&condition)
        .expect("condition node")
        .ports
        .iter()
        .copied()
        .find(|port_id| {
            snapshot
                .graph()
                .port(port_id)
                .is_some_and(|port| port.key.0 == "input")
        })
        .expect("condition input port");
    let first_branch = condition_cases(&snapshot, condition)
        .expect("condition branches")
        .into_iter()
        .next()
        .expect("first condition branch");
    let else_branch = condition_outputs(&snapshot, condition)
        .expect("condition outputs")
        .else_branch()
        .clone();
    drop(snapshot);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let weak_view = view.downgrade();
    let (_host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
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
    let first_branch_right = first_branch_bounds.origin.x + first_branch_bounds.size.width;
    assert!(
        (input_center_x - node_bounds.origin.x).abs() <= 1.0,
        "the input marker center {input_center_x} must stay on the node's left boundary {}",
        node_bounds.origin.x,
    );
    let node_right = node_bounds.origin.x + node_bounds.size.width;
    assert!(
        (first_branch_right - node_right).abs() <= 1.0,
        "the branch marker edge {first_branch_right} must stay on the node's right boundary \
         {node_right}"
    );
}

#[test]
fn workflow_demo_fixed_ports_match_the_native_catalog() {
    let catalog = native_node_catalog().expect("native catalog");
    let graph = workflow_demo_graph().expect("workflow graph");

    for node_id in [workflow_llm_node_id(), workflow_condition_node_id()] {
        let node = graph.node(&node_id).expect("workflow demo node");
        let spec = catalog.get(&node.kind).expect("workflow kind in catalog");
        assert_eq!(node.size, spec.default_size());
        assert!(node.ports.len() >= spec.ports().len());
        for port_spec in spec.ports() {
            let port_id = node
                .ports
                .iter()
                .copied()
                .find(|port_id| {
                    graph.port(port_id).is_some_and(|port| {
                        &port.key == port_spec.key()
                            && port.dir == port_spec.direction()
                            && port.kind == port_spec.kind()
                    })
                })
                .expect("workflow fixed port by catalog identity");
            let port = graph.port(&port_id).expect("workflow fixed port");
            assert_eq!(port.node, node_id);
            assert_eq!(&port.key, port_spec.key());
            assert_eq!(port.dir, port_spec.direction());
            assert_eq!(port.kind, port_spec.kind());
            assert_eq!(port.capacity, port_spec.capacity());
            assert_eq!(port.ty.as_ref(), port_spec.ty());
        }
        if node_id == workflow_condition_node_id() {
            let last = node.ports.last().and_then(|port| graph.port(port));
            assert!(
                last.is_some_and(|port| port.key.0 == "else"),
                "the immutable Else port stays after every dynamic case",
            );
        }
    }
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
    let initial_branches = condition_cases(&initial, condition).unwrap();
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
        assert_eq!(host.stats().protected_instances(), 1);
        assert!(host.stats().protected_leases() >= 2);
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

    let add = cx
        .debug_bounds(&workflow_condition_add_selector(condition))
        .expect("native add-branch button");
    cx.simulate_click(add.center(), Default::default());
    assert_eq!(
        condition_cases(&host_runtime.snapshot().unwrap(), condition)
            .unwrap()
            .len(),
        3,
        "the click drives the queued command through a real host frame"
    );
    draw(cx);
    let updated = host_runtime.snapshot().unwrap();
    let branches = condition_cases(&updated, condition).unwrap();
    assert_eq!(branches.len(), 3);
    assert_eq!(updated.graph().node(&condition).unwrap().ports.len(), 5);
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
    for branch in &branches {
        assert!(updated.graph().port(&branch.port_id()).is_some());
        let layout = geometry
            .port(branch.port_id())
            .expect("fresh branch port geometry");
        assert_eq!(layout.node(), condition);
        assert!(layout.visual_bounds().size.is_positive_finite());
    }
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().active_nodes(), 2);
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
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    for _ in 0..5 {
        let add = cx
            .debug_bounds(&workflow_condition_add_selector(condition))
            .expect("native add-branch button");
        cx.simulate_click(add.center(), Default::default());
        draw(cx);
    }

    let branches = condition_cases(&runtime.snapshot().unwrap(), condition).unwrap();
    let first_branch = branches.first().expect("first condition branch");
    let first_selector = workflow_condition_case_selector(condition, first_branch.port_id());
    let first_before = cx
        .debug_bounds(&first_selector)
        .expect("first branch before scrolling");
    let scroll = cx
        .debug_bounds(&workflow_condition_scroll_selector(condition))
        .expect("native condition scroll region");
    let viewport_before = weak_view.viewport_transform().unwrap();

    cx.simulate_event(ScrollWheelEvent {
        position: scroll.center(),
        delta: ScrollDelta::Lines(point(0.0, -3.0)),
        modifiers: Default::default(),
        touch_phase: TouchPhase::Moved,
    });
    draw(cx);

    let first_after = cx
        .debug_bounds(&first_selector)
        .expect("first branch after scrolling");
    assert!(first_after.origin.y < first_before.origin.y);
    assert_eq!(weak_view.viewport_transform().unwrap(), viewport_before);
}

#[open_gpui::test]
fn native_condition_up_and_remove_buttons_preserve_atomic_topology_history(
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
    });
    draw(cx);

    let condition = workflow_condition_node_id();
    let initial = runtime.snapshot().unwrap();
    let initial_graph = frozen_graph_value(initial.graph());
    let initial_branches = condition_cases(&initial, condition).unwrap();
    let first = initial_branches[0].clone();
    let connected = initial_branches[1].clone();
    let initial_edge_count = initial.graph().edges().len();
    assert!(
        cx.debug_bounds(&workflow_condition_up_selector(condition, first.port_id()))
            .is_none(),
        "the first branch must not expose an upward mutation"
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);

    let up = cx
        .debug_bounds(&workflow_condition_up_selector(
            condition,
            connected.port_id(),
        ))
        .expect("second branch Up button");
    cx.simulate_click(up.center(), Default::default());
    draw(cx);

    let after_up = runtime.snapshot().unwrap();
    assert_eq!(after_up.revision(), initial.revision() + 1);
    assert_eq!(
        condition_cases(&after_up, condition).unwrap(),
        vec![connected.clone(), first.clone()]
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    let after_up_graph = frozen_graph_value(after_up.graph());
    let geometry = weak_view.geometry_snapshot().unwrap();
    for branch in [&first, &connected] {
        let layout = geometry
            .port(branch.port_id())
            .expect("reordered branch keeps port geometry");
        assert_eq!(layout.node(), condition);
        assert!(layout.visual_bounds().size.is_positive_finite());
    }

    runtime
        .execute(GraphCommand::undo(after_up.revision()))
        .unwrap()
        .applied()
        .expect("Up undo applies");
    draw(cx);
    let undone_up = runtime.snapshot().unwrap();
    assert_eq!(frozen_graph_value(undone_up.graph()), initial_graph);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 1);

    runtime
        .execute(GraphCommand::redo(undone_up.revision()))
        .unwrap()
        .applied()
        .expect("Up redo applies");
    draw(cx);
    let redone_up = runtime.snapshot().unwrap();
    assert_eq!(frozen_graph_value(redone_up.graph()), after_up_graph);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);

    let remove = cx
        .debug_bounds(&workflow_condition_remove_selector(
            condition,
            connected.port_id(),
        ))
        .expect("connected branch Remove button");
    cx.simulate_click(remove.center(), Default::default());
    draw(cx);

    let after_remove = runtime.snapshot().unwrap();
    let after_remove_graph = frozen_graph_value(after_remove.graph());
    assert!(
        after_remove.revision() >= redone_up.revision() + 2,
        "remove plus derived content-height convergence must both publish revisions"
    );
    assert!(
        after_remove
            .graph()
            .node(&condition)
            .and_then(|node| node.size)
            .expect("condition size after remove")
            .height
            < redone_up
                .graph()
                .node(&condition)
                .and_then(|node| node.size)
                .expect("condition size before remove")
                .height,
        "removing a visible branch must shrink the content-authoritative condition node"
    );
    assert_eq!(
        condition_cases(&after_remove, condition).unwrap(),
        vec![first.clone()]
    );
    assert!(after_remove.graph().port(&connected.port_id()).is_none());
    assert_eq!(after_remove.graph().edges().len(), initial_edge_count);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    let geometry = weak_view.geometry_snapshot().unwrap();
    assert!(geometry.port(connected.port_id()).is_none());
    assert_eq!(
        geometry
            .port(first.port_id())
            .expect("surviving branch geometry")
            .node(),
        condition
    );

    runtime
        .execute(GraphCommand::undo(after_remove.revision()))
        .unwrap()
        .applied()
        .expect("Remove undo applies");
    draw(cx);
    let undone_remove = runtime.snapshot().unwrap();
    assert_eq!(frozen_graph_value(undone_remove.graph()), after_up_graph);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 1);
    let restored = weak_view.geometry_snapshot().unwrap();
    assert_eq!(
        restored
            .port(connected.port_id())
            .expect("undo restores the same branch port geometry")
            .node(),
        condition
    );

    runtime
        .execute(GraphCommand::redo(undone_remove.revision()))
        .unwrap()
        .applied()
        .expect("Remove redo applies");
    draw(cx);
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        after_remove_graph
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    assert!(
        weak_view
            .geometry_snapshot()
            .unwrap()
            .port(connected.port_id())
            .is_none()
    );
}

#[open_gpui::test]
fn prompt_conflict_can_explicitly_reapply_local_text_after_another_view_wins(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
    });
    draw(cx);
    let mounted_revision = runtime.snapshot().unwrap().revision();

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
    let reapply = cx
        .debug_bounds(&workflow_prompt_reapply_selector(llm))
        .expect("explicit local reapply action");
    cx.simulate_click(reapply.center(), Default::default());

    assert_eq!(prompt_value(&runtime), "local prompt");
    assert_eq!(runtime.snapshot().unwrap().revision(), mounted_revision + 2);
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_prompt_conflict_selector(llm))
            .is_none()
    );
    cx.read_entity(&host, |host, _| assert_eq!(host.stats().mounts(), 2));
}

#[open_gpui::test]
fn prompt_conflict_can_discard_local_text_without_writing_the_graph(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
    });
    draw(cx);
    let mounted_revision = runtime.snapshot().unwrap().revision();

    let llm = workflow_llm_node_id();
    let prompt = cx
        .debug_bounds(&workflow_prompt_selector(llm))
        .expect("native prompt input");
    cx.simulate_click(prompt.center(), Default::default());
    cx.simulate_input("discard me");
    apply_remote_prompt(&remote_view, "authoritative prompt");
    draw(cx);
    let accept = cx
        .debug_bounds(&workflow_prompt_accept_remote_selector(llm))
        .expect("explicit remote acceptance action");
    cx.simulate_click(accept.center(), Default::default());
    draw(cx);

    assert_eq!(prompt_value(&runtime), "authoritative prompt");
    assert_eq!(runtime.snapshot().unwrap().revision(), mounted_revision + 1);
    assert!(
        cx.debug_bounds(&workflow_prompt_conflict_selector(llm))
            .is_none()
    );
    cx.read_entity(&host, |host, _| assert_eq!(host.stats().mounts(), 2));
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
    });
    draw(cx);
    let mounted_revision = runtime.snapshot().unwrap().revision();

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
    assert_eq!(runtime.snapshot().unwrap().revision(), mounted_revision + 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert!(
        cx.debug_bounds(&workflow_model_conflict_selector(llm))
            .is_some()
    );
    let reapply = cx
        .debug_bounds(&workflow_model_reapply_selector(llm))
        .expect("explicit model reapply action");
    cx.simulate_click(reapply.center(), Default::default());

    assert_eq!(model_value(&runtime), "claude-3.7-sonnet");
    assert_eq!(runtime.snapshot().unwrap().revision(), mounted_revision + 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_conflict_selector(llm))
            .is_none()
    );
}

#[open_gpui::test]
fn stale_condition_add_waits_for_refresh_and_explicit_retry_without_render_resubmission(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let host_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let remote_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(host_view, native_registry(cx), viewport())
    });
    draw(cx);
    let mounted_revision = runtime.snapshot().unwrap().revision();

    let condition = workflow_condition_node_id();
    let remote_snapshot = remote_view.graph_snapshot().unwrap();
    let concurrent = WorkflowCase::new(
        CONCURRENT_CASE_ID,
        "Needs review",
        PortId::from_u128(CONCURRENT_CASE_PORT),
    )
    .unwrap();
    let remote_add = plan_condition_case_action(
        &remote_snapshot,
        condition,
        ConditionCaseAction::Add(concurrent.clone()),
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    let _interleave_remote_command = cx.update(|_, cx| {
        let mut remote_add = Some(remote_add);
        cx.observe(&host, move |_, _| {
            let Some(remote_add) = remote_add.take() else {
                return;
            };
            remote_view
                .execute(remote_add)
                .unwrap()
                .applied()
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
    let retry = cx
        .debug_bounds(&condition_retry_selector(condition))
        .expect("stale condition intent exposes explicit Retry after refresh");
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let refreshed = condition_cases(&runtime.snapshot().unwrap(), condition).unwrap();
    assert_eq!(refreshed.len(), 3);
    assert_eq!(refreshed.last(), Some(&concurrent));

    draw(cx);
    draw(cx);
    assert_eq!(runtime.snapshot().unwrap().revision(), conflict_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert!(runtime.snapshot().unwrap().revision() > conflict_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert!(
        cx.debug_bounds(&condition_retry_selector(condition))
            .is_none()
    );
    let committed = condition_cases(&runtime.snapshot().unwrap(), condition).unwrap();
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
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    cx.simulate_keystrokes("escape");
    draw(cx);

    assert!(cx.debug_bounds(&option_selector).is_none());
    assert!(
        cx.debug_selector_is_focused(&model_selector),
        "Escape returns focus to the controlled select trigger",
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
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
fn model_overlay_lease_protects_culling_and_releases_on_every_close_path(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let runtime = Rc::new(native_runtime(workflow_demo_graph().unwrap()).unwrap());
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let (host, cx) = cx.add_window_view(move |_, cx| {
        OpenGpuiSceneHost::new(view, native_registry(cx), viewport())
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
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 1);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
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
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 2);
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
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });

    let model = cx
        .debug_bounds(&model_selector)
        .expect("model select before retirement");
    cx.simulate_click(model.center(), Default::default());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });
    let revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::topology(
            revision,
            TopologyEdit::new().remove_node(llm, IncidentEdgePolicy::Cascade),
        ))
        .unwrap()
        .applied()
        .expect("LLM retirement applies");
    draw(cx);

    assert!(cx.debug_bounds(&model_selector).is_none());
    assert!(cx.debug_bounds(&option_selector).is_none());
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().hard_retirements(), 1);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
    });
}

#[open_gpui::test]
fn model_overlay_lease_preserves_native_workflow_across_semantic_zoom(
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
    });
    draw(cx);

    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    let model_selector = workflow_model_selector(llm);
    let option_selector = workflow_model_option_selector(llm, "claude-3.7-sonnet");
    let prompt_selector = workflow_prompt_selector(llm);
    let condition_branch =
        condition_cases(&runtime.snapshot().unwrap(), condition).unwrap()[0].port_id();
    let baseline = runtime.snapshot().unwrap();
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
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    weak_view.set_viewport(CanvasPoint::default(), 0.5).unwrap();
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
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
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
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().strong_instances(), 2);
    });

    weak_view.set_viewport(CanvasPoint::default(), 1.0).unwrap();
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
        assert_eq!(host.stats().mounts(), 2);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().strong_instances(), 2);
        assert!(host.last_diagnostic().is_none());
    });
}

#[test]
fn condition_else_is_stable_last_and_outside_the_case_action_domain() {
    let runtime = native_runtime(workflow_demo_graph().unwrap()).unwrap();
    let condition = workflow_condition_node_id();
    let initial = runtime.snapshot().unwrap();
    let outputs = condition_outputs(&initial, condition).unwrap();
    let else_port = outputs.else_branch().port_id();
    assert_eq!(outputs.else_branch().label(), "Else");
    assert_eq!(
        initial.graph().node(&condition).unwrap().ports.last(),
        Some(&else_port),
        "Else must be the final output regardless of case count"
    );
    assert_eq!(
        initial.graph().port(&else_port).unwrap().key.0,
        "else",
        "Else is a fixed semantic port, not a case encoded in node data"
    );
    let data = &initial.graph().node(&condition).unwrap().data;
    assert!(data.get("branches").is_none());
    assert_eq!(
        data.get("cases")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len),
        Some(outputs.cases().len())
    );
    assert_eq!(data["else_branch"]["label"], "Else");

    let remove_else = plan_condition_case_action(
        &initial,
        condition,
        ConditionCaseAction::Remove {
            case_id: "else".to_owned(),
        },
        IncidentEdgePolicy::Cascade,
    );
    assert!(matches!(
        remove_else,
        Err(WorkflowNodeError::CaseNotFound { case_id }) if case_id == "else"
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), initial.revision());
}

#[test]
fn condition_topology_transaction_rolls_back_data_when_port_replacement_is_rejected() {
    let runtime = native_runtime(workflow_demo_graph().unwrap()).unwrap();
    let condition = workflow_condition_node_id();
    let before = runtime.snapshot().unwrap();
    let before_graph = frozen_graph_value(before.graph());
    let mut changed_data = before.graph().node(&condition).unwrap().data.clone();
    changed_data["title"] = serde_json::json!("must roll back");
    let invalid = GraphCommand::topology(
        before.revision(),
        TopologyEdit::new()
            .set_node_data(condition, changed_data)
            .replace_node_ports(
                condition,
                std::iter::empty::<TopologyPort>(),
                IncidentEdgePolicy::Reject,
            ),
    );

    assert!(runtime.execute(invalid).unwrap().rejection().is_some());
    let after = runtime.snapshot().unwrap();
    assert_eq!(after.revision(), before.revision());
    assert_eq!(frozen_graph_value(after.graph()), before_graph);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn condition_case_actions_are_atomic_stable_and_exactly_undoable() {
    let runtime = native_runtime(workflow_demo_graph().unwrap()).unwrap();
    let condition = workflow_condition_node_id();
    let initial = runtime.snapshot().unwrap();
    let initial_branches = condition_cases(&initial, condition).unwrap();
    let stable_ports: Vec<_> = initial_branches.iter().map(WorkflowCase::port_id).collect();
    let connected_branch = initial_branches[1].clone();
    let added = WorkflowCase::new(
        ADDED_CASE_ID,
        "Fallback",
        PortId::from_u128(ADDED_CASE_PORT),
    )
    .unwrap();

    let add = plan_condition_case_action(
        &initial,
        condition,
        ConditionCaseAction::Add(added.clone()),
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    runtime
        .execute(add)
        .unwrap()
        .applied()
        .expect("add applies");
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let after_add = runtime.snapshot().unwrap();
    let branches = condition_cases(&after_add, condition).unwrap();
    let else_port = condition_outputs(&after_add, condition)
        .unwrap()
        .else_branch()
        .port_id();
    assert_eq!(
        after_add.graph().node(&condition).unwrap().ports.last(),
        Some(&else_port)
    );
    assert_eq!(
        branches
            .iter()
            .take(2)
            .map(WorkflowCase::port_id)
            .collect::<Vec<_>>(),
        stable_ports
    );
    assert_eq!(branches[2], added);

    let reorder = plan_condition_case_action(
        &after_add,
        condition,
        ConditionCaseAction::Reorder {
            case_id: ADDED_CASE_ID.to_owned(),
            index: 0,
        },
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    runtime
        .execute(reorder)
        .unwrap()
        .applied()
        .expect("reorder applies");
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    let before_remove = runtime.snapshot().unwrap();
    let before_remove_graph = frozen_graph_value(before_remove.graph());
    assert_eq!(
        condition_cases(&before_remove, condition).unwrap()[0],
        added
    );

    let remove = plan_condition_case_action(
        &before_remove,
        condition,
        ConditionCaseAction::Remove {
            case_id: connected_branch.id().to_owned(),
        },
        IncidentEdgePolicy::Cascade,
    )
    .unwrap();
    runtime
        .execute(remove)
        .unwrap()
        .applied()
        .expect("connected branch removal cascades");
    let after_remove = runtime.snapshot().unwrap();
    let after_remove_graph = frozen_graph_value(after_remove.graph());
    assert_eq!(after_remove.revision(), 3);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 3);
    assert!(
        after_remove
            .graph()
            .port(&connected_branch.port_id())
            .is_none()
    );
    assert_eq!(
        condition_outputs(&after_remove, condition)
            .unwrap()
            .else_branch()
            .port_id(),
        else_port
    );
    assert_eq!(
        after_remove.graph().node(&condition).unwrap().ports.last(),
        Some(&else_port)
    );
    assert_eq!(
        after_remove.graph().edges().len(),
        before_remove.graph().edges().len(),
        "removing a case must not disturb the immutable Else edge"
    );

    runtime
        .execute(GraphCommand::undo(after_remove.revision()))
        .unwrap()
        .applied()
        .expect("remove undo applies");
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        before_remove_graph,
        "one undo must restore payload, order, ports, and incident edges exactly"
    );

    let undo_revision = runtime.snapshot().unwrap().revision();
    runtime
        .execute(GraphCommand::redo(undo_revision))
        .unwrap()
        .applied()
        .expect("remove redo applies");
    assert_eq!(
        frozen_graph_value(runtime.snapshot().unwrap().graph()),
        after_remove_graph,
        "redo must reproduce the complete atomic branch removal"
    );
}

#[test]
fn stale_view_replans_case_intent_without_overwriting_remote_addition() {
    let runtime = native_runtime(workflow_demo_graph().unwrap()).unwrap();
    let view_a = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let view_b = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let condition = workflow_condition_node_id();
    let base_a = view_a.graph_snapshot().unwrap();
    let base_b = view_b.graph_snapshot().unwrap();
    let initial = condition_cases(&base_a, condition).unwrap();
    let reordered_id = initial[1].id().to_owned();
    let reorder_intent = ConditionCaseAction::Reorder {
        case_id: reordered_id.clone(),
        index: 0,
    };
    let stale_reorder = plan_condition_case_action(
        &base_a,
        condition,
        reorder_intent.clone(),
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    let concurrent = WorkflowCase::new(
        CONCURRENT_CASE_ID,
        "Needs review",
        PortId::from_u128(CONCURRENT_CASE_PORT),
    )
    .unwrap();
    let remote_add = plan_condition_case_action(
        &base_b,
        condition,
        ConditionCaseAction::Add(concurrent.clone()),
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    view_b
        .execute(remote_add)
        .unwrap()
        .applied()
        .expect("remote add applies");

    let stale = view_a.execute(stale_reorder).unwrap();
    let conflict = stale.conflict().expect("old view conflicts structurally");
    assert_eq!(conflict.expected_revision(), 0);
    assert_eq!(conflict.current().revision(), 1);
    assert!(conflict.footprint().nodes.contains(&condition));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    let recovered = plan_condition_case_action(
        conflict.current(),
        condition,
        reorder_intent,
        IncidentEdgePolicy::Reject,
    )
    .unwrap();
    view_a
        .execute(recovered)
        .unwrap()
        .applied()
        .expect("replanned intent applies");
    let final_snapshot = view_a.graph_snapshot().unwrap();
    let final_branches = condition_cases(&final_snapshot, condition).unwrap();
    assert_eq!(final_branches[0].id(), reordered_id);
    assert_eq!(final_branches.last(), Some(&concurrent));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
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

fn condition_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-retry-{}", node.0)
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
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
}

fn apply_remote_prompt(view: &jellyflow_runtime::ViewSession, value: &str) {
    let snapshot = view.graph_snapshot().unwrap();
    let llm = workflow_llm_node_id();
    let mut data = snapshot.graph().node(&llm).unwrap().data.clone();
    data["prompt"] = value.into();
    view.execute(GraphCommand::set_node_data(snapshot.revision(), llm, data))
        .unwrap()
        .applied()
        .expect("remote prompt update applies");
}

fn apply_remote_model(view: &jellyflow_runtime::ViewSession, value: &str) {
    let snapshot = view.graph_snapshot().unwrap();
    let llm = workflow_llm_node_id();
    let mut data = snapshot.graph().node(&llm).unwrap().data.clone();
    data["model"] = value.into();
    view.execute(GraphCommand::set_node_data(snapshot.revision(), llm, data))
        .unwrap()
        .applied()
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
