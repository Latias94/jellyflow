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
                let edge = self.created_edge.ok_or("input_created_edge_missing")?;
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
                self.transition(ProbePhase::ReconnectEdgeRelease);
                Ok(None)
            }
            ProbePhase::ReconnectEdgeRelease => {
                let point = self.press.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, point));
                self.transition(ProbePhase::ReconnectEdgeWaitSelected);
                Ok(None)
            }
            ProbePhase::ReconnectEdgeWaitSelected => {
                let edge = self.created_edge.ok_or("input_created_edge_missing")?;
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
                let edge = self.created_edge.ok_or("input_created_edge_missing")?;
                let reconnect = match self.reconnect {
                    Some(reconnect) => reconnect,
                    None => {
                        let reconnect = choose_reconnect(view, edge, self.second_edge)?
                            .ok_or("input_reconnect_plan_missing")?;
                        self.reconnect = Some(reconnect);
                        reconnect
                    }
                };
                let anchor = match reconnect.endpoint {
                    EdgeEndpoint::From => EdgePathAnchor::Source,
                    EdgeEndpoint::To => EdgePathAnchor::Target,
                };
                let Some(source) = edge_point(view, edge, anchor)? else {
                    return self.wait_or_timeout();
                };
                let Some(target) = port_point(view, reconnect.target)? else {
                    return self.wait_or_timeout();
                };
                self.press = Some(source);
                self.release = Some(target);
                self.dispatch(window, cx, mouse_down(MouseButton::Left, source));
                self.transition(ProbePhase::ReconnectMove);
                Ok(None)
            }
            ProbePhase::ReconnectMove => {
                let target = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_move(MouseButton::Left, target));
                self.transition(ProbePhase::ReconnectWaitCandidate);
                Ok(None)
            }
            ProbePhase::ReconnectWaitCandidate => {
                let target = self.reconnect.ok_or("input_reconnect_plan_missing")?.target;
                if let Some(reason) = connection_candidate_failure(surface, cx, target) {
                    self.wait_with_reason(reason)
                } else {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ReconnectPreview,
                    )?;
                    self.transition(ProbePhase::ReconnectRelease);
                    Ok(None)
                }
            }
            ProbePhase::ReconnectRelease => {
                let target = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, target));
                self.transition(ProbePhase::ReconnectWaitCommitted);
                Ok(None)
            }
            ProbePhase::ReconnectWaitCommitted => {
                let edge = self.created_edge.ok_or("input_created_edge_missing")?;
                let reconnect = self.reconnect.ok_or("input_reconnect_plan_missing")?;
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
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ReconnectCommitted,
                    )?;
                    self.reconnect_applied = true;
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
