use std::path::PathBuf;

mod events;
mod evidence;
mod phase;
mod planning;
mod scenario;

use jellyflow::core::{CanvasPoint, EdgeId, PortId};
use jellyflow::runtime::runtime::geometry::EdgePathAnchor;
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{
    CanvasInteractionPhase, EdgeEndpoint, ViewPoint, ViewSize, WeakViewSession,
};
use open_gpui::{
    App, Entity, FocusHandle, MouseButton, PlatformInput, ScrollDelta, ScrollWheelEvent,
    TouchPhase, Window, point, px,
};
use open_gpui_canvas_jellyflow::blackboard::NATIVE_BLACKBOARD_WIDTH;
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

use self::events::{key_down, key_up, mouse_down, mouse_move, mouse_up};
use self::evidence::inspect_current_frame;
use self::phase::ProbePhase;
use self::planning::{
    EdgePlan, NodePlan, ReconnectPlan, blank_surface_point, choose_reconnect,
    dragged_geometry_status, edge_point, free_connection_port, geometry_fits_surface,
    graph_contains_edge, point_changed, point_inside_surface, port_point,
};
use super::{
    SMOKE_INPUT_EVENT_COUNT, SMOKE_WINDOW_HEIGHT, SMOKE_WINDOW_WIDTH, SmokeExpectations,
    SmokeGeometryEvidence, SmokeGeometryInput, SmokeInputEvidence, SmokePaintEvidence,
    canvas_pixel_width, inspect_canvas_paint, inspect_geometry,
};

const MAX_PHASE_WAIT_TICKS: u16 = 180;
const NODE_DRAG_X: f32 = -48.0;
const NODE_DRAG_Y: f32 = 32.0;
const EDGE_SELECTION_PROGRESS: f32 = 0.35;

#[derive(Debug, Default)]
pub(super) struct RealWindowSmokeProbe {
    phase: ProbePhase,
    wait_ticks: u16,
    dispatched_events: u16,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    node: Option<NodePlan>,
    first_edge: Option<EdgePlan>,
    second_edge: Option<EdgePlan>,
    created_edge: Option<EdgeId>,
    reconnect: Option<ReconnectPlan>,
    press: Option<ViewPoint>,
    release: Option<ViewPoint>,
    viewport_before: Option<ViewportTransform>,
    node_dragged: bool,
    viewport_panned: bool,
    viewport_zoomed: bool,
    connection_created: bool,
    keyboard_edge_deleted: bool,
    reconnect_applied: bool,
    search_opened: bool,
    search_focus_moved: bool,
    search_cancelled: bool,
    search_focus_restored: bool,
    search_focus_baseline: Option<FocusHandle>,
    search_render_requested: bool,
    final_frame_validated: bool,
    compact_layout: bool,
}

impl RealWindowSmokeProbe {
    pub(super) fn geometry(&self) -> Option<&SmokeGeometryEvidence> {
        self.geometry.as_ref()
    }

    pub(super) fn paint(&self) -> Option<&SmokePaintEvidence> {
        self.paint.as_ref()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        scenario::advance(self, window, cx, surface, view, expectations, surface_size)
    }

    fn capture(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<(), &'static str> {
        let screenshot_path = std::env::var_os("JELLYFLOW_SMOKE_SCREENSHOT").map(PathBuf::from);
        let (snapshot, transform, geometry, paint) = inspect_current_frame(
            window,
            view,
            expectations,
            surface_size,
            screenshot_path.as_deref(),
        )?;
        let graph = view
            .graph_snapshot()
            .map_err(|_| "input_graph_snapshot_failed")?;
        let mut edges = expectations.edges.iter().filter_map(|edge| {
            graph.graph().edge(edge).map(|model| EdgePlan {
                edge: *edge,
                from: model.from,
                to: model.to,
            })
        });
        self.first_edge = edges.next();
        self.second_edge = edges.next();
        if self.first_edge.is_none() || self.second_edge.is_none() {
            return Err("input_edge_fixture_incomplete");
        }
        self.node = expectations.node_ports.keys().find_map(|node| {
            let model = graph.graph().node(node)?;
            let bounds = snapshot.node(*node)?.bounds();
            let min = transform.screen_point_for_canvas(bounds.origin)?;
            let max = transform.screen_point_for_canvas(CanvasPoint {
                x: bounds.origin.x + bounds.size.width,
                y: bounds.origin.y + bounds.size.height,
            })?;
            let press = ViewPoint {
                x: (min.x + max.x) * 0.5,
                y: min.y + 20.0_f32.min((max.y - min.y) * 0.5),
            };
            let release = ViewPoint {
                x: press.x + NODE_DRAG_X,
                y: press.y + NODE_DRAG_Y,
            };
            point_inside_surface(press, surface_size)
                .then_some(NodePlan {
                    node: *node,
                    original_position: model.pos,
                    original_geometry_origin: bounds.origin,
                    press,
                    release,
                })
                .filter(|_| point_inside_surface(release, surface_size))
        });
        if self.node.is_none() {
            return Err("input_node_fixture_incomplete");
        }
        self.compact_layout =
            (surface_size.width + NATIVE_BLACKBOARD_WIDTH - SMOKE_WINDOW_WIDTH).abs() <= 2.0
                && (surface_size.height - SMOKE_WINDOW_HEIGHT).abs() <= 2.0;
        self.geometry = Some(geometry);
        self.paint = Some(paint);
        Ok(())
    }

