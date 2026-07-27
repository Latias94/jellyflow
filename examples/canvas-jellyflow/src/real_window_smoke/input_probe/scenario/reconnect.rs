use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_reconnect(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::ReconnectEdgePress => {
                if self.rendered_interaction_frame_pending()?
                    && !self.wait_for_rendered_interaction_frame(window, cx, surface)?
                {
                    return self.wait_or_timeout();
                }
                let edge = self
                    .interaction()?
                    .created_edge
                    .ok_or("input_created_edge_missing")?;
                let Some(point) =
                    self.edge_press_point(window, cx, surface, view, edge, surface_size)?
                else {
                    return self.wait_or_timeout();
                };
                self.interaction_mut()?.press = Some(point);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Left, point),
                )?;
                self.transition(ProbePhase::ReconnectEdgeRelease);
                Ok(None)
            }
            ProbePhase::ReconnectEdgeRelease => {
                let point = self
                    .interaction()?
                    .press
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(window, cx, view, mouse_up(MouseButton::Left, point))?;
                self.transition(ProbePhase::ReconnectEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::ReconnectEdgeWaitSelected => {
                let edge = self
                    .interaction()?
                    .created_edge
                    .ok_or("input_created_edge_missing")?;
                let route_selected = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?
                    .route(edge)
                    .is_some_and(|route| route.facts().interaction.selected);
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                if selection.edges().contains(&edge) && route_selected {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ReconnectSelected,
                    )?;
                    self.transition(ProbePhase::ReconnectPress);
                    Ok(None)
                } else {
                    self.wait_with_reason("input_reconnect_selection_stale")
                }
            }
            ProbePhase::ReconnectPress => {
                let interaction = self.interaction()?;
                let edge = interaction
                    .created_edge
                    .ok_or("input_created_edge_missing")?;
                let reconnect = match interaction.reconnect {
                    Some(reconnect) => reconnect,
                    None => {
                        let reconnect = choose_reconnect(view, edge, interaction.second_edge)?
                            .ok_or("input_reconnect_plan_missing")?;
                        self.interaction_mut()?.reconnect = Some(reconnect);
                        reconnect
                    }
                };
                let Some(source) =
                    visible_reconnect_grip(view, edge, reconnect.endpoint, surface_size)?
                else {
                    window.refresh();
                    return self.wait_or_timeout();
                };
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let target = transform
                    .screen_point_for_canvas(reconnect.target_anchor)
                    .ok_or("input_reconnect_target_projection_failed")?;
                let edge = auto_pan_pointer_toward(target, surface_size)
                    .ok_or("input_autopan_plan_invalid")?;
                let strategy = auto_pan_pointer_strategy(edge, surface_size)
                    .ok_or("input_autopan_plan_invalid")?;
                let interaction = self.interaction_mut()?;
                interaction.press = Some(source);
                interaction.release = Some(edge);
                interaction.reconnect_auto_pan_strategy = Some(strategy);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Left, source),
                )?;
                self.transition(ProbePhase::ReconnectAutoPanMove);
                Ok(None)
            }
            ProbePhase::ReconnectAutoPanMove => {
                let edge = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                self.interaction_mut()?.reconnect_auto_pan_last = Some(transform);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, edge),
                )?;
                self.transition(ProbePhase::ReconnectAutoPanWait);
                Ok(None)
            }
            ProbePhase::ReconnectAutoPanWait => {
                let _ = advance_pending_interaction_frame(surface, window, cx);
                let current = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let interaction = self.interaction_mut()?;
                if interaction
                    .reconnect_auto_pan_last
                    .is_some_and(|previous| previous.pan != current.pan)
                {
                    interaction.reconnect_auto_pan_frames =
                        interaction.reconnect_auto_pan_frames.saturating_add(1);
                    interaction.reconnect_auto_pan_last = Some(current);
                }
                let frames = interaction.reconnect_auto_pan_frames;
                let target_port = interaction
                    .reconnect
                    .ok_or("input_reconnect_plan_missing")?
                    .target;
                let target_anchor = interaction
                    .reconnect
                    .ok_or("input_reconnect_plan_missing")?
                    .target_anchor;
                let target = current
                    .screen_point_for_canvas(target_anchor);
                let settled_target =
                    target.and_then(|point| settled_connection_target_point(point, surface_size));
                let _ = interaction;
                if frames >= SMOKE_AUTOPAN_MIN_FRAMES
                    && let Some(target) = settled_target
                {
                    self.interaction_mut()?.release = Some(target);
                    self.dispatch_at_target_zoom(
                        window,
                        cx,
                        view,
                        mouse_move(MouseButton::Left, target),
                    )?;
                    self.transition(ProbePhase::ReconnectMaterializeTarget);
                    return Ok(None);
                }
                if settled_target.is_none()
                    && let Some(target) = target
                {
                    let steering = steer_auto_pan_pointer_toward(target, surface_size)
                        .ok_or("input_autopan_plan_invalid")?;
                    let strategy = auto_pan_pointer_strategy(steering, surface_size)
                        .ok_or("input_autopan_plan_invalid")?;
                    if self.interaction()?.reconnect_auto_pan_strategy != Some(strategy) {
                        self.dispatch_at_target_zoom(
                            window,
                            cx,
                            view,
                            mouse_move(MouseButton::Left, steering),
                        )?;
                        let interaction = self.interaction_mut()?;
                        interaction.release = Some(steering);
                        interaction.reconnect_auto_pan_strategy = Some(strategy);
                        interaction.auto_pan_steering_events =
                            interaction.auto_pan_steering_events.saturating_add(1);
                    }
                }
                if self.wait_ticks == MAX_PHASE_WAIT_TICKS {
                    let geometry = view
                        .geometry_snapshot()
                        .map_err(|_| "geometry_snapshot_read_failed")?;
                    let tick_stats = surface.read(cx).host().read(cx).stats();
                    eprintln!(
                        "JELLYFLOW_REAL_WINDOW_SMOKE_RECONNECT_AUTOPAN target_port={} \
                         target_point={target:?} settled_target={settled_target:?} frames={frames} \
                         pan_x={} pan_y={} zoom={} target_outcome={:?} tick_registrations={} \
                         tick_entries={} tick_frame_requests={}",
                        target_port.0,
                        current.pan.x,
                        current.pan.y,
                        current.zoom,
                        geometry.port_outcome(target_port),
                        tick_stats.interaction_tick_callback_registrations(),
                        tick_stats.interaction_tick_callback_entries(),
                        tick_stats.interaction_tick_frame_requests(),
                    );
                }
                window.refresh();
                self.wait_or_timeout()
            }
            ProbePhase::ReconnectMaterializeTarget => {
                let target_port = self
                    .interaction()?
                    .reconnect
                    .ok_or("input_reconnect_plan_missing")?
                    .target;
                let Some(target) = port_point(view, target_port)? else {
                    window.refresh();
                    return self.wait_or_timeout();
                };
                let Some(target) = settled_connection_target_point(target, surface_size) else {
                    return Err("input_reconnect_materialized_target_outside_safe_area");
                };
                self.interaction_mut()?.release = Some(target);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, target),
                )?;
                self.transition(ProbePhase::ReconnectWaitCandidate);
                Ok(None)
            }
            ProbePhase::ReconnectWaitCandidate => {
                let target = self
                    .interaction()?
                    .reconnect
                    .ok_or("input_reconnect_plan_missing")?
                    .target;
                if let Some(reason) = connection_candidate_failure(surface, cx, target) {
                    self.wait_with_reason(reason)
                } else if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    self.wait_or_timeout()
                } else {
                    let presentation = interaction_presentation(surface, cx);
                    let suppressed_edge = presentation
                        .connection()
                        .and_then(|connection| connection.suppressed_edge())
                        .ok_or("input_reconnect_suppressed_edge_missing")?;
                    self.capture_checkpoint_omitting_edge(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ReconnectPreview,
                        Some(suppressed_edge),
                    )?;
                    let path = connection_preview_path(view, &presentation)?;
                    inspect_edge_path_frame(
                        window,
                        view,
                        surface_size,
                        &path,
                        self.paint_palette.canvas_background,
                    )?;
                    let interaction = self.interaction_mut()?;
                    interaction.release_wire_path = Some(path);
                    interaction.reconnect_preview_wire_painted = true;
                    self.transition(ProbePhase::ReconnectRelease);
                    Ok(None)
                }
            }
            ProbePhase::ReconnectRelease => {
                let path = self
                    .interaction()?
                    .release_wire_path
                    .as_ref()
                    .cloned()
                    .ok_or("input_connection_preview_path_missing")?;
                // The previous rendered preview remains the visual handoff frame until the
                // deferred native release is resolved.
                inspect_edge_path_frame(
                    window,
                    view,
                    surface_size,
                    &path,
                    self.paint_palette.canvas_background,
                )?;
                self.interaction_mut()?.reconnect_release_frame_painted = true;
                let target = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_up(MouseButton::Left, target),
                )?;
                self.transition(ProbePhase::ReconnectVerifyReleaseFrame);
                Ok(None)
            }
            ProbePhase::ReconnectVerifyReleaseFrame => {
                let presentation = interaction_presentation(surface, cx);
                if presentation.phase() != CanvasInteractionPhase::Committing
                    && presentation.terminal().is_none()
                {
                    return self.wait_with_reason("input_reconnect_release_unresolved");
                }
                let path = self
                    .interaction()?
                    .release_wire_path
                    .as_ref()
                    .cloned()
                    .ok_or("input_connection_preview_path_missing")?;
                inspect_edge_path_frame(
                    window,
                    view,
                    surface_size,
                    &path,
                    self.paint_palette.canvas_background,
                )?;
                let interaction = self.interaction_mut()?;
                interaction.reconnect_release_continuous = true;
                self.transition(ProbePhase::ReconnectWaitCommitted);
                Ok(None)
            }
            ProbePhase::ReconnectWaitCommitted => {
                let interaction = self.interaction()?;
                let edge = interaction
                    .created_edge
                    .ok_or("input_created_edge_missing")?;
                let reconnect = interaction
                    .reconnect
                    .ok_or("input_reconnect_plan_missing")?;
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let applied =
                    snapshot
                        .graph()
                        .edge(&edge)
                        .is_some_and(|model| match reconnect.endpoint {
                            EdgeEndpoint::From => model.from == reconnect.target,
                            EdgeEndpoint::To => model.to == reconnect.target,
                        });
                if applied && interaction_is_settled(surface, cx) {
                    if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                        return self.wait_or_timeout();
                    }
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ReconnectCommitted,
                    )?;
                    self.interaction_mut()?.reconnect_applied = true;
                    self.transition(ProbePhase::SearchPress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            _ => unreachable!("reconnect scenario received an unrelated phase"),
        }
    }
}
