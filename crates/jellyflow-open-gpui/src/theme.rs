//! Adapter-local visual defaults for native graph chrome.

use open_gpui::{Hsla, rgb, rgba};

/// Default diameter of an idle native port marker in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER: f32 = 10.0;

/// Default diameter of a hovered native port marker in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER: f32 = 12.0;

/// Default minimum port hit diameter in screen pixels.
pub const OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER: f32 = 22.0;

/// Default ordinary edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH: f32 = 1.5;

/// Default canvas-grid stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_GRID_LINE_WIDTH: f32 = 1.0;

/// Default alignment-guide stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_GUIDE_WIDTH: f32 = 1.0;

/// Default hovered edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH: f32 = 2.0;

/// Default selected and active edge stroke width in screen pixels.
pub const OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH: f32 = 2.5;

/// Default reconnect handle diameter in screen pixels.
pub const OPEN_GPUI_DEFAULT_RECONNECT_HANDLE_DIAMETER: f32 = 9.0;

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
/// view space before applying them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiGraphMetrics {
    /// Canvas-grid stroke width.
    pub grid_line_width: f32,
    /// Alignment-guide stroke width.
    pub guide_width: f32,
    /// Idle port marker diameter.
    pub port_marker_diameter: f32,
    /// Hovered port marker diameter.
    pub port_hover_marker_diameter: f32,
    /// Minimum port hit diameter.
    pub port_hit_diameter: f32,
    /// Ordinary edge stroke width.
    pub edge_ordinary_width: f32,
    /// Hovered edge stroke width.
    pub edge_hovered_width: f32,
    /// Selected edge stroke width.
    pub edge_selected_width: f32,
    /// Active and preview edge stroke width.
    pub edge_active_width: f32,
    /// Reconnect handle visual diameter.
    pub reconnect_handle_diameter: f32,
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
            port_marker_diameter: OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER,
            port_hover_marker_diameter: OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER,
            port_hit_diameter: OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
            edge_ordinary_width: OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH,
            edge_hovered_width: OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
            edge_selected_width: OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
            edge_active_width: OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
            reconnect_handle_diameter: OPEN_GPUI_DEFAULT_RECONNECT_HANDLE_DIAMETER,
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
            self.port_hit_diameter,
            self.edge_ordinary_width,
            self.edge_hovered_width,
            self.edge_selected_width,
            self.edge_active_width,
            self.reconnect_handle_diameter,
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
            && self.port_hit_diameter >= self.port_hover_marker_diameter
            && self.reconnect_hit_diameter >= self.reconnect_handle_diameter
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
}

impl OpenGpuiGraphTheme {
    /// Creates a theme from an application palette and screen-space metrics.
    #[must_use]
    pub const fn new(palette: OpenGpuiGraphPalette, metrics: OpenGpuiGraphMetrics) -> Self {
        Self { palette, metrics }
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
        assert_eq!(metrics.port_marker_diameter, 10.0);
        assert_eq!(metrics.port_hover_marker_diameter, 12.0);
        assert_eq!(metrics.port_hit_diameter, 22.0);
        assert_eq!(metrics.edge_ordinary_width, 1.5);
        assert_eq!(metrics.edge_hovered_width, 2.0);
        assert_eq!(metrics.edge_selected_width, 2.5);
        assert_eq!(metrics.edge_active_width, 2.5);
        assert_eq!(metrics.reconnect_handle_diameter, 9.0);
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
            reconnect_hit_diameter: 8.0,
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
                reconnect_handle_diameter: -1.0,
                ..OpenGpuiGraphMetrics::default()
            },
        )
        .sanitized();

        assert_eq!(theme.palette, palette);
        assert_eq!(theme.metrics, OpenGpuiGraphMetrics::default());
    }
}
