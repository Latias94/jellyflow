//! Adapter-local visual defaults for native graph chrome.

use open_gpui::{Hsla, rgb, rgba};

/// Maximum number of application-native graph style slots retained by one theme.
///
/// The fixed upper bound keeps port and wire recipes value-semantic and guarantees that product
/// type systems cannot create an unbounded paint-batch vocabulary.
pub const OPEN_GPUI_GRAPH_STYLE_SLOT_CAPACITY: usize = 16;

/// One validated application-native graph style slot.
///
/// A slot is an adapter-local visual index, not a graph-domain type. Applications map their own
/// renderer-neutral types to these values through the edge painter registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiGraphStyleSlot(u8);

impl OpenGpuiGraphStyleSlot {
    /// Creates a slot when `index` fits the fixed native style budget.
    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if (index as usize) < OPEN_GPUI_GRAPH_STYLE_SLOT_CAPACITY {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Returns the stable array index for this slot.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Finite non-color cue shared by typed port markers and typed wires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiGraphStyleCue {
    /// Preserve the painter's ordinary stroke and draw no extra port glyph.
    #[default]
    Inherit,
    /// Use a dashed wire and a short bar inside the matching port marker.
    Dashed,
    /// Use a dotted wire and a dot inside the matching port marker.
    Dotted,
}

/// Finite marker-shape vocabulary for native graph ports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiPortMarkerShape {
    /// Circular data-flow marker.
    #[default]
    Circle,
    /// Directional triangular marker.
    Triangle,
    /// Diamond marker commonly used by typed shader ports.
    Diamond,
    /// Square marker commonly used by control-flow ports.
    Square,
}

/// Theme-level marker profile for the three bounded port families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiPortMarkerShapes {
    /// Shape used for unknown or untyped ports.
    pub default: OpenGpuiPortMarkerShape,
    /// Shape used for data-flow ports without a style-slot override.
    pub data: OpenGpuiPortMarkerShape,
    /// Shape used for execution or control-flow ports without a style-slot override.
    pub control: OpenGpuiPortMarkerShape,
}

impl Default for OpenGpuiPortMarkerShapes {
    fn default() -> Self {
        Self {
            default: OpenGpuiPortMarkerShape::Circle,
            data: OpenGpuiPortMarkerShape::Circle,
            control: OpenGpuiPortMarkerShape::Circle,
        }
    }
}

/// Immutable application recipe for one bounded graph style slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiGraphStyleRecipe {
    color: Hsla,
    cue: OpenGpuiGraphStyleCue,
    marker_shape: Option<OpenGpuiPortMarkerShape>,
}

impl OpenGpuiGraphStyleRecipe {
    /// Creates a solid typed recipe with an application-owned color.
    #[must_use]
    pub const fn new(color: Hsla) -> Self {
        Self {
            color,
            cue: OpenGpuiGraphStyleCue::Inherit,
            marker_shape: None,
        }
    }

    /// Adds a finite non-color cue to the typed recipe.
    #[must_use]
    pub const fn with_cue(mut self, cue: OpenGpuiGraphStyleCue) -> Self {
        self.cue = cue;
        self
    }

    /// Overrides the native port-marker shape for this bounded style slot.
    #[must_use]
    pub const fn with_port_marker_shape(mut self, shape: OpenGpuiPortMarkerShape) -> Self {
        self.marker_shape = Some(shape);
        self
    }

    /// Returns the base color shared by ports and wires using this slot.
    #[must_use]
    pub const fn color(self) -> Hsla {
        self.color
    }

    /// Returns the finite non-color cue shared by ports and wires using this slot.
    #[must_use]
    pub const fn cue(self) -> OpenGpuiGraphStyleCue {
        self.cue
    }

    /// Returns the style-slot marker override, when present.
    #[must_use]
    pub const fn port_marker_shape(self) -> Option<OpenGpuiPortMarkerShape> {
        self.marker_shape
    }
}

