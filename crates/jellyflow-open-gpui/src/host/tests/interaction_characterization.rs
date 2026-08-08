use super::*;

#[open_gpui::test]
fn unavailable_release_reports_and_cancels_semantic_and_native_connection_state(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    let owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());
    let sample = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
        );
        assert!(!host.interaction_owner.test_has_pending_submission());
        assert_eq!(host.active_port_drag(), Some(owner.clone()));
        host.viewport_pointer_sample(input.center())
            .expect("capture release sample before runtime retirement")
    });

    drop(runtime);
    let (diagnostic, phase, terminal, has_pending_submission, pending_work_empty, active_port) = cx
        .update(|window, cx| {
            host.update(cx, |host, cx| {
                host.release_connection_sample_for_test(sample, window, cx);
                let presentation = host.interaction_presentation();
                (
                    host.last_diagnostic()
                        .map(|diagnostic| diagnostic.message().to_owned()),
                    presentation.phase(),
                    presentation.terminal(),
                    host.interaction_owner.test_has_pending_submission(),
                    host.interaction_owner.test_pending_work_empty(),
                    host.active_port_drag(),
                )
            })
        });
    let diagnostic = diagnostic.expect("unavailable release diagnostic");
    assert!(
        diagnostic.contains("interaction view cannot provide an authoritative graph snapshot"),
        "unexpected release diagnostic: {diagnostic}",
    );
    assert_eq!(phase, CanvasInteractionPhase::Idle);
    assert_eq!(terminal, Some(InteractionTerminalReason::Cancelled));
    assert!(!has_pending_submission);
    assert!(pending_work_empty);
    assert!(active_port.is_none());
}

#[open_gpui::test]
fn invalid_connection_preview_uses_the_bounded_non_color_cue(cx: &mut open_gpui::TestAppContext) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, occupied_input_graph());
    draw(cx);

    let source = cx
        .debug_bounds("jellyflow-test-replacement-output-port")
        .expect("replacement output bounds");
    let invalid_target = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("same-direction output bounds");
    cx.simulate_mouse_down(source.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        invalid_target.center(),
        Some(MouseButton::Left),
        Default::default(),
    );
    draw(cx);

    cx.update_entity(&host, |host, _| {
        assert!(matches!(
            host.interaction_presentation()
                .connection()
                .expect("invalid connection presentation")
                .probe(),
            jellyflow_runtime::interaction::ConnectionProbe::Invalid { .. }
        ));
        let plan = host
            .connection_preview_plan()
            .expect("invalid preview plan");
        assert_eq!(plan.probe, OpenGpuiConnectionPreviewProbeOutcome::Invalid);
        assert!(
            host.resolve_connection_preview_paint_recipe(&plan)
                .has_invalid_connection_cue(),
            "the production preview resolver must add the bounded non-color invalid cue",
        );
    });

    release_held_connection(&host, invalid_target.center(), cx);
}

