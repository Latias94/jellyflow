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
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                let interaction = self.interaction_mut()?;
                interaction.viewport_before = Some(transform);
                interaction.press = Some(start);
                interaction.release = Some(end);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Middle, start),
                )?;
                self.transition(ProbePhase::PanMove);
                Ok(None)
            }
            ProbePhase::PanMove => {
                let point = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Middle, point),
                )?;
                self.transition(ProbePhase::PanRelease);
                Ok(None)
            }
            ProbePhase::PanRelease => {
                let point = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_up(MouseButton::Middle, point),
                )?;
                self.transition(ProbePhase::PanWait);
                Ok(None)
            }
            ProbePhase::PanWait => {
                let before = self
                    .interaction()?
                    .viewport_before
                    .ok_or("input_viewport_baseline_missing")?;
                let after = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                if after.pan != before.pan && interaction_is_settled(surface, cx) {
                    self.transition(ProbePhase::Zoom);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::Zoom => {
                let zoom_point = blank_surface_point(view, surface_size)?
                    .ok_or("input_blank_surface_point_missing")?;
                let transform = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                self.interaction_mut()?.viewport_before = Some(transform);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    PlatformInput::Pinch(PinchEvent {
                        position: point(px(zoom_point.x), px(zoom_point.y)),
                        delta: 0.1,
                        modifiers: Default::default(),
                        phase: TouchPhase::Moved,
                    }),
                )?;
                self.transition(ProbePhase::ZoomWait);
                Ok(None)
            }
            ProbePhase::ZoomWait => {
                let before = self
                    .interaction()?
                    .viewport_before
                    .ok_or("input_viewport_baseline_missing")?;
                let after = view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?;
                if after.zoom != before.zoom {
                    self.transition(ProbePhase::ZoomRestore);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::ZoomRestore => {
                set_zoom_preserving_surface_center(view, surface_size, self.config.zoom().value())?;
                window.refresh();
                self.transition(ProbePhase::ZoomRestoreWait);
                Ok(None)
            }
            ProbePhase::ZoomRestoreWait => {
                if self.verify_target_zoom(view).is_ok() {
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
