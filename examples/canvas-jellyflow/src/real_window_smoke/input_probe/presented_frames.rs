use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use jellyflow_open_gpui::host::{
    OpenGpuiSceneHost, OpenGpuiSmokeNodeBounds, OpenGpuiSmokePresentedScene, OpenGpuiSmokeRoute,
};
use open_gpui::{App, Entity, Window};
use open_gpui_canvas_jellyflow::smoke_receipt::{
    SmokeFrameStageV1, SmokeNodeBoundsV1, SmokePointV1, SmokePresentedFrameV1,
    SmokePreviewTargetV1, SmokeRectV1, SmokeRouteV1, SmokeViewportV1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingRouteSwap {
    Connection,
    Reconnect,
}

type PresentedFrameAction = Box<dyn FnOnce(&mut Window, &mut App)>;

#[derive(Default)]
struct PresentedFrameRecorderState {
    active: bool,
    final_requested: bool,
    final_presented: bool,
    node_drag_pending: bool,
    pending_route_swap: Option<PendingRouteSwap>,
    next_presented_sequence: u64,
    last_render_sequence: Option<u64>,
    frames: Vec<SmokePresentedFrameV1>,
    failure: Option<&'static str>,
    next_action: Option<(SmokeFrameStageV1, PresentedFrameAction)>,
    action_completed: bool,
}

#[derive(Clone, Default)]
pub(super) struct PresentedFrameRecorder {
    state: Rc<RefCell<PresentedFrameRecorderState>>,
}

impl fmt::Debug for PresentedFrameRecorder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.borrow();
        formatter
            .debug_struct("PresentedFrameRecorder")
            .field("active", &state.active)
            .field("frames", &state.frames.len())
            .field("failure", &state.failure)
            .field("action_pending", &state.next_action.is_some())
            .field("action_completed", &state.action_completed)
            .finish()
    }
}

impl PresentedFrameRecorder {
    pub(super) fn start(
        &self,
        window: &Window,
        host: Entity<OpenGpuiSceneHost>,
    ) -> Result<(), &'static str> {
        {
            let mut state = self.state.borrow_mut();
            if state.active || !state.frames.is_empty() {
                return Err("presented_frame_recorder_already_started");
            }
            state.active = true;
        }
        schedule_presented_frame(window, host, Rc::clone(&self.state));
        Ok(())
    }

    pub(super) fn request_final(&self) -> Result<(), &'static str> {
        let mut state = self.state.borrow_mut();
        if !state.active || state.final_requested {
            return Err("presented_frame_final_request_invalid");
        }
        state.final_requested = true;
        Ok(())
    }

    pub(super) fn final_presented(&self) -> bool {
        self.state.borrow().final_presented
    }

    pub(super) fn is_active(&self) -> bool {
        self.state.borrow().active
    }

    pub(super) fn failure(&self) -> Option<&'static str> {
        self.state.borrow().failure
    }

    pub(super) fn frames(&self) -> Vec<SmokePresentedFrameV1> {
        self.state.borrow().frames.clone()
    }

    pub(super) fn observed_stage(&self, stage: SmokeFrameStageV1) -> bool {
        self.state
            .borrow()
            .frames
            .iter()
            .any(|frame| frame.stage == stage)
    }

    pub(super) fn after_next_stage(
        &self,
        stage: SmokeFrameStageV1,
        action: impl FnOnce(&mut Window, &mut App) + 'static,
    ) -> Result<(), &'static str> {
        let mut state = self.state.borrow_mut();
        if !state.active || state.next_action.is_some() || state.action_completed {
            return Err("presented_frame_action_invalid");
        }
        state.next_action = Some((stage, Box::new(action)));
        Ok(())
    }

    pub(super) fn action_completed(&self) -> bool {
        self.state.borrow().action_completed
    }

    pub(super) fn record_lod_frame(
        &self,
        scene: OpenGpuiSmokePresentedScene,
    ) -> Result<(), &'static str> {
        let mut state = self.state.borrow_mut();
        if state.active || state.final_requested || state.final_presented {
            return Err("presented_frame_lod_recording_invalid");
        }
        state.record(scene, SmokeFrameStageV1::Lod)
    }
}