#[open_gpui::test]
fn typed_connection_keeps_style_slot_across_preview_commit_and_hover(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(
        cx,
        typed_connection_graph(jellyflow_core::TypeDesc::Float, false),
    );
    let slot = cx.update_entity(&host, install_float_style);
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("typed output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("typed input bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());
    draw(cx);

    let preview_stroke_pattern = cx.update_entity(&host, |host, _| {
        let plan = host.connection_preview_plan().expect("typed preview plan");
        assert_eq!(plan.style_slot, Some(slot));
        let resolved = host.resolve_connection_preview_paint_recipe(&plan);
        assert_eq!(resolved.style().style_slot(), Some(slot));
        resolved.path_batch_key().stroke_pattern()
    });
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());
    cx.simulate_mouse_up(input.center(), MouseButton::Left, Default::default());
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let snapshot = runtime.snapshot().expect("typed committed graph");
    assert_eq!(snapshot.graph().edges().len(), 1);
    let edge = *snapshot
        .graph()
        .edges()
        .keys()
        .next()
        .expect("typed committed edge");
    cx.update_entity(&host, |host, _| {
        let wires = host.scene_wires().expect("typed committed scene wires");
        let wire = wires
            .published
            .iter()
            .find(|wire| wire.edge == edge)
            .expect("typed committed wire");
        assert_eq!(wire.style_slot, Some(slot));
        let geometry = host.view.geometry_snapshot().expect("typed geometry");
        let facts = geometry.route(edge).expect("typed route").facts();
        let expected = host.edge_painters.resolve_with_style_slot(
            facts.edge_kind,
            OpenGpuiEdgeVisualFacts::default(),
            &facts.presentation,
            wire.style_slot,
            &host.graph_theme,
        );
        assert_eq!(expected.style().style_slot(), Some(slot));
        assert_eq!(wire.base_paint_key, expected.path_batch_key());
        assert_eq!(wire.base_paint_key.stroke_pattern(), preview_stroke_pattern);
    });

    let pointer = cx.read_entity(&host, |host, _| {
        let transform = host.scene_transform.expect("typed scene transform");
        let geometry = host.view.geometry_snapshot().expect("typed hover geometry");
        let sample = geometry
            .route(edge)
            .expect("typed hover route")
            .facts()
            .path
            .sample(jellyflow_runtime::geometry::EdgePathAnchor::Center)
            .expect("typed route midpoint");
        let screen = transform
            .screen_point_for_canvas(sample.point)
            .expect("project typed route midpoint");
        let origin = host.scene_origin.expect("typed scene origin");
        point(origin.x + px(screen.x), origin.y + px(screen.y))
    });
    cx.simulate_mouse_move(pointer, None, Default::default());
    draw(cx);
    draw(cx);
    cx.update_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().hovered_edge(),
            host.view.edge_instance_key(edge).unwrap()
        );
        let wires = host.scene_wires().expect("typed hovered scene wires");
        let hover = wires
            .published
            .iter()
            .find(|wire| wire.edge == edge)
            .expect("hovered wire remains in its published position");
        assert_eq!(hover.style_slot, Some(slot));
        let geometry = host.view.geometry_snapshot().expect("typed hover geometry");
        let facts = geometry.route(edge).expect("typed hover route").facts();
        let expected = host.edge_painters.resolve_with_style_slot(
            facts.edge_kind,
            OpenGpuiEdgeVisualFacts::default().with_hovered(true),
            &facts.presentation,
            hover.style_slot,
            &host.graph_theme,
        );
        assert_eq!(expected.style().style_slot(), Some(slot));
        assert_eq!(hover.base_paint_key, expected.path_batch_key());
        assert_eq!(hover.halo_paint_key, expected.halo_path_batch_key());
    });
}

#[open_gpui::test]
fn unregistered_committed_source_type_uses_the_absent_type_fallback(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(
        cx,
        typed_connection_graph(jellyflow_core::TypeDesc::Int, true),
    );
    cx.update_entity(&host, |host, cx| {
        install_float_style(host, cx);
    });
    draw(cx);
    draw(cx);

    cx.update_entity(&host, |host, _| {
        let edge = EdgeId::from_u128(30);
        let wires = host.scene_wires().expect("unregistered typed scene wires");
        let wire = wires
            .published
            .iter()
            .find(|wire| wire.edge == edge)
            .expect("unregistered typed wire");
        assert_eq!(wire.style_slot, None);
        let graph = host.view.graph_snapshot().expect("unregistered graph");
        let source_type = graph
            .graph()
            .port(&PortId::from_u128(20))
            .and_then(|port| port.ty.as_ref())
            .expect("unregistered source type");
        assert_eq!(source_type, &jellyflow_core::TypeDesc::Int);
        assert_eq!(host.edge_painters.style_slot_for_type(source_type), None);
        let geometry = host
            .view
            .geometry_snapshot()
            .expect("unregistered geometry");
        let facts = geometry.route(edge).expect("unregistered route").facts();
        let typed = host.edge_painters.resolve_with_style_slot(
            facts.edge_kind,
            wire.visual_facts,
            &facts.presentation,
            wire.style_slot,
            &host.graph_theme,
        );
        let absent = host.edge_painters.resolve(
            facts.edge_kind,
            wire.visual_facts,
            &facts.presentation,
            &host.graph_theme,
        );
        assert_eq!(typed.style().style_slot(), None);
        assert_eq!(typed.path_batch_key(), absent.path_batch_key());
        assert_eq!(wire.base_paint_key, absent.path_batch_key());
        assert_eq!(wire.halo_paint_key, absent.halo_path_batch_key());
    });
}

