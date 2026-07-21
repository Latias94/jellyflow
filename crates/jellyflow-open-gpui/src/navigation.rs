use jellyflow_core::CanvasSize;
use jellyflow_runtime::{AlignmentSnapConfig, CanvasInteractionConfig};

const DEFAULT_MIN_ZOOM: f32 = 0.1;
const DEFAULT_MAX_ZOOM: f32 = 4.0;
const DEFAULT_WHEEL_ZOOM_SENSITIVITY: f32 = 0.005;
const DEFAULT_FIT_VIEW_PADDING: f32 = 48.0;
const DEFAULT_ALIGNMENT_TOLERANCE: f32 = 6.0;
const DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW: f32 = 1.0;
const DEFAULT_GRID_SIZE: CanvasSize = CanvasSize {
    width: 20.0,
    height: 20.0,
};

/// Native input mapping for one Open GPUI graph surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenGpuiNavigationProfile {
    /// Wheel zoom, Shift/horizontal wheel pan, middle-button drag pan.
    #[default]
    Mouse,
    /// Two-axis wheel pan, pinch zoom, middle-button drag pan.
    Trackpad,
    /// Wheel zoom plus middle- or right-button drag pan.
    Blueprint,
}

impl OpenGpuiNavigationProfile {
    pub(crate) const fn wheel_pans(self) -> bool {
        matches!(self, Self::Trackpad)
    }

    pub(crate) const fn right_button_pans(self) -> bool {
        matches!(self, Self::Blueprint)
    }
}

/// Adapter-local navigation policy for a native Open GPUI graph surface.
///
/// The renderer-neutral runtime owns gesture state, thresholds, auto-pan, grid snapping, and
/// alignment facts. This type only adds native event mapping and presentation settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNavigationConfig {
    profile: OpenGpuiNavigationProfile,
    interaction: CanvasInteractionConfig,
    grid_size_in_canvas: Option<CanvasSize>,
    snap_to_grid: bool,
    alignment_snap_tolerance_in_view: Option<f32>,
    min_zoom: f32,
    max_zoom: f32,
    wheel_zoom_sensitivity: f32,
    fit_view_padding_in_view: f32,
}

impl OpenGpuiNavigationConfig {
    /// Creates the native defaults for an explicit input profile.
    #[must_use]
    pub fn for_profile(profile: OpenGpuiNavigationProfile) -> Self {
        let interaction = CanvasInteractionConfig {
            node_drag_threshold_in_view: DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW,
            connection_drag_threshold_in_view: DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW,
            ..CanvasInteractionConfig::default()
        };
        Self {
            profile,
            interaction,
            grid_size_in_canvas: Some(DEFAULT_GRID_SIZE),
            snap_to_grid: false,
            alignment_snap_tolerance_in_view: Some(DEFAULT_ALIGNMENT_TOLERANCE),
            min_zoom: DEFAULT_MIN_ZOOM,
            max_zoom: DEFAULT_MAX_ZOOM,
            wheel_zoom_sensitivity: DEFAULT_WHEEL_ZOOM_SENSITIVITY,
            fit_view_padding_in_view: DEFAULT_FIT_VIEW_PADDING,
        }
    }

    #[must_use]
    pub const fn profile(self) -> OpenGpuiNavigationProfile {
        self.profile
    }

    #[must_use]
    pub const fn grid_size_in_canvas(self) -> Option<CanvasSize> {
        self.grid_size_in_canvas
    }

    #[must_use]
    pub const fn snaps_to_grid(self) -> bool {
        self.snap_to_grid
    }

    #[must_use]
    pub const fn alignment_snap_tolerance_in_view(self) -> Option<f32> {
        self.alignment_snap_tolerance_in_view
    }

    #[must_use]
    pub const fn min_zoom(self) -> f32 {
        self.min_zoom
    }

    #[must_use]
    pub const fn max_zoom(self) -> f32 {
        self.max_zoom
    }

    #[must_use]
    pub const fn wheel_zoom_sensitivity(self) -> f32 {
        self.wheel_zoom_sensitivity
    }

    #[must_use]
    pub const fn fit_view_padding_in_view(self) -> f32 {
        self.fit_view_padding_in_view
    }

    /// Replaces the independent native drag thresholds, in logical screen pixels.
    #[must_use]
    pub fn with_drag_thresholds(mut self, node: f32, connection: f32, canvas: f32) -> Self {
        self.interaction.node_drag_threshold_in_view = node;
        self.interaction.connection_drag_threshold_in_view = connection;
        self.interaction.canvas_drag_threshold_in_view = canvas;
        self.sanitized()
    }

    /// Replaces node and blank-canvas click tolerances, in logical screen pixels.
    ///
    /// Movement above a click tolerance but not beyond its drag threshold is an intentional dead
    /// zone: it neither activates the click nor starts a drag.
    #[must_use]
    pub fn with_click_tolerances(mut self, node: f32, canvas: f32) -> Self {
        self.interaction.node_click_tolerance_in_view = node;
        self.interaction.canvas_click_tolerance_in_view = canvas;
        self.sanitized()
    }

    /// Replaces the edge band and maximum view-pixel speed used by runtime auto-pan.
    #[must_use]
    pub fn with_auto_pan(mut self, band_in_view: f32, speed_in_view_per_second: f32) -> Self {
        self.interaction.auto_pan_band_in_view = band_in_view;
        self.interaction.auto_pan_speed_in_view_per_second = speed_in_view_per_second;
        self.sanitized()
    }

