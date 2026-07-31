use std::cell::Cell;
use std::fmt;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use jellyflow_core::{Edge, NodeId};
use jellyflow_runtime::geometry::ViewSize;
use jellyflow_runtime::geometry::{
    EdgeEndpointPosition, EdgePath, EdgePathAnchor, EdgePosition, resolve_edge_route_path,
};
use jellyflow_runtime::graph::EdgeEndpoint;
use jellyflow_runtime::interaction::CanvasInteractionPresentation;
use jellyflow_runtime::view::{GeometrySnapshot, WeakViewSession};
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::{App, Entity, Task, Window};
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

use super::super::{
    MIN_CANVAS_DISTINCT_COLORS, MIN_CANVAS_NON_BACKGROUND_PIXELS, MIN_WIRE_VISIBLE_SAMPLES,
    SmokeExpectations, SmokeGeometryEvidence, SmokeGeometryInput, SmokePaintEvidence,
    SmokePaintPalette, SmokePathSampleInput, SmokeSelectionChromeEvidence, SmokeWirePaintEvidence,
    WIRE_PATH_SAMPLE_COUNT, canvas_pixel_width, inspect_canvas_framebuffer, inspect_canvas_paint,
    inspect_selection_chrome, inspect_visible_geometry, inspect_wire_corridor,
    project_canvas_sample,
};

const RENDERED_FRAME_WAKE_ATTEMPTS: u16 = 384;
const RENDERED_FRAME_WAKE_INTERVAL: Duration = Duration::from_millis(8);

/// Waits for the native window to replace the frame that was current when a
/// semantic interaction update became observable.
///
/// `render_to_image` intentionally captures the last rendered scene, not a
/// speculative redraw. A connection query can therefore settle between two
/// frames. The smoke probe must only compare pixels after Open GPUI completes
/// one more frame callback for the settled state.
pub(super) struct RenderedFrameFence {
    completed: Rc<Cell<bool>>,
    armed: bool,
    wake: Option<Task<()>>,
}