#[open_gpui::test]
fn same_id_fixed_port_replacement_cancels_instead_of_switching_preview_type(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(
        cx,
        typed_connection_graph(jellyflow_core::TypeDesc::Float, false),
    );
    cx.update_entity(&host, |host, cx| {
        install_float_style(host, cx);
    });
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("replaceable typed output bounds");
    let pointer = point(px(700.0), px(500.0));
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(pointer, Some(MouseButton::Left), Default::default());
    draw(cx);
    cx.update_entity(&host, |host, _| {
        let plan = host
            .connection_preview_plan()
            .expect("initial typed preview plan");
        assert!(plan.style_slot.is_some());
    });

    let snapshot = runtime
        .snapshot()
        .expect("snapshot before port replacement");
    let source = NodeId::from_u128(10);
    let output_id = PortId::from_u128(20);
    let mut replacement = port(source, "replacement-output", PortDirection::Out);
    replacement.ty = Some(jellyflow_core::TypeDesc::Int);
    runtime
        .execute(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                source,
                [TopologyPort::new(output_id, replacement)],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("replace fixed port semantics")
        .applied()
        .expect("same-ID fixed port replacement applies");
    cx.run_until_parked();
    draw(cx);

    cx.update_entity(&host, |host, _| {
        assert!(host.connection_preview_plan().is_none());
        assert!(host.interaction_presentation().connection().is_none());
        assert!(host.active_port_drag().is_none());
    });
}

#[open_gpui::test]
fn late_events_from_replaced_port_lifetime_cannot_settle_successor_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let old_owner = cx.read_entity(&host, |host, _| {
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
            old_owner.clone(),
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

    let snapshot = runtime.snapshot().expect("snapshot before replacement");
    let source = NodeId::from_u128(10);
    let output_id = PortId::from_u128(20);
    runtime
        .execute(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                source,
                [TopologyPort::new(
                    output_id,
                    port(source, "replacement-output", PortDirection::Out),
                )],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("replace output lifetime")
        .applied()
        .expect("same-ID replacement applies");
    cx.run_until_parked();
    draw(cx);
    draw(cx);

    let new_owner = cx.read_entity(&host, |host, _| {
        current_port_drag_owner(host, NodeId::from_u128(10), PortId::from_u128(20))
    });
    assert_eq!(old_owner.port(), new_owner.port());
    assert_ne!(old_owner.port_instance(), new_owner.port_instance());
    dispatch(
        OpenGpuiPortDragEvent::for_test(
            new_owner.clone(),
            2,
            OpenGpuiPortDragPhase::Start,
            output.center(),
        ),
        cx,
    );
    let successor_generation = cx.read_entity(&host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
        presentation.generation().expect("successor generation")
    });

    for phase in [
        OpenGpuiPortDragPhase::Move,
        OpenGpuiPortDragPhase::Release,
        OpenGpuiPortDragPhase::Cancel(OpenGpuiPortDragCancelReason::OwnerRetired),
    ] {
        dispatch(
            OpenGpuiPortDragEvent::for_test(
                old_owner.clone(),
                1,
                phase,
                point(px(700.0), px(500.0)),
            ),
            cx,
        );
        cx.read_entity(&host, |host, _| {
            let presentation = host.interaction_presentation();
            assert_eq!(presentation.phase(), CanvasInteractionPhase::Armed);
            assert_eq!(presentation.generation(), Some(successor_generation));
            assert_eq!(
                host.interaction_owner
                    .connection_identity()
                    .expect("successor context")
                    .0,
                new_owner,
            );
        });
    }

    dispatch(
        OpenGpuiPortDragEvent::for_test(
            new_owner,
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
}

fn install_float_style(
    host: &mut OpenGpuiSceneHost,
    cx: &mut Context<OpenGpuiSceneHost>,
) -> crate::theme::OpenGpuiGraphStyleSlot {
    let slot = crate::theme::OpenGpuiGraphStyleSlot::new(3).expect("bounded test style slot");
    let theme = crate::theme::OpenGpuiGraphTheme::default().with_style_slot(
        slot,
        crate::theme::OpenGpuiGraphStyleRecipe::new(open_gpui::rgb(0xe6b85c).into())
            .with_cue(crate::theme::OpenGpuiGraphStyleCue::Dotted),
    );
    let mut painters = crate::chrome::OpenGpuiEdgePainterRegistry::builder();
    painters
        .register_type_style(jellyflow_core::TypeDesc::Float, slot)
        .expect("register Float test style");
    host.set_graph_theme(theme, cx);
    host.set_edge_painter_registry(painters.build(), cx);
    slot
}

fn typed_connection_graph(ty: jellyflow_core::TypeDesc, connected: bool) -> jellyflow_core::Graph {
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let output = PortId::from_u128(20);
    let input = PortId::from_u128(21);
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x71_500));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_node(target, node(CanvasPoint { x: 360.0, y: 80.0 }, input));
    let mut output_port = port(source, "output", PortDirection::Out);
    output_port.ty = Some(ty.clone());
    let mut input_port = port(target, "input", PortDirection::In);
    input_port.ty = Some(ty);
    graph.insert_port(output, output_port);
    graph.insert_port(input, input_port);
    if connected {
        graph.insert_edge(
            EdgeId::from_u128(30),
            Edge::new(EdgeKind::Data, output, input),
        );
    }
    graph.build().expect("valid typed connection graph")
}

#[open_gpui::test]
fn stale_query_callback_cannot_advance_a_successor_generation(cx: &mut open_gpui::TestAppContext) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before stale query");
    let history_before = runtime.history_state().expect("history before stale query");

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let origin = host.scene_origin.expect("measured scene origin");
            let sample = host
                .viewport_pointer_sample(point(origin.x + px(700.0), origin.y + px(500.0)))
                .expect("canvas press position");
            let target = || {
                jellyflow_runtime::interaction::CanvasPressTarget::Canvas(
                    jellyflow_runtime::interaction::CanvasPressFallback::Pan {
                        selection_mode: jellyflow_runtime::interaction::SelectionMode::Replace,
                    },
                )
            };

            let update = host
                .interaction_owner
                .press(target(), sample)
                .expect("G1 canvas press");
            assert!(update.query_work_pending());
            let generation_one = host
                .interaction_presentation()
                .generation()
                .expect("G1 semantic generation");
            let (_, stale_callback) = host
                .interaction_owner
                .prepare_query_frame_callback()
                .expect("G1 query callback");

            let cancelled = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(cancelled, window, cx));
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Idle,
            );

            let update = host
                .interaction_owner
                .press(target(), sample)
                .expect("G2 canvas press");
            assert!(update.query_work_pending());
            let generation_two = host
                .interaction_presentation()
                .generation()
                .expect("G2 semantic generation");
            assert_ne!(generation_two, generation_one);
            let (_, current_callback) = host
                .interaction_owner
                .prepare_query_frame_callback()
                .expect("G2 query callback replaces G1 callback authority");
            assert_ne!(current_callback, stale_callback);
            assert!(
                !host
                    .interaction_owner
                    .consume_query_frame_callback(stale_callback)
            );
            assert!(
                host.interaction_owner
                    .consume_query_frame_callback(current_callback)
            );

            host.apply_canvas_interaction_update(update, window, cx);
            assert_eq!(
                host.interaction_presentation().generation(),
                Some(generation_two),
            );
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Armed,
            );
        });
    });

    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert!(
            !host.interaction_owner.test_query_work_pending(),
            "stale work must not create a continuation",
        );
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Armed,
        );
    });

    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after stale query")));
    assert_eq!(
        runtime.history_state().expect("history after stale query"),
        history_before,
    );
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("close replacement query host");
        });
    });
    cx.run_until_parked();
}

