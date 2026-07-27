//! Bounded renderer-key lookup and immutable recipes for native edge chrome.

use std::collections::BTreeMap;

use jellyflow_core::{EdgeKind, EdgeLabelAnchor, TypeDesc};
use jellyflow_runtime::geometry::EdgePresentationFacts;

use super::edge::{
    OpenGpuiEdgeBatchKey, OpenGpuiEdgeStyle, OpenGpuiEdgeTone, OpenGpuiEdgeVisualFacts,
    OpenGpuiEdgeWeightTier,
};
use crate::theme::{OpenGpuiGraphStyleCue, OpenGpuiGraphStyleSlot, OpenGpuiGraphTheme};

/// Maximum number of application edge painters retained by one registry.
pub const OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS: usize = 64;

/// Maximum number of renderer-neutral graph types mapped to native style slots by one registry.
pub const OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS: usize = 16;

/// Maximum UTF-8 byte length retained for one edge renderer key.
pub const OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES: usize = 128;

const DEFAULT_RENDERER_KEY: &str = "default";

/// Finite stroke vocabulary understood by the native batch painter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeStrokePattern {
    /// One continuous path.
    #[default]
    Solid,
    /// Repeating dash segments.
    Dashed,
    /// Repeating round dots.
    Dotted,
}

impl OpenGpuiEdgeStrokePattern {
    const fn invalid_cue(self) -> Self {
        match self {
            Self::Solid => Self::Dashed,
            Self::Dashed => Self::Dotted,
            Self::Dotted => Self::Dashed,
        }
    }
}

/// Finite marker vocabulary carried to endpoint chrome layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeMarkerShape {
    /// No endpoint marker.
    #[default]
    None,
    /// Filled circular marker.
    Circle,
    /// Directional arrow marker.
    Arrow,
    /// Filled diamond marker.
    Diamond,
}

/// Finite label appearance vocabulary carried to label chrome layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeLabelAppearance {
    /// Plain theme-backed label chrome.
    #[default]
    Plain,
    /// Compact badge chrome.
    Badge,
    /// Suppress the presentation label for this painter.
    Hidden,
}

/// Endpoint associated with a marker recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiEdgeMarkerEndpoint {
    /// Source endpoint.
    Source,
    /// Target endpoint.
    Target,
}

/// Immutable semantic recipe registered for an edge renderer key.
///
/// Recipes intentionally contain only finite semantic enums. Application colors and screen-space
/// metrics remain in [`OpenGpuiGraphTheme`], so renderer keys cannot create an unbounded set of
/// paint batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiEdgePainterRecipe {
    stroke_pattern: OpenGpuiEdgeStrokePattern,
    default_tone: OpenGpuiEdgeTone,
    source_marker: OpenGpuiEdgeMarkerShape,
    target_marker: OpenGpuiEdgeMarkerShape,
    label_appearance: OpenGpuiEdgeLabelAppearance,
    weight_tier: OpenGpuiEdgeWeightTier,
}

impl OpenGpuiEdgePainterRecipe {
    /// Creates a finite semantic painter recipe.
    #[must_use]
    pub const fn new(
        stroke_pattern: OpenGpuiEdgeStrokePattern,
        default_tone: OpenGpuiEdgeTone,
        source_marker: OpenGpuiEdgeMarkerShape,
        target_marker: OpenGpuiEdgeMarkerShape,
        label_appearance: OpenGpuiEdgeLabelAppearance,
    ) -> Self {
        Self {
            stroke_pattern,
            default_tone,
            source_marker,
            target_marker,
            label_appearance,
            weight_tier: OpenGpuiEdgeWeightTier::Regular,
        }
    }

    /// Returns the built-in recipe for an edge kind.
    #[must_use]
    pub const fn for_edge_kind(edge_kind: EdgeKind) -> Self {
        match edge_kind {
            EdgeKind::Data => Self::new(
                OpenGpuiEdgeStrokePattern::Solid,
                OpenGpuiEdgeTone::Data,
                OpenGpuiEdgeMarkerShape::None,
                OpenGpuiEdgeMarkerShape::None,
                OpenGpuiEdgeLabelAppearance::Plain,
            ),
            EdgeKind::Exec => Self::new(
                OpenGpuiEdgeStrokePattern::Solid,
                OpenGpuiEdgeTone::Control,
                OpenGpuiEdgeMarkerShape::None,
                OpenGpuiEdgeMarkerShape::Arrow,
                OpenGpuiEdgeLabelAppearance::Plain,
            )
            .with_weight_tier(OpenGpuiEdgeWeightTier::Strong),
        }
    }

    /// Returns the path stroke pattern.
    #[must_use]
    pub const fn stroke_pattern(self) -> OpenGpuiEdgeStrokePattern {
        self.stroke_pattern
    }

    /// Returns the tone used when no recognized style token overrides it.
    #[must_use]
    pub const fn default_tone(self) -> OpenGpuiEdgeTone {
        self.default_tone
    }

    /// Returns the source marker fallback.
    #[must_use]
    pub const fn source_marker(self) -> OpenGpuiEdgeMarkerShape {
        self.source_marker
    }

    /// Returns the target marker fallback.
    #[must_use]
    pub const fn target_marker(self) -> OpenGpuiEdgeMarkerShape {
        self.target_marker
    }

    /// Returns the label appearance policy.
    #[must_use]
    pub const fn label_appearance(self) -> OpenGpuiEdgeLabelAppearance {
        self.label_appearance
    }

    /// Sets the closed screen-space width tier for this immutable recipe.
    #[must_use]
    pub const fn with_weight_tier(mut self, weight_tier: OpenGpuiEdgeWeightTier) -> Self {
        self.weight_tier = weight_tier;
        self
    }

    /// Returns the closed screen-space width tier selected by this recipe.
    #[must_use]
    pub const fn weight_tier(self) -> OpenGpuiEdgeWeightTier {
        self.weight_tier
    }

    const fn uses_round_line_cap(self) -> bool {
        true
    }

    const fn uses_round_line_join(self) -> bool {
        true
    }
}

