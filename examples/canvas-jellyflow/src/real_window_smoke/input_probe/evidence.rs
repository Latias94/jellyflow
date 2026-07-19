use std::path::Path;

use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{GeometrySnapshot, ViewSize, WeakViewSession};
use open_gpui::Window;

use super::{
    SmokeExpectations, SmokeGeometryEvidence, SmokeGeometryInput, SmokePaintEvidence,
    canvas_pixel_width, inspect_canvas_paint, inspect_geometry,
};

pub(super) fn inspect_current_frame(
    window: &mut Window,
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    surface_size: ViewSize,
    screenshot_path: Option<&Path>,
) -> Result<
    (
        GeometrySnapshot,
        ViewportTransform,
        SmokeGeometryEvidence,
        SmokePaintEvidence,
    ),
    &'static str,
> {
    let snapshot = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let image = window
        .render_to_image()
        .map_err(|_| "paint_capture_failed")?;
    if let Some(path) = screenshot_path {
        image
            .save(path)
            .map_err(|_| "paint_screenshot_write_failed")?;
    }
    let canvas_width = canvas_pixel_width(image.width(), surface_size)?;
    let geometry_input = SmokeGeometryInput::from_snapshot(expectations, &snapshot);
    let geometry = inspect_geometry(
        expectations,
        &geometry_input,
        transform,
        surface_size,
        image.width(),
        image.height(),
        canvas_width,
    )?;
    let paint = inspect_canvas_paint(image.width(), image.height(), image.as_raw(), &geometry)?;
    Ok((snapshot, transform, geometry, paint))
}
