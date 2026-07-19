use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_viewport(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::PanPress => {
                let start = ViewPoint {
                    x: surface_size.width * 0.72,
                    y: surface_size.height * 0.82,
                };
                let end = ViewPoint {
                    x: start.x + 40.0,
                    y: start.y + 30.0,
                };
                self.viewport_before = Some(
                    view.viewport_transform()
                        .map_err(|_| "input_viewport_read_failed")?,
                );
                self.press = Some(start);
                self.release = Some(end);
                self.dispatch(window, cx, mouse_down(MouseButton::Middle, start));
                self.transition(ProbePhase::PanMove);
                Ok(None)
            }
            ProbePhase::PanMove => {
                let point = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_move(MouseButton::Middle, point));
                self.transition(ProbePhase::PanRelease);
                Ok(None)
            }
            ProbePhase::PanRelease => {
                let point = self.release.ok_or("input_pointer_plan_missing")?;
                self.dispatch(window, cx, mouse_up(MouseButton::Middle, point));
                self.transition(ProbePhase::PanWait);
                Ok(None)
            }
            ProbePhase::PanWait => {
                let before = self
                    .viewport_before
                    .ok_or("input_viewport_baseline_missing")?;
                let after = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                if after.pan != before.pan && interaction_is_settled(surface, cx) {
                    self.viewport_panned = true;
                    self.transition(ProbePhase::Zoom);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::Zoom => {
                self.viewport_before = Some(
                    view.viewport_transform()
                        .map_err(|_| "input_viewport_read_failed")?,
                );
                self.dispatch(
                    window,
                    cx,
                    PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: point(
                            px(surface_size.width * 0.5),
                            px(surface_size.height * 0.5),
                        ),
                        delta: ScrollDelta::Lines(point(0.0, -1.0)),
                        modifiers: Default::default(),
                        touch_phase: TouchPhase::Moved,
                    }),
                );
                self.transition(ProbePhase::ZoomWait);
                Ok(None)
            }
            ProbePhase::ZoomWait => {
                let before = self
                    .viewport_before
                    .ok_or("input_viewport_baseline_missing")?;
                let after = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                if after.zoom != before.zoom {
                    self.viewport_zoomed = true;
                    self.transition(ProbePhase::FirstEdgePress);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            _ => unreachable!("viewport scenario received an unrelated phase"),
        }
    }
}