impl PresentedFrameRecorderState {
    fn observe(&mut self, scene: OpenGpuiSmokePresentedScene) -> Option<SmokeFrameStageV1> {
        if !self.active || self.failure.is_some() {
            return None;
        }
        let stage = self.classify(&scene);
        if let Err(reason) = self.record(scene, stage) {
            self.fail(reason);
            return None;
        }
        if stage == SmokeFrameStageV1::Final {
            self.final_presented = true;
            self.active = false;
        }
        Some(stage)
    }

    fn classify(&mut self, scene: &OpenGpuiSmokePresentedScene) -> SmokeFrameStageV1 {
        if self.final_requested {
            return SmokeFrameStageV1::Final;
        }
        if scene.node_drag_active() {
            self.node_drag_pending = true;
            return SmokeFrameStageV1::NodeDragPending;
        }
        if self.node_drag_pending {
            self.node_drag_pending = false;
            return SmokeFrameStageV1::NodeDragAuthoritative;
        }

        if scene.preview_route().is_none() {
            if let Some(pending) = self.pending_route_swap.take() {
                return match pending {
                    PendingRouteSwap::Connection => SmokeFrameStageV1::ConnectionAuthoritative,
                    PendingRouteSwap::Reconnect => SmokeFrameStageV1::ReconnectAuthoritative,
                };
            }
        }
        if scene.phase() == jellyflow_runtime::interaction::CanvasInteractionPhase::ConnectionSearch
        {
            return SmokeFrameStageV1::Search;
        }
        if scene.preview_route().is_some() {
            let reconnect = scene.suppressed_edge().is_some();
            if scene.preview_target().is_some() {
                let pending = if reconnect {
                    PendingRouteSwap::Reconnect
                } else {
                    PendingRouteSwap::Connection
                };
                self.pending_route_swap = Some(pending);
                return match pending {
                    PendingRouteSwap::Connection => SmokeFrameStageV1::ConnectionPending,
                    PendingRouteSwap::Reconnect => SmokeFrameStageV1::ReconnectPending,
                };
            }
            return if reconnect {
                SmokeFrameStageV1::ReconnectExploring
            } else {
                SmokeFrameStageV1::ConnectionExploring
            };
        }
        SmokeFrameStageV1::Interaction
    }

    fn record(
        &mut self,
        scene: OpenGpuiSmokePresentedScene,
        stage: SmokeFrameStageV1,
    ) -> Result<(), &'static str> {
        let render_sequence = scene.render_sequence();
        if self
            .last_render_sequence
            .is_some_and(|previous| render_sequence < previous)
        {
            return Err("presented_frame_render_sequence_reversed");
        }
        let presented_sequence = self.take_presented_sequence()?;
        let frame = scene_into_receipt(presented_sequence, stage, scene)?;
        self.last_render_sequence = Some(render_sequence);
        self.frames.push(frame);
        Ok(())
    }

    fn take_presented_sequence(&mut self) -> Result<u64, &'static str> {
        let presented_sequence = self.next_presented_sequence;
        self.next_presented_sequence = presented_sequence
            .checked_add(1)
            .ok_or("presented_frame_sequence_exhausted")?;
        Ok(presented_sequence)
    }

    fn fail(&mut self, reason: &'static str) {
        self.failure = Some(reason);
        self.active = false;
    }

    fn take_ready_action(
        &mut self,
        observed_stage: Option<SmokeFrameStageV1>,
    ) -> Option<PresentedFrameAction> {
        let Some(observed_stage) = observed_stage else {
            return None;
        };
        let expected = self.next_action.as_ref().map(|(expected, _)| *expected)?;
        if observed_stage != expected {
            self.fail("presented_frame_action_stage_mismatch");
            return None;
        }
        self.next_action.take().map(|(_, action)| action)
    }
}