#[open_gpui::test]
fn query_callback_after_close_cannot_advance_retired_runtime_work(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before close");
    let history_before = runtime.history_state().expect("history before close");

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let origin = host.scene_origin.expect("measured scene origin");
            let sample = host
                .viewport_pointer_sample(point(origin.x + px(700.0), origin.y + px(500.0)))
                .expect("canvas press position");
            let update = host
                .interaction_owner
                .press(
                    jellyflow_runtime::interaction::CanvasPressTarget::Canvas(
                        jellyflow_runtime::interaction::CanvasPressFallback::Pan {
                            selection_mode: jellyflow_runtime::interaction::SelectionMode::Replace,
                        },
                    ),
                    sample,
                )
                .expect("canvas press before close");
            assert!(update.query_work_pending());
            let (_, stale_callback) = host
                .interaction_owner
                .prepare_query_frame_callback()
                .expect("query callback before close");

            host.close_in_window(window, cx).expect("close query host");
            assert!(
                !host
                    .interaction_owner
                    .consume_query_frame_callback(stale_callback)
            );
            host.process_one_interaction_query(window, cx);

            assert!(host.interaction_owner.test_pending_work_empty());
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Closed,
            );
        });
    });
    cx.run_until_parked();

    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after late query")));
    assert_eq!(
        runtime.history_state().expect("history after late query"),
        history_before,
    );
}

