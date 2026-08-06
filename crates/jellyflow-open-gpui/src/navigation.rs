use jellyflow_core::CanvasSize;
use jellyflow_runtime::{
    interaction::AlignmentSnapConfig, interaction::CanvasInteractionConfig,
    interaction::SelectionMode,
};
use open_gpui::Modifiers;

const DEFAULT_MIN_ZOOM: f32 = 0.1;
const DEFAULT_MAX_ZOOM: f32 = 4.0;
const DEFAULT_WHEEL_ZOOM_SENSITIVITY: f32 = 0.005;
const DEFAULT_FIT_VIEW_PADDING: f32 = 48.0;
const DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW: f32 = 1.0;
const DEFAULT_CONNECTION_ACQUIRE_RADIUS_IN_VIEW: f32 = 20.0;
const DEFAULT_CONNECTION_RELEASE_RADIUS_IN_VIEW: f32 = 26.0;
const DEFAULT_GRID_SIZE: CanvasSize = CanvasSize {
    width: 20.0,
    height: 20.0,
};

/// Named native input presets for one Open GPUI graph surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenGpuiInputPreset {
    /// Wheel zoom, Shift/horizontal wheel pan, middle-button drag pan.
    #[default]
    Mouse,
    /// Two-axis wheel pan, pinch zoom, middle-button drag pan.
    Trackpad,
    /// Dify-like two-axis wheel pan, middle-button pan, and Space-plus-left pan.
    Dify,
    /// Wheel zoom plus middle- or right-button drag pan.
    Blueprint,
}

/// Native wheel behavior selected by an input-binding preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiWheelBinding {
    Zoom,
    Pan,
}

/// One native modifier that can alter graph selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiSelectionModifier {
    Shift,
    Secondary,
    Alt,
}

impl OpenGpuiSelectionModifier {
    fn is_pressed(self, modifiers: Modifiers) -> bool {
        match self {
            Self::Shift => modifiers.shift,
            Self::Secondary => modifiers.secondary(),
            Self::Alt => modifiers.alt,
        }
    }
}

/// Shared native modifier policy for node, edge, and marquee selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiSelectionBindings {
    add: Option<OpenGpuiSelectionModifier>,
    toggle: Option<OpenGpuiSelectionModifier>,
}

impl OpenGpuiSelectionBindings {
    #[must_use]
    pub const fn new(
        add: Option<OpenGpuiSelectionModifier>,
        toggle: Option<OpenGpuiSelectionModifier>,
    ) -> Self {
        Self { add, toggle }
    }

    #[must_use]
    pub const fn add(self) -> Option<OpenGpuiSelectionModifier> {
        self.add
    }

    #[must_use]
    pub const fn toggle(self) -> Option<OpenGpuiSelectionModifier> {
        self.toggle
    }

    pub(crate) fn selection_mode(self, modifiers: Modifiers) -> SelectionMode {
        if self
            .toggle
            .is_some_and(|modifier| modifier.is_pressed(modifiers))
        {
            SelectionMode::Toggle
        } else if self
            .add
            .is_some_and(|modifier| modifier.is_pressed(modifiers))
        {
            SelectionMode::Add
        } else {
            SelectionMode::Replace
        }
    }
}

impl Default for OpenGpuiSelectionBindings {
    fn default() -> Self {
        Self::new(
            Some(OpenGpuiSelectionModifier::Shift),
            Some(OpenGpuiSelectionModifier::Secondary),
        )
    }
}

/// Adapter-local native event bindings.
///
/// Runtime interaction state remains renderer-neutral. This value only determines how Open GPUI
/// wheel, button, held-key, and modifier facts map onto those semantic interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiInputBindings {
    wheel: OpenGpuiWheelBinding,
    middle_button_pan: bool,
    right_button_pan: bool,
    space_left_pan: bool,
    connect_on_click: bool,
    selection: OpenGpuiSelectionBindings,
}

impl OpenGpuiInputBindings {
    #[must_use]
    pub const fn for_preset(preset: OpenGpuiInputPreset) -> Self {
        match preset {
            OpenGpuiInputPreset::Mouse => Self {
                wheel: OpenGpuiWheelBinding::Zoom,
                middle_button_pan: true,
                right_button_pan: false,
                space_left_pan: true,
                connect_on_click: true,
                selection: OpenGpuiSelectionBindings::new(
                    Some(OpenGpuiSelectionModifier::Shift),
                    Some(OpenGpuiSelectionModifier::Secondary),
                ),
            },
            OpenGpuiInputPreset::Trackpad => Self {
                wheel: OpenGpuiWheelBinding::Pan,
                middle_button_pan: false,
                right_button_pan: false,
                space_left_pan: true,
                connect_on_click: true,
                selection: OpenGpuiSelectionBindings::new(
                    Some(OpenGpuiSelectionModifier::Shift),
                    Some(OpenGpuiSelectionModifier::Secondary),
                ),
            },
            OpenGpuiInputPreset::Dify => Self {
                wheel: OpenGpuiWheelBinding::Pan,
                middle_button_pan: true,
                right_button_pan: false,
                space_left_pan: true,
                connect_on_click: true,
                selection: OpenGpuiSelectionBindings::new(
                    Some(OpenGpuiSelectionModifier::Shift),
                    Some(OpenGpuiSelectionModifier::Secondary),
                ),
            },
            OpenGpuiInputPreset::Blueprint => Self {
                wheel: OpenGpuiWheelBinding::Zoom,
                middle_button_pan: true,
                right_button_pan: true,
                space_left_pan: true,
                connect_on_click: false,
                selection: OpenGpuiSelectionBindings::new(
                    Some(OpenGpuiSelectionModifier::Shift),
                    Some(OpenGpuiSelectionModifier::Secondary),
                ),
            },
        }
    }

