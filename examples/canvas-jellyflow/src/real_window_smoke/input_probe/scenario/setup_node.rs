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
                    self.capture_fit = Some(
                        view.viewport_transform()
                            .map_err(|_| "input_viewport_read_failed")?,
                    );
                    self.transition(ProbePhase::CaptureZoom);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::CaptureZoom => {
                let current = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let center = CanvasPoint {
                    x: surface_size.width * 0.5,
                    y: surface_size.height * 0.5,
                };
                let canvas_center = current.canvas_point_at_screen(center);
                let zoom = self.config.zoom().value();
                let pan = CanvasPoint {
                    x: center.x / zoom - canvas_center.x,
                    y: center.y / zoom - canvas_center.y,
                };
                view.set_viewport(pan, zoom)
                    .map_err(|_| "input_capture_zoom_failed")?;
                window.refresh();
                self.transition(ProbePhase::CaptureZoomWait);
                Ok(None)
            }
            ProbePhase::CaptureZoomWait => {
                let current = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                if (current.zoom - self.config.zoom().value()).abs() <= f32::EPSILON {
                    self.transition(ProbePhase::Capture);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::Capture => {
                self.capture_visual(window, surface_size)?;
                self.transition(ProbePhase::RestoreFit);
                Ok(None)
            }
            ProbePhase::RestoreFit => {
                let transform = self.capture_fit.ok_or("input_capture_fit_missing")?;
                view.set_viewport(transform.pan, transform.zoom)
                    .map_err(|_| "input_restore_fit_failed")?;
                window.refresh();
                self.transition(ProbePhase::RestoreFitWait);
                Ok(None)
            }
            ProbePhase::RestoreFitWait => {
                if geometry_fits_surface(view, expectations, surface_size)? {
                    self.transition(ProbePhase::PlanInput);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::PlanInput => {
                self.plan_input(window, view, expectations, surface_size)?;
                self.transition(ProbePhase::NodePress);
                Ok(None)
            }
            ProbePhase::NodePress => {
                let point = self.node.ok_or("input_node_plan_missing")?.press;
                self.dispatch(window, cx, mouse_down(MouseButton::Left, point));
                self.transition(ProbePhase::NodeMove);
                Ok(None)
            }
            ProbePhase::NodeMove => {
                let point = self.node.ok_or("input_node_plan_missing")?.release;
                self.dispatch(window, cx, mouse_move(MouseButton::Left, point));
                self.transition(ProbePhase::NodeWaitActive);
                Ok(None)
            }
            ProbePhase::NodeWaitActive => {
                let node = self.node.ok_or("input_node_plan_missing")?.node;
                let presentation = interaction_presentation(surface, cx);
                if presentation.phase() == CanvasInteractionPhase::NodeDrag
                    && presentation
                        .node_drag()
                        .is_some_and(|drag| drag.nodes().contains(&node))
                {
                    self.transition(ProbePhase::NodeRelease);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::NodeRelease => {
                let point = self.node.ok_or("input_node_plan_missing")?.release;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, point));
                self.transition(ProbePhase::NodeWaitSettled);
                Ok(None)
            }
            ProbePhase::NodeWaitSettled => {
                let plan = self.node.ok_or("input_node_plan_missing")?;
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
                    self.refresh_evidence(window, view, expectations, surface_size)?;
                    self.node_dragged = true;
                    self.transition(ProbePhase::PanPress);
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
                    self.wait_with_reason(reason)
                }
            }
            _ => unreachable!("setup and node scenario received an unrelated phase"),
        }
    }
}