/// Registration failure for an edge painter recipe.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenGpuiEdgePainterRegistrationError {
    /// Renderer keys must contain at least one non-whitespace character.
    #[error("edge renderer key is empty or contains only whitespace")]
    EmptyRendererKey,
    /// Renderer keys are bounded before they enter the registry.
    #[error("edge renderer key has {actual_bytes} bytes; maximum is {maximum_bytes} bytes")]
    RendererKeyTooLong {
        /// Supplied UTF-8 byte length.
        actual_bytes: usize,
        /// Registry limit.
        maximum_bytes: usize,
    },
    /// The built-in `default` renderer key cannot be replaced.
    #[error("edge renderer key {renderer_key:?} is reserved")]
    ReservedRendererKey {
        /// Reserved key supplied by the caller.
        renderer_key: String,
    },
    /// Duplicate registrations are rejected; the original recipe remains authoritative.
    #[error("edge renderer key {renderer_key:?} is already registered")]
    DuplicateRendererKey {
        /// Duplicate key supplied by the caller.
        renderer_key: String,
    },
    /// A registry cannot grow beyond its documented semantic budget.
    #[error("edge painter registry reached its capacity of {capacity} registrations")]
    RegistryCapacityExceeded {
        /// Maximum number of application registrations.
        capacity: usize,
    },
    /// One renderer-neutral type can map to only one native style slot.
    #[error("graph type already has a native style slot")]
    DuplicateTypeStyle,
    /// Type-to-slot mappings share a fixed host-scoped budget.
    #[error("graph type style registry reached its capacity of {capacity} registrations")]
    TypeStyleCapacityExceeded {
        /// Maximum number of renderer-neutral type mappings.
        capacity: usize,
    },
}

/// Mutable, bounded construction phase for an edge painter registry.
#[derive(Debug, Clone, Default)]
pub struct OpenGpuiEdgePainterRegistryBuilder {
    registrations: BTreeMap<String, OpenGpuiEdgePainterRecipe>,
    type_style_slots: Vec<(TypeDesc, OpenGpuiGraphStyleSlot)>,
}

impl OpenGpuiEdgePainterRegistryBuilder {
    /// Registers one immutable semantic recipe for an exact renderer key.
    ///
    /// # Errors
    ///
    /// Returns an explicit configuration error for empty, oversized, reserved, duplicate, or
    /// over-capacity registrations. A failed registration never changes the builder.
    pub fn register(
        &mut self,
        renderer_key: impl Into<String>,
        recipe: OpenGpuiEdgePainterRecipe,
    ) -> Result<(), OpenGpuiEdgePainterRegistrationError> {
        let renderer_key = renderer_key.into();
        let actual_bytes = renderer_key.len();
        if actual_bytes > OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES {
            return Err(OpenGpuiEdgePainterRegistrationError::RendererKeyTooLong {
                actual_bytes,
                maximum_bytes: OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES,
            });
        }
        if renderer_key.trim().is_empty() {
            return Err(OpenGpuiEdgePainterRegistrationError::EmptyRendererKey);
        }
        if renderer_key == DEFAULT_RENDERER_KEY {
            return Err(OpenGpuiEdgePainterRegistrationError::ReservedRendererKey { renderer_key });
        }
        if self.registrations.contains_key(&renderer_key) {
            return Err(OpenGpuiEdgePainterRegistrationError::DuplicateRendererKey {
                renderer_key,
            });
        }
        if self.registrations.len() >= OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS {
            return Err(
                OpenGpuiEdgePainterRegistrationError::RegistryCapacityExceeded {
                    capacity: OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS,
                },
            );
        }

        self.registrations.insert(renderer_key, recipe);
        Ok(())
    }

    /// Maps one renderer-neutral graph type to a bounded application-native style slot.
    ///
    /// The adapter does not interpret product type names. Exact [`TypeDesc`] values remain owned by
    /// the application and the fixed registration budget prevents unbounded batch vocabularies.
    ///
    /// # Errors
    ///
    /// Returns an explicit error for a duplicate type or an exhausted type-style budget. A failed
    /// registration never changes the builder.
    pub fn register_type_style(
        &mut self,
        ty: TypeDesc,
        style_slot: OpenGpuiGraphStyleSlot,
    ) -> Result<(), OpenGpuiEdgePainterRegistrationError> {
        if self
            .type_style_slots
            .iter()
            .any(|(registered, _)| registered == &ty)
        {
            return Err(OpenGpuiEdgePainterRegistrationError::DuplicateTypeStyle);
        }
        if self.type_style_slots.len() >= OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS {
            return Err(
                OpenGpuiEdgePainterRegistrationError::TypeStyleCapacityExceeded {
                    capacity: OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS,
                },
            );
        }
        self.type_style_slots.push((ty, style_slot));
        Ok(())
    }

    /// Seals the current registrations into an immutable lookup table.
    #[must_use]
    pub fn build(self) -> OpenGpuiEdgePainterRegistry {
        OpenGpuiEdgePainterRegistry {
            registrations: self.registrations,
            type_style_slots: self.type_style_slots,
        }
    }
}

/// Host-scoped immutable registry for bounded edge painter recipes.
#[derive(Debug, Clone, Default)]
pub struct OpenGpuiEdgePainterRegistry {
    registrations: BTreeMap<String, OpenGpuiEdgePainterRecipe>,
    type_style_slots: Vec<(TypeDesc, OpenGpuiGraphStyleSlot)>,
}

impl OpenGpuiEdgePainterRegistry {
    /// Starts construction of an edge painter registry.
    #[must_use]
    pub fn builder() -> OpenGpuiEdgePainterRegistryBuilder {
        OpenGpuiEdgePainterRegistryBuilder::default()
    }