    fn refresh_evidence(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<(), &'static str> {
        let (_, _, geometry, paint) =
            inspect_current_frame(window, view, expectations, surface_size, None)?;
        self.geometry = Some(geometry);
        self.paint = Some(paint);
        Ok(())
    }

    fn validate_current_frame(
        &self,
        window: &mut Window,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<(), &'static str> {
        let graph = view
            .graph_snapshot()
            .map_err(|_| "input_graph_snapshot_failed")?;
        let expectations = SmokeExpectations::from_graph(graph.graph());
        let (_, _, _, paint) =
            inspect_current_frame(window, view, &expectations, surface_size, None)?;
        if let Some(reason) = paint.failure_reason() {
            return Err(reason);
        }
        Ok(())
    }

    fn dispatch(&mut self, window: &mut Window, cx: &mut App, event: PlatformInput) {
        window.dispatch_event(event, cx);
        window.refresh();
        self.dispatched_events = self.dispatched_events.saturating_add(1);
    }

    fn transition(&mut self, phase: ProbePhase) {
        self.phase = phase;
        self.wait_ticks = 0;
    }

    fn wait_or_timeout(&mut self) -> Result<Option<SmokeInputEvidence>, &'static str> {
        self.wait_with_reason(self.phase.timeout_reason())
    }

    fn wait_with_reason(
        &mut self,
        reason: &'static str,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        self.wait_ticks = self.wait_ticks.saturating_add(1);
        if self.wait_ticks > MAX_PHASE_WAIT_TICKS {
            Err(reason)
        } else {
            Ok(None)
        }
    }

    fn complete(&self) -> Result<Option<SmokeInputEvidence>, &'static str> {
        if self.dispatched_events != SMOKE_INPUT_EVENT_COUNT {
            return Err("input_dispatch_incomplete");
        }
        Ok(Some(SmokeInputEvidence {
            dispatched_events: self.dispatched_events,
            node_dragged: self.node_dragged,
            viewport_panned: self.viewport_panned,
            viewport_zoomed: self.viewport_zoomed,
            connection_created: self.connection_created,
            keyboard_edge_deleted: self.keyboard_edge_deleted,
            reconnect_applied: self.reconnect_applied,
            search_opened: self.search_opened,
            search_focus_moved: self.search_focus_moved,
            search_cancelled: self.search_cancelled,
            search_focus_restored: self.search_focus_restored,
            final_frame_validated: self.final_frame_validated,
            compact_layout: self.compact_layout,
        }))
    }
}

fn interaction_presentation(
    surface: &Entity<NativeGraphSurface>,
    cx: &App,
) -> jellyflow::runtime::CanvasInteractionPresentation {
    let host = surface.read(cx).host().clone();
    host.read(cx).interaction_presentation()
}

fn interaction_is_settled(surface: &Entity<NativeGraphSurface>, cx: &App) -> bool {
    !matches!(
        interaction_presentation(surface, cx).phase(),
        CanvasInteractionPhase::Armed
            | CanvasInteractionPhase::NodeDrag
            | CanvasInteractionPhase::ConnectionDrag
            | CanvasInteractionPhase::ConnectionSearch
            | CanvasInteractionPhase::Committing
    )
}

fn connection_candidate_failure(
    surface: &Entity<NativeGraphSurface>,
    cx: &App,
    expected: PortId,
) -> Option<&'static str> {
    let presentation = interaction_presentation(surface, cx);
    match presentation.phase() {
        CanvasInteractionPhase::ConnectionDrag => {}
        CanvasInteractionPhase::Idle => return Some("input_connection_drag_idle"),
        CanvasInteractionPhase::Armed => return Some("input_connection_drag_armed"),
        CanvasInteractionPhase::EdgeActive => return Some("input_connection_edge_active"),
        _ => return Some("input_connection_drag_inactive"),
    }
    let Some(connection) = presentation.connection() else {
        return Some("input_connection_presentation_missing");
    };
    let Some(target) = connection.probe().target() else {
        return Some("input_connection_candidate_missing");
    };
    if target.port() != expected {
        return Some("input_connection_candidate_mismatch");
    }
    (!connection.probe().is_valid()).then_some("input_connection_candidate_invalid")
}
