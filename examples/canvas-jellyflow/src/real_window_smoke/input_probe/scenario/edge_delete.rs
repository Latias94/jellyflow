use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_edge_delete(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::FirstEdgePress => {
                let edge = self.first_edge.ok_or("input_edge_plan_missing")?.edge;
                let Some(point) = edge_point(
                    view,
                    edge,
                    EdgePathAnchor::Progress(EDGE_SELECTION_PROGRESS),
                )?
                else {
                    return self.wait_or_timeout();
                };
                self.press = Some(point);
                self.dispatch(window, cx, mouse_down(MouseButton::Left, point));
                self.transition(ProbePhase::FirstEdgeRelease);
                Ok(None)
            }
            ProbePhase::FirstEdgeRelease => {
                let point = self.press.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, point));
                self.transition(ProbePhase::FirstEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::FirstEdgeWaitSelected => {
                let edge = self.first_edge.ok_or("input_edge_plan_missing")?.edge;
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                let selected = selection.edges();
                if selected.contains(&edge) {
                    self.transition(ProbePhase::FirstEdgeDelete);
                    Ok(None)
                } else if selected.is_empty() {
                    self.wait_or_timeout()
                } else {
                    Err("input_edge_selection_mismatch")
                }
            }
            ProbePhase::FirstEdgeDelete => {
                self.dispatch(window, cx, key_down("delete")?);
                self.dispatch(window, cx, key_up("delete")?);
                self.transition(ProbePhase::FirstEdgeWaitDeleted);
                Ok(None)
            }
            ProbePhase::FirstEdgeWaitDeleted => {
                let edge = self.first_edge.ok_or("input_edge_plan_missing")?.edge;
                if !graph_contains_edge(view, edge)? && interaction_is_settled(surface, cx) {
                    self.keyboard_edge_deleted = true;
                    self.transition(ProbePhase::ConnectPress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::SecondEdgePress => {
                let edge = self.second_edge.ok_or("input_edge_plan_missing")?.edge;
                let Some(point) = edge_point(view, edge, EdgePathAnchor::Center)? else {
                    return self.wait_or_timeout();
                };
                self.press = Some(point);
                self.dispatch(window, cx, mouse_down(MouseButton::Left, point));
                self.transition(ProbePhase::SecondEdgeRelease);
                Ok(None)
            }
            ProbePhase::SecondEdgeRelease => {
                let point = self.press.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, point));
                self.transition(ProbePhase::SecondEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::SecondEdgeWaitSelected => {
                let edge = self.second_edge.ok_or("input_edge_plan_missing")?.edge;
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                let selected = selection.edges();
                if selected.contains(&edge) {
                    self.transition(ProbePhase::SecondEdgeDelete);
                    Ok(None)
                } else if selected.is_empty() {
                    self.wait_or_timeout()
                } else {
                    Err("input_edge_selection_mismatch")
                }
            }
            ProbePhase::SecondEdgeDelete => {
                self.dispatch(window, cx, key_down("delete")?);
                self.dispatch(window, cx, key_up("delete")?);
                self.transition(ProbePhase::SecondEdgeWaitDeleted);
                Ok(None)
            }
            ProbePhase::SecondEdgeWaitDeleted => {
                let edge = self.second_edge.ok_or("input_edge_plan_missing")?.edge;
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
