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
                let plan = self.first_edge.ok_or("input_edge_plan_missing")?;
                let Some(source) = port_point(view, plan.from)? else {
                    return self.wait_or_timeout();
                };
                let Some(target) = port_point(view, plan.to)? else {
                    return self.wait_or_timeout();
                };
                self.press = Some(source);
                self.release = Some(target);
                self.dispatch(window, cx, mouse_down(MouseButton::Left, source));
                self.transition(ProbePhase::ConnectMove);
                Ok(None)
            }
            ProbePhase::ConnectMove => {
                let target = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_move(MouseButton::Left, target));
                self.transition(ProbePhase::ConnectWaitCandidate);
                Ok(None)
            }
            ProbePhase::ConnectWaitCandidate => {
                let target = self.first_edge.ok_or("input_edge_plan_missing")?.to;
                if let Some(reason) = connection_candidate_failure(surface, cx, target) {
                    self.wait_with_reason(reason)
                } else {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ConnectionPreview,
                    )?;
                    self.transition(ProbePhase::ConnectRelease);
                    Ok(None)
                }
            }
            ProbePhase::ConnectRelease => {
                let target = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Left, target));
                self.transition(ProbePhase::ConnectWaitCommitted);
                Ok(None)
            }
            ProbePhase::ConnectWaitCommitted => {
                let plan = self.first_edge.ok_or("input_edge_plan_missing")?;
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let created = snapshot.graph().edges().iter().find_map(|(edge, model)| {
                    (model.from == plan.from && model.to == plan.to).then_some(*edge)
                });
                if let Some(created) = created.filter(|edge| *edge != plan.edge)
                    && interaction_is_settled(surface, cx)
                {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::ConnectionCommitted,
                    )?;
                    self.created_edge = Some(created);
                    self.connection_created = true;
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
