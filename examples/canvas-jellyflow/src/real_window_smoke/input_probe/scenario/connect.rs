use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_connect(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::ConnectPress => {
                let plan = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?;
                let Some(source) = visible_port_point(view, plan.from, surface_size)? else {
                    if self.wait_ticks == MAX_PHASE_WAIT_TICKS {
                        let geometry = view
                            .geometry_snapshot()
                            .map_err(|_| "geometry_snapshot_read_failed")?;
                        let transform = view
                            .viewport_transform()
                            .map_err(|_| "input_viewport_read_failed")?;
                        eprintln!(
                            "JELLYFLOW_REAL_WINDOW_SMOKE_CONNECT_PRESS source_port={} \
                             target_port={} source_outcome={:?} target_outcome={:?} \
                             source_owner_center={:?} target_owner_center={:?} pan_x={} \
                             pan_y={} zoom={} wait_ticks={}",
                            plan.from.0,
                            plan.to.0,
                            geometry.port_outcome(plan.from),
                            geometry.port_outcome(plan.to),
                            port_owner_view_center(view, plan.from)?,
                            port_owner_view_center(view, plan.to)?,
                            transform.pan.x,
                            transform.pan.y,
                            transform.zoom,
                            self.wait_ticks,
                        );
                    }
                    window.refresh();
                    return self.wait_or_timeout();
                };
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let target_anchor = plan
                    .to_anchor
                    .ok_or("input_connection_target_anchor_missing")?;
                let target = transform
                    .screen_point_for_canvas(target_anchor)
                    .ok_or("input_connection_target_projection_failed")?;
                let edge = auto_pan_pointer_toward(target, surface_size)
                    .ok_or("input_autopan_plan_invalid")?;
                let strategy = auto_pan_pointer_strategy(edge, surface_size)
                    .ok_or("input_autopan_plan_invalid")?;
                let interaction = self.interaction_mut()?;
                interaction.press = Some(source);
                interaction.release = Some(edge);
                interaction.connection_auto_pan_strategy = Some(strategy);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Left, source),
                )?;
                self.transition(ProbePhase::ConnectAutoPanMove);
                Ok(None)
            }
            ProbePhase::ConnectAutoPanMove => {
                let edge = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                self.interaction_mut()?.connection_auto_pan_last = Some(transform);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, edge),
                )?;
                self.transition(ProbePhase::ConnectAutoPanWait);
                Ok(None)
            }
            ProbePhase::ConnectAutoPanWait => {
                let _ = advance_pending_interaction_frame(surface, window, cx);
                let current = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let interaction = self.interaction_mut()?;
                if interaction
                    .connection_auto_pan_last
                    .is_some_and(|previous| previous.pan != current.pan)
                {
                    interaction.connection_auto_pan_frames =
                        interaction.connection_auto_pan_frames.saturating_add(1);
                    interaction.connection_auto_pan_last = Some(current);
                }
                let frames = interaction.connection_auto_pan_frames;
                let plan = interaction.first_edge.ok_or("input_edge_plan_missing")?;
                let target_port = plan.to;
                let target_anchor = plan
                    .to_anchor
                    .ok_or("input_connection_target_anchor_missing")?;
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
                    self.transition(ProbePhase::ConnectMaterializeTarget);
                    return Ok(None);
                }
                if settled_target.is_none()
                    && let Some(target) = target
                {
                    let steering = steer_auto_pan_pointer_toward(target, surface_size)
                        .ok_or("input_autopan_plan_invalid")?;
                    let strategy = auto_pan_pointer_strategy(steering, surface_size)
                        .ok_or("input_autopan_plan_invalid")?;
                    if self.interaction()?.connection_auto_pan_strategy != Some(strategy) {
                        self.dispatch_at_target_zoom(
                            window,
                            cx,
                            view,
                            mouse_move(MouseButton::Left, steering),
                        )?;
                        let interaction = self.interaction_mut()?;
                        interaction.release = Some(steering);
                        interaction.connection_auto_pan_strategy = Some(strategy);
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
                        "JELLYFLOW_REAL_WINDOW_SMOKE_CONNECTION_AUTOPAN target_port={} \
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
            ProbePhase::ConnectMaterializeTarget => {
                let target_port = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?
                    .to;
                let Some(target) = port_point(view, target_port)? else {
                    window.refresh();
                    return self.wait_or_timeout();
                };
                let Some(target) = settled_connection_target_point(target, surface_size) else {
                    return Err("input_connection_materialized_target_outside_safe_area");
                };
                self.interaction_mut()?.release = Some(target);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, target),
                )?;
                self.transition(ProbePhase::ConnectWaitCandidate);
                Ok(None)
            }
            ProbePhase::ConnectWaitCandidate => {
                let target = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?
                    .to;
                if let Some(reason) = connection_candidate_failure(surface, cx, target) {
                    self.wait_with_reason(reason)
                } else if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    self.wait_or_timeout()
                } else {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ConnectionPreview,
                    )?;
                    let presentation = interaction_presentation(surface, cx);
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
                    interaction.connection_preview_wire_painted = true;
                    self.transition(ProbePhase::ConnectRelease);
                    Ok(None)
                }
            }
            ProbePhase::ConnectRelease => {
                let path = self
                    .interaction()?
                    .release_wire_path
                    .as_ref()
                    .cloned()
                    .ok_or("input_connection_preview_path_missing")?;
                // Mouse release is deferred by the native bridge. The already-rendered preview
                // must remain visible during that handoff instead of exposing a blank frame.
                inspect_edge_path_frame(
                    window,
                    view,
                    surface_size,
                    &path,
                    self.paint_palette.canvas_background,
                )?;
                self.interaction_mut()?.connection_release_frame_painted = true;
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
                self.transition(ProbePhase::ConnectVerifyReleaseFrame);
                Ok(None)
            }
            ProbePhase::ConnectVerifyReleaseFrame => {
                let presentation = interaction_presentation(surface, cx);
                if presentation.phase() != CanvasInteractionPhase::Committing
                    && presentation.terminal().is_none()
                {
                    return self.wait_with_reason("input_connection_release_unresolved");
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
                interaction.connection_release_continuous = true;
                self.transition(ProbePhase::ConnectWaitCommitted);
                Ok(None)
            }
            ProbePhase::ConnectWaitCommitted => {
                let plan = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?;
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let created = snapshot.graph().edges().iter().find_map(|(edge, model)| {
                    (model.from == plan.from && model.to == plan.to).then_some(*edge)
                });
                if let Some(created) = created.filter(|edge| *edge != plan.edge)
                    && interaction_is_settled(surface, cx)
                {
                    if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                        return self.wait_or_timeout();
                    }
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ConnectionCommitted,
                    )?;
                    let interaction = self.interaction_mut()?;
                    interaction.created_edge = Some(created);
                    interaction.connection_created = true;
                    self.transition(ProbePhase::SecondEdgePress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            _ => unreachable!("connection scenario received an unrelated phase"),
        }
    }
}