#[open_gpui::test]
fn paged_connection_probe_restarts_after_viewport_change_without_cancelling_drag(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) =
        mount_connection_scene_for_graph(cx, paged_overlapping_target_graph());
    draw(cx);
    cx.update(|_, cx| {
        host.update(cx, |host, _| {
            let session = jellyflow_runtime::interaction::CanvasInteractionSession::attach(
                &host.view,
                jellyflow_runtime::interaction::CanvasInteractionConfig {
                    query_work_budget: 1,
                    ..jellyflow_runtime::interaction::CanvasInteractionConfig::default()
                },
            )
            .expect("valid bounded interaction configuration");
            host.interaction_owner.test_replace_session(session);
        });
    });

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("source output bounds");
    let target = cx.read_entity(&host, |host, _| {
        let anchor = host
            .view
            .geometry_snapshot()
            .expect("fresh geometry")
            .port(PortId::from_u128(99))
            .expect("overlapping target geometry")
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
    cx.simulate_mouse_move(target, Some(MouseButton::Left), Default::default());

    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert!(
            host.interaction_owner.test_query_work_pending(),
            "one-entry first page must leave a probe continuation",
        );
    });

    cx.update(|_, cx| {
        host.update(cx, |host, _| {
            host.view
                .set_viewport(ViewportTransform::new(CanvasPoint { x: 3.0, y: -2.0 }, 1.0).unwrap())
                .expect("change viewport between query pages");
        });
    });
    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
            "a stale continuation must not cancel the semantic drag"
        );
        assert!(
            host.interaction_owner.test_query_work_pending(),
            "stale continuation must restart from page one",
        );
    });

    for _ in 0..16 {
        if cx.read_entity(&host, |host, _| {
            !host.interaction_owner.test_query_work_pending()
        }) {
            break;
        }
        draw_once(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.test_query_work_pending());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        assert!(
            host.interaction_presentation()
                .connection()
                .expect("drag preview survives pagination")
                .probe()
                .is_valid()
        );
    });

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            host.close_in_window(window, cx)
                .expect("close paged interaction host");
        });
    });
    cx.run_until_parked();
}

#[open_gpui::test]
fn paged_edge_pick_restarts_after_paint_order_change_without_terminal_failure(
    cx: &mut open_gpui::TestAppContext,
) {
    let (graph, edges) = bounded_edge_graph(4, 0);
    let (_runtime, host, cx) = mount_gesture_scene_for_graph(cx, graph);
    draw(cx);
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let session = jellyflow_runtime::interaction::CanvasInteractionSession::attach(
                &host.view,
                jellyflow_runtime::interaction::CanvasInteractionConfig {
                    edge_pick_radius_in_view: 1_000.0,
                    query_work_budget: 1,
                    ..jellyflow_runtime::interaction::CanvasInteractionConfig::default()
                },
            )
            .expect("valid bounded interaction configuration");
            host.interaction_owner.test_replace_session(session);
            let origin = host.scene_origin.expect("measured scene origin");
            let sample = host
                .viewport_pointer_sample(point(origin.x + px(240.0), origin.y + px(80.0)))
                .expect("pick point inside scene");
            let update = host
                .interaction_owner
                .request_edge_pick(sample)
                .expect("request bounded edge pick");
            host.apply_canvas_interaction_update(update, window, cx);
        });
    });

    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert!(
            host.interaction_owner.test_query_work_pending(),
            "one-entry first page must leave an edge-pick continuation",
        );
    });
    cx.update(|_, cx| {
        host.update(cx, |host, _| {
            set_selection_by_id(&host.view, Vec::new(), vec![edges[0]])
                .expect("change edge paint order between query pages");
        });
    });
    draw_once(cx);
    cx.read_entity(&host, |host, _| {
        assert!(
            host.interaction_owner.test_query_work_pending(),
            "stale edge-pick continuation must restart from page one",
        );
    });

    for _ in 0..128 {
        if cx.read_entity(&host, |host, _| {
            !host.interaction_owner.test_query_work_pending()
        }) {
            break;
        }
        draw_once(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.test_query_work_pending());
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert!(host.interaction_presentation().terminal().is_none());
        assert!(host.interaction_presentation().hovered_edge().is_some());
    });
}

fn paged_overlapping_target_graph() -> jellyflow_core::Graph {
    let source = NodeId::from_u128(10);
    let output = PortId::from_u128(20);
    let target_position = CanvasPoint { x: 360.0, y: 80.0 };
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xA13));
    graph.insert_node(source, node(CanvasPoint { x: 40.0, y: 80.0 }, output));
    graph.insert_port(output, port(source, "output", PortDirection::Out));
    for index in 0..8_u128 {
        let target = NodeId::from_u128(100 + index);
        let input = if index == 0 {
            PortId::from_u128(21)
        } else {
            PortId::from_u128(98 + index)
        };
        graph.insert_node(target, node(target_position, input));
        graph.insert_port(input, port(target, "input", PortDirection::In));
    }
    graph.build().expect("valid paged target graph")
}

fn release_held_connection(
    host: &Entity<OpenGpuiSceneHost>,
    release_position: open_gpui::Point<open_gpui::Pixels>,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.simulate_mouse_up(release_position, MouseButton::Left, Default::default());
    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let update = host.interaction_owner.cancel();
            assert!(host.apply_canvas_interaction_result(update, window, cx));
        });
    });
    cx.run_until_parked();
}
