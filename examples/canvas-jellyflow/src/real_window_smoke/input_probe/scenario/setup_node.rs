use super::super::*;

impl RealWindowSmokeProbe {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_setup_node(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::Fit => {
                let host = surface.read(cx).host().clone();
                host.update(cx, |host, cx| host.fit_view(cx))
                    .map_err(|_| "input_fit_view_failed")?;
                window.refresh();
                self.transition(ProbePhase::FitWait);
                Ok(None)
            }
            ProbePhase::FitWait => {
                if geometry_fits_surface(view, expectations, surface_size)? {
                    self.bounds_verified = self.config.surface_matches(surface_size);
                    if !self.bounds_verified {
                        return Err("input_window_bounds_mismatch");
                    }
                    self.transition(match self.config.scenario() {
                        SmokeScenario::Interaction => ProbePhase::CaptureZoom,
                        SmokeScenario::LodThreshold => ProbePhase::LodSet,
                    });
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::CaptureZoom => {
                let geometry = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?;
                let bounds = expectations
                    .node_ports
                    .keys()
                    .find_map(|node| {
                        geometry
                            .node(*node)
                            .filter(|node| node.is_fresh())
                            .map(|node| node.bounds())
                    })
                    .ok_or("input_node_fixture_incomplete")?;
                let anchor = CanvasPoint {
                    x: bounds.origin.x + bounds.size.width * 0.5,
                    y: bounds.origin.y + bounds.size.height * 0.5,
                };
                set_zoom_centered_on_canvas_point(
                    view,
                    surface_size,
                    self.config.zoom().value(),
                    anchor,
                )?;
                window.refresh();
                self.transition(ProbePhase::CaptureZoomWait);
                Ok(None)
            }
            ProbePhase::CaptureZoomWait => {
                if self.verify_target_zoom(view).is_ok()
                    && self.wait_for_rendered_interaction_frame(window, cx, surface)?
                {
                    self.transition(ProbePhase::Capture);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::Capture => {
                self.capture_visual(window, view, surface_size)?;
                self.transition(ProbePhase::PlanInput);
                Ok(None)
            }
            ProbePhase::PlanInput => {
                self.verify_target_zoom(view)?;
                self.plan_input(window, surface, cx, view, expectations, surface_size)?;
                self.transition(ProbePhase::NodePress);
                Ok(None)
            }
            ProbePhase::NodePress => {
                let host = surface.read(cx).host().clone();
                self.presented_frames.start(window, host)?;
                let point = self
                    .interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .press;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Left, point),
                )?;
                self.transition(ProbePhase::NodeMove);
                Ok(None)
            }
            ProbePhase::NodeMove => {
                let point = self
                    .interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .activation;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, point),
                )?;
                self.transition(ProbePhase::NodeWaitActive);
                Ok(None)
            }
            ProbePhase::NodeWaitActive => {
                let node = self
                    .interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .node;
                let presentation = interaction_presentation(surface, cx);
                if presentation.phase() == CanvasInteractionPhase::NodeDrag
                    && presentation
                        .node_drag()
                        .is_some_and(|drag| drag.nodes().contains(&node))
                {
                    self.transition(ProbePhase::NodeAutoPanMove);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::NodeAutoPanMove => {
                let point = self
                    .interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .release;
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let interaction = self.interaction_mut()?;
                interaction.auto_pan_origin = Some(transform);
                interaction.auto_pan_last = Some(transform);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, point),
                )?;
                self.transition(ProbePhase::NodeAutoPanWait);
                Ok(None)
            }
            ProbePhase::NodeAutoPanWait => {
                let _ = advance_pending_interaction_frame(surface, window, cx);
                let current = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let frame_advanced = self
                    .interaction()?
                    .auto_pan_last
                    .is_some_and(|previous| previous.pan != current.pan);
                let auto_pan_frames = {
                    let interaction = self.interaction_mut()?;
                    if frame_advanced {
                        interaction.auto_pan_frames = interaction.auto_pan_frames.saturating_add(1);
                        interaction.auto_pan_last = Some(current);
                    }
                    interaction.auto_pan_frames
                };
                if auto_pan_frames >= SMOKE_AUTOPAN_MIN_FRAMES {
                    self.transition(ProbePhase::NodeRelease);
                    Ok(None)
                } else {
                    window.refresh();
                    if self.wait_ticks == MAX_PHASE_WAIT_TICKS {
                        let interaction = self.interaction()?;
                        let pointer = interaction.node.ok_or("input_node_plan_missing")?.release;
                        let baseline = interaction
                            .auto_pan_origin
                            .ok_or("input_autopan_baseline_missing")?;
                        let presentation = interaction_presentation(surface, cx);
                        let tick_resources = surface.read(cx).host().read(cx).resource_snapshot();
                        let drag_delta = presentation
                            .node_drag()
                            .map(|drag| drag.delta())
                            .unwrap_or_default();
                        let release_pending = presentation
                            .node_drag()
                            .is_some_and(|drag| drag.is_release_pending());
                        eprintln!(
                            "JELLYFLOW_REAL_WINDOW_SMOKE_AUTOPAN phase={:?} pointer_x={} \
                             pointer_y={} baseline_pan_x={} baseline_pan_y={} final_pan_x={} \
                             final_pan_y={} zoom={} drag_delta_x={} drag_delta_y={} \
                             release_pending={} tick_registrations={} tick_entries={} \
                             tick_frame_requests={} observed_frames={}",
                            presentation.phase(),
                            pointer.x,
                            pointer.y,
                            baseline.pan.x,
                            baseline.pan.y,
                            current.pan.x,
                            current.pan.y,
                            current.zoom,
                            drag_delta.x,
                            drag_delta.y,
                            release_pending,
                            tick_ordinal(tick_resources.interaction_registered_tick_id()),
                            tick_ordinal(tick_resources.interaction_entered_tick_id()),
                            tick_ordinal(tick_resources.interaction_requested_tick_id()),
                            auto_pan_frames,
                        );
                    }
                    self.wait_or_timeout()
                }
            }
            ProbePhase::NodeRelease => {
                let point = self
                    .interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .release;
                self.verify_target_zoom(view)?;
                let event = mouse_up(MouseButton::Left, point);
                self.presented_frames.after_next_stage(
                    SmokeFrameStageV1::NodeDragPending,
                    move |window, cx| {
                        window.dispatch_event(event, cx);
                        window.refresh();
                    },
                )?;
                jellyflow_open_gpui::host::request_native_test_frame(window);
                self.transition(ProbePhase::NodeReleaseFrameWait);
                Ok(None)
            }
            ProbePhase::NodeReleaseFrameWait => {
                if !self.presented_frames.action_completed() {
                    jellyflow_open_gpui::host::request_native_test_frame(window);
                    return self.wait_or_timeout();
                }
                let dispatched_events = self.interaction()?.dispatched_events.saturating_add(1);
                self.interaction_mut()?.dispatched_events = dispatched_events;
                self.transition(ProbePhase::NodeAuthoritativeFrameWait);
                Ok(None)
            }
            ProbePhase::NodeAuthoritativeFrameWait => {
                if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    return self.wait_or_timeout();
                }
                if !self
                    .presented_frames
                    .observed_stage(SmokeFrameStageV1::NodeDragAuthoritative)
                {
                    return self.wait_with_reason("input_node_authoritative_frame_not_presented");
                }
                self.transition(ProbePhase::NodeWaitSettled);
                Ok(None)
            }
            ProbePhase::NodeWaitSettled => {
                let plan = self.interaction()?.node.ok_or("input_node_plan_missing")?;
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let moved = snapshot
                    .graph()
                    .node(&plan.node)
                    .is_some_and(|node| point_changed(node.pos, plan.original_position));
                let (node_geometry_converged, routes_converged) =
                    dragged_geometry_status(view, expectations, plan)?;
                let interaction_settled = interaction_is_settled(surface, cx);
                if moved && node_geometry_converged && routes_converged && interaction_settled {
                    let selection = view
                        .selection()
                        .map_err(|_| "input_selection_read_failed")?;
                    if !selection.nodes().contains(&plan.node) {
                        return self.wait_with_reason("input_node_selection_missing");
                    }
                    if !self.interaction()?.selection_reframed {
                        position_node_for_selection(view, plan.node, surface_size)?;
                        self.interaction_mut()?.selection_reframed = true;
                        window.refresh();
                        return Ok(None);
                    }
                    if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                        return self.wait_or_timeout();
                    }
                    self.capture_checkpoint(
                        window,
                        surface,
                        cx,
                        view,
                        surface_size,
                        SmokeCaptureState::Selected,
                    )?;
                    let interaction = self.interaction_mut()?;
                    interaction.node_dragged = true;
                    interaction.tab_focus_origin = window.focused(cx);
                    self.transition(ProbePhase::TabForward);
                    Ok(None)
                } else {
                    let reason = if !moved {
                        "input_node_graph_unchanged"
                    } else if !node_geometry_converged {
                        "input_node_geometry_stale"
                    } else if !routes_converged {
                        "input_node_routes_missing"
                    } else {
                        "input_node_interaction_unsettled"
                    };
                    if self.wait_ticks == MAX_PHASE_WAIT_TICKS {
                        let geometry = view
                            .geometry_snapshot()
                            .map_err(|_| "geometry_snapshot_read_failed")?;
                        let current_model = snapshot.graph().node(&plan.node).map(|node| node.pos);
                        let published = geometry.node(plan.node);
                        let published_origin = published.map(|node| node.bounds().origin);
                        let model_delta = current_model.map(|position| CanvasPoint {
                            x: position.x - plan.original_position.x,
                            y: position.y - plan.original_position.y,
                        });
                        let geometry_delta = published_origin.map(|origin| CanvasPoint {
                            x: origin.x - plan.original_geometry_origin.x,
                            y: origin.y - plan.original_geometry_origin.y,
                        });
                        let presentation = interaction_presentation(surface, cx);
                        eprintln!(
                            "JELLYFLOW_REAL_WINDOW_SMOKE_NODE_SETTLEMENT node={} reason={reason} \
                             original_model={:?} current_model={current_model:?} model_delta={model_delta:?} \
                             original_geometry={:?} published_origin={published_origin:?} \
                             geometry_delta={geometry_delta:?} geometry_fresh={:?} \
                             layout_generation={:?} layout_revision={} graph_revision={} \
                             interaction_phase={:?}",
                            plan.node.0,
                            plan.original_position,
                            plan.original_geometry_origin,
                            published.map(|node| node.is_fresh()),
                            published.map(|node| node.layout_generation()),
                            geometry.layout_revision(),
                            snapshot.revision(),
                            presentation.phase(),
                        );
                    }
                    self.wait_with_reason(reason)
                }
            }
            ProbePhase::TabForward => {
                let origin = self
                    .interaction()?
                    .tab_focus_origin
                    .clone()
                    .ok_or("input_tab_focus_origin_missing")?;
                if !origin.is_focused(window) {
                    return Err("input_tab_focus_origin_stale");
                }
                self.dispatch_at_target_zoom(window, cx, view, key_down("tab")?)?;
                self.dispatch_at_target_zoom(window, cx, view, key_up("tab")?)?;
                self.transition(ProbePhase::TabForwardWait);
                Ok(None)
            }
            ProbePhase::TabForwardWait => {
                let current = window.focused(cx);
                let origin = self.interaction()?.tab_focus_origin.clone();
                if current.is_some() && current != origin {
                    let interaction = self.interaction_mut()?;
                    interaction.tab_focus_forward = current;
                    interaction.tab_focus_moved = true;
                    self.transition(ProbePhase::TabBackward);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::TabBackward => {
                let forward = self
                    .interaction()?
                    .tab_focus_forward
                    .clone()
                    .ok_or("input_tab_focus_forward_missing")?;
                if !forward.is_focused(window) {
                    return Err("input_tab_focus_forward_stale");
                }
                self.dispatch_at_target_zoom(window, cx, view, key_down("shift-tab")?)?;
                self.dispatch_at_target_zoom(window, cx, view, key_up("shift-tab")?)?;
                self.transition(ProbePhase::TabBackwardWait);
                Ok(None)
            }
            ProbePhase::TabBackwardWait => {
                let current = window.focused(cx);
                let forward = self.interaction()?.tab_focus_forward.clone();
                if current.is_some()
                    && current != forward
                    && surface.read(cx).graph_has_focus(window, cx)
                {
                    self.interaction_mut()?.tab_focus_restored = true;
                    self.transition(ProbePhase::PanPress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            _ => unreachable!("setup and node scenario received an unrelated phase"),
        }
    }
}