/// Default diameter of an idle native port marker in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER: f32 = 8.0;

/// Default diameter of emphasized native port chrome in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER: f32 = 10.0;

/// Default stable outer layout-slot diameter for native port chrome in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_LAYOUT_DIAMETER: f32 = 12.0;

/// Default minimum port hit diameter in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER: f32 = 22.0;

/// Default idle port outline width in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_OUTLINE_WIDTH: f32 = 1.0;

/// Default emphasized port outline width in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_EMPHASIZED_OUTLINE_WIDTH: f32 = 1.5;

/// Default keyboard-focus ring width around a port in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_FOCUS_RING_WIDTH: f32 = 1.0;

/// Default ordinary edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH: f32 = 1.5;

/// Default canvas-grid stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_GRID_LINE_WIDTH: f32 = 1.0;

/// Default alignment-guide stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_GUIDE_WIDTH: f32 = 1.0;

/// Default corner radius shared by native node surfaces and host-owned node chrome.
pub const OPEN_GPUI_DEFAULT_NODE_CORNER_RADIUS: f32 = 6.0;

/// Default hovered edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH: f32 = 2.0;

/// Default selected and active edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH: f32 = 2.5;

/// Default ordinary strong-edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_STRONG_ORDINARY_WIDTH: f32 = 2.0;

/// Default hovered strong-edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_STRONG_HOVERED_WIDTH: f32 = 2.5;

/// Default selected, active, and preview strong-edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_STRONG_EMPHASIZED_WIDTH: f32 = 3.0;

/// Default reconnect endpoint-cap length in screen pixels.
pub const OPEN_GPUI_DEFAULT_RECONNECT_CAP_LENGTH: f32 = 9.0;

/// Default reconnect handle hit diameter in screen pixels.
pub const OPEN_GPUI_DEFAULT_RECONNECT_HIT_DIAMETER: f32 = 22.0;

/// Default edge endpoint marker diameter in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_MARKER_DIAMETER: f32 = 8.0;

/// Default maximum width of edge-label chrome in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_LABEL_MAX_WIDTH: f32 = 120.0;

/// Default edge-label font size in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_LABEL_FONT_SIZE: f32 = 11.0;

/// Default horizontal edge-label padding in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_LABEL_HORIZONTAL_PADDING: f32 = 5.0;

/// Default vertical edge-label padding in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_LABEL_VERTICAL_PADDING: f32 = 2.0;

/// Default edge-label corner radius in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_LABEL_CORNER_RADIUS: f32 = 4.0;

/// Semantic colors used by the default native graph chrome.
///
/// Colors are kept adapter-local so arbitrary Open GPUI node interiors remain under application
/// control. All fields use [`Hsla`], which can be passed directly to Open GPUI paint APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiGraphPalette {
    /// Canvas background.
    pub canvas: Hsla,
    /// Minor grid lines or dots.
    pub grid_minor: Hsla,
    /// Major grid lines or dots.
    pub grid_major: Hsla,
    /// Alignment guides.
    pub guide: Hsla,
    /// Default node surface.
    pub node_surface: Hsla,
    /// Default node border.
    pub node_border: Hsla,
    /// Selected node border and focus chrome.
    pub selection: Hsla,
    /// Keyboard focus ring.
    pub focus: Hsla,
    /// Idle port marker.
    pub port_idle: Hsla,
    /// Hovered port marker.
    pub port_hovered: Hsla,
    /// Port currently acting as the connection source.
    pub port_source: Hsla,
    /// Compatible connection target.
    pub port_valid: Hsla,
    /// Incompatible connection target.
    pub port_invalid: Hsla,
    /// Connected port marker.
    pub port_connected: Hsla,
    /// Disabled port marker.
    pub port_disabled: Hsla,
    /// Fallback edge tone.
    pub edge_default: Hsla,
    /// Data-flow edge tone.
    pub edge_data: Hsla,
    /// Control-flow edge tone.
    pub edge_control: Hsla,
    /// Successful-result edge tone.
    pub edge_success: Hsla,
    /// Warning edge tone.
    pub edge_warning: Hsla,
    /// Error or destructive edge tone.
    pub edge_danger: Hsla,
    /// Tint blended over a hovered edge tone.
    pub edge_hover_tint: Hsla,
    /// Tint blended over a selected edge tone.
    pub edge_selection_tint: Hsla,
    /// Tint blended over an active edge tone.
    pub edge_active_tint: Hsla,
    /// Tint blended over a connection preview tone.
    pub edge_preview_tint: Hsla,
    /// Edge label text.
    pub label_text: Hsla,
    /// Edge label background.
    pub label_background: Hsla,
    /// Warning diagnostic chrome.
    pub diagnostic_warning: Hsla,
    /// Error diagnostic chrome.
    pub diagnostic_error: Hsla,
}