    /// Returns the number of application recipes retained by this registry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Returns whether no application painter or type-style registrations are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty() && self.type_style_slots.is_empty()
    }

    /// Returns the number of renderer-neutral graph types mapped to native style slots.
    #[must_use]
    pub fn type_style_len(&self) -> usize {
        self.type_style_slots.len()
    }

    /// Resolves an exact renderer-neutral graph type to its bounded native style slot.
    #[must_use]
    pub fn style_slot_for_type(&self, ty: &TypeDesc) -> Option<OpenGpuiGraphStyleSlot> {
        self.type_style_slots
            .iter()
            .find_map(|(registered, slot)| (registered == ty).then_some(*slot))
    }

    /// Resolves authoritative presentation facts into a bounded, immutable paint recipe.
    ///
    /// Unknown persisted values are never inserted into the registry or a batch key. The result
    /// borrows them only long enough to expose an adapter diagnostic and otherwise uses stable
    /// edge-kind or recipe fallbacks.
    #[must_use]
    pub fn resolve<'a>(
        &self,
        edge_kind: EdgeKind,
        visual_facts: OpenGpuiEdgeVisualFacts,
        presentation: &'a EdgePresentationFacts,
        theme: &OpenGpuiGraphTheme,
    ) -> OpenGpuiResolvedEdgePaintRecipe<'a> {
        self.resolve_with_style_slot(edge_kind, visual_facts, presentation, None, theme)
    }

    /// Resolves paint from an optional renderer-neutral graph type mapping.
    ///
    /// Unregistered types use the same deterministic tone and painter fallback as an absent type.
    #[must_use]
    pub fn resolve_for_type<'a>(
        &self,
        edge_kind: EdgeKind,
        visual_facts: OpenGpuiEdgeVisualFacts,
        presentation: &'a EdgePresentationFacts,
        ty: Option<&TypeDesc>,
        theme: &OpenGpuiGraphTheme,
    ) -> OpenGpuiResolvedEdgePaintRecipe<'a> {
        let style_slot = ty.and_then(|ty| self.style_slot_for_type(ty));
        self.resolve_with_style_slot(edge_kind, visual_facts, presentation, style_slot, theme)
    }

    /// Resolves paint from an explicitly selected bounded application-native style slot.
    #[must_use]
    pub fn resolve_with_style_slot<'a>(
        &self,
        edge_kind: EdgeKind,
        visual_facts: OpenGpuiEdgeVisualFacts,
        presentation: &'a EdgePresentationFacts,
        style_slot: Option<OpenGpuiGraphStyleSlot>,
        theme: &OpenGpuiGraphTheme,
    ) -> OpenGpuiResolvedEdgePaintRecipe<'a> {
        let (painter, unknown_renderer_key) =
            self.resolve_painter(edge_kind, presentation.renderer_key.as_deref());
        let (tone, unknown_style_token) =
            resolve_tone(presentation.style_token.as_deref(), painter.default_tone);
        let (source_marker, unknown_source_marker_key) = resolve_marker(
            presentation.source_marker_key.as_deref(),
            painter.source_marker,
        );
        let (target_marker, unknown_target_marker_key) = resolve_marker(
            presentation.target_marker_key.as_deref(),
            painter.target_marker,
        );
        let style = theme.resolve_edge_style_slot_with_weight_tier(
            OpenGpuiEdgeVisualFacts::default(),
            tone,
            style_slot,
            painter.weight_tier,
        );
        let stroke_pattern = style
            .style_slot()
            .and_then(|slot| theme.style_slot(slot))
            .map_or(painter.stroke_pattern, |recipe| match recipe.cue() {
                OpenGpuiGraphStyleCue::Inherit => painter.stroke_pattern,
                OpenGpuiGraphStyleCue::Dashed => OpenGpuiEdgeStrokePattern::Dashed,
                OpenGpuiGraphStyleCue::Dotted => OpenGpuiEdgeStrokePattern::Dotted,
            });
        let label = presentation
            .label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .filter(|_| painter.label_appearance != OpenGpuiEdgeLabelAppearance::Hidden)
            .map(|text| OpenGpuiEdgeLabelRecipe {
                text,
                anchor: presentation.label_anchor,
                appearance: painter.label_appearance,
            });

        OpenGpuiResolvedEdgePaintRecipe {
            painter,
            style,
            path_batch_key: OpenGpuiEdgePainterBatchKey {
                path: style.batch_key(),
                stroke_pattern,
                round_line_cap: painter.uses_round_line_cap(),
                round_line_join: painter.uses_round_line_join(),
                invalid_cue: false,
            },
            halo_path_batch_key: None,
            source_marker: OpenGpuiEdgeMarkerRecipe {
                endpoint: OpenGpuiEdgeMarkerEndpoint::Source,
                shape: source_marker,
                follows_path_tangent: true,
            },
            target_marker: OpenGpuiEdgeMarkerRecipe {
                endpoint: OpenGpuiEdgeMarkerEndpoint::Target,
                shape: target_marker,
                follows_path_tangent: true,
            },
            label,
            fallbacks: OpenGpuiEdgePaintFallbacks {
                unknown_renderer_key,
                unknown_style_token,
                unknown_source_marker_key,
                unknown_target_marker_key,
            },
        }
        .with_visual_facts(visual_facts, theme)
    }

    fn resolve_painter<'a>(
        &self,
        edge_kind: EdgeKind,
        renderer_key: Option<&'a str>,
    ) -> (OpenGpuiEdgePainterRecipe, Option<&'a str>) {
        let fallback = OpenGpuiEdgePainterRecipe::for_edge_kind(edge_kind);
        match renderer_key {
            None | Some(DEFAULT_RENDERER_KEY) => (fallback, None),
            Some(renderer_key) => self
                .registrations
                .get(renderer_key)
                .copied()
                .map_or((fallback, Some(renderer_key)), |painter| (painter, None)),
        }
    }
}

/// Allocation-free path grouping key produced by a resolved painter recipe.
///
/// Label text, marker keys, renderer keys, arbitrary style tokens, and the semantic weight tier
/// are deliberately excluded. The tier has already resolved to one validated screen-space width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiEdgePainterBatchKey {
    path: OpenGpuiEdgeBatchKey,
    stroke_pattern: OpenGpuiEdgeStrokePattern,
    round_line_cap: bool,
    round_line_join: bool,
    invalid_cue: bool,
}