fn schedule_presented_frame(
    window: &Window,
    host: Entity<OpenGpuiSceneHost>,
    state: Rc<RefCell<PresentedFrameRecorderState>>,
) {
    window.on_next_frame(move |window, cx| {
        let scene = host.read(cx).smoke_presented_scene_snapshot();
        let action = {
            let mut state = state.borrow_mut();
            let observed_stage = match scene {
                Some(scene) => state.observe(scene),
                None => {
                    state.fail("presented_frame_scene_snapshot_missing");
                    None
                }
            };
            state.take_ready_action(observed_stage)
        };
        if let Some(action) = action {
            action(window, cx);
            state.borrow_mut().action_completed = true;
        }
        if state.borrow().active {
            schedule_presented_frame(window, host, Rc::clone(&state));
        }
    });
}

fn scene_into_receipt(
    presented_sequence: u64,
    stage: SmokeFrameStageV1,
    scene: OpenGpuiSmokePresentedScene,
) -> Result<SmokePresentedFrameV1, &'static str> {
    let node_bounds = scene
        .node_bounds()
        .iter()
        .copied()
        .map(receipt_node_bounds)
        .collect::<Result<Vec<_>, _>>()?;
    let chrome_bounds = scene
        .chrome_bounds()
        .iter()
        .copied()
        .map(receipt_node_bounds)
        .collect::<Result<Vec<_>, _>>()?;
    let authoritative_routes = scene
        .authoritative_routes()
        .iter()
        .copied()
        .map(receipt_route)
        .collect::<Result<Vec<_>, _>>()?;
    let preview_route = scene
        .preview_route()
        .map(|preview| -> Result<SmokeRouteV1, &'static str> {
            Ok(SmokeRouteV1 {
                edge_id: "preview".to_owned(),
                source: receipt_point(preview.source())?,
                target: receipt_point(preview.target())?,
            })
        })
        .transpose()?;
    let preview_target = scene
        .preview_target()
        .map(|target| -> Result<SmokePreviewTargetV1, &'static str> {
            Ok(SmokePreviewTargetV1 {
                port_id: target.port().0.to_string(),
                anchor: receipt_point(target.anchor())?,
            })
        })
        .transpose()?;
    Ok(SmokePresentedFrameV1 {
        presented_sequence,
        render_sequence: scene.render_sequence(),
        stage,
        phase: format!("{:?}", scene.phase()),
        graph_revision: scene.graph_revision(),
        viewport: SmokeViewportV1 {
            pan: SmokePointV1 {
                x: scene.viewport().pan.x,
                y: scene.viewport().pan.y,
            },
            zoom: scene.viewport().zoom,
        },
        node_bounds,
        chrome_bounds,
        preview_route,
        preview_target,
        suppressed_edge_id: scene.suppressed_edge().map(|edge| edge.0.to_string()),
        authoritative_routes,
    })
}

fn receipt_node_bounds(entry: OpenGpuiSmokeNodeBounds) -> Result<SmokeNodeBoundsV1, &'static str> {
    let bounds = entry.bounds();
    if !bounds.is_positive_finite() {
        return Err("presented_frame_node_bounds_invalid");
    }
    Ok(SmokeNodeBoundsV1 {
        node_id: entry.node().0.to_string(),
        bounds: SmokeRectV1 {
            x: bounds.origin.x,
            y: bounds.origin.y,
            width: bounds.size.width,
            height: bounds.size.height,
        },
    })
}

fn receipt_route(route: OpenGpuiSmokeRoute) -> Result<SmokeRouteV1, &'static str> {
    Ok(SmokeRouteV1 {
        edge_id: route.edge().0.to_string(),
        source: receipt_point(route.source())?,
        target: receipt_point(route.target())?,
    })
}

fn receipt_point(
    point: jellyflow_runtime::geometry::ViewPoint,
) -> Result<SmokePointV1, &'static str> {
    point
        .is_finite()
        .then_some(SmokePointV1 {
            x: point.x,
            y: point.y,
        })
        .ok_or("presented_frame_point_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presented_sequence_advances_at_the_native_frame_callback_boundary() {
        let mut state = PresentedFrameRecorderState::default();

        assert_eq!(state.take_presented_sequence(), Ok(0));
        assert_eq!(state.take_presented_sequence(), Ok(1));
        assert_eq!(state.take_presented_sequence(), Ok(2));
    }
}
