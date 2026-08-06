use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use super::super::presented_frame::{
    SmokeFrameStage, SmokeNodeBounds, SmokePortGeometry, SmokePresentedFrame, SmokePreviewRoute,
    SmokePreviewTarget, SmokeRoute, validate_port_geometries,
};
use jellyflow_open_gpui::host::{
    OpenGpuiSceneHost, OpenGpuiSmokeNodeBounds, OpenGpuiSmokePortGeometry,
    OpenGpuiSmokePresentedScene, OpenGpuiSmokeRoute,
};
use open_gpui::{App, Entity, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingRouteSwap {
    Connection,
    Reconnect,
}

type PresentedFrameAction = Box<dyn FnOnce(&mut Window, &mut App)>;

#[derive(Default)]
struct PresentedFrameRecorderState {
    active: bool,
    primed: bool,
    final_requested: bool,
    final_presented: bool,
    node_drag_pending: bool,
    pending_route_swap: Option<PendingRouteSwap>,
    last_render_sequence: Option<u64>,
    frames: Vec<SmokePresentedFrame>,
    failure: Option<&'static str>,
    next_action: Option<(SmokeFrameStage, PresentedFrameAction)>,
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
            .field("primed", &state.primed)
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
        cx: &mut App,
        host: Entity<OpenGpuiSceneHost>,
    ) -> Result<(), &'static str> {
        {
            let mut state = self.state.borrow_mut();
            if state.active || !state.frames.is_empty() {
                return Err("presented_frame_recorder_already_started");
            }
            let baseline = host.update(cx, |host, _| host.drain_smoke_presented_scenes());
            state.last_render_sequence = baseline.last().map(|scene| scene.render_sequence());
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

    pub(super) fn is_primed(&self) -> bool {
        self.state.borrow().primed
    }

    pub(super) fn failure(&self) -> Option<&'static str> {
        self.state.borrow().failure
    }

    pub(super) fn frames(&self) -> Vec<SmokePresentedFrame> {
        self.state.borrow().frames.clone()
    }

    pub(super) fn observed_stage(&self, stage: SmokeFrameStage) -> bool {
        self.state
            .borrow()
            .frames
            .iter()
            .any(|frame| frame.stage == stage)
    }

    pub(super) fn after_next_stage(
        &self,
        stage: SmokeFrameStage,
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

    pub(super) fn poll_lod_frames(
        &self,
        mut scenes: Vec<OpenGpuiSmokePresentedScene>,
        expected_zoom: f32,
    ) -> Result<bool, &'static str> {
        let mut state = self.state.borrow_mut();
        if state.active || state.final_requested || state.final_presented {
            return Err("presented_frame_lod_recording_invalid");
        }
        if scenes.is_empty() {
            return Ok(false);
        }
        let ready = scenes
            .last()
            .is_some_and(|scene| (scene.viewport().zoom - expected_zoom).abs() <= 0.001);
        if state.last_render_sequence.is_none() {
            let scene = scenes
                .pop()
                .expect("a non-empty native LOD scene drain has a latest frame");
            state.record(scene, SmokeFrameStage::Lod)?;
            return Ok(ready);
        }
        for scene in scenes {
            state.record(scene, SmokeFrameStage::Lod)?;
        }
        Ok(ready)
    }
}

impl PresentedFrameRecorderState {
    fn observe(&mut self, scene: OpenGpuiSmokePresentedScene) -> Option<SmokeFrameStage> {
        if !self.active || self.failure.is_some() {
            return None;
        }
        let stage = self.classify(&scene);
        if let Err(reason) = self.record(scene, stage) {
            self.fail(reason);
            return None;
        }
        if stage == SmokeFrameStage::Final {
            self.final_presented = true;
            self.active = false;
        }
        Some(stage)
    }

