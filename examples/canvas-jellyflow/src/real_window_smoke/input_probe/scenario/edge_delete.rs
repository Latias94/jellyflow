use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_edge_delete(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::FirstEdgePress => {
                if self.rendered_interaction_frame_pending()?
                    && !self.wait_for_rendered_interaction_frame(window, cx, surface)?
                {
                    return self.wait_or_timeout();
                }
                let edge = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
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
                self.transition(ProbePhase::FirstEdgeRelease);
                Ok(None)
            }
            ProbePhase::FirstEdgeRelease => {
                let point = self
                    .interaction()?
                    .press
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(window, cx, view, mouse_up(MouseButton::Left, point))?;
                self.transition(ProbePhase::FirstEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::FirstEdgeWaitSelected => {
                let edge = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                let selected = selection.edges();
                if selected.contains(&edge) {
                    let plan = refresh_edge_plan(
                        view,
                        self.interaction()?
                            .first_edge
                            .ok_or("input_edge_plan_missing")?,
                    )?;
                    self.interaction_mut()?.first_edge = Some(plan);
                    self.transition(ProbePhase::FirstEdgeDelete);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::FirstEdgeDelete => {
                self.dispatch_at_target_zoom(window, cx, view, key_down("delete")?)?;
                self.dispatch_at_target_zoom(window, cx, view, key_up("delete")?)?;
                self.transition(ProbePhase::FirstEdgeWaitDeleted);
                Ok(None)
            }
            ProbePhase::FirstEdgeWaitDeleted => {
                let edge = self
                    .interaction()?
                    .first_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
                if !graph_contains_edge(view, edge)? && interaction_is_settled(surface, cx) {
                    self.transition(ProbePhase::ConnectPress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::SecondEdgePress => {
                if self.rendered_interaction_frame_pending()?
                    && !self.wait_for_rendered_interaction_frame(window, cx, surface)?
                {
                    return self.wait_or_timeout();
                }
                let edge = self
                    .interaction()?
                    .second_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
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
                self.transition(ProbePhase::SecondEdgeRelease);
                Ok(None)
            }
            ProbePhase::SecondEdgeRelease => {
                let point = self
                    .interaction()?
                    .press
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(window, cx, view, mouse_up(MouseButton::Left, point))?;
                self.transition(ProbePhase::SecondEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::SecondEdgeWaitSelected => {
                let edge = self
                    .interaction()?
                    .second_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                let selected = selection.edges();
                if selected.contains(&edge) {
                    let plan = refresh_edge_plan(
                        view,
                        self.interaction()?
                            .second_edge
                            .ok_or("input_edge_plan_missing")?,
                    )?;
                    self.interaction_mut()?.second_edge = Some(plan);
                    self.transition(ProbePhase::SecondEdgeDelete);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::SecondEdgeDelete => {
                self.dispatch_at_target_zoom(window, cx, view, key_down("delete")?)?;
                self.dispatch_at_target_zoom(window, cx, view, key_up("delete")?)?;
                self.transition(ProbePhase::SecondEdgeWaitDeleted);
                Ok(None)
            }
            ProbePhase::SecondEdgeWaitDeleted => {
                let edge = self
                    .interaction()?
                    .second_edge
                    .ok_or("input_edge_plan_missing")?
                    .edge;
                if !graph_contains_edge(view, edge)? && interaction_is_settled(surface, cx) {
                    self.transition(ProbePhase::ReconnectEdgePress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            _ => unreachable!("edge deletion scenario received an unrelated phase"),
        }
    }
}