impl OpenGpuiEdgePainterBatchKey {
    /// Builds the native fallback recipe used when no persisted edge presentation is available.
    #[must_use]
    pub const fn native_fallback(path: OpenGpuiEdgeBatchKey) -> Self {
        Self {
            path,
            stroke_pattern: OpenGpuiEdgeStrokePattern::Solid,
            round_line_cap: true,
            round_line_join: true,
            invalid_cue: false,
        }
    }

    const fn with_path(mut self, path: OpenGpuiEdgeBatchKey) -> Self {
        self.path = path;
        self
    }

    /// Returns the existing style/layer batch key.
    #[must_use]
    pub const fn path(self) -> OpenGpuiEdgeBatchKey {
        self.path
    }

    /// Returns the finite stroke pattern discriminator.
    #[must_use]
    pub const fn stroke_pattern(self) -> OpenGpuiEdgeStrokePattern {
        self.stroke_pattern
    }

    /// Returns whether the stroke uses a round cap at open path and dash boundaries.
    #[must_use]
    pub const fn uses_round_line_cap(self) -> bool {
        self.round_line_cap
    }

    /// Returns whether adjacent route segments use a round join.
    #[must_use]
    pub const fn uses_round_line_join(self) -> bool {
        self.round_line_join
    }

    /// Returns whether this batch carries the non-color invalid-connection cue.
    #[must_use]
    pub const fn has_invalid_cue(self) -> bool {
        self.invalid_cue
    }
}

/// Immutable marker facts ready for endpoint layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiEdgeMarkerRecipe {
    endpoint: OpenGpuiEdgeMarkerEndpoint,
    shape: OpenGpuiEdgeMarkerShape,
    follows_path_tangent: bool,
}

impl OpenGpuiEdgeMarkerRecipe {
    /// Returns the marker endpoint.
    #[must_use]
    pub const fn endpoint(self) -> OpenGpuiEdgeMarkerEndpoint {
        self.endpoint
    }

    /// Returns the finite marker shape.
    #[must_use]
    pub const fn shape(self) -> OpenGpuiEdgeMarkerShape {
        self.shape
    }

    /// Returns whether endpoint chrome should paint a marker.
    #[must_use]
    pub const fn is_visible(self) -> bool {
        !matches!(self.shape, OpenGpuiEdgeMarkerShape::None)
    }

    /// Returns whether marker orientation follows the sampled endpoint tangent.
    #[must_use]
    pub const fn follows_path_tangent(self) -> bool {
        self.follows_path_tangent
    }
}

/// Immutable label facts ready for path-relative layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiEdgeLabelRecipe<'a> {
    text: &'a str,
    anchor: EdgeLabelAnchor,
    appearance: OpenGpuiEdgeLabelAppearance,
}

impl<'a> OpenGpuiEdgeLabelRecipe<'a> {
    /// Returns the graph-owned label text without allocating.
    #[must_use]
    pub const fn text(self) -> &'a str {
        self.text
    }

    /// Returns the semantic path anchor.
    #[must_use]
    pub const fn anchor(self) -> EdgeLabelAnchor {
        self.anchor
    }

    /// Returns the finite label appearance.
    #[must_use]
    pub const fn appearance(self) -> OpenGpuiEdgeLabelAppearance {
        self.appearance
    }
}

/// Unknown persisted presentation values that used a bounded fallback.
///
/// Values are borrowed for immediate diagnostics and never retained in registry or batch state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenGpuiEdgePaintFallbacks<'a> {
    unknown_renderer_key: Option<&'a str>,
    unknown_style_token: Option<&'a str>,
    unknown_source_marker_key: Option<&'a str>,
    unknown_target_marker_key: Option<&'a str>,
}

impl<'a> OpenGpuiEdgePaintFallbacks<'a> {
    /// Returns the unknown renderer key, when the edge-kind painter was used instead.
    #[must_use]
    pub const fn unknown_renderer_key(self) -> Option<&'a str> {
        self.unknown_renderer_key
    }

    /// Returns the unknown style token, when the painter's default tone was used instead.
    #[must_use]
    pub const fn unknown_style_token(self) -> Option<&'a str> {
        self.unknown_style_token
    }

    /// Returns the unknown source marker key, when the painter fallback was used instead.
    #[must_use]
    pub const fn unknown_source_marker_key(self) -> Option<&'a str> {
        self.unknown_source_marker_key
    }

    /// Returns the unknown target marker key, when the painter fallback was used instead.
    #[must_use]
    pub const fn unknown_target_marker_key(self) -> Option<&'a str> {
        self.unknown_target_marker_key
    }

    /// Returns whether any persisted presentation value required a fallback.
    #[must_use]
    pub const fn has_any(self) -> bool {
        self.unknown_renderer_key.is_some()
            || self.unknown_style_token.is_some()
            || self.unknown_source_marker_key.is_some()
            || self.unknown_target_marker_key.is_some()
    }
}

/// Fully resolved immutable edge recipe consumed by batch paint and overlay chrome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiResolvedEdgePaintRecipe<'a> {
    painter: OpenGpuiEdgePainterRecipe,
    style: OpenGpuiEdgeStyle,
    path_batch_key: OpenGpuiEdgePainterBatchKey,
    halo_path_batch_key: Option<OpenGpuiEdgePainterBatchKey>,
    source_marker: OpenGpuiEdgeMarkerRecipe,
    target_marker: OpenGpuiEdgeMarkerRecipe,
    label: Option<OpenGpuiEdgeLabelRecipe<'a>>,
    fallbacks: OpenGpuiEdgePaintFallbacks<'a>,
}

impl<'a> OpenGpuiResolvedEdgePaintRecipe<'a> {
    /// Returns the registered or edge-kind fallback painter recipe.
    #[must_use]
    pub const fn painter(self) -> OpenGpuiEdgePainterRecipe {
        self.painter
    }

    /// Returns the theme-resolved path style.
    #[must_use]
    pub const fn style(self) -> OpenGpuiEdgeStyle {
        self.style
    }

    /// Returns the closed screen-space weight tier retained by the resolved style.
    #[must_use]
    pub const fn weight_tier(self) -> OpenGpuiEdgeWeightTier {
        self.style.weight_tier()
    }

