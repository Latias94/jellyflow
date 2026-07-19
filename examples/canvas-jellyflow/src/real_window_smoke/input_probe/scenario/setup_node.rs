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
                    self.transition(ProbePhase::Capture);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::Capture => {
                self.capture(window, view, expectations, surface_size)?;
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
