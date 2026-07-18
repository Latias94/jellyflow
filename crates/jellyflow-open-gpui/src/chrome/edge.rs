//! Immutable style resolution and batch keys for native edge chrome.

use open_gpui::Hsla;

use crate::theme::{
    OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH, OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
    OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH, OpenGpuiGraphPalette, OpenGpuiGraphTheme,
};

/// Semantic visual state of a native edge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeVisualState {
    /// Ordinary committed edge.
    #[default]
    Ordinary,
    /// Edge currently under the pointer.
    Hovered,
    /// Selected committed edge.
    Selected,
    /// Edge participating in an active reconnect or commit presentation.
    Active,
    /// Adapter-owned connection preview.
    Preview,
}

/// Explicit paint layer for batchable native edge chrome.
///
/// Variant order is paint order: later variants must be painted over earlier variants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeLayer {
    /// Ordinary committed edges.
    #[default]
    Ordinary,
    /// Hovered committed edges.
    Hovered,
    /// Selected committed edges.
    Selected,
    /// Active reconnect or commit presentation.
    Active,
    /// Adapter-owned connection preview.
    Preview,
}

impl From<OpenGpuiEdgeVisualState> for OpenGpuiEdgeLayer {
    fn from(state: OpenGpuiEdgeVisualState) -> Self {
        match state {
            OpenGpuiEdgeVisualState::Ordinary => Self::Ordinary,
            OpenGpuiEdgeVisualState::Hovered => Self::Hovered,
            OpenGpuiEdgeVisualState::Selected => Self::Selected,
            OpenGpuiEdgeVisualState::Active => Self::Active,
            OpenGpuiEdgeVisualState::Preview => Self::Preview,
        }
    }
}

/// Normalized semantic tone selected by an edge style token.
///
/// The finite set keeps paint batching bounded. Unknown tokens resolve to [`Self::Default`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeTone {
    /// Stable fallback for absent or unknown tokens.
    #[default]
    Default,
    /// Typed data flow.
    Data,
    /// Execution or control flow.
    Control,
    /// Successful result or accepted branch.
    Success,
    /// Warning or conditional branch.
    Warning,
    /// Error or destructive flow.
    Danger,
}

impl OpenGpuiEdgeTone {
    /// Resolves a persisted style token into a bounded native tone.
    #[must_use]
    pub fn from_style_token(style_token: Option<&str>) -> Self {
        match style_token.map(str::trim).filter(|token| !token.is_empty()) {
            Some("data") | Some("data.primary") | Some("data.secondary") => Self::Data,
            Some("control") | Some("control.flow") | Some("flow.control") => Self::Control,
            Some("success") | Some("flow.success") => Self::Success,
            Some("warning") | Some("flow.warning") => Self::Warning,
            Some("danger") | Some("error") | Some("flow.danger") | Some("flow.error") => {
                Self::Danger
            }
            _ => Self::Default,
        }
    }

    fn color(self, palette: OpenGpuiGraphPalette) -> Hsla {
        match self {
            Self::Default => palette.edge_default,
            Self::Data => palette.edge_data,
            Self::Control => palette.edge_control,
            Self::Success => palette.edge_success,
            Self::Warning => palette.edge_warning,
            Self::Danger => palette.edge_danger,
        }
    }
}

/// Fully resolved immutable paint style for one native edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiEdgeStyle {
    color: Hsla,
    stroke_width: f32,
    layer: OpenGpuiEdgeLayer,
    tone: OpenGpuiEdgeTone,
}

impl OpenGpuiEdgeStyle {
    /// Returns the color passed to Open GPUI path painting.
    #[must_use]
    pub const fn color(self) -> Hsla {
        self.color
    }

    /// Returns the visual stroke width in screen pixels.
    #[must_use]
    pub const fn stroke_width(self) -> f32 {
        self.stroke_width
    }

    /// Returns the explicit paint layer.
    #[must_use]
    pub const fn layer(self) -> OpenGpuiEdgeLayer {
        self.layer
    }

    /// Returns the normalized semantic tone.
    #[must_use]
    pub const fn tone(self) -> OpenGpuiEdgeTone {
        self.tone
    }

    /// Returns the allocation-free key used to group compatible paint work.
    #[must_use]
    pub fn batch_key(self) -> OpenGpuiEdgeBatchKey {
        OpenGpuiEdgeBatchKey::from_style(self)
    }
}

/// Allocation-free grouping key for one native edge paint batch.
///
/// The key contains paint facts only. It deliberately owns no edge id, framework entity, retained
/// element, callback, or arbitrary style-token string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiEdgeBatchKey {
    layer: OpenGpuiEdgeLayer,
    color: Hsla,
    stroke_width_bits: u32,
}

impl OpenGpuiEdgeBatchKey {
    /// Creates a batch key from an immutable resolved style.
    #[must_use]
    pub fn from_style(style: OpenGpuiEdgeStyle) -> Self {
        Self {
            layer: style.layer,
            color: style.color,
            stroke_width_bits: style.stroke_width.to_bits(),
        }
    }

    /// Returns the batch paint layer.
    #[must_use]
    pub const fn layer(self) -> OpenGpuiEdgeLayer {
        self.layer
    }

    /// Returns the batch path color.
    #[must_use]
    pub const fn color(self) -> Hsla {
        self.color
    }

    /// Returns the batch stroke width in screen pixels.
    #[must_use]
    pub fn stroke_width(self) -> f32 {
        f32::from_bits(self.stroke_width_bits)
    }
}