impl Default for OpenGpuiGraphPalette {
    fn default() -> Self {
        Self {
            canvas: rgb(0x111318).into(),
            grid_minor: rgba(0x53607124).into(),
            grid_major: rgba(0x64748b3d).into(),
            guide: rgb(0x7ab8ff).into(),
            node_surface: rgb(0x20262d).into(),
            node_border: rgb(0x3b4450).into(),
            selection: rgb(0x78a9ff).into(),
            focus: rgb(0x9fc4ff).into(),
            port_idle: rgb(0x8b96a5).into(),
            port_hovered: rgb(0xd7dde8).into(),
            port_source: rgb(0x78a9ff).into(),
            port_valid: rgb(0x7fd5ac).into(),
            port_invalid: rgb(0xff8b84).into(),
            port_connected: rgb(0xc9d3df).into(),
            port_disabled: rgba(0x64748b8f).into(),
            edge_default: rgb(0x8b96a5).into(),
            edge_data: rgb(0x59c2a6).into(),
            edge_control: rgb(0x78a9ff).into(),
            edge_success: rgb(0x7fd5ac).into(),
            edge_warning: rgb(0xe6cf9e).into(),
            edge_danger: rgb(0xe58b7b).into(),
            edge_hover_tint: rgba(0xffffff29).into(),
            edge_selection_tint: rgba(0x78a9ffb3).into(),
            edge_active_tint: rgba(0x9fc4ffd9).into(),
            edge_preview_tint: rgba(0x78a9ffe6).into(),
            label_text: rgb(0xe8edf3).into(),
            label_background: rgba(0x171b20eb).into(),
            diagnostic_warning: rgb(0xe6cf9e).into(),
            diagnostic_error: rgb(0xff8b84).into(),
        }
    }
}