    /// Reapplies orthogonal visual facts to the same semantic recipe.
    ///
    /// Painter, stroke-pattern, marker, label, and fallback facts remain unchanged. This lets a
    /// preview and its eventual committed edge share one base recipe while varying only native
    /// visual treatment.
    #[must_use]
    pub fn with_visual_facts(
        mut self,
        visual_facts: OpenGpuiEdgeVisualFacts,
        theme: &OpenGpuiGraphTheme,
    ) -> Self {
        self.style = self.style.with_visual_facts(visual_facts, theme);
        self.path_batch_key.path = self.style.batch_key();
        self.halo_path_batch_key = self
            .style
            .halo_batch_key()
            .map(|path| self.path_batch_key.with_path(path));
        self
    }

    /// Adds a finite, non-color cue for an invalid connection attempt.
    ///
    /// The semantic tone and base painter remain intact. The alternate stroke pattern is guaranteed
    /// to differ from the base pattern, and repeated application is idempotent.
    #[must_use]
    pub fn with_invalid_connection_cue(mut self) -> Self {
        if !self.path_batch_key.invalid_cue {
            self.path_batch_key.stroke_pattern = self.path_batch_key.stroke_pattern.invalid_cue();
            self.path_batch_key.invalid_cue = true;
            if let Some(halo) = &mut self.halo_path_batch_key {
                halo.stroke_pattern = halo.stroke_pattern.invalid_cue();
                halo.invalid_cue = true;
            }
        }
        self
    }

    /// Returns the bounded path batch key.
    #[must_use]
    pub const fn path_batch_key(self) -> OpenGpuiEdgePainterBatchKey {
        self.path_batch_key
    }

    /// Returns the bounded halo batch key when any visual emphasis fact is active.
    #[must_use]
    pub const fn halo_path_batch_key(self) -> Option<OpenGpuiEdgePainterBatchKey> {
        self.halo_path_batch_key
    }

    /// Returns whether this resolved recipe carries a non-color invalid-connection cue.
    #[must_use]
    pub const fn has_invalid_connection_cue(self) -> bool {
        self.path_batch_key.has_invalid_cue()
    }

    /// Returns source marker facts without performing layout.
    #[must_use]
    pub const fn source_marker(self) -> OpenGpuiEdgeMarkerRecipe {
        self.source_marker
    }

    /// Returns target marker facts without performing layout.
    #[must_use]
    pub const fn target_marker(self) -> OpenGpuiEdgeMarkerRecipe {
        self.target_marker
    }

    /// Returns label facts without performing path-relative layout.
    #[must_use]
    pub const fn label(self) -> Option<OpenGpuiEdgeLabelRecipe<'a>> {
        self.label
    }

    /// Returns every bounded fallback used during resolution.
    #[must_use]
    pub const fn fallbacks(self) -> OpenGpuiEdgePaintFallbacks<'a> {
        self.fallbacks
    }
}

fn resolve_tone(
    style_token: Option<&str>,
    fallback: OpenGpuiEdgeTone,
) -> (OpenGpuiEdgeTone, Option<&str>) {
    let Some(style_token) = style_token.filter(|token| !token.trim().is_empty()) else {
        return (fallback, None);
    };
    if matches!(style_token.trim(), "default" | "edge.default") {
        return (OpenGpuiEdgeTone::Default, None);
    }
    let tone = OpenGpuiEdgeTone::from_style_token(Some(style_token));
    if tone == OpenGpuiEdgeTone::Default {
        (fallback, Some(style_token))
    } else {
        (tone, None)
    }
}

