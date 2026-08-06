//! Immutable style resolution and batch keys for native edge chrome.

use open_gpui::Hsla;

use crate::theme::{
    OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH, OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
    OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH, OPEN_GPUI_DEFAULT_EDGE_STRONG_EMPHASIZED_WIDTH,
    OPEN_GPUI_DEFAULT_EDGE_STRONG_HOVERED_WIDTH, OPEN_GPUI_DEFAULT_EDGE_STRONG_ORDINARY_WIDTH,
    OpenGpuiGraphPalette, OpenGpuiGraphStyleSlot, OpenGpuiGraphTheme,
};

use super::positive_or;

/// Orthogonal, allocation-free visual facts for one native edge.
///
/// Pointer, selection, keyboard focus, active interaction, and preview ownership remain separate
/// facts. Resolution composes them into one bounded halo recipe while the typed ordinary stroke is
/// retained as an independent base layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiEdgeVisualFacts {
    hovered: bool,
    selected: bool,
    focused: bool,
    active: bool,
    preview: bool,
}

impl OpenGpuiEdgeVisualFacts {
    /// Returns whether the pointer currently emphasizes this edge.
    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    /// Returns whether this edge belongs to the current selection.
    #[must_use]
    pub const fn selected(self) -> bool {
        self.selected
    }

    /// Returns whether keyboard focus currently targets this edge.
    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    /// Returns whether this edge participates in an active graph interaction.
    #[must_use]
    pub const fn active(self) -> bool {
        self.active
    }

    /// Returns whether this edge is an adapter-owned connection preview.
    #[must_use]
    pub const fn preview(self) -> bool {
        self.preview
    }

    /// Sets pointer emphasis without replacing any other fact.
    #[must_use]
    pub const fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Sets selection membership without replacing any other fact.
    #[must_use]
    pub const fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Sets keyboard focus without replacing any other fact.
    #[must_use]
    pub const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Sets active-interaction emphasis without replacing any other fact.
    #[must_use]
    pub const fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Sets preview ownership without replacing any other fact.
    #[must_use]
    pub const fn with_preview(mut self, preview: bool) -> Self {
        self.preview = preview;
        self
    }

    /// Returns whether at least one non-base visual layer is present.
    #[must_use]
    pub const fn has_emphasis(self) -> bool {
        self.hovered || self.selected || self.focused || self.active || self.preview
    }

    /// Returns the deterministic topmost semantic layer for the composed halo.
    ///
    /// Priority is preview, active interaction, keyboard focus, selection, then hover.
    #[must_use]
    pub const fn dominant_layer(self) -> OpenGpuiEdgeLayer {
        if self.preview {
            OpenGpuiEdgeLayer::Preview
        } else if self.active {
            OpenGpuiEdgeLayer::Active
        } else if self.focused {
            OpenGpuiEdgeLayer::Focused
        } else if self.selected {
            OpenGpuiEdgeLayer::Selected
        } else if self.hovered {
            OpenGpuiEdgeLayer::Hovered
        } else {
            OpenGpuiEdgeLayer::Base
        }
    }
}

/// Closed screen-space weight vocabulary for native edge painters.
///
/// Tiers select one of the theme's two validated width tables. They cannot carry an arbitrary
/// width or callback, so application edge recipes retain a finite paint vocabulary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeWeightTier {
    /// Standard data-flow weight.
    #[default]
    Regular,
    /// Emphasized control-flow weight.
    Strong,
}