/// Screen-space metrics used by the default native graph chrome.
///
/// These values remain independent from the canvas zoom. Consumers project route geometry into
/// view space before applying them. The unprefixed edge widths form the regular table, while the
/// `edge_strong_*` widths form the strong table selected only by the closed edge weight tier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiGraphMetrics {
    /// Canvas-grid stroke width.
    pub grid_line_width: f32,
    /// Alignment-guide stroke width.
    pub guide_width: f32,
    /// Node-surface and host-owned node-chrome corner radius.
    pub node_corner_radius: f32,
    /// Idle port marker diameter.
    pub port_marker_diameter: f32,
    /// Emphasized port-marker diameter.
    pub port_hover_marker_diameter: f32,
    /// Stable outer port layout-slot diameter.
    pub port_layout_diameter: f32,
    /// Minimum port hit diameter.
    pub port_hit_diameter: f32,
    /// Idle port outline width.
    pub port_outline_width: f32,
    /// Hovered, source, and candidate port outline width.
    pub port_emphasized_outline_width: f32,
    /// Keyboard-focus ring width around a port.
    pub port_focus_ring_width: f32,
    /// Ordinary regular-edge stroke width.
    pub edge_ordinary_width: f32,
    /// Hovered regular-edge stroke width.
    pub edge_hovered_width: f32,
    /// Selected regular-edge stroke width.
    pub edge_selected_width: f32,
    /// Active and preview regular-edge stroke width.
    pub edge_active_width: f32,
    /// Ordinary strong-edge stroke width.
    pub edge_strong_ordinary_width: f32,
    /// Hovered strong-edge stroke width.
    pub edge_strong_hovered_width: f32,
    /// Selected strong-edge stroke width.
    pub edge_strong_selected_width: f32,
    /// Active and preview strong-edge stroke width.
    pub edge_strong_active_width: f32,
    /// Reconnect endpoint-cap visual length.
    pub reconnect_cap_length: f32,
    /// Reconnect handle hit diameter.
    pub reconnect_hit_diameter: f32,
    /// Edge endpoint marker diameter.
    pub edge_marker_diameter: f32,
    /// Maximum edge-label width.
    pub edge_label_max_width: f32,
    /// Edge-label font size.
    pub edge_label_font_size: f32,
    /// Horizontal edge-label padding.
    pub edge_label_horizontal_padding: f32,
    /// Vertical edge-label padding.
    pub edge_label_vertical_padding: f32,
    /// Edge-label corner radius.
    pub edge_label_corner_radius: f32,
}

impl Default for OpenGpuiGraphMetrics {
    fn default() -> Self {
        Self {
            grid_line_width: OPEN_GPUI_DEFAULT_GRID_LINE_WIDTH,
            guide_width: OPEN_GPUI_DEFAULT_GUIDE_WIDTH,
            node_corner_radius: OPEN_GPUI_DEFAULT_NODE_CORNER_RADIUS,
            port_marker_diameter: OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER,
            port_hover_marker_diameter: OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER,
            port_layout_diameter: OPEN_GPUI_DEFAULT_PORT_LAYOUT_DIAMETER,
            port_hit_diameter: OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
            port_outline_width: OPEN_GPUI_DEFAULT_PORT_OUTLINE_WIDTH,
            port_emphasized_outline_width: OPEN_GPUI_DEFAULT_PORT_EMPHASIZED_OUTLINE_WIDTH,
            port_focus_ring_width: OPEN_GPUI_DEFAULT_PORT_FOCUS_RING_WIDTH,
            edge_ordinary_width: OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH,
            edge_hovered_width: OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
            edge_selected_width: OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
            edge_active_width: OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
            edge_strong_ordinary_width: OPEN_GPUI_DEFAULT_EDGE_STRONG_ORDINARY_WIDTH,
            edge_strong_hovered_width: OPEN_GPUI_DEFAULT_EDGE_STRONG_HOVERED_WIDTH,
            edge_strong_selected_width: OPEN_GPUI_DEFAULT_EDGE_STRONG_EMPHASIZED_WIDTH,
            edge_strong_active_width: OPEN_GPUI_DEFAULT_EDGE_STRONG_EMPHASIZED_WIDTH,
            reconnect_cap_length: OPEN_GPUI_DEFAULT_RECONNECT_CAP_LENGTH,
            reconnect_hit_diameter: OPEN_GPUI_DEFAULT_RECONNECT_HIT_DIAMETER,
            edge_marker_diameter: OPEN_GPUI_DEFAULT_EDGE_MARKER_DIAMETER,
            edge_label_max_width: OPEN_GPUI_DEFAULT_EDGE_LABEL_MAX_WIDTH,
            edge_label_font_size: OPEN_GPUI_DEFAULT_EDGE_LABEL_FONT_SIZE,
            edge_label_horizontal_padding: OPEN_GPUI_DEFAULT_EDGE_LABEL_HORIZONTAL_PADDING,
            edge_label_vertical_padding: OPEN_GPUI_DEFAULT_EDGE_LABEL_VERTICAL_PADDING,
            edge_label_corner_radius: OPEN_GPUI_DEFAULT_EDGE_LABEL_CORNER_RADIUS,
        }
    }
}