impl fmt::Debug for RenderedFrameFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderedFrameFence")
            .field("completed", &self.completed.get())
            .field("armed", &self.armed)
            .field("wake_active", &self.wake.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderedFrameFencePoll {
    Rendered,
    Armed,
    Waiting,
}

impl Default for RenderedFrameFence {
    fn default() -> Self {
        Self {
            completed: Rc::new(Cell::new(false)),
            armed: false,
            wake: None,
        }
    }
}

impl RenderedFrameFence {
    pub(super) fn is_pending(&self) -> bool {
        self.armed
    }

    pub(super) fn poll(&mut self, window: &mut Window, cx: &mut App) -> RenderedFrameFencePoll {
        if self.armed && self.completed.replace(false) {
            self.armed = false;
            self.wake = None;
            return RenderedFrameFencePoll::Rendered;
        }
        if !self.armed {
            self.armed = true;
            let callback_completed = Rc::clone(&self.completed);
            window.on_next_frame(move |_, _| callback_completed.set(true));
            let wake_completed = Rc::clone(&self.completed);
            self.wake = Some(window.spawn(cx, async move |cx| {
                for _ in 0..RENDERED_FRAME_WAKE_ATTEMPTS {
                    cx.background_executor()
                        .timer(RENDERED_FRAME_WAKE_INTERVAL)
                        .await;
                    if wake_completed.get() {
                        return;
                    }
                    if cx
                        .update(|window, _| {
                            jellyflow_open_gpui::host::request_native_test_frame(window);
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }));
            return RenderedFrameFencePoll::Armed;
        }
        RenderedFrameFencePoll::Waiting
    }
}

pub(super) fn capture_visual_frame(
    window: &mut Window,
    surface_size: ViewSize,
    screenshot_path: Option<&Path>,
    canvas_background: [u8; 4],
) -> Result<(), &'static str> {
    let image = window
        .render_to_image()
        .map_err(|_| "paint_capture_failed")?;
    if let Some(path) = screenshot_path {
        image
            .save(path)
            .map_err(|_| "paint_screenshot_write_failed")?;
    }
    let canvas_width = canvas_pixel_width(image.width(), surface_size)?;
    let (non_background_pixels, distinct_colors) = inspect_canvas_framebuffer(
        image.width(),
        image.height(),
        canvas_width,
        image.as_raw(),
        canvas_background,
    )?;
    if non_background_pixels < MIN_CANVAS_NON_BACKGROUND_PIXELS {
        return Err("paint_blank_canvas");
    }
    if distinct_colors < MIN_CANVAS_DISTINCT_COLORS {
        return Err("paint_insufficient_colors");
    }
    Ok(())
}

pub(super) fn inspect_current_frame(
    window: &mut Window,
    surface: &Entity<NativeGraphSurface>,
    cx: &App,
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    surface_size: ViewSize,
    screenshot_path: Option<&Path>,
    selected_node: Option<NodeId>,
    paint_palette: SmokePaintPalette,
) -> Result<
    (
        GeometrySnapshot,
        ViewportTransform,
        SmokeGeometryEvidence,
        SmokePaintEvidence,
        Option<SmokeSelectionChromeEvidence>,
    ),
    &'static str,
> {
    let image = window
        .render_to_image()
        .map_err(|_| "paint_capture_failed")?;
    // Frame feedback publishes geometry while the native frame is rendered.
    // Read it after capture so pixel and semantic evidence describe that frame.
    let snapshot = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let graph = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    if let Some(path) = screenshot_path {
        image
            .save(path)
            .map_err(|_| "paint_screenshot_write_failed")?;
    }
    let canvas_width = canvas_pixel_width(image.width(), surface_size)?;
    let footprints = surface.read(cx).host().read(cx).smoke_node_footprints();
    let geometry_input = SmokeGeometryInput::from_snapshot(
        expectations,
        &snapshot,
        graph.graph(),
        transform,
        &footprints,
    );
    let geometry = inspect_visible_geometry(
        expectations,
        &geometry_input,
        transform,
        surface_size,
        image.width(),
        image.height(),
        canvas_width,
    )?;
    let paint = inspect_canvas_paint(
        image.width(),
        image.height(),
        image.as_raw(),
        &geometry,
        paint_palette.canvas_background,
    )?;
    let selection_chrome = selected_node
        .map(|node| {
            let bounds = snapshot
                .node(node)
                .filter(|geometry| geometry.is_fresh())
                .ok_or("paint_selection_node_geometry_missing")?
                .bounds();
            inspect_selection_chrome(
                bounds,
                transform,
                surface_size,
                image.width(),
                image.height(),
                canvas_width,
                paint_palette.selection_chrome,
                image.as_raw(),
            )
        })
        .transpose()?;
    Ok((snapshot, transform, geometry, paint, selection_chrome))
}

pub(super) fn inspect_edge_path_frame(
    window: &mut Window,
    view: &WeakViewSession,
    surface_size: ViewSize,
    path: &EdgePath,
    canvas_background: [u8; 4],
) -> Result<SmokeWirePaintEvidence, &'static str> {
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let image = window
        .render_to_image()
        .map_err(|_| "paint_capture_failed")?;
    let canvas_width = canvas_pixel_width(image.width(), surface_size)?;
    let samples = sample_edge_path(path)?
        .into_iter()
        .filter(|sample| sample.progress > 0.03 && sample.progress < 0.97)
        .filter_map(|sample| {
            project_canvas_sample(
                sample.point,
                transform,
                surface_size,
                image.height(),
                canvas_width,
            )
        })
        .collect::<Vec<_>>();
    let evidence = inspect_wire_corridor(
        image.width(),
        image.height(),
        canvas_width,
        surface_size,
        image.as_raw(),
        &samples,
        canvas_background,
    )?;
    if samples.len() < MIN_WIRE_VISIBLE_SAMPLES {
        return Err("paint_wire_out_of_frame");
    }
    if let Some(reason) = evidence.failure_reason() {
        return Err(reason);
    }
    Ok(evidence)
}

pub(super) fn connection_preview_path(
    view: &WeakViewSession,
    presentation: &CanvasInteractionPresentation,
) -> Result<EdgePath, &'static str> {
    let connection = presentation
        .connection()
        .ok_or("input_connection_presentation_missing")?;
    let target = connection
        .probe()
        .target()
        .filter(|_| connection.probe().is_committable())
        .ok_or("input_connection_candidate_missing")?;
    let fixed = EdgeEndpointPosition {
        point: connection.fixed_anchor(),
        position: connection.fixed_outward(),
    };
    let moving = EdgeEndpointPosition {
        point: target.route_anchor(),
        position: target.outward(),
    };
    let snapshot = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let graph = snapshot.graph();
    let binding = connection.binding();
    let (endpoints, from, to) = match binding.fixed_endpoint() {
        EdgeEndpoint::From => (
            EdgePosition {
                source: fixed,
                target: moving,
            },
            connection.fixed_port(),
            target.port(),
        ),
        EdgeEndpoint::To => (
            EdgePosition {
                source: moving,
                target: fixed,
            },
            target.port(),
            connection.fixed_port(),
        ),
    };
    let edge = if let Some(edge) = binding.reconnect_edge() {
        graph
            .edge(&edge.edge())
            .cloned()
            .ok_or("input_created_edge_missing")?
    } else {
        let fixed_port = graph
            .port(&connection.fixed_port())
            .ok_or("input_source_port_missing")?;
        Edge::new(fixed_port.kind.edge_kind(), from, to)
    };
    resolve_edge_route_path(&edge, endpoints)
        .map(|route| route.path)
        .ok_or("input_connection_preview_path_missing")
}

fn sample_edge_path(path: &EdgePath) -> Result<Vec<SmokePathSampleInput>, &'static str> {
    (0..WIRE_PATH_SAMPLE_COUNT)
        .map(|index| {
            let progress = index as f32 / (WIRE_PATH_SAMPLE_COUNT - 1) as f32;
            path.sample(EdgePathAnchor::Progress(progress))
                .map(|sample| SmokePathSampleInput {
                    progress,
                    point: sample.point,
                })
                .map_err(|_| "geometry_invalid_route_samples")
        })
        .collect()
}