    #[must_use]
    pub const fn wheel(self) -> OpenGpuiWheelBinding {
        self.wheel
    }

    #[must_use]
    pub const fn selection(self) -> OpenGpuiSelectionBindings {
        self.selection
    }

    #[must_use]
    pub const fn with_selection(mut self, selection: OpenGpuiSelectionBindings) -> Self {
        self.selection = selection;
        self
    }

    /// Returns whether a click on one port arms a non-drag connection and the next port click
    /// activates its target.
    #[must_use]
    pub const fn connects_on_click(self) -> bool {
        self.connect_on_click
    }

    /// Replaces click-to-connect mapping without changing runtime connection semantics.
    #[must_use]
    pub const fn with_connect_on_click(mut self, enabled: bool) -> Self {
        self.connect_on_click = enabled;
        self
    }

    pub(crate) const fn wheel_pans(self) -> bool {
        matches!(self.wheel, OpenGpuiWheelBinding::Pan)
    }

    pub(crate) const fn middle_button_pans(self) -> bool {
        self.middle_button_pan
    }

    pub(crate) const fn right_button_pans(self) -> bool {
        self.right_button_pan
    }

    pub(crate) const fn space_left_pans(self) -> bool {
        self.space_left_pan
    }
}

impl Default for OpenGpuiInputBindings {
    fn default() -> Self {
        Self::for_preset(OpenGpuiInputPreset::default())
    }
}