impl OpenGpuiGraphMetrics {
    /// Returns whether every metric is finite, positive, and preserves its minimum hit target.
    #[must_use]
    pub fn is_valid(self) -> bool {
        let values = [
            self.grid_line_width,
            self.guide_width,
            self.port_marker_diameter,
            self.port_hover_marker_diameter,
            self.port_layout_diameter,
            self.port_hit_diameter,
            self.port_outline_width,
            self.port_emphasized_outline_width,
            self.port_focus_ring_width,
            self.edge_ordinary_width,
            self.edge_hovered_width,
            self.edge_selected_width,
            self.edge_active_width,
            self.edge_strong_ordinary_width,
            self.edge_strong_hovered_width,
            self.edge_strong_selected_width,
            self.edge_strong_active_width,
            self.reconnect_cap_length,
            self.reconnect_hit_diameter,
            self.edge_marker_diameter,
            self.edge_label_max_width,
            self.edge_label_font_size,
            self.edge_label_horizontal_padding,
            self.edge_label_vertical_padding,
        ];
        values
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            && self.port_hover_marker_diameter >= self.port_marker_diameter
            && self.port_layout_diameter >= self.port_hover_marker_diameter
            && self.port_hit_diameter >= self.port_layout_diameter
            && self.port_emphasized_outline_width >= self.port_outline_width
            && self.edge_ordinary_width < self.edge_hovered_width
            && self.edge_hovered_width < self.edge_selected_width
            && self.edge_selected_width <= self.edge_active_width
            && self.edge_strong_ordinary_width < self.edge_strong_hovered_width
            && self.edge_strong_hovered_width < self.edge_strong_selected_width
            && self.edge_strong_selected_width <= self.edge_strong_active_width
            && self.edge_strong_ordinary_width >= self.edge_ordinary_width
            && self.edge_strong_hovered_width >= self.edge_hovered_width
            && self.edge_strong_selected_width >= self.edge_selected_width
            && self.edge_strong_active_width >= self.edge_active_width
            && self.reconnect_hit_diameter >= self.reconnect_cap_length
            && self.node_corner_radius.is_finite()
            && self.node_corner_radius >= 0.0
            && self.edge_label_corner_radius.is_finite()
            && self.edge_label_corner_radius >= 0.0
    }
}

/// Complete visual theme for native Open GPUI graph chrome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiGraphTheme {
    /// Semantic graph colors.
    pub palette: OpenGpuiGraphPalette,
    /// Screen-space graph metrics.
    pub metrics: OpenGpuiGraphMetrics,
    /// Native marker shapes selected by bounded port family.
    pub port_marker_shapes: OpenGpuiPortMarkerShapes,
    style_slots: [Option<OpenGpuiGraphStyleRecipe>; OPEN_GPUI_GRAPH_STYLE_SLOT_CAPACITY],
}

impl OpenGpuiGraphTheme {
    /// Creates a theme from an application palette and screen-space metrics.
    #[must_use]
    pub const fn new(palette: OpenGpuiGraphPalette, metrics: OpenGpuiGraphMetrics) -> Self {
        Self {
            palette,
            metrics,
            port_marker_shapes: OpenGpuiPortMarkerShapes {
                default: OpenGpuiPortMarkerShape::Circle,
                data: OpenGpuiPortMarkerShape::Circle,
                control: OpenGpuiPortMarkerShape::Circle,
            },
            style_slots: [None; OPEN_GPUI_GRAPH_STYLE_SLOT_CAPACITY],
        }
    }

    /// Applies a bounded marker-shape profile for default, data, and control ports.
    #[must_use]
    pub const fn with_port_marker_shapes(mut self, shapes: OpenGpuiPortMarkerShapes) -> Self {
        self.port_marker_shapes = shapes;
        self
    }

