use super::*;

use jellyflow_runtime::{CanvasInteractionQuery, EdgePickQuery};

#[open_gpui::test]
fn connection_commit_keeps_exactly_one_preview_across_native_frames(
    cx: &mut open_gpui::TestAppContext,
) {
    let (_runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);

    let output = cx
        .debug_bounds("jellyflow-test-output-port")
        .expect("output bounds");
    let input = cx
        .debug_bounds("jellyflow-test-input-port")
        .expect("input bounds");
    cx.simulate_mouse_down(output.center(), MouseButton::Left, Default::default());
    cx.simulate_mouse_move(input.center(), Some(MouseButton::Left), Default::default());

    hold_released_connection_at_adapter_boundary(&host, input.center(), cx);

    for frame in 0..3 {
        draw_once(cx);
        assert_committing_preview_frame(&host, None, frame, cx);
    }
    release_held_connection(&host, input.center(), cx);
}

#[open_gpui::test]
fn reconnect_commit_suppresses_old_edge_without_preview_flicker(
    cx: &mut open_gpui::TestAppContext,
) {
    let edge = EdgeId::from_u128(30);
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, reconnect_graph());
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

    hold_released_connection_at_adapter_boundary(&host, new_input.center(), cx);

    for frame in 0..3 {
        draw_once(cx);
        assert_committing_preview_frame(&host, Some(edge), frame, cx);
        assert!(
            runtime
                .snapshot()
                .expect("unchanged graph while commit is pending")
                .graph()
                .edge(&edge)
                .is_some(),
            "frame {frame}: suppression must not mutate the authoritative edge",
        );
    }
    release_held_connection(&host, new_input.center(), cx);
}