    fn classify(&mut self, scene: &OpenGpuiSmokePresentedScene) -> SmokeFrameStage {
        if self.final_requested {
            return SmokeFrameStage::Final;
        }
        if !scene.node_drag_nodes().is_empty() {
            self.node_drag_pending = true;
            return SmokeFrameStage::NodeDragPending;
        }
        if self.node_drag_pending {
            self.node_drag_pending = false;
            return SmokeFrameStage::NodeDragAuthoritative;
        }

        if scene.preview_route().is_none()
            && let Some(pending) = self.pending_route_swap.take()
        {
            return match pending {
                PendingRouteSwap::Connection => SmokeFrameStage::ConnectionAuthoritative,
                PendingRouteSwap::Reconnect => SmokeFrameStage::ReconnectAuthoritative,
            };
        }
        if scene.phase() == jellyflow_runtime::interaction::CanvasInteractionPhase::ConnectionSearch
        {
            return SmokeFrameStage::Search;
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
                    PendingRouteSwap::Connection => SmokeFrameStage::ConnectionPending,
                    PendingRouteSwap::Reconnect => SmokeFrameStage::ReconnectPending,
                };
            }
            return if reconnect {
                SmokeFrameStage::ReconnectExploring
            } else {
                SmokeFrameStage::ConnectionExploring
            };
        }
        SmokeFrameStage::Interaction
    }

    fn record(
        &mut self,
        scene: OpenGpuiSmokePresentedScene,
        stage: SmokeFrameStage,
    ) -> Result<(), &'static str> {
        let render_sequence = scene.render_sequence();
        if self.last_render_sequence.is_some_and(|previous| {
            previous
                .checked_add(1)
                .is_none_or(|next| render_sequence != next)
        }) {
            return Err("presented_frame_render_sequence_not_contiguous");
        }
        let frame = scene_into_frame(stage, scene)?;
        self.last_render_sequence = Some(render_sequence);
        self.frames.push(frame);
        Ok(())
    }

    fn fail(&mut self, reason: &'static str) {
        self.failure = Some(reason);
        self.active = false;
    }

    fn take_ready_action(
        &mut self,
        observed_stage: Option<SmokeFrameStage>,
    ) -> Option<PresentedFrameAction> {
        let observed_stage = observed_stage?;
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
        let deferred_host = host.clone();
        let deferred_state = Rc::clone(&state);
        window.defer(cx, move |window, cx| {
            let mut scenes =
                deferred_host.update(cx, |host, _| host.drain_smoke_presented_scenes());
            let action = {
                let mut state = deferred_state.borrow_mut();
                let observed_stage = if !state.primed {
                    if let Some(scene) = scenes.last() {
                        state.last_render_sequence = Some(scene.render_sequence());
                    }
                    state.primed = true;
                    None
                } else if scenes.is_empty() {
                    None
                } else if scenes.len() == 1 {
                    let scene = scenes.pop().expect("one drained native scene was counted");
                    state.observe(scene)
                } else {
                    eprintln!(
                        "JELLYFLOW_REAL_WINDOW_SMOKE_FRAME_DRAIN count={} sequences={:?} phases={:?}",
                        scenes.len(),
                        scenes
                            .iter()
                            .map(OpenGpuiSmokePresentedScene::render_sequence)
                            .collect::<Vec<_>>(),
                        scenes
                            .iter()
                            .map(OpenGpuiSmokePresentedScene::phase)
                            .collect::<Vec<_>>(),
                    );
                    state.fail("presented_frame_sampling_gap");
                    None
                };
                state.take_ready_action(observed_stage)
            };
            if let Some(action) = action {
                action(window, cx);
                deferred_state.borrow_mut().action_completed = true;
            }
            if deferred_state.borrow().active {
                schedule_presented_frame(window, deferred_host, Rc::clone(&deferred_state));
            }
        });
    });
}

fn scene_into_frame(
    stage: SmokeFrameStage,
    scene: OpenGpuiSmokePresentedScene,
) -> Result<SmokePresentedFrame, &'static str> {
    let node_bounds = scene
        .node_bounds()
        .iter()
        .copied()
        .map(presented_node_bounds)
        .collect::<Result<Vec<_>, _>>()?;
    let authoritative_routes = scene
        .authoritative_routes()
        .iter()
        .copied()
        .map(presented_route)
        .collect::<Result<Vec<_>, _>>()?;
    let port_geometries = scene
        .port_geometries()
        .iter()
        .copied()
        .map(presented_port_geometry)
        .collect::<Vec<_>>();
    validate_port_geometries(&port_geometries)?;
    let preview_route = scene
        .preview_route()
        .map(|preview| -> Result<SmokePreviewRoute, &'static str> {
            Ok(SmokePreviewRoute {
                source: presented_point(preview.source())?,
                target: presented_point(preview.target())?,
            })
        })
        .transpose()?;
    let preview_target = scene
        .preview_target()
        .map(|target| -> Result<SmokePreviewTarget, &'static str> {
            Ok(SmokePreviewTarget {
                port: target.port(),
                anchor: presented_point(target.anchor())?,
            })
        })
        .transpose()?;
    Ok(SmokePresentedFrame {
        render_sequence: scene.render_sequence(),
        stage,
        phase: scene.phase(),
        graph_revision: scene.graph_revision(),
        viewport: scene.viewport(),
        node_bounds,
        port_geometries,
        node_drag_nodes: scene.node_drag_nodes().to_vec(),
        preview_route,
        preview_target,
        suppressed_edge: scene.suppressed_edge(),
        authoritative_routes,
    })
}

fn presented_port_geometry(entry: OpenGpuiSmokePortGeometry) -> SmokePortGeometry {
    SmokePortGeometry {
        node: entry.node(),
        port: entry.port(),
        visual_bounds: entry.visual_bounds(),
        hit_bounds: entry.hit_bounds(),
        interaction_bounds: entry.interaction_bounds(),
        route_anchor: entry.route_anchor(),
    }
}

fn presented_node_bounds(entry: OpenGpuiSmokeNodeBounds) -> Result<SmokeNodeBounds, &'static str> {
    let bounds = entry.bounds();
    if !bounds.is_positive_finite() {
        return Err("presented_frame_node_bounds_invalid");
    }
    Ok(SmokeNodeBounds {
        node: entry.node(),
        bounds,
        selected: entry.selected(),
        chrome_expected: entry.chrome_expected(),
        chrome_bounds: entry.chrome_bounds(),
    })
}

fn presented_route(route: OpenGpuiSmokeRoute) -> Result<SmokeRoute, &'static str> {
    Ok(SmokeRoute {
        edge: route.edge(),
        source: presented_point(route.source())?,
        target: presented_point(route.target())?,
    })
}

fn presented_point(
    point: jellyflow_runtime::geometry::ViewPoint,
) -> Result<jellyflow_runtime::geometry::ViewPoint, &'static str> {
    point
        .is_finite()
        .then_some(point)
        .ok_or("presented_frame_point_invalid")
}