    /// Registers or replaces one bounded application-native style recipe.
    #[must_use]
    pub const fn with_style_slot(
        mut self,
        slot: OpenGpuiGraphStyleSlot,
        recipe: OpenGpuiGraphStyleRecipe,
    ) -> Self {
        self.style_slots[slot.index()] = Some(recipe);
        self
    }

    /// Returns the application recipe registered for `slot`.
    #[must_use]
    pub const fn style_slot(
        &self,
        slot: OpenGpuiGraphStyleSlot,
    ) -> Option<OpenGpuiGraphStyleRecipe> {
        self.style_slots[slot.index()]
    }

    /// Returns a theme whose screen-space metrics are safe for native layout and hit testing.
    ///
    /// Invalid application metrics fall back as one coherent set so dependent minimum-size
    /// relationships cannot be partially broken. Palette overrides are preserved.
    #[must_use]
    pub fn sanitized(self) -> Self {
        if self.metrics.is_valid() {
            self
        } else {
            Self {
                palette: self.palette,
                metrics: OpenGpuiGraphMetrics::default(),
                port_marker_shapes: self.port_marker_shapes,
                style_slots: self.style_slots,
            }
        }
    }
}

impl Default for OpenGpuiGraphTheme {
    fn default() -> Self {
        Self::new(
            OpenGpuiGraphPalette::default(),
            OpenGpuiGraphMetrics::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_metrics_match_the_native_chrome_contract() {
        let metrics = OpenGpuiGraphMetrics::default();

        assert_eq!(metrics.grid_line_width, 1.0);
        assert_eq!(metrics.guide_width, 1.0);
        assert_eq!(metrics.node_corner_radius, 6.0);
        assert_eq!(metrics.port_marker_diameter, 8.0);
        assert_eq!(metrics.port_hover_marker_diameter, 10.0);
        assert_eq!(metrics.port_layout_diameter, 12.0);
        assert_eq!(metrics.port_hit_diameter, 22.0);
        assert_eq!(metrics.port_outline_width, 1.0);
        assert_eq!(metrics.port_emphasized_outline_width, 1.5);
        assert_eq!(metrics.port_focus_ring_width, 1.0);
        assert_eq!(metrics.edge_ordinary_width, 1.5);
        assert_eq!(metrics.edge_hovered_width, 2.0);
        assert_eq!(metrics.edge_selected_width, 2.5);
        assert_eq!(metrics.edge_active_width, 2.5);
        assert_eq!(metrics.edge_strong_ordinary_width, 2.0);
        assert_eq!(metrics.edge_strong_hovered_width, 2.5);
        assert_eq!(metrics.edge_strong_selected_width, 3.0);
        assert_eq!(metrics.edge_strong_active_width, 3.0);
        assert_eq!(metrics.reconnect_cap_length, 9.0);
        assert_eq!(metrics.reconnect_hit_diameter, 22.0);
        assert_eq!(metrics.edge_marker_diameter, 8.0);
        assert_eq!(metrics.edge_label_max_width, 120.0);
        assert_eq!(metrics.edge_label_font_size, 11.0);
        assert_eq!(metrics.edge_label_horizontal_padding, 5.0);
        assert_eq!(metrics.edge_label_vertical_padding, 2.0);
        assert_eq!(metrics.edge_label_corner_radius, 4.0);
        assert!(metrics.is_valid());
    }

    #[test]
    fn invalid_hit_or_hover_relationships_are_rejected() {
        let metrics = OpenGpuiGraphMetrics {
            port_hit_diameter: 9.0,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            port_layout_diameter: 9.0,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            reconnect_hit_diameter: 8.0,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            port_emphasized_outline_width: 0.5,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            node_corner_radius: -1.0,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());
    }

    #[test]
    fn edge_widths_preserve_the_visual_emphasis_hierarchy() {
        let metrics = OpenGpuiGraphMetrics {
            edge_hovered_width: OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            edge_active_width: OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            edge_strong_hovered_width: OPEN_GPUI_DEFAULT_EDGE_STRONG_ORDINARY_WIDTH,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            edge_strong_selected_width: OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            edge_strong_active_width: f32::NAN,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());

        let metrics = OpenGpuiGraphMetrics {
            edge_strong_ordinary_width: 1.0,
            ..OpenGpuiGraphMetrics::default()
        };
        assert!(!metrics.is_valid());
    }

    #[test]
    fn default_palette_keeps_state_colors_distinct() {
        let palette = OpenGpuiGraphPalette::default();

        assert_ne!(palette.edge_default, palette.edge_data);
        assert_ne!(palette.edge_data, palette.edge_control);
        assert_ne!(palette.port_valid, palette.port_invalid);
        assert!(palette.edge_default.is_opaque());
        assert!(!palette.edge_hover_tint.is_opaque());
    }

    #[test]
    fn sanitization_preserves_palette_and_replaces_invalid_metrics_atomically() {
        let palette = OpenGpuiGraphPalette {
            canvas: rgb(0x010203).into(),
            ..OpenGpuiGraphPalette::default()
        };
        let theme = OpenGpuiGraphTheme::new(
            palette,
            OpenGpuiGraphMetrics {
                reconnect_cap_length: -1.0,
                ..OpenGpuiGraphMetrics::default()
            },
        )
        .sanitized();

        assert_eq!(theme.palette, palette);
        assert_eq!(theme.metrics, OpenGpuiGraphMetrics::default());
    }

    #[test]
    fn style_slots_are_bounded_and_unregistered_slots_stay_empty() {
        let first = OpenGpuiGraphStyleSlot::new(0).expect("first style slot");
        let last = OpenGpuiGraphStyleSlot::new(15).expect("last style slot");
        assert!(OpenGpuiGraphStyleSlot::new(16).is_none());

        let recipe = OpenGpuiGraphStyleRecipe::new(rgb(0xe8b95c).into())
            .with_cue(OpenGpuiGraphStyleCue::Dotted)
            .with_port_marker_shape(OpenGpuiPortMarkerShape::Diamond);
        let theme = OpenGpuiGraphTheme::default().with_style_slot(last, recipe);

        assert_eq!(theme.style_slot(first), None);
        assert_eq!(theme.style_slot(last), Some(recipe));
    }

    #[test]
    fn marker_shape_profiles_are_finite_and_survive_metric_sanitization() {
        let shapes = OpenGpuiPortMarkerShapes {
            default: OpenGpuiPortMarkerShape::Circle,
            data: OpenGpuiPortMarkerShape::Diamond,
            control: OpenGpuiPortMarkerShape::Square,
        };
        let theme = OpenGpuiGraphTheme::new(
            OpenGpuiGraphPalette::default(),
            OpenGpuiGraphMetrics {
                port_hit_diameter: -1.0,
                ..OpenGpuiGraphMetrics::default()
            },
        )
        .with_port_marker_shapes(shapes)
        .sanitized();

        assert_eq!(theme.port_marker_shapes, shapes);
        assert_eq!(theme.metrics, OpenGpuiGraphMetrics::default());
    }

    #[test]
    fn sanitization_preserves_bounded_style_recipes() {
        let slot = OpenGpuiGraphStyleSlot::new(3).expect("in-range style slot");
        let recipe = OpenGpuiGraphStyleRecipe::new(rgb(0x7cc7ff).into())
            .with_cue(OpenGpuiGraphStyleCue::Dashed);
        let theme = OpenGpuiGraphTheme::new(
            OpenGpuiGraphPalette::default(),
            OpenGpuiGraphMetrics {
                reconnect_hit_diameter: -1.0,
                ..OpenGpuiGraphMetrics::default()
            },
        )
        .with_style_slot(slot, recipe)
        .sanitized();

        assert_eq!(theme.style_slot(slot), Some(recipe));
        assert_eq!(theme.metrics, OpenGpuiGraphMetrics::default());
    }
}