#[open_gpui::test]
fn query_from_replaced_interaction_generation_is_inert_before_view_access(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before stale query");
    let history_before = runtime.history_state().expect("history before stale query");

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let origin = host.scene_origin.expect("measured scene origin");
            let position = host
                .interaction_position(point(origin.x + px(700.0), origin.y + px(500.0)))
                .expect("canvas press position");
            let target = || {
                jellyflow_runtime::CanvasPressTarget::Canvas(
                    jellyflow_runtime::CanvasPressFallback::Pan {
                        selection_mode: jellyflow_runtime::SelectionMode::Replace,
                    },
                )
            };

            let update = host
                .interaction_owner
                .press(target(), position)
                .expect("G1 canvas press");
            host.apply_canvas_interaction_update(update, window, cx);
            let generation_one = host
                .interaction_presentation()
                .generation()
                .expect("G1 semantic generation");
            let stale_query = host
                .interaction_owner
                .test_take_query_probe()
                .expect("G1 queued edge pick");

            let cancelled = host.interaction_owner.cancel();
            host.apply_canvas_interaction_update(cancelled, window, cx);
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Idle,
            );

            let update = host
                .interaction_owner
                .press(target(), position)
                .expect("G2 canvas press");
            host.apply_canvas_interaction_update(update, window, cx);
            let generation_two = host
                .interaction_presentation()
                .generation()
                .expect("G2 semantic generation");
            assert_ne!(generation_two, generation_one);
            let current_query_count = host.interaction_owner.test_pending_query_count();
            assert_eq!(current_query_count, 1);

            host.interaction_owner.test_restore_query_front(stale_query);
            host.process_one_interaction_query(window, cx);
            assert_eq!(
                host.interaction_owner.test_pending_query_count(),
                current_query_count,
                "processing stale G1 work must not consume or replace G2 work",
            );
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
            host.interaction_owner.test_pending_queries_empty(),
            "the current generation query must continue on the next notified frame",
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
fn queued_query_reintroduced_after_close_cannot_mutate_or_enqueue_follow_up_work(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before close");
    let history_before = runtime.history_state().expect("history before close");

    cx.update(|window, cx| {
        host.update(cx, |host, cx| {
            let origin = host.scene_origin.expect("measured scene origin");
            let position = host
                .interaction_position(point(origin.x + px(700.0), origin.y + px(500.0)))
                .expect("canvas press position");
            let update = host
                .interaction_owner
                .press(
                    jellyflow_runtime::CanvasPressTarget::Canvas(
                        jellyflow_runtime::CanvasPressFallback::Pan {
                            selection_mode: jellyflow_runtime::SelectionMode::Replace,
                        },
                    ),
                    position,
                )
                .expect("canvas press before close");
            host.apply_canvas_interaction_update(update, window, cx);
            let stale_query = host
                .interaction_owner
                .test_take_query_probe()
                .expect("queued edge pick before close");

            host.close_in_window(window, cx).expect("close query host");
            host.interaction_owner.test_restore_query_back(stale_query);
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
fn fatal_edge_pick_query_cancels_once_without_native_or_graph_residue(
    cx: &mut open_gpui::TestAppContext,
) {
    let (runtime, host, cx) = mount_connection_scene_for_graph(cx, unconnected_graph());
    draw(cx);
    let graph_before = runtime.snapshot().expect("graph before fatal edge pick");
    let history_before = runtime
        .history_state()
        .expect("history before fatal edge pick");
    let view = cx.read_entity(&host, |host, _| host.view_handle());
    let selection_before = view.selection().expect("selection before fatal edge pick");
    let previous_focus = cx.update(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        focus
    });
    let pointer = point(px(700.0), px(500.0));

    cx.update(|window, cx| {
        window.dispatch_event(
            open_gpui::PlatformInput::MouseDown(MouseDownEvent {
                position: pointer,
                modifiers: Default::default(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        assert!(window.captured_hitbox().is_some());
        host.update(cx, |host, cx| {
            assert!(host.active_scene_gesture());
            assert!(host.interaction_owner.test_native_requires_window_cleanup());
            assert_eq!(host.stats().protected_instances(), 0);
            assert_eq!(host.stats().protected_leases(), 0);

            let position = host.interaction_position(pointer).expect("canvas position");
            assert_eq!(
                host.interaction_presentation().phase(),
                CanvasInteractionPhase::Armed,
            );

            let Some(CanvasInteractionQuery::EdgePick { id, .. }) =
                host.interaction_owner.test_take_query()
            else {
                panic!("an armed canvas press must schedule one edge-pick query");
            };
            host.queue_interaction_queries([CanvasInteractionQuery::EdgePick {
                id,
                query: EdgePickQuery::new(position.view(), f32::NAN, 64),
            }]);
            host.process_one_interaction_query(window, cx);
            assert!(
                window.captured_hitbox().is_some(),
                "fatal query cancellation must not release native capture reentrantly",
            );

            let presentation = host.interaction_presentation();
            assert_eq!(presentation.phase(), CanvasInteractionPhase::Idle);
            assert_eq!(
                presentation.terminal(),
                Some(InteractionTerminalReason::Cancelled),
            );
            assert!(host.interaction_owner.test_pending_work_empty());
            assert!(!host.interaction_owner.has_connection());
            assert!(host.active_port_drag().is_none());
            assert!(!host.active_scene_gesture());
            assert_eq!(host.stats().protected_instances(), 0);
            assert_eq!(host.stats().protected_leases(), 0);

            let diagnostic = host
                .last_diagnostic()
                .cloned()
                .expect("fatal edge-pick diagnostic");
            assert!(
                diagnostic
                    .message()
                    .contains("edge-pick point or radius is invalid")
            );
            host.process_one_interaction_query(window, cx);
            assert_eq!(host.last_diagnostic(), Some(&diagnostic));
        });
    });
    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.run_until_parked();

    assert!(cx.update(|window, _| window.captured_hitbox()).is_none());
    cx.update(|window, cx| assert_eq!(window.focused(cx), Some(previous_focus)));
    cx.read_entity(&host, |host, _| {
        assert!(!host.interaction_owner.test_native_requires_window_cleanup());
        assert_eq!(
            host.scene_evidence.last_scene_phase.get(),
            Some(OpenGpuiSceneGesturePhase::Cancel(
                OpenGpuiSceneGestureCancelReason::InteractionUnavailable,
            )),
        );
    });
    assert!(graph_before.ptr_eq(&runtime.snapshot().expect("graph after fatal edge pick")));
    assert_eq!(
        runtime
            .history_state()
            .expect("history after fatal edge pick"),
        history_before,
    );
    assert_eq!(
        view.selection().expect("selection after fatal edge pick"),
        selection_before,
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
            let session = jellyflow_runtime::CanvasInteractionSession::new(
                host.view.id(),
                host.view.generation().expect("open host view"),
                jellyflow_runtime::CanvasInteractionConfig {
                    query_entry_budget: 1,
                    ..jellyflow_runtime::CanvasInteractionConfig::default()
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
    let stale_id = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag
        );
        let Some(jellyflow_runtime::CanvasInteractionQuery::ConnectionProbe { id, .. }) =
            host.interaction_owner.test_front_query()
        else {
            panic!("one-entry first page must leave a probe continuation");
        };
        *id
    });

    cx.update(|_, cx| {
        host.update(cx, |host, _| {
            host.view
                .set_viewport(CanvasPoint { x: 3.0, y: -2.0 }, 1.0)
                .expect("change viewport between query pages");
        });
    });
    draw_once(cx);
    let restarted_id = cx.read_entity(&host, |host, _| {
        assert_eq!(
            host.interaction_presentation().phase(),
            CanvasInteractionPhase::ConnectionDrag,
            "a stale continuation must not cancel the semantic drag"
        );
        let Some(jellyflow_runtime::CanvasInteractionQuery::ConnectionProbe { id, .. }) =
            host.interaction_owner.test_front_query()
        else {
            panic!("stale continuation must restart from page one");
        };
        *id
    });
    assert_ne!(restarted_id, stale_id);

    for _ in 0..16 {
        if cx.read_entity(&host, |host, _| {
            host.interaction_owner.test_pending_queries_empty()
        }) {
            break;
        }
        draw_once(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_owner.test_pending_queries_empty());
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
            let session = jellyflow_runtime::CanvasInteractionSession::new(
                host.view.id(),
                host.view.generation().expect("open host view"),
                jellyflow_runtime::CanvasInteractionConfig {
                    edge_pick_radius_in_view: 1_000.0,
                    query_entry_budget: 1,
                    ..jellyflow_runtime::CanvasInteractionConfig::default()
                },
            )
            .expect("valid bounded interaction configuration");
            host.interaction_owner.test_replace_session(session);
            let origin = host.scene_origin.expect("measured scene origin");
            let position = host
                .interaction_position(point(origin.x + px(240.0), origin.y + px(80.0)))
                .expect("pick point inside scene");
            let update = host
                .interaction_owner
                .request_edge_pick(position)
                .expect("request bounded edge pick");
            host.apply_canvas_interaction_update(update, window, cx);
        });
    });

    draw_once(cx);
    let stale_id = cx.read_entity(&host, |host, _| {
        let Some(jellyflow_runtime::CanvasInteractionQuery::EdgePick { id, .. }) =
            host.interaction_owner.test_front_query()
        else {
            panic!("one-entry first page must leave an edge-pick continuation");
        };
        *id
    });
    cx.update(|_, cx| {
        host.update(cx, |host, _| {
            host.view
                .set_selection(Vec::new(), vec![edges[0]], Vec::new())
                .expect("change edge paint order between query pages");
        });
    });
    draw_once(cx);
    let restarted_id = cx.read_entity(&host, |host, _| {
        let Some(jellyflow_runtime::CanvasInteractionQuery::EdgePick { id, .. }) =
            host.interaction_owner.test_front_query()
        else {
            panic!("stale edge-pick continuation must restart from page one");
        };
        *id
    });
    assert_ne!(restarted_id, stale_id);

    for _ in 0..32 {
        if cx.read_entity(&host, |host, _| {
            host.interaction_owner.test_pending_queries_empty()
        }) {
            break;
        }
        draw_once(cx);
    }
    cx.read_entity(&host, |host, _| {
        assert!(host.interaction_owner.test_pending_queries_empty());
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

fn hold_released_connection_at_adapter_boundary(
    host: &Entity<OpenGpuiSceneHost>,
    release_position: open_gpui::Point<open_gpui::Pixels>,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.update_entity(host, |host, _| {
        if host.interaction_presentation().phase()
            == jellyflow_runtime::CanvasInteractionPhase::Committing
        {
            host.discard_pending_interaction_submission();
            host.interaction_owner.test_clear_queries();
            return;
        }

        let position = host
            .interaction_position(release_position)
            .expect("release position inside the measured scene");
        let release = host
            .interaction_owner
            .release(position)
            .expect("release active connection");
        host.queue_interaction_queries(release.into_dispatch().queries);

        let mut pending = host.interaction_owner.test_take_query().unwrap_or_else(|| {
            panic!(
                "release schedules a fresh connection probe from {:?}",
                host.interaction_presentation().phase()
            )
        });
        loop {
            let jellyflow_runtime::CanvasInteractionQuery::ConnectionProbe { id, query, .. } =
                pending
            else {
                panic!("connection release cannot schedule an edge pick");
            };
            let outcome = host
                .view
                .connection_probe(query)
                .expect("resolve connection probe from published geometry");
            let update = host
                .interaction_owner
                .resolve_connection_probe(id, outcome)
                .expect("apply connection probe result");
            let dispatch = update.into_dispatch();
            let queries = dispatch.queries;
            let effects = dispatch.pre_query_effects;

            if host.interaction_presentation().phase()
                == jellyflow_runtime::CanvasInteractionPhase::Committing
            {
                assert!(queries.is_empty());
                assert!(matches!(
                    effects.as_slice(),
                    [jellyflow_runtime::CanvasInteractionEffect::SubmitGraph { .. }]
                ));
                break;
            }

            let [query] = <[_; 1]>::try_from(queries)
                .expect("an incomplete probe schedules exactly one continuation");
            pending = query;
        }
    });
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
            host.apply_canvas_interaction_update(update, window, cx);
        });
    });
    cx.run_until_parked();
}

fn assert_committing_preview_frame(
    host: &Entity<OpenGpuiSceneHost>,
    suppressed_edge: Option<EdgeId>,
    frame: usize,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.read_entity(host, |host, _| {
        let presentation = host.interaction_presentation();
        assert_eq!(
            presentation.phase(),
            jellyflow_runtime::CanvasInteractionPhase::Committing,
            "frame {frame}: adapter settlement is intentionally pending",
        );
        let connection = presentation
            .connection()
            .expect("committing connection presentation");
        assert!(connection.ticket().is_some());
        assert_eq!(connection.suppressed_edge(), suppressed_edge);

        let evidence = host
            .scene_evidence
            .last_connection_preview
            .get()
            .expect("committing presentation plans one preview");
        assert_eq!(evidence.suppressed_edge, suppressed_edge);
        assert_eq!(
            host.scene_evidence.last_planned_wire_count.get(),
            0,
            "frame {frame}: a reconnect paints its pending preview instead of its old edge",
        );
        assert_eq!(
            host.scene_evidence
                .last_painted_connection_preview_count
                .get(),
            1,
            "frame {frame}: the optimistic connection preview must not flicker",
        );
    });
}