fn resolve_marker(
    marker_key: Option<&str>,
    fallback: OpenGpuiEdgeMarkerShape,
) -> (OpenGpuiEdgeMarkerShape, Option<&str>) {
    let Some(marker_key) = marker_key.filter(|key| !key.trim().is_empty()) else {
        return (fallback, None);
    };
    let shape = match marker_key.trim() {
        "none" => OpenGpuiEdgeMarkerShape::None,
        "circle" | "dot" | "source.dot" | "target.dot" => OpenGpuiEdgeMarkerShape::Circle,
        "arrow" | "arrow.closed" | "source.arrow" | "target.arrow" => {
            OpenGpuiEdgeMarkerShape::Arrow
        }
        "diamond" | "source.diamond" | "target.diamond" => OpenGpuiEdgeMarkerShape::Diamond,
        _ => return (fallback, Some(marker_key)),
    };
    (shape, None)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::chrome::{OpenGpuiPortMarker, OpenGpuiPortMarkerCue, OpenGpuiPortTone};
    use crate::presentation::OpenGpuiPortInteractionState;
    use crate::theme::OpenGpuiGraphStyleRecipe;

    use super::*;

    fn presentation() -> EdgePresentationFacts {
        EdgePresentationFacts {
            renderer_key: None,
            label: None,
            label_anchor: EdgeLabelAnchor::Center,
            source_marker_key: None,
            target_marker_key: None,
            style_token: None,
        }
    }

    fn ordinary() -> OpenGpuiEdgeVisualFacts {
        OpenGpuiEdgeVisualFacts::default()
    }

    fn selected() -> OpenGpuiEdgeVisualFacts {
        OpenGpuiEdgeVisualFacts::default().with_selected(true)
    }

    fn preview_facts() -> OpenGpuiEdgeVisualFacts {
        OpenGpuiEdgeVisualFacts::default().with_preview(true)
    }

    #[test]
    fn default_recipe_uses_edge_kind_and_carries_label_and_marker_facts() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let mut presentation = presentation();
        presentation.label = Some("Branch".to_owned());
        presentation.label_anchor = EdgeLabelAnchor::Target;
        presentation.target_marker_key = Some("target.diamond".to_owned());
        presentation.style_token = Some("warning".to_owned());

        let resolved = registry.resolve(
            EdgeKind::Exec,
            selected(),
            &presentation,
            &OpenGpuiGraphTheme::default(),
        );

        assert_eq!(resolved.painter().default_tone(), OpenGpuiEdgeTone::Control);
        assert_eq!(
            resolved.painter().weight_tier(),
            OpenGpuiEdgeWeightTier::Strong
        );
        assert_eq!(resolved.weight_tier(), OpenGpuiEdgeWeightTier::Strong);
        assert_eq!(resolved.style().stroke_width(), 2.0);
        assert_eq!(
            resolved
                .style()
                .halo()
                .expect("selected halo")
                .stroke_width(),
            3.0
        );
        assert_eq!(resolved.style().tone(), OpenGpuiEdgeTone::Warning);
        assert_eq!(
            resolved.path_batch_key().stroke_pattern(),
            OpenGpuiEdgeStrokePattern::Solid
        );
        assert_eq!(
            resolved.target_marker().shape(),
            OpenGpuiEdgeMarkerShape::Diamond
        );
        assert_eq!(
            resolved.source_marker().shape(),
            OpenGpuiEdgeMarkerShape::None
        );
        let label = resolved.label().expect("label recipe");
        assert_eq!(label.text(), "Branch");
        assert_eq!(label.anchor(), EdgeLabelAnchor::Target);
        assert!(!resolved.fallbacks().has_any());
    }

    #[test]
    fn registered_recipe_resolves_without_retaining_its_renderer_key_in_batch_state() {
        let registered = OpenGpuiEdgePainterRecipe::new(
            OpenGpuiEdgeStrokePattern::Dashed,
            OpenGpuiEdgeTone::Danger,
            OpenGpuiEdgeMarkerShape::Circle,
            OpenGpuiEdgeMarkerShape::Arrow,
            OpenGpuiEdgeLabelAppearance::Badge,
        );
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register("workflow.error", registered)
            .expect("valid registration");
        let registry = builder.build();
        let mut presentation = presentation();
        presentation.renderer_key = Some("workflow.error".to_owned());
        presentation.label = Some("Failure".to_owned());

        let resolved = registry.resolve(
            EdgeKind::Data,
            ordinary(),
            &presentation,
            &OpenGpuiGraphTheme::default(),
        );

        assert_eq!(registry.len(), 1);
        assert_eq!(resolved.painter(), registered);
        assert_eq!(resolved.style().tone(), OpenGpuiEdgeTone::Danger);
        assert_eq!(
            resolved.path_batch_key().stroke_pattern(),
            OpenGpuiEdgeStrokePattern::Dashed
        );
        assert_eq!(
            resolved.label().expect("label").appearance(),
            OpenGpuiEdgeLabelAppearance::Badge
        );
        assert!(!resolved.fallbacks().has_any());
    }

    #[test]
    fn registered_weight_tier_is_closed_and_preserved_by_resolved_visual_states() {
        let regular = OpenGpuiEdgePainterRecipe::new(
            OpenGpuiEdgeStrokePattern::Solid,
            OpenGpuiEdgeTone::Data,
            OpenGpuiEdgeMarkerShape::None,
            OpenGpuiEdgeMarkerShape::None,
            OpenGpuiEdgeLabelAppearance::Plain,
        );
        let strong = regular.with_weight_tier(OpenGpuiEdgeWeightTier::Strong);
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register("regular", regular)
            .expect("regular recipe registration");
        builder
            .register("strong", strong)
            .expect("strong recipe registration");
        let registry = builder.build();
        let theme = OpenGpuiGraphTheme::default();

        let mut regular_presentation = presentation();
        regular_presentation.renderer_key = Some("regular".to_owned());
        let mut strong_presentation = presentation();
        strong_presentation.renderer_key = Some("strong".to_owned());
        let resolved_regular =
            registry.resolve(EdgeKind::Data, ordinary(), &regular_presentation, &theme);
        let resolved_strong =
            registry.resolve(EdgeKind::Data, ordinary(), &strong_presentation, &theme);
        let preview = resolved_strong.with_visual_facts(preview_facts(), &theme);

        assert_eq!(regular.weight_tier(), OpenGpuiEdgeWeightTier::Regular);
        assert_eq!(strong.weight_tier(), OpenGpuiEdgeWeightTier::Strong);
        assert_eq!(
            resolved_regular.weight_tier(),
            OpenGpuiEdgeWeightTier::Regular
        );
        assert_eq!(
            resolved_strong.weight_tier(),
            OpenGpuiEdgeWeightTier::Strong
        );
        assert_eq!(resolved_regular.style().stroke_width(), 1.5);
        assert_eq!(resolved_strong.style().stroke_width(), 2.0);
        assert_eq!(preview.weight_tier(), OpenGpuiEdgeWeightTier::Strong);
        assert_eq!(preview.style().stroke_width(), 2.0);
        assert_eq!(
            preview.style().halo().expect("preview halo").stroke_width(),
            3.0
        );
        assert_ne!(
            resolved_regular.path_batch_key(),
            resolved_strong.path_batch_key()
        );
    }

    #[test]
    fn registry_clone_keeps_the_same_closed_recipe_table() {
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register(
                "control",
                OpenGpuiEdgePainterRecipe::for_edge_kind(EdgeKind::Exec),
            )
            .expect("control recipe registration");
        let registry = builder.build();
        let cloned = registry.clone();
        let mut presentation = presentation();
        presentation.renderer_key = Some("control".to_owned());
        let theme = OpenGpuiGraphTheme::default();

        let original = registry.resolve(EdgeKind::Data, selected(), &presentation, &theme);
        let copied = cloned.resolve(EdgeKind::Data, selected(), &presentation, &theme);

        assert_eq!(original, copied);
        assert_eq!(copied.weight_tier(), OpenGpuiEdgeWeightTier::Strong);
    }

    #[test]
    fn unknown_presentation_keys_report_fallbacks_without_changing_semantic_defaults() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let mut presentation = presentation();
        presentation.renderer_key = Some("plugin.unknown".to_owned());
        presentation.style_token = Some("tenant-42-unbounded-color".to_owned());
        presentation.source_marker_key = Some("plugin.source-marker".to_owned());
        presentation.target_marker_key = Some("plugin.target-marker".to_owned());

        let resolved = registry.resolve(
            EdgeKind::Exec,
            ordinary(),
            &presentation,
            &OpenGpuiGraphTheme::default(),
        );
        let fallbacks = resolved.fallbacks();

        assert_eq!(fallbacks.unknown_renderer_key(), Some("plugin.unknown"));
        assert_eq!(
            fallbacks.unknown_style_token(),
            Some("tenant-42-unbounded-color")
        );
        assert_eq!(
            fallbacks.unknown_source_marker_key(),
            Some("plugin.source-marker")
        );
        assert_eq!(
            fallbacks.unknown_target_marker_key(),
            Some("plugin.target-marker")
        );
        assert_eq!(resolved.style().tone(), OpenGpuiEdgeTone::Control);
        assert_eq!(
            resolved.target_marker().shape(),
            OpenGpuiEdgeMarkerShape::Arrow
        );
    }

    #[test]
    fn duplicate_registration_is_rejected_and_keeps_the_original_recipe() {
        let first = OpenGpuiEdgePainterRecipe::for_edge_kind(EdgeKind::Data);
        let second = OpenGpuiEdgePainterRecipe::for_edge_kind(EdgeKind::Exec);
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register("workflow.edge", first)
            .expect("first registration");

        assert_eq!(
            builder.register("workflow.edge", second),
            Err(OpenGpuiEdgePainterRegistrationError::DuplicateRendererKey {
                renderer_key: "workflow.edge".to_owned(),
            })
        );

        let registry = builder.build();
        let mut presentation = presentation();
        presentation.renderer_key = Some("workflow.edge".to_owned());
        let resolved = registry.resolve(
            EdgeKind::Exec,
            ordinary(),
            &presentation,
            &OpenGpuiGraphTheme::default(),
        );
        assert_eq!(resolved.painter(), first);
    }

    #[test]
    fn registry_and_unknown_token_batch_cardinality_are_bounded() {
        let recipe = OpenGpuiEdgePainterRecipe::for_edge_kind(EdgeKind::Data);
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        for index in 0..OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS {
            builder
                .register(format!("renderer.{index}"), recipe)
                .expect("registration within capacity");
        }
        assert_eq!(
            builder.register("renderer.overflow", recipe),
            Err(
                OpenGpuiEdgePainterRegistrationError::RegistryCapacityExceeded {
                    capacity: OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS,
                }
            )
        );
        let registry = builder.build();
        assert_eq!(registry.len(), OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS);

        let theme = OpenGpuiGraphTheme::default();
        let batch_keys = (0..1_024)
            .map(|index| {
                let mut presentation = presentation();
                presentation.style_token = Some(format!("arbitrary.style.{index}"));
                registry
                    .resolve(EdgeKind::Data, ordinary(), &presentation, &theme)
                    .path_batch_key()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(batch_keys.len(), 1);
    }

    #[test]
    fn preview_reuses_the_committed_semantic_recipe() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let mut presentation = presentation();
        presentation.label = Some("Flow".to_owned());
        presentation.target_marker_key = Some("arrow".to_owned());
        presentation.style_token = Some("warning".to_owned());
        let theme = OpenGpuiGraphTheme::default();
        let committed = registry.resolve(EdgeKind::Exec, ordinary(), &presentation, &theme);
        let preview = committed.with_visual_facts(preview_facts(), &theme);
        let directly_resolved_preview =
            registry.resolve(EdgeKind::Exec, preview_facts(), &presentation, &theme);

        assert_eq!(preview, directly_resolved_preview);
        assert_eq!(preview.painter(), committed.painter());
        assert_eq!(preview.source_marker(), committed.source_marker());
        assert_eq!(preview.target_marker(), committed.target_marker());
        assert_eq!(preview.label(), committed.label());
        assert_eq!(preview.fallbacks(), committed.fallbacks());
        assert_eq!(preview.style().tone(), committed.style().tone());
        assert_eq!(preview.style().layer(), committed.style().layer());
        assert_eq!(preview.style().color(), committed.style().color());
        assert!(committed.halo_path_batch_key().is_none());
        assert!(preview.halo_path_batch_key().is_some());
        assert_eq!(
            preview.path_batch_key().stroke_pattern(),
            committed.path_batch_key().stroke_pattern()
        );
    }

    #[test]
    fn composed_edge_paint_plan_keeps_one_typed_base_and_one_finite_halo_key() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let theme = OpenGpuiGraphTheme::default();
        let presentation = presentation();
        let base = registry.resolve(EdgeKind::Data, ordinary(), &presentation, &theme);
        let composed = registry.resolve(
            EdgeKind::Data,
            OpenGpuiEdgeVisualFacts::default()
                .with_hovered(true)
                .with_selected(true)
                .with_focused(true),
            &presentation,
            &theme,
        );

        assert_eq!(composed.path_batch_key(), base.path_batch_key());
        assert_eq!(composed.style().color(), base.style().color());
        assert_eq!(composed.style().stroke_width(), base.style().stroke_width());
        let halo = composed
            .halo_path_batch_key()
            .expect("composed facts produce one halo batch");
        assert_eq!(
            halo.path().layer(),
            crate::chrome::OpenGpuiEdgeLayer::Focused
        );
        assert!(halo.path().stroke_width() > composed.path_batch_key().path().stroke_width());
    }

    #[test]
    fn invalid_connection_cue_is_non_color_idempotent_and_bounded() {
        let registered = OpenGpuiEdgePainterRecipe::new(
            OpenGpuiEdgeStrokePattern::Dashed,
            OpenGpuiEdgeTone::Warning,
            OpenGpuiEdgeMarkerShape::Circle,
            OpenGpuiEdgeMarkerShape::Arrow,
            OpenGpuiEdgeLabelAppearance::Plain,
        );
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register("conditional", registered)
            .expect("valid registration");
        let registry = builder.build();
        let mut presentation = presentation();
        presentation.renderer_key = Some("conditional".to_owned());
        let theme = OpenGpuiGraphTheme::default();
        let base = registry.resolve(EdgeKind::Exec, preview_facts(), &presentation, &theme);
        let invalid = base.with_invalid_connection_cue();

        assert!(invalid.has_invalid_connection_cue());
        assert_eq!(invalid.painter(), base.painter());
        assert_eq!(invalid.style(), base.style());
        assert_ne!(invalid.path_batch_key(), base.path_batch_key());
        assert_ne!(
            invalid.path_batch_key().stroke_pattern(),
            base.path_batch_key().stroke_pattern()
        );
        assert_eq!(invalid.with_invalid_connection_cue(), invalid);

        let keys = (0..1_024)
            .map(|_| base.with_invalid_connection_cue().path_batch_key())
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn resolved_geometry_recipe_requests_smooth_strokes_and_tangent_aligned_markers() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let presentation = presentation();
        let theme = OpenGpuiGraphTheme::default();
        let resolved = registry.resolve(EdgeKind::Exec, ordinary(), &presentation, &theme);
        let batch = resolved.path_batch_key();

        assert!(batch.uses_round_line_cap());
        assert!(batch.uses_round_line_join());
        assert!(resolved.source_marker().follows_path_tangent());
        assert!(resolved.target_marker().follows_path_tangent());
    }

    #[test]
    fn oversized_and_reserved_renderer_keys_are_not_retained() {
        let recipe = OpenGpuiEdgePainterRecipe::for_edge_kind(EdgeKind::Data);
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        let oversized = "x".repeat(OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES + 1);

        assert_eq!(
            builder.register(oversized, recipe),
            Err(OpenGpuiEdgePainterRegistrationError::RendererKeyTooLong {
                actual_bytes: OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES + 1,
                maximum_bytes: OPEN_GPUI_MAX_EDGE_RENDERER_KEY_BYTES,
            })
        );
        assert_eq!(
            builder.register(DEFAULT_RENDERER_KEY, recipe),
            Err(OpenGpuiEdgePainterRegistrationError::ReservedRendererKey {
                renderer_key: DEFAULT_RENDERER_KEY.to_owned(),
            })
        );
        assert!(builder.build().is_empty());
    }

    #[test]
    fn type_style_mapping_is_bounded_exact_and_rejects_duplicates() {
        let slot = OpenGpuiGraphStyleSlot::new(1).expect("in-range style slot");
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register_type_style(TypeDesc::Float, slot)
            .expect("first Float style mapping");
        assert_eq!(
            builder.register_type_style(
                TypeDesc::Float,
                OpenGpuiGraphStyleSlot::new(2).expect("in-range style slot"),
            ),
            Err(OpenGpuiEdgePainterRegistrationError::DuplicateTypeStyle)
        );
        for index in 1..OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS {
            builder
                .register_type_style(
                    TypeDesc::Opaque {
                        key: format!("test.type.{index}"),
                        params: Vec::new(),
                    },
                    slot,
                )
                .expect("mapping within fixed capacity");
        }
        assert_eq!(
            builder.register_type_style(TypeDesc::Int, slot),
            Err(
                OpenGpuiEdgePainterRegistrationError::TypeStyleCapacityExceeded {
                    capacity: OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS,
                }
            )
        );

        let registry = builder.build();
        assert_eq!(
            registry.type_style_len(),
            OPEN_GPUI_MAX_TYPE_STYLE_REGISTRATIONS
        );
        assert_eq!(registry.style_slot_for_type(&TypeDesc::Float), Some(slot));
        assert_eq!(registry.style_slot_for_type(&TypeDesc::Int), None);
    }

    #[test]
    fn typed_port_preview_and_committed_wire_share_slot_color_and_cue() {
        let slot = OpenGpuiGraphStyleSlot::new(3).expect("in-range style slot");
        let color = open_gpui::rgb(0xe8b95c).into();
        let theme = OpenGpuiGraphTheme::default().with_style_slot(
            slot,
            OpenGpuiGraphStyleRecipe::new(color).with_cue(OpenGpuiGraphStyleCue::Dotted),
        );
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register_type_style(TypeDesc::Float, slot)
            .expect("Float style mapping");
        let registry = builder.build();
        let presentation = presentation();
        let committed = registry.resolve_for_type(
            EdgeKind::Data,
            ordinary(),
            &presentation,
            Some(&TypeDesc::Float),
            &theme,
        );
        let preview = registry.resolve_for_type(
            EdgeKind::Data,
            preview_facts(),
            &presentation,
            Some(&TypeDesc::Float),
            &theme,
        );
        let marker = OpenGpuiPortMarker::new(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::Data,
            theme,
        )
        .with_style_slot(slot)
        .style();

        assert_eq!(committed.style().style_slot(), Some(slot));
        assert_eq!(preview.style().style_slot(), Some(slot));
        assert_eq!(marker.visual_facts().style_slot(), Some(slot));
        assert_eq!(committed.style().color(), marker.border_color());
        assert_eq!(committed.style().color(), color);
        assert_eq!(marker.cue(), OpenGpuiPortMarkerCue::StyleDot);
        assert_eq!(
            committed.path_batch_key().stroke_pattern(),
            OpenGpuiEdgeStrokePattern::Dotted
        );
        assert_eq!(
            preview.path_batch_key().stroke_pattern(),
            OpenGpuiEdgeStrokePattern::Dotted
        );
    }

    #[test]
    fn unregistered_type_and_theme_slot_share_the_default_batch_fallback() {
        let slot = OpenGpuiGraphStyleSlot::new(5).expect("in-range style slot");
        let mut builder = OpenGpuiEdgePainterRegistry::builder();
        builder
            .register_type_style(TypeDesc::Float, slot)
            .expect("Float style mapping");
        let registry = builder.build();
        let presentation = presentation();
        let theme = OpenGpuiGraphTheme::default();
        let absent =
            registry.resolve_for_type(EdgeKind::Data, ordinary(), &presentation, None, &theme);
        let unknown = registry.resolve_for_type(
            EdgeKind::Data,
            ordinary(),
            &presentation,
            Some(&TypeDesc::Int),
            &theme,
        );
        let theme_unregistered = registry.resolve_for_type(
            EdgeKind::Data,
            ordinary(),
            &presentation,
            Some(&TypeDesc::Float),
            &theme,
        );

        assert_eq!(absent, unknown);
        assert_eq!(unknown, theme_unregistered);
        assert_eq!(theme_unregistered.style().style_slot(), None);
        assert_eq!(absent.path_batch_key(), theme_unregistered.path_batch_key());
    }
}