/// Explicit paint layer for batchable native edge chrome.
///
/// Variant order is paint order: later variants must be painted over earlier variants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeLayer {
    /// Hovered committed edges.
    Hovered,
    /// Selected committed edges.
    Selected,
    /// Keyboard-focused committed edges.
    Focused,
    /// Active reconnect or commit presentation.
    Active,
    /// Adapter-owned connection preview.
    Preview,
    /// Typed ordinary stroke retained and painted over every halo.
    #[default]
    Base,
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
    tone: OpenGpuiEdgeTone,
    style_slot: Option<OpenGpuiGraphStyleSlot>,
    weight_tier: OpenGpuiEdgeWeightTier,
    visual_facts: OpenGpuiEdgeVisualFacts,
    halo: Option<OpenGpuiEdgeHaloStyle>,
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

    /// Returns the typed base paint layer.
    #[must_use]
    pub const fn layer(self) -> OpenGpuiEdgeLayer {
        OpenGpuiEdgeLayer::Base
    }

    /// Returns the normalized semantic tone.
    #[must_use]
    pub const fn tone(self) -> OpenGpuiEdgeTone {
        self.tone
    }

    /// Returns the registered application-native style slot used by this edge.
    ///
    /// An absent or theme-unregistered slot resolves to `None` and uses the semantic tone.
    #[must_use]
    pub const fn style_slot(self) -> Option<OpenGpuiGraphStyleSlot> {
        self.style_slot
    }

    /// Returns the closed weight tier that selected this style's screen-space width table.
    #[must_use]
    pub const fn weight_tier(self) -> OpenGpuiEdgeWeightTier {
        self.weight_tier
    }

    /// Returns every orthogonal fact used to compose the optional halo.
    #[must_use]
    pub const fn visual_facts(self) -> OpenGpuiEdgeVisualFacts {
        self.visual_facts
    }

    /// Returns the optional composed halo painted behind the typed base stroke.
    #[must_use]
    pub const fn halo(self) -> Option<OpenGpuiEdgeHaloStyle> {
        self.halo
    }

    /// Reapplies visual facts while preserving semantic tone, style slot, and weight tier.
    ///
    /// Stroke widths remain screen-space metrics. Callers must project route geometry before
    /// painting and must not multiply this style by canvas zoom.
    #[must_use]
    pub fn with_visual_facts(
        self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        theme: &OpenGpuiGraphTheme,
    ) -> Self {
        theme.resolve_edge_tone_style(visual_facts, self.tone, self.style_slot, self.weight_tier)
    }

    /// Returns the allocation-free key used to group compatible paint work.
    #[must_use]
    pub fn batch_key(self) -> OpenGpuiEdgeBatchKey {
        OpenGpuiEdgeBatchKey::from_style(self)
    }

    /// Returns the allocation-free halo key when at least one emphasis fact is active.
    #[must_use]
    pub fn halo_batch_key(self) -> Option<OpenGpuiEdgeBatchKey> {
        self.halo.map(OpenGpuiEdgeBatchKey::from_halo)
    }
}

/// Fully resolved, finite halo painted behind an edge's typed base stroke.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiEdgeHaloStyle {
    color: Hsla,
    stroke_width: f32,
    layer: OpenGpuiEdgeLayer,
}

impl OpenGpuiEdgeHaloStyle {
    /// Returns the composed halo color.
    #[must_use]
    pub const fn color(self) -> Hsla {
        self.color
    }

    /// Returns the halo width in screen pixels.
    #[must_use]
    pub const fn stroke_width(self) -> f32 {
        self.stroke_width
    }

    /// Returns the deterministic dominant semantic layer.
    #[must_use]
    pub const fn layer(self) -> OpenGpuiEdgeLayer {
        self.layer
    }

