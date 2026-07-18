//! Bounded renderer-key lookup and immutable recipes for native edge chrome.

use std::collections::BTreeMap;

use jellyflow::core::{EdgeKind, EdgeLabelAnchor};
use jellyflow::runtime::runtime::geometry::EdgePresentationFacts;

use super::edge::{
    OpenGpuiEdgeBatchKey, OpenGpuiEdgeStyle, OpenGpuiEdgeTone, OpenGpuiEdgeVisualState,
};
use crate::theme::OpenGpuiGraphTheme;

/// Maximum number of application edge painters retained by one registry.
pub const OPEN_GPUI_MAX_EDGE_PAINTER_REGISTRATIONS: usize = 64;

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
            ),
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
}

/// Mutable, bounded construction phase for an edge painter registry.
#[derive(Debug, Default)]
pub struct OpenGpuiEdgePainterRegistryBuilder {
    registrations: BTreeMap<String, OpenGpuiEdgePainterRecipe>,
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

    /// Seals the current registrations into an immutable lookup table.
    #[must_use]
    pub fn build(self) -> OpenGpuiEdgePainterRegistry {
        OpenGpuiEdgePainterRegistry {
            registrations: self.registrations,
        }
    }
}

/// Host-scoped immutable registry for bounded edge painter recipes.
#[derive(Debug, Default)]
pub struct OpenGpuiEdgePainterRegistry {
    registrations: BTreeMap<String, OpenGpuiEdgePainterRecipe>,
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

    /// Returns whether no application recipes are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
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
        visual_state: OpenGpuiEdgeVisualState,
        presentation: &'a EdgePresentationFacts,
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
        let style = theme.resolve_edge_style(visual_state, canonical_style_token(tone));
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
                stroke_pattern: painter.stroke_pattern,
            },
            source_marker: OpenGpuiEdgeMarkerRecipe {
                endpoint: OpenGpuiEdgeMarkerEndpoint::Source,
                shape: source_marker,
            },
            target_marker: OpenGpuiEdgeMarkerRecipe {
                endpoint: OpenGpuiEdgeMarkerEndpoint::Target,
                shape: target_marker,
            },
            label,
            fallbacks: OpenGpuiEdgePaintFallbacks {
                unknown_renderer_key,
                unknown_style_token,
                unknown_source_marker_key,
                unknown_target_marker_key,
            },
        }
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
/// Label text, marker keys, renderer keys, and arbitrary style tokens are deliberately excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiEdgePainterBatchKey {
    path: OpenGpuiEdgeBatchKey,
    stroke_pattern: OpenGpuiEdgeStrokePattern,
}

impl OpenGpuiEdgePainterBatchKey {
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
}

/// Immutable marker facts ready for endpoint layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenGpuiEdgeMarkerRecipe {
    endpoint: OpenGpuiEdgeMarkerEndpoint,
    shape: OpenGpuiEdgeMarkerShape,
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

    /// Returns the bounded path batch key.
    #[must_use]
    pub const fn path_batch_key(self) -> OpenGpuiEdgePainterBatchKey {
        self.path_batch_key
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

fn canonical_style_token(tone: OpenGpuiEdgeTone) -> Option<&'static str> {
    match tone {
        OpenGpuiEdgeTone::Default => None,
        OpenGpuiEdgeTone::Data => Some("data"),
        OpenGpuiEdgeTone::Control => Some("control"),
        OpenGpuiEdgeTone::Success => Some("success"),
        OpenGpuiEdgeTone::Warning => Some("warning"),
        OpenGpuiEdgeTone::Danger => Some("danger"),
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
            OpenGpuiEdgeVisualState::Selected,
            &presentation,
            &OpenGpuiGraphTheme::default(),
        );

        assert_eq!(resolved.painter().default_tone(), OpenGpuiEdgeTone::Control);
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
            OpenGpuiEdgeVisualState::Ordinary,
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
    fn unknown_presentation_keys_report_fallbacks_without_changing_semantic_defaults() {
        let registry = OpenGpuiEdgePainterRegistry::default();
        let mut presentation = presentation();
        presentation.renderer_key = Some("plugin.unknown".to_owned());
        presentation.style_token = Some("tenant-42-unbounded-color".to_owned());
        presentation.source_marker_key = Some("plugin.source-marker".to_owned());
        presentation.target_marker_key = Some("plugin.target-marker".to_owned());

        let resolved = registry.resolve(
            EdgeKind::Exec,
            OpenGpuiEdgeVisualState::Ordinary,
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
            OpenGpuiEdgeVisualState::Ordinary,
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
                    .resolve(
                        EdgeKind::Data,
                        OpenGpuiEdgeVisualState::Ordinary,
                        &presentation,
                        &theme,
                    )
                    .path_batch_key()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(batch_keys.len(), 1);
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
}