    /// Configures a visible canvas grid and whether node movement snaps to it.
    #[must_use]
    pub fn with_grid(mut self, grid: Option<CanvasSize>, snap_to_grid: bool) -> Self {
        self.grid_size_in_canvas = grid;
        self.snap_to_grid = snap_to_grid && grid.is_some();
        self.sanitized()
    }

    /// Configures deterministic node-alignment snapping in logical screen pixels.
    #[must_use]
    pub fn with_alignment_snap_tolerance(mut self, tolerance_in_view: Option<f32>) -> Self {
        self.alignment_snap_tolerance_in_view = tolerance_in_view;
        self.sanitized()
    }

    /// Replaces the native zoom clamp.
    #[must_use]
    pub fn with_zoom_range(mut self, min_zoom: f32, max_zoom: f32) -> Self {
        self.min_zoom = min_zoom;
        self.max_zoom = max_zoom;
        self.sanitized()
    }

    /// Replaces exponential wheel zoom sensitivity.
    #[must_use]
    pub fn with_wheel_zoom_sensitivity(mut self, sensitivity: f32) -> Self {
        self.wheel_zoom_sensitivity = sensitivity;
        self.sanitized()
    }

    /// Replaces symmetric screen-pixel padding used by fit view.
    #[must_use]
    pub fn with_fit_view_padding(mut self, padding_in_view: f32) -> Self {
        self.fit_view_padding_in_view = padding_in_view;
        self.sanitized()
    }

    pub(crate) fn runtime_config(self) -> CanvasInteractionConfig {
        CanvasInteractionConfig {
            grid_size_in_canvas: self
                .snap_to_grid
                .then_some(self.grid_size_in_canvas)
                .flatten(),
            alignment_snap: self
                .alignment_snap_tolerance_in_view
                .and_then(|tolerance| AlignmentSnapConfig::new(tolerance, tolerance * 1.5)),
            ..self.interaction
        }
    }

    pub(crate) fn sanitized(mut self) -> Self {
        let defaults = Self::for_profile(self.profile);
        if !self.interaction.is_valid() {
            self.interaction = defaults.interaction;
        }
        if self
            .grid_size_in_canvas
            .is_some_and(|grid| !grid.is_positive_finite())
        {
            self.grid_size_in_canvas = defaults.grid_size_in_canvas;
            self.snap_to_grid = false;
        }
        if self
            .alignment_snap_tolerance_in_view
            .is_some_and(|tolerance| !tolerance.is_finite() || tolerance <= 0.0)
        {
            self.alignment_snap_tolerance_in_view = defaults.alignment_snap_tolerance_in_view;
        }
        if !self.min_zoom.is_finite() || self.min_zoom <= 0.0 {
            self.min_zoom = defaults.min_zoom;
        }
        if !self.max_zoom.is_finite() || self.max_zoom <= 0.0 {
            self.max_zoom = defaults.max_zoom;
        }
        if self.min_zoom > self.max_zoom {
            std::mem::swap(&mut self.min_zoom, &mut self.max_zoom);
        }
        if !self.wheel_zoom_sensitivity.is_finite() || self.wheel_zoom_sensitivity <= 0.0 {
            self.wheel_zoom_sensitivity = defaults.wheel_zoom_sensitivity;
        }
        if !self.fit_view_padding_in_view.is_finite() || self.fit_view_padding_in_view < 0.0 {
            self.fit_view_padding_in_view = defaults.fit_view_padding_in_view;
        }
        self
    }
}

impl Default for OpenGpuiNavigationConfig {
    fn default() -> Self {
        Self::for_profile(OpenGpuiNavigationProfile::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_only_change_native_event_mapping() {
        let mouse = OpenGpuiNavigationConfig::for_profile(OpenGpuiNavigationProfile::Mouse);
        let trackpad = OpenGpuiNavigationConfig::for_profile(OpenGpuiNavigationProfile::Trackpad);
        let blueprint = OpenGpuiNavigationConfig::for_profile(OpenGpuiNavigationProfile::Blueprint);

        assert_eq!(mouse.runtime_config(), trackpad.runtime_config());
        assert_eq!(mouse.runtime_config(), blueprint.runtime_config());
        assert!(!mouse.profile().wheel_pans());
        assert!(trackpad.profile().wheel_pans());
        assert!(blueprint.profile().right_button_pans());
    }

    #[test]
    fn native_node_and_connection_drags_arm_after_one_screen_pixel() {
        let runtime = OpenGpuiNavigationConfig::default().runtime_config();

        assert_eq!(
            runtime.node_drag_threshold_in_view,
            DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW
        );
        assert_eq!(
            runtime.connection_drag_threshold_in_view,
            DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW
        );
        assert_eq!(runtime.canvas_drag_threshold_in_view, 3.0);
    }

    #[test]
    fn grid_visibility_and_runtime_snapping_are_independent() {
        let visible = OpenGpuiNavigationConfig::default();
        assert_eq!(visible.grid_size_in_canvas(), Some(DEFAULT_GRID_SIZE));
        assert!(visible.runtime_config().grid_size_in_canvas.is_none());

        let snapping = visible.with_grid(Some(DEFAULT_GRID_SIZE), true);
        assert_eq!(
            snapping.runtime_config().grid_size_in_canvas,
            Some(DEFAULT_GRID_SIZE)
        );
    }
}
