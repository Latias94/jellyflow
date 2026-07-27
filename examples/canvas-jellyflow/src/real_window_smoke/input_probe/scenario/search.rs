use super::super::*;

impl RealWindowSmokeProbe {
    pub(super) fn advance_search(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        match self.phase {
            ProbePhase::SearchPress => {
                let Some(source) = free_connection_port(view)? else {
                    return self.wait_or_timeout();
                };
                let Some(source_point) = visible_port_point(view, source, surface_size)? else {
                    window.refresh();
                    return self.wait_or_timeout();
                };
                let Some(blank) = blank_surface_point(view, surface_size)? else {
                    return Err("input_blank_point_missing");
                };
                let interaction = self.interaction_mut()?;
                interaction.press = Some(source_point);
                interaction.release = Some(blank);
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_down(MouseButton::Left, source_point),
                )?;
                self.transition(ProbePhase::SearchMove);
                Ok(None)
            }
            ProbePhase::SearchMove => {
                let blank = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    mouse_move(MouseButton::Left, blank),
                )?;
                self.transition(ProbePhase::SearchWaitDrag);
                Ok(None)
            }
            ProbePhase::SearchWaitDrag => {
                let presentation = interaction_presentation(surface, cx);
                let blank = presentation.phase() == CanvasInteractionPhase::ConnectionDrag
                    && presentation
                        .connection()
                        .is_some_and(|connection| connection.probe().target().is_none());
                if blank {
                    self.transition(ProbePhase::SearchRelease);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::SearchRelease => {
                let blank = self
                    .interaction()?
                    .release
                    .ok_or("input_pointer_plan_missing")?;
                self.dispatch_at_target_zoom(window, cx, view, mouse_up(MouseButton::Left, blank))?;
                self.interaction_mut()?.search_focus_baseline = window.focused(cx);
                self.transition(ProbePhase::SearchWaitOpen);
                Ok(None)
            }
            ProbePhase::SearchWaitOpen => {
                let host = surface.read(cx).host().clone();
                let (search_active, request_present) = {
                    let host = host.read(cx);
                    (
                        host.interaction_presentation().phase()
                            == CanvasInteractionPhase::ConnectionSearch,
                        host.connection_search_request().is_some(),
                    )
                };
                if search_active && request_present && !self.interaction()?.search_render_requested
                {
                    surface.update(cx, |_, cx| cx.notify());
                    window.refresh();
                    window
                        .render_to_image()
                        .map_err(|_| "input_search_render_failed")?;
                    self.interaction_mut()?.search_render_requested = true;
                }
                let search_has_focus = surface.read(cx).connection_search_has_focus(window, cx);
                if search_active && request_present && search_has_focus {
                    self.capture_checkpoint(
                        window,
                        view,
                        surface_size,
                        SmokeCaptureState::SearchOpen,
                    )?;
                    let interaction = self.interaction_mut()?;
                    interaction.search_opened = true;
                    interaction.search_focus_moved = true;
                    self.transition(ProbePhase::SearchEscape);
                    Ok(None)
                } else {
                    self.wait_or_timeout()
                }
            }
            ProbePhase::SearchEscape => {
                self.dispatch_at_target_zoom(window, cx, view, key_down("escape")?)?;
                self.transition(ProbePhase::SearchWaitClosed);
                Ok(None)
            }
            ProbePhase::SearchWaitClosed => {
                let host = surface.read(cx).host().clone();
                let (request_closed, interaction_closed) = {
                    let host = host.read(cx);
                    (
                        host.connection_search_request().is_none(),
                        !matches!(
                            host.interaction_presentation().phase(),
                            CanvasInteractionPhase::ConnectionSearch
                                | CanvasInteractionPhase::Committing
                        ),
                    )
                };
                let surface = surface.read(cx);
                let focus_restored = !surface.connection_search_has_focus(window, cx)
                    && (window.focused(cx) == self.interaction()?.search_focus_baseline
                        || surface.graph_has_focus(window, cx));
                if request_closed && interaction_closed && focus_restored {
                    let interaction = self.interaction_mut()?;
                    interaction.search_cancelled = true;
                    interaction.search_focus_restored = true;
                    self.capture_checkpoint(window, view, surface_size, SmokeCaptureState::Final)?;
                    self.transition(ProbePhase::Complete);
                    self.complete()
                } else {
                    let reason = if !request_closed {
                        "input_search_request_not_closed"
                    } else if !interaction_closed {
                        "input_search_interaction_not_closed"
                    } else {
                        "input_search_focus_not_restored_timeout"
                    };
                    self.wait_with_reason(reason)
                }
            }
            _ => unreachable!("search scenario received an unrelated phase"),
        }
    }
}