/// Adapter-local navigation policy for a native Open GPUI graph surface.
///
/// The renderer-neutral runtime owns gesture state, thresholds, auto-pan, grid snapping, and
/// alignment facts. This type only adds native event mapping and presentation settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiNavigationConfig {
    input_bindings: OpenGpuiInputBindings,
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
    pub fn for_preset(preset: OpenGpuiInputPreset) -> Self {
        Self::for_input_bindings(OpenGpuiInputBindings::for_preset(preset))
    }

    /// Creates native navigation settings for an explicit event-binding policy.
    #[must_use]
    pub fn for_input_bindings(input_bindings: OpenGpuiInputBindings) -> Self {
        let interaction = CanvasInteractionConfig {
            node_drag_threshold_in_view: DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW,
            connection_drag_threshold_in_view: DEFAULT_NATIVE_DRAG_THRESHOLD_IN_VIEW,
            connection_acquire_radius_in_view: DEFAULT_CONNECTION_ACQUIRE_RADIUS_IN_VIEW,
            connection_release_radius_in_view: DEFAULT_CONNECTION_RELEASE_RADIUS_IN_VIEW,
            ..CanvasInteractionConfig::default()
        };
        Self {
            input_bindings,
            interaction,
            grid_size_in_canvas: Some(DEFAULT_GRID_SIZE),
            snap_to_grid: false,
            alignment_snap_tolerance_in_view: None,
            min_zoom: DEFAULT_MIN_ZOOM,
            max_zoom: DEFAULT_MAX_ZOOM,
            wheel_zoom_sensitivity: DEFAULT_WHEEL_ZOOM_SENSITIVITY,
            fit_view_padding_in_view: DEFAULT_FIT_VIEW_PADDING,
        }
    }

    #[must_use]
    pub const fn input_bindings(self) -> OpenGpuiInputBindings {
        self.input_bindings
    }

    /// Replaces only the adapter-local event mapping.
    #[must_use]
    pub const fn with_input_bindings(mut self, input_bindings: OpenGpuiInputBindings) -> Self {
        self.input_bindings = input_bindings;
        self
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

    #[must_use]
    pub const fn connection_acquire_radius_in_view(self) -> f32 {
        self.interaction.connection_acquire_radius_in_view
    }

    #[must_use]
    pub const fn connection_release_radius_in_view(self) -> f32 {
        self.interaction.connection_release_radius_in_view
    }

    #[must_use]
    pub const fn edge_pick_radius_in_view(self) -> f32 {
        self.interaction.edge_pick_radius_in_view
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

    /// Replaces connection target acquisition and release radii in logical screen pixels.
    ///
    /// The release radius must be at least the acquisition radius so an acquired target remains
    /// stable while the pointer moves through the surrounding hysteresis band.
    #[must_use]
    pub fn with_connection_targeting(mut self, acquire: f32, release: f32) -> Self {
        self.interaction.connection_acquire_radius_in_view = acquire;
        self.interaction.connection_release_radius_in_view = release;
        self.sanitized()
    }

    /// Replaces the selectable edge-pick radius in logical screen pixels.
    #[must_use]
    pub fn with_edge_pick_radius(mut self, radius: f32) -> Self {
        self.interaction.edge_pick_radius_in_view = radius;
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
        let defaults = Self::for_input_bindings(self.input_bindings);
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
        Self::for_preset(OpenGpuiInputPreset::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_only_change_native_event_mapping() {
        let mouse = OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Mouse);
        let trackpad = OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Trackpad);
        let dify = OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Dify);
        let blueprint = OpenGpuiNavigationConfig::for_preset(OpenGpuiInputPreset::Blueprint);

        assert_eq!(mouse.runtime_config(), trackpad.runtime_config());
        assert_eq!(mouse.runtime_config(), dify.runtime_config());
        assert_eq!(mouse.runtime_config(), blueprint.runtime_config());
        assert!(!mouse.input_bindings().wheel_pans());
        assert!(trackpad.input_bindings().wheel_pans());
        assert!(dify.input_bindings().wheel_pans());
        assert!(!blueprint.input_bindings().wheel_pans());
        assert!(dify.input_bindings().middle_button_pans());
        assert!(blueprint.input_bindings().right_button_pans());
        assert!(mouse.input_bindings().connects_on_click());
        assert!(trackpad.input_bindings().connects_on_click());
        assert!(dify.input_bindings().connects_on_click());
        assert!(!blueprint.input_bindings().connects_on_click());
    }

    #[test]
    fn selection_policy_is_shared_and_toggle_has_precedence() {
        let selection = OpenGpuiSelectionBindings::default();
        assert_eq!(
            selection.selection_mode(Modifiers::default()),
            SelectionMode::Replace
        );
        assert_eq!(
            selection.selection_mode(Modifiers::shift()),
            SelectionMode::Add
        );
        assert_eq!(
            selection.selection_mode(Modifiers::secondary_key()),
            SelectionMode::Toggle
        );

        let mut both = Modifiers::shift();
        let secondary = Modifiers::secondary_key();
        both.control |= secondary.control;
        both.platform |= secondary.platform;
        assert_eq!(selection.selection_mode(both), SelectionMode::Toggle);
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
    fn native_connection_targeting_uses_a_forgiving_screen_pixel_corridor() {
        let config = OpenGpuiNavigationConfig::default();

        assert_eq!(
            config.connection_acquire_radius_in_view(),
            DEFAULT_CONNECTION_ACQUIRE_RADIUS_IN_VIEW
        );
        assert_eq!(
            config.connection_release_radius_in_view(),
            DEFAULT_CONNECTION_RELEASE_RADIUS_IN_VIEW
        );
        assert_eq!(
            config.runtime_config().connection_acquire_radius_in_view,
            DEFAULT_CONNECTION_ACQUIRE_RADIUS_IN_VIEW
        );
        assert_eq!(
            config.runtime_config().connection_release_radius_in_view,
            DEFAULT_CONNECTION_RELEASE_RADIUS_IN_VIEW
        );
    }

    #[test]
    fn connection_and_edge_targeting_are_publicly_configurable() {
        let config = OpenGpuiNavigationConfig::default()
            .with_connection_targeting(18.0, 24.0)
            .with_edge_pick_radius(14.0);

        assert_eq!(config.connection_acquire_radius_in_view(), 18.0);
        assert_eq!(config.connection_release_radius_in_view(), 24.0);
        assert_eq!(config.edge_pick_radius_in_view(), 14.0);
        assert_eq!(config.runtime_config().edge_pick_radius_in_view, 14.0);
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

    #[test]
    fn alignment_snap_is_opt_in_and_preserves_runtime_hysteresis() {
        let default = OpenGpuiNavigationConfig::default();
        assert_eq!(default.alignment_snap_tolerance_in_view(), None);
        assert_eq!(default.runtime_config().alignment_snap, None);

        let enabled = default.with_alignment_snap_tolerance(Some(6.0));
        assert_eq!(enabled.alignment_snap_tolerance_in_view(), Some(6.0));
        let runtime = enabled
            .runtime_config()
            .alignment_snap
            .expect("explicit positive tolerance enables alignment snap");
        assert_eq!(runtime.acquire_tolerance_in_view(), 6.0);
        assert_eq!(runtime.release_tolerance_in_view(), 9.0);

        let invalid = default.with_alignment_snap_tolerance(Some(f32::NAN));
        assert_eq!(invalid.alignment_snap_tolerance_in_view(), None);
        assert_eq!(invalid.runtime_config().alignment_snap, None);
    }
}