    /// Returns the allocation-free key used to batch this halo.
    #[must_use]
    pub fn batch_key(self) -> OpenGpuiEdgeBatchKey {
        OpenGpuiEdgeBatchKey::from_halo(self)
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
            layer: OpenGpuiEdgeLayer::Base,
            color: style.color,
            stroke_width_bits: style.stroke_width.to_bits(),
        }
    }

    fn from_halo(halo: OpenGpuiEdgeHaloStyle) -> Self {
        Self {
            layer: halo.layer,
            color: halo.color,
            stroke_width_bits: halo.stroke_width.to_bits(),
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
    /// Resolves native edge chrome from orthogonal visual facts and an optional style token.
    ///
    /// Unknown tokens deliberately share the default tone and batch key. State tinting preserves a
    /// hint of the semantic tone while making hover, selection, active, and preview states obvious.
    #[must_use]
    pub fn resolve_edge_style(
        &self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        style_token: Option<&str>,
    ) -> OpenGpuiEdgeStyle {
        self.resolve_edge_style_with_weight_tier(
            visual_facts,
            style_token,
            OpenGpuiEdgeWeightTier::Regular,
        )
    }

    /// Resolves native edge chrome from a closed weight tier and optional persisted style token.
    ///
    /// Unknown tokens still share the default tone. `weight_tier` selects only a validated theme
    /// metric table and never introduces arbitrary edge-specific paint state.
    #[must_use]
    pub fn resolve_edge_style_with_weight_tier(
        &self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        style_token: Option<&str>,
        weight_tier: OpenGpuiEdgeWeightTier,
    ) -> OpenGpuiEdgeStyle {
        let tone = OpenGpuiEdgeTone::from_style_token(style_token);
        self.resolve_edge_tone_style(visual_facts, tone, None, weight_tier)
    }

    /// Resolves native edge chrome from a bounded application-native style slot.
    ///
    /// The semantic tone remains the deterministic fallback when the theme did not register the
    /// requested slot. No arbitrary application token enters the resulting batch key.
    #[must_use]
    pub fn resolve_edge_style_slot(
        &self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        tone: OpenGpuiEdgeTone,
        style_slot: Option<OpenGpuiGraphStyleSlot>,
    ) -> OpenGpuiEdgeStyle {
        self.resolve_edge_style_slot_with_weight_tier(
            visual_facts,
            tone,
            style_slot,
            OpenGpuiEdgeWeightTier::Regular,
        )
    }

    /// Resolves native edge chrome from a bounded style slot and closed weight tier.
    #[must_use]
    pub fn resolve_edge_style_slot_with_weight_tier(
        &self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        tone: OpenGpuiEdgeTone,
        style_slot: Option<OpenGpuiGraphStyleSlot>,
        weight_tier: OpenGpuiEdgeWeightTier,
    ) -> OpenGpuiEdgeStyle {
        self.resolve_edge_tone_style(visual_facts, tone, style_slot, weight_tier)
    }

    fn resolve_edge_tone_style(
        &self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        tone: OpenGpuiEdgeTone,
        requested_slot: Option<OpenGpuiGraphStyleSlot>,
        weight_tier: OpenGpuiEdgeWeightTier,
    ) -> OpenGpuiEdgeStyle {
        let style_slot = requested_slot.filter(|slot| self.style_slot(*slot).is_some());
        let base = style_slot
            .and_then(|slot| self.style_slot(slot))
            .map_or_else(|| tone.color(self.palette), |recipe| recipe.color());
        let (ordinary_width, hovered_width, selected_width, active_width, fallback_widths) =
            match weight_tier {
                OpenGpuiEdgeWeightTier::Regular => (
                    self.metrics.edge_ordinary_width,
                    self.metrics.edge_hovered_width,
                    self.metrics.edge_selected_width,
                    self.metrics.edge_active_width,
                    (
                        OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH,
                        OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
                        OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH,
                    ),
                ),
                OpenGpuiEdgeWeightTier::Strong => (
                    self.metrics.edge_strong_ordinary_width,
                    self.metrics.edge_strong_hovered_width,
                    self.metrics.edge_strong_selected_width,
                    self.metrics.edge_strong_active_width,
                    (
                        OPEN_GPUI_DEFAULT_EDGE_STRONG_ORDINARY_WIDTH,
                        OPEN_GPUI_DEFAULT_EDGE_STRONG_HOVERED_WIDTH,
                        OPEN_GPUI_DEFAULT_EDGE_STRONG_EMPHASIZED_WIDTH,
                    ),
                ),
            };
        let base_width = positive_or(ordinary_width, fallback_widths.0);
        let hovered_width = positive_or(hovered_width, fallback_widths.1);
        let selected_width = positive_or(selected_width, fallback_widths.2);
        let active_width = positive_or(active_width, fallback_widths.2);
        let halo = visual_facts.has_emphasis().then(|| {
            let layer = visual_facts.dominant_layer();
            let (tint, stroke_width) = match layer {
                OpenGpuiEdgeLayer::Hovered => (self.palette.edge_hover_tint, hovered_width),
                OpenGpuiEdgeLayer::Selected => (self.palette.edge_selection_tint, selected_width),
                OpenGpuiEdgeLayer::Focused => (self.palette.focus.opacity(0.72), selected_width),
                OpenGpuiEdgeLayer::Active => (self.palette.edge_active_tint, active_width),
                OpenGpuiEdgeLayer::Preview => (self.palette.edge_preview_tint, active_width),
                OpenGpuiEdgeLayer::Base => {
                    unreachable!("emphasis cannot resolve to the base layer")
                }
            };
            OpenGpuiEdgeHaloStyle {
                color: base.blend(tint),
                stroke_width: stroke_width.max(base_width),
                layer,
            }
        });

        OpenGpuiEdgeStyle {
            color: base,
            stroke_width: base_width,
            tone,
            style_slot,
            weight_tier,
            visual_facts,
            halo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_facts_are_orthogonal_and_have_deterministic_layer_priority() {
        let facts = OpenGpuiEdgeVisualFacts::default()
            .with_hovered(true)
            .with_selected(true)
            .with_focused(true)
            .with_active(true)
            .with_preview(true);

        assert!(facts.hovered());
        assert!(facts.selected());
        assert!(facts.focused());
        assert!(facts.active());
        assert!(facts.preview());
        assert_eq!(facts.dominant_layer(), OpenGpuiEdgeLayer::Preview);
        assert_eq!(
            facts.with_preview(false).dominant_layer(),
            OpenGpuiEdgeLayer::Active
        );
        assert_eq!(
            facts
                .with_preview(false)
                .with_active(false)
                .dominant_layer(),
            OpenGpuiEdgeLayer::Focused
        );
        assert!(OpenGpuiEdgeLayer::Hovered < OpenGpuiEdgeLayer::Selected);
        assert!(OpenGpuiEdgeLayer::Preview < OpenGpuiEdgeLayer::Base);
    }

    #[test]
    fn emphasis_adds_a_halo_without_replacing_the_typed_base_stroke() {
        let theme = OpenGpuiGraphTheme::default();
        let ordinary =
            theme.resolve_edge_style(OpenGpuiEdgeVisualFacts::default(), Some("data.primary"));
        let emphasized = ordinary.with_visual_facts(
            OpenGpuiEdgeVisualFacts::default()
                .with_hovered(true)
                .with_selected(true),
            &theme,
        );

        assert_eq!(emphasized.color(), ordinary.color());
        assert_eq!(emphasized.stroke_width(), ordinary.stroke_width());
        assert_eq!(emphasized.layer(), OpenGpuiEdgeLayer::Base);
        assert_eq!(emphasized.batch_key(), ordinary.batch_key());
        let halo = emphasized.halo().expect("selected hover halo");
        assert_eq!(halo.layer(), OpenGpuiEdgeLayer::Selected);
        assert_eq!(halo.stroke_width(), 2.5);
        assert_ne!(halo.color(), emphasized.color());
        assert_ne!(halo.batch_key(), emphasized.batch_key());
    }

    #[test]
    fn weight_tiers_change_only_finite_base_and_halo_width_tables() {
        let theme = OpenGpuiGraphTheme::default();
        let facts = OpenGpuiEdgeVisualFacts::default().with_active(true);
        let regular = theme.resolve_edge_style_with_weight_tier(
            facts,
            Some("control"),
            OpenGpuiEdgeWeightTier::Regular,
        );
        let strong = theme.resolve_edge_style_with_weight_tier(
            facts,
            Some("control"),
            OpenGpuiEdgeWeightTier::Strong,
        );

        assert_eq!(regular.stroke_width(), 1.5);
        assert_eq!(regular.halo().expect("regular halo").stroke_width(), 2.5);
        assert_eq!(strong.stroke_width(), 2.0);
        assert_eq!(strong.halo().expect("strong halo").stroke_width(), 3.0);
        assert_eq!(strong.weight_tier(), OpenGpuiEdgeWeightTier::Strong);
    }

    #[test]
    fn absent_blank_and_unknown_tokens_share_a_stable_fallback() {
        let theme = OpenGpuiGraphTheme::default();
        let facts = OpenGpuiEdgeVisualFacts::default().with_selected(true);
        let absent = theme.resolve_edge_style(facts, None);
        let blank = theme.resolve_edge_style(facts, Some("  "));
        let unknown = theme.resolve_edge_style(facts, Some("plugin.unregistered"));

        assert_eq!(absent, blank);
        assert_eq!(blank, unknown);
        assert_eq!(absent.tone(), OpenGpuiEdgeTone::Default);
        assert_eq!(absent.batch_key(), unknown.batch_key());
    }

    #[test]
    fn dominant_layer_is_the_only_halo_tint_and_width_authority() {
        let theme = OpenGpuiGraphTheme::default();
        let combined = theme.resolve_edge_style(
            OpenGpuiEdgeVisualFacts::default()
                .with_hovered(true)
                .with_selected(true)
                .with_focused(true)
                .with_active(true),
            Some("data"),
        );
        let active_only = theme.resolve_edge_style(
            OpenGpuiEdgeVisualFacts::default().with_active(true),
            Some("data"),
        );

        assert_eq!(combined.color(), active_only.color());
        assert_eq!(combined.halo(), active_only.halo());
        assert_eq!(
            combined.halo().expect("combined active halo").layer(),
            OpenGpuiEdgeLayer::Active
        );
    }

    #[test]
    fn invalid_custom_widths_fall_back_without_panicking() {
        let mut theme = OpenGpuiGraphTheme::default();
        theme.metrics.edge_hovered_width = f32::NAN;

        let style =
            theme.resolve_edge_style(OpenGpuiEdgeVisualFacts::default().with_hovered(true), None);
        assert_eq!(style.stroke_width(), 1.5);
        assert_eq!(style.halo().expect("hover halo").stroke_width(), 2.0);
    }

    #[test]
    fn registered_style_slot_survives_composed_visual_facts() {
        let slot = OpenGpuiGraphStyleSlot::new(2).expect("in-range style slot");
        let color = open_gpui::rgb(0xe8b95c).into();
        let theme = OpenGpuiGraphTheme::default()
            .with_style_slot(slot, crate::theme::OpenGpuiGraphStyleRecipe::new(color));
        let preview = theme.resolve_edge_style_slot(
            OpenGpuiEdgeVisualFacts::default()
                .with_selected(true)
                .with_preview(true),
            OpenGpuiEdgeTone::Data,
            Some(slot),
        );

        assert_eq!(preview.color(), color);
        assert_eq!(preview.style_slot(), Some(slot));
        assert_eq!(preview.tone(), OpenGpuiEdgeTone::Data);
        assert_eq!(
            preview.halo().expect("preview halo").layer(),
            OpenGpuiEdgeLayer::Preview
        );
    }
}