impl OpenGpuiGraphTheme {
    /// Resolves native edge chrome from visual state and an optional persisted style token.
    ///
    /// Unknown tokens deliberately share the default tone and batch key. State tinting preserves a
    /// hint of the semantic tone while making hover, selection, active, and preview states obvious.
    #[must_use]
    pub fn resolve_edge_style(
        &self,
        state: OpenGpuiEdgeVisualState,
        style_token: Option<&str>,
    ) -> OpenGpuiEdgeStyle {
        let tone = OpenGpuiEdgeTone::from_style_token(style_token);
        let base = tone.color(self.palette);
        let (color, configured_width) = match state {
            OpenGpuiEdgeVisualState::Ordinary => (base, self.metrics.edge_ordinary_width),
            OpenGpuiEdgeVisualState::Hovered => (
                base.blend(self.palette.edge_hover_tint),
                self.metrics.edge_hovered_width,
            ),
            OpenGpuiEdgeVisualState::Selected => (
                base.blend(self.palette.edge_selection_tint),
                self.metrics.edge_selected_width,
            ),
            OpenGpuiEdgeVisualState::Active => (
                base.blend(self.palette.edge_active_tint),
                self.metrics.edge_active_width,
            ),
            OpenGpuiEdgeVisualState::Preview => (
                base.blend(self.palette.edge_preview_tint),
                self.metrics.edge_active_width,
            ),
        };
        let fallback_width = match state {
            OpenGpuiEdgeVisualState::Ordinary => OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH,
            OpenGpuiEdgeVisualState::Hovered => OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
            OpenGpuiEdgeVisualState::Selected
            | OpenGpuiEdgeVisualState::Active
            | OpenGpuiEdgeVisualState::Preview => OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
        };

        OpenGpuiEdgeStyle {
            color,
            stroke_width: positive_or(configured_width, fallback_width),
            layer: state.into(),
            tone,
        }
    }
}

fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_states_resolve_to_ordered_layers_and_expected_widths() {
        let theme = OpenGpuiGraphTheme::default();
        let cases = [
            (
                OpenGpuiEdgeVisualState::Ordinary,
                OpenGpuiEdgeLayer::Ordinary,
                1.5,
            ),
            (
                OpenGpuiEdgeVisualState::Hovered,
                OpenGpuiEdgeLayer::Hovered,
                2.0,
            ),
            (
                OpenGpuiEdgeVisualState::Selected,
                OpenGpuiEdgeLayer::Selected,
                2.5,
            ),
            (
                OpenGpuiEdgeVisualState::Active,
                OpenGpuiEdgeLayer::Active,
                2.5,
            ),
            (
                OpenGpuiEdgeVisualState::Preview,
                OpenGpuiEdgeLayer::Preview,
                2.5,
            ),
        ];

        for (state, layer, width) in cases {
            let style = theme.resolve_edge_style(state, None);
            assert_eq!(style.layer(), layer);
            assert_eq!(style.stroke_width(), width);
        }
        assert!(OpenGpuiEdgeLayer::Ordinary < OpenGpuiEdgeLayer::Hovered);
        assert!(OpenGpuiEdgeLayer::Hovered < OpenGpuiEdgeLayer::Selected);
        assert!(OpenGpuiEdgeLayer::Selected < OpenGpuiEdgeLayer::Active);
        assert!(OpenGpuiEdgeLayer::Active < OpenGpuiEdgeLayer::Preview);
    }

    #[test]
    fn known_tokens_resolve_to_bounded_semantic_tones() {
        assert_eq!(
            OpenGpuiEdgeTone::from_style_token(Some("data.primary")),
            OpenGpuiEdgeTone::Data
        );
        assert_eq!(
            OpenGpuiEdgeTone::from_style_token(Some("flow.control")),
            OpenGpuiEdgeTone::Control
        );
        assert_eq!(
            OpenGpuiEdgeTone::from_style_token(Some("flow.error")),
            OpenGpuiEdgeTone::Danger
        );
    }

    #[test]
    fn absent_blank_and_unknown_tokens_share_a_stable_fallback() {
        let theme = OpenGpuiGraphTheme::default();
        let absent = theme.resolve_edge_style(OpenGpuiEdgeVisualState::Ordinary, None);
        let blank = theme.resolve_edge_style(OpenGpuiEdgeVisualState::Ordinary, Some("  "));
        let unknown = theme.resolve_edge_style(
            OpenGpuiEdgeVisualState::Ordinary,
            Some("plugin.unregistered"),
        );

        assert_eq!(absent, blank);
        assert_eq!(blank, unknown);
        assert_eq!(absent.tone(), OpenGpuiEdgeTone::Default);
        assert_eq!(absent.batch_key(), unknown.batch_key());
    }

    #[test]
    fn state_tints_preserve_token_specific_paint_batches() {
        let theme = OpenGpuiGraphTheme::default();
        let selected_data =
            theme.resolve_edge_style(OpenGpuiEdgeVisualState::Selected, Some("data.primary"));
        let selected_control =
            theme.resolve_edge_style(OpenGpuiEdgeVisualState::Selected, Some("flow.control"));

        assert_ne!(selected_data.color(), selected_control.color());
        assert_ne!(selected_data.batch_key(), selected_control.batch_key());
    }

    #[test]
    fn invalid_custom_widths_fall_back_without_panicking() {
        let mut theme = OpenGpuiGraphTheme::default();
        theme.metrics.edge_hovered_width = f32::NAN;

        let style = theme.resolve_edge_style(OpenGpuiEdgeVisualState::Hovered, None);
        assert_eq!(style.stroke_width(), 2.0);
    }
}
