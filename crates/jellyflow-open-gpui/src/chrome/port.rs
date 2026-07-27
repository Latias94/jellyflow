//! Stateless native port-marker chrome.

use jellyflow_core::PortKind;
use open_gpui::{
    App, Hsla, IntoElement, ParentElement, Path, PathBuilder, Pixels, RenderOnce, Styled, Window,
    canvas, div, point, px,
};

use crate::presentation::{OpenGpuiPortCandidateState, OpenGpuiPortInteractionState};
use crate::theme::{
    OPEN_GPUI_DEFAULT_PORT_EMPHASIZED_OUTLINE_WIDTH, OPEN_GPUI_DEFAULT_PORT_FOCUS_RING_WIDTH,
    OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER, OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER,
    OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER, OPEN_GPUI_DEFAULT_PORT_OUTLINE_WIDTH,
    OpenGpuiGraphPalette, OpenGpuiGraphStyleCue, OpenGpuiGraphStyleSlot, OpenGpuiGraphTheme,
};

use super::OpenGpuiEdgeTone;
use super::positive_or;

const PORT_MARKER_CUE_THICKNESS: f32 = 1.5;
const PORT_MARKER_CUE_SCALE: f32 = 0.5;
const PORT_MARKER_CIRCLE_SEGMENTS: usize = 20;

pub use crate::theme::OpenGpuiPortMarkerShape;

/// Finite candidate role used by native port chrome.
///
/// Structured rejection diagnostics remain in [`OpenGpuiPortInteractionState`]. This normalized
/// role contains only paint-relevant facts, so it can participate in bounded batch keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiPortCandidateVisual {
    /// The port is not participating in a connection gesture.
    #[default]
    Idle,
    /// The port owns the fixed end of the active gesture.
    Source,
    /// The port is a compatible target.
    ValidTarget,
    /// The port is an incompatible target.
    InvalidTarget,
}

/// Finite semantic tone shared by a port marker and its compatible edge family.
///
/// Product-specific types and arbitrary style tokens stay outside this adapter-local batch key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiPortTone {
    /// Stable fallback for an untyped or unknown port family.
    #[default]
    Default,
    /// Typed data flow.
    Data,
    /// Execution or control flow.
    Control,
}

impl OpenGpuiPortTone {
    pub(crate) const fn color(self, palette: OpenGpuiGraphPalette) -> Hsla {
        match self {
            Self::Default => palette.edge_default,
            Self::Data => palette.edge_data,
            Self::Control => palette.edge_control,
        }
    }
}

impl From<PortKind> for OpenGpuiPortTone {
    fn from(kind: PortKind) -> Self {
        match kind {
            PortKind::Data => Self::Data,
            PortKind::Exec => Self::Control,
        }
    }
}

impl From<OpenGpuiEdgeTone> for OpenGpuiPortTone {
    fn from(tone: OpenGpuiEdgeTone) -> Self {
        match tone {
            OpenGpuiEdgeTone::Data => Self::Data,
            OpenGpuiEdgeTone::Control => Self::Control,
            OpenGpuiEdgeTone::Default
            | OpenGpuiEdgeTone::Success
            | OpenGpuiEdgeTone::Warning
            | OpenGpuiEdgeTone::Danger => Self::Default,
        }
    }
}

/// Composable, allocation-free visual facts for one native port marker.
///
/// Capability, candidate outcome, connection relationship, pointer emphasis, and keyboard focus
/// remain orthogonal. The finite value space is suitable for batch keys and deliberately excludes
/// port identity, callbacks, diagnostic text, and framework entities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpenGpuiPortVisualFacts {
    tone: OpenGpuiPortTone,
    style_slot: Option<OpenGpuiGraphStyleSlot>,
    disabled: bool,
    candidate: OpenGpuiPortCandidateVisual,
    connected: bool,
    hovered: bool,
    focused: bool,
}

impl OpenGpuiPortVisualFacts {
    /// Normalizes adapter presentation into finite paint facts without collapsing layers.
    #[must_use]
    pub fn from_interaction(state: &OpenGpuiPortInteractionState, tone: OpenGpuiPortTone) -> Self {
        Self::from_interaction_with_style_slot(state, tone, None)
    }

    /// Normalizes presentation with one theme-registered application-native style slot.
    #[must_use]
    pub fn from_interaction_with_style_slot(
        state: &OpenGpuiPortInteractionState,
        tone: OpenGpuiPortTone,
        style_slot: Option<OpenGpuiGraphStyleSlot>,
    ) -> Self {
        let candidate = match state.candidate() {
            OpenGpuiPortCandidateState::Idle => OpenGpuiPortCandidateVisual::Idle,
            OpenGpuiPortCandidateState::Source => OpenGpuiPortCandidateVisual::Source,
            OpenGpuiPortCandidateState::ValidTarget => OpenGpuiPortCandidateVisual::ValidTarget,
            OpenGpuiPortCandidateState::InvalidTarget { .. } => {
                OpenGpuiPortCandidateVisual::InvalidTarget
            }
        };
        Self {
            tone,
            style_slot,
            disabled: state.disabled(),
            candidate,
            connected: state.connected(),
            hovered: state.hovered(),
            focused: state.focused(),
        }
    }

    /// Returns the stable semantic port family.
    #[must_use]
    pub const fn tone(self) -> OpenGpuiPortTone {
        self.tone
    }

    /// Returns the registered application-native style slot used by this marker.
    #[must_use]
    pub const fn style_slot(self) -> Option<OpenGpuiGraphStyleSlot> {
        self.style_slot
    }

    /// Returns whether native graph interaction is disabled.
    #[must_use]
    pub const fn disabled(self) -> bool {
        self.disabled
    }

    /// Returns the normalized connection-candidate role.
    #[must_use]
    pub const fn candidate(self) -> OpenGpuiPortCandidateVisual {
        self.candidate
    }

    /// Returns whether the port has at least one graph connection.
    #[must_use]
    pub const fn connected(self) -> bool {
        self.connected
    }

    /// Returns whether the pointer currently emphasizes the port.
    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    /// Returns whether the independent keyboard focus overlay is visible.
    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    const fn emphasizes_marker(self) -> bool {
        self.hovered || !matches!(self.candidate, OpenGpuiPortCandidateVisual::Idle)
    }
}

/// Non-color cue painted inside the default native port marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpenGpuiPortMarkerCue {
    /// No internal cue.
    #[default]
    None,
    /// A short bar paired with a dashed typed wire.
    StyleBar,
    /// A centered dot paired with a dotted typed wire.
    StyleDot,
    /// A fixed-size cross paired with a dashed outline for an incompatible target.
    InvalidCross,
}

/// Fully resolved immutable style for one default native port marker.
///
/// `layout_diameter` is the state-invariant outer slot, while `visual_diameter` describes only the
/// centered painted marker. `minimum_hit_diameter` is a separate value intended for
/// [`crate::authoring::OpenGpuiPortHandle::with_minimum_hit_diameter`]; the semantic wrapper grows
/// its hitbox without changing either layout or route-anchor geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiPortMarkerStyle {
    visual_facts: OpenGpuiPortVisualFacts,
    shape: OpenGpuiPortMarkerShape,
    fill_color: Hsla,
    border_color: Hsla,
    cue_color: Hsla,
    focus_ring_color: Hsla,
    layout_diameter: f32,
    visual_diameter: f32,
    minimum_hit_diameter: f32,
    border_width: f32,
    focus_ring_width: f32,
    dashed_border: bool,
    cue: OpenGpuiPortMarkerCue,
}

impl OpenGpuiPortMarkerStyle {
    /// Returns the complete finite visual facts used to resolve this recipe.
    #[must_use]
    pub const fn visual_facts(self) -> OpenGpuiPortVisualFacts {
        self.visual_facts
    }

    /// Returns the finite marker shape selected by the marker, style slot, or theme profile.
    #[must_use]
    pub const fn shape(self) -> OpenGpuiPortMarkerShape {
        self.shape
    }

    /// Returns the marker fill color.
    #[must_use]
    pub const fn fill_color(self) -> Hsla {
        self.fill_color
    }

    /// Returns the marker outline color.
    #[must_use]
    pub const fn border_color(self) -> Hsla {
        self.border_color
    }

    /// Returns the color used by the internal non-color cue.
    #[must_use]
    pub const fn cue_color(self) -> Hsla {
        self.cue_color
    }

    /// Returns the color of the independent keyboard-focus overlay.
    #[must_use]
    pub const fn focus_ring_color(self) -> Hsla {
        self.focus_ring_color
    }

    /// Returns the state-invariant outer layout-slot diameter in screen pixels.
    ///
    /// Route geometry is measured from this slot, so hover and candidate feedback cannot move an
    /// attached edge endpoint.
    #[must_use]
    pub const fn layout_diameter(self) -> f32 {
        self.layout_diameter
    }

    /// Returns the painted marker diameter in screen pixels.
    #[must_use]
    pub const fn visual_diameter(self) -> f32 {
        self.visual_diameter
    }

    /// Returns the minimum pointer-target diameter in screen pixels.
    ///
    /// Apply this to the surrounding [`crate::authoring::OpenGpuiPortHandle`]. It is deliberately
    /// not part of
    /// the marker element's layout.
    #[must_use]
    pub const fn minimum_hit_diameter(self) -> f32 {
        self.minimum_hit_diameter
    }

    /// Returns the marker outline width in screen pixels.
    #[must_use]
    pub const fn border_width(self) -> f32 {
        self.border_width
    }

    /// Returns whether the independent keyboard-focus overlay must be painted.
    #[must_use]
    pub const fn has_focus_ring(self) -> bool {
        self.visual_facts.focused()
    }

    /// Returns the keyboard-focus overlay width in screen pixels.
    #[must_use]
    pub const fn focus_ring_width(self) -> f32 {
        self.focus_ring_width
    }

    /// Returns whether the marker outline has a non-color dashed treatment.
    #[must_use]
    pub const fn has_dashed_border(self) -> bool {
        self.dashed_border
    }

    /// Returns the internal non-color cue.
    #[must_use]
    pub const fn cue(self) -> OpenGpuiPortMarkerCue {
        self.cue
    }
}

impl OpenGpuiGraphTheme {
    /// Resolves default native port-marker chrome from immutable presentation facts.
    #[must_use]
    pub fn resolve_port_marker_style(
        &self,
        state: &OpenGpuiPortInteractionState,
        tone: OpenGpuiPortTone,
    ) -> OpenGpuiPortMarkerStyle {
        self.resolve_port_marker_style_slot(state, tone, None)
    }

    /// Resolves native port-marker chrome with one bounded application-native style slot.
    #[must_use]
    pub fn resolve_port_marker_style_slot(
        &self,
        state: &OpenGpuiPortInteractionState,
        tone: OpenGpuiPortTone,
        requested_slot: Option<OpenGpuiGraphStyleSlot>,
    ) -> OpenGpuiPortMarkerStyle {
        self.resolve_port_marker_style_slot_with_shape(state, tone, requested_slot, None)
    }

    /// Resolves native port chrome with bounded style-slot and explicit shape overrides.
    #[must_use]
    pub fn resolve_port_marker_style_slot_with_shape(
        &self,
        state: &OpenGpuiPortInteractionState,
        tone: OpenGpuiPortTone,
        requested_slot: Option<OpenGpuiGraphStyleSlot>,
        requested_shape: Option<OpenGpuiPortMarkerShape>,
    ) -> OpenGpuiPortMarkerStyle {
        let style_slot = requested_slot.filter(|slot| self.style_slot(*slot).is_some());
        let visual_facts =
            OpenGpuiPortVisualFacts::from_interaction_with_style_slot(state, tone, style_slot);
        let idle_diameter = positive_or(
            self.metrics.port_marker_diameter,
            OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER,
        );
        let emphasized_diameter = positive_or(
            self.metrics.port_hover_marker_diameter,
            OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER,
        )
        .max(idle_diameter);
        let layout_diameter = emphasized_diameter;
        let visual_diameter = if visual_facts.emphasizes_marker() {
            layout_diameter
        } else {
            idle_diameter
        };
        let minimum_hit_diameter = positive_or(
            self.metrics.port_hit_diameter,
            OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
        )
        .max(layout_diameter);
        let outline_width = positive_or(
            self.metrics.port_outline_width,
            OPEN_GPUI_DEFAULT_PORT_OUTLINE_WIDTH,
        );
        let emphasized_outline_width = positive_or(
            self.metrics.port_emphasized_outline_width,
            OPEN_GPUI_DEFAULT_PORT_EMPHASIZED_OUTLINE_WIDTH,
        )
        .max(outline_width);
        let focus_ring_width = positive_or(
            self.metrics.port_focus_ring_width,
            OPEN_GPUI_DEFAULT_PORT_FOCUS_RING_WIDTH,
        );
        let slot_recipe = style_slot.and_then(|slot| self.style_slot(slot));
        let shape = requested_shape
            .or_else(|| slot_recipe.and_then(|recipe| recipe.port_marker_shape()))
            .unwrap_or(match tone {
                OpenGpuiPortTone::Default => self.port_marker_shapes.default,
                OpenGpuiPortTone::Data => self.port_marker_shapes.data,
                OpenGpuiPortTone::Control => self.port_marker_shapes.control,
            });
        let tone_color =
            slot_recipe.map_or_else(|| tone.color(self.palette), |recipe| recipe.color());
        let (fill_color, border_color) = match visual_facts.candidate() {
            OpenGpuiPortCandidateVisual::InvalidTarget => {
                (self.palette.canvas, self.palette.port_invalid)
            }
            OpenGpuiPortCandidateVisual::ValidTarget => {
                (self.palette.port_valid, self.palette.port_valid)
            }
            OpenGpuiPortCandidateVisual::Source => (self.palette.canvas, self.palette.port_source),
            OpenGpuiPortCandidateVisual::Idle if visual_facts.disabled() => {
                (self.palette.canvas, self.palette.port_disabled)
            }
            OpenGpuiPortCandidateVisual::Idle if visual_facts.connected() => {
                (tone_color, tone_color)
            }
            OpenGpuiPortCandidateVisual::Idle => (self.palette.canvas, tone_color),
        };
        let invalid = matches!(
            visual_facts.candidate(),
            OpenGpuiPortCandidateVisual::InvalidTarget
        );

        OpenGpuiPortMarkerStyle {
            visual_facts,
            shape,
            fill_color,
            border_color,
            cue_color: if invalid {
                self.palette.port_invalid
            } else if visual_facts.connected() {
                self.palette.canvas
            } else {
                tone_color
            },
            focus_ring_color: self.palette.focus,
            layout_diameter,
            visual_diameter,
            minimum_hit_diameter,
            border_width: if visual_facts.emphasizes_marker() {
                emphasized_outline_width
            } else {
                outline_width
            },
            focus_ring_width,
            dashed_border: invalid,
            cue: if invalid {
                OpenGpuiPortMarkerCue::InvalidCross
            } else {
                match slot_recipe.map(|recipe| recipe.cue()) {
                    Some(OpenGpuiGraphStyleCue::Dashed) => OpenGpuiPortMarkerCue::StyleBar,
                    Some(OpenGpuiGraphStyleCue::Dotted) => OpenGpuiPortMarkerCue::StyleDot,
                    None | Some(OpenGpuiGraphStyleCue::Inherit) => OpenGpuiPortMarkerCue::None,
                }
            },
        }
    }
}

/// Stateless default Open GPUI marker for one real graph port.
///
/// This component paints visual chrome only. Wrap it with
/// [`crate::authoring::OpenGpuiPortHandle::interactive`] or
/// [`crate::authoring::OpenGpuiPortHandle::measure`] so the
/// headless runtime receives geometry and the existing wrapper owns pointer and accessibility
/// semantics.
#[derive(Debug, Clone, IntoElement)]
pub struct OpenGpuiPortMarker {
    presentation: OpenGpuiPortInteractionState,
    tone: OpenGpuiPortTone,
    style_slot: Option<OpenGpuiGraphStyleSlot>,
    shape: Option<OpenGpuiPortMarkerShape>,
    theme: OpenGpuiGraphTheme,
}

impl OpenGpuiPortMarker {
    /// Creates a marker from the latest immutable port presentation and graph theme.
    #[must_use]
    pub fn new(
        presentation: &OpenGpuiPortInteractionState,
        tone: OpenGpuiPortTone,
        theme: OpenGpuiGraphTheme,
    ) -> Self {
        Self {
            presentation: presentation.clone(),
            tone,
            style_slot: None,
            shape: None,
            theme,
        }
    }

    /// Applies one bounded application-native style slot to this marker.
    #[must_use]
    pub const fn with_style_slot(mut self, style_slot: OpenGpuiGraphStyleSlot) -> Self {
        self.style_slot = Some(style_slot);
        self
    }

    /// Overrides the theme or style-slot marker shape for this port.
    #[must_use]
    pub const fn with_shape(mut self, shape: OpenGpuiPortMarkerShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Resolves the immutable visual style used by this marker.
    #[must_use]
    pub fn style(&self) -> OpenGpuiPortMarkerStyle {
        self.theme.resolve_port_marker_style_slot_with_shape(
            &self.presentation,
            self.tone,
            self.style_slot,
            self.shape,
        )
    }

    /// Returns the hit diameter that callers should apply to the surrounding semantic handle.
    #[must_use]
    pub fn minimum_hit_diameter(&self) -> f32 {
        self.style().minimum_hit_diameter()
    }

    /// Returns state and rejection text suitable for the surrounding accessible port name.
    #[must_use]
    pub fn accessible_state_text(&self) -> String {
        self.presentation.accessible_state_text()
    }
}

impl RenderOnce for OpenGpuiPortMarker {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let style = self.theme.resolve_port_marker_style_slot_with_shape(
            &self.presentation,
            self.tone,
            self.style_slot,
            self.shape,
        );
        let layout_diameter = style.layout_diameter();
        let visual_diameter = style.visual_diameter();
        let shape = style.shape();
        let border_width = style.border_width();
        let dashed_border = style.has_dashed_border();
        let fill_color = style.fill_color();
        let border_color = style.border_color();
        let mut visual = div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w(px(visual_diameter))
            .h(px(visual_diameter))
            .flex_none()
            .child(
                canvas(
                    move |bounds, _, _| {
                        build_port_marker_paths(shape, bounds, border_width, dashed_border)
                    },
                    move |_, (fill, outline), window, _| {
                        if let Some(fill) = fill {
                            window.paint_path(fill, fill_color);
                        }
                        if let Some(outline) = outline {
                            window.paint_path(outline, border_color);
                        }
                    },
                )
                .absolute()
                .inset_0(),
            );

        visual = match style.cue() {
            OpenGpuiPortMarkerCue::None => visual,
            OpenGpuiPortMarkerCue::StyleBar => {
                let cue_width =
                    (visual_diameter * PORT_MARKER_CUE_SCALE).max(PORT_MARKER_CUE_THICKNESS);
                visual.child(
                    div()
                        .w(px(cue_width))
                        .h(px(PORT_MARKER_CUE_THICKNESS))
                        .bg(style.cue_color()),
                )
            }
            OpenGpuiPortMarkerCue::StyleDot => {
                let cue_diameter = PORT_MARKER_CUE_THICKNESS * 2.0;
                visual.child(
                    div()
                        .w(px(cue_diameter))
                        .h(px(cue_diameter))
                        .rounded_full()
                        .bg(style.cue_color()),
                )
            }
            OpenGpuiPortMarkerCue::InvalidCross => {
                let cue_size =
                    (visual_diameter * PORT_MARKER_CUE_SCALE).max(PORT_MARKER_CUE_THICKNESS);
                let cue_offset = (cue_size - PORT_MARKER_CUE_THICKNESS) * 0.5;
                visual.child(
                    div()
                        .relative()
                        .w(px(cue_size))
                        .h(px(cue_size))
                        .child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(cue_offset))
                                .w(px(cue_size))
                                .h(px(PORT_MARKER_CUE_THICKNESS))
                                .bg(style.cue_color()),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(cue_offset))
                                .top(px(0.0))
                                .w(px(PORT_MARKER_CUE_THICKNESS))
                                .h(px(cue_size))
                                .bg(style.cue_color()),
                        ),
                )
            }
        };

        let mut marker = div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w(px(layout_diameter))
            .h(px(layout_diameter))
            .flex_none()
            .child(visual);
        if style.has_focus_ring() {
            let focus_shape = style.shape();
            let focus_width = style.focus_ring_width();
            let focus_color = style.focus_ring_color();
            marker = marker.child(
                canvas(
                    move |bounds, _, _| {
                        build_port_marker_paths(focus_shape, bounds, focus_width, false).1
                    },
                    move |_, outline, window, _| {
                        if let Some(outline) = outline {
                            window.paint_path(outline, focus_color);
                        }
                    },
                )
                .absolute()
                .inset_0(),
            );
        }
        marker
    }
}

fn build_port_marker_paths(
    shape: OpenGpuiPortMarkerShape,
    bounds: open_gpui::Bounds<Pixels>,
    stroke_width: f32,
    dashed: bool,
) -> (Option<Path<Pixels>>, Option<Path<Pixels>>) {
    let diameter = bounds.size.width.as_f32().min(bounds.size.height.as_f32());
    if !diameter.is_finite() || diameter <= 0.0 || !stroke_width.is_finite() || stroke_width <= 0.0
    {
        return (None, None);
    }
    let radius = (diameter - stroke_width).max(0.0) * 0.5;
    let center_x = bounds.origin.x.as_f32() + bounds.size.width.as_f32() * 0.5;
    let center_y = bounds.origin.y.as_f32() + bounds.size.height.as_f32() * 0.5;
    let at = |x: f32, y: f32| point(px(center_x + x), px(center_y + y));
    let points = match shape {
        OpenGpuiPortMarkerShape::Circle => (0..PORT_MARKER_CIRCLE_SEGMENTS)
            .map(|index| {
                let angle =
                    std::f32::consts::TAU * index as f32 / PORT_MARKER_CIRCLE_SEGMENTS as f32;
                at(angle.cos() * radius, angle.sin() * radius)
            })
            .collect::<Vec<_>>(),
        OpenGpuiPortMarkerShape::Triangle => vec![
            at(0.0, -radius),
            at(radius * 0.88, radius * 0.72),
            at(-radius * 0.88, radius * 0.72),
        ],
        OpenGpuiPortMarkerShape::Diamond => vec![
            at(0.0, -radius),
            at(radius, 0.0),
            at(0.0, radius),
            at(-radius, 0.0),
        ],
        OpenGpuiPortMarkerShape::Square => vec![
            at(-radius, -radius),
            at(radius, -radius),
            at(radius, radius),
            at(-radius, radius),
        ],
    };
    let mut fill = PathBuilder::fill();
    fill.add_polygon(&points, true);
    let outline = if dashed {
        PathBuilder::stroke(px(stroke_width)).dash_array(&[px(2.0), px(1.5)])
    } else {
        PathBuilder::stroke(px(stroke_width))
    };
    let mut outline = outline;
    outline.add_polygon(&points, true);
    (fill.build().ok(), outline.build().ok())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use jellyflow_runtime::graph::Diagnostic;

    use super::*;

    #[test]
    fn visual_facts_preserve_every_orthogonal_layer() {
        let state = OpenGpuiPortInteractionState::idle()
            .with_connected(true)
            .with_hovered(true)
            .with_focused(true)
            .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new()))
            .with_disabled(true);
        let facts = OpenGpuiPortVisualFacts::from_interaction(&state, OpenGpuiPortTone::Data);

        assert_eq!(facts.tone(), OpenGpuiPortTone::Data);
        assert!(facts.disabled());
        assert!(facts.connected());
        assert!(facts.hovered());
        assert!(facts.focused());
        assert_eq!(
            facts.candidate(),
            OpenGpuiPortCandidateVisual::InvalidTarget
        );
    }

    #[test]
    fn invalid_target_keeps_non_color_cues_and_the_focus_overlay() {
        let state = OpenGpuiPortInteractionState::idle()
            .with_connected(true)
            .with_hovered(true)
            .with_focused(true)
            .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new()));
        let marker = OpenGpuiPortMarker::new(
            &state,
            OpenGpuiPortTone::Data,
            OpenGpuiGraphTheme::default(),
        );
        let style = marker.style();

        assert_eq!(
            style.visual_facts().candidate(),
            OpenGpuiPortCandidateVisual::InvalidTarget
        );
        assert!(style.visual_facts().connected());
        assert!(style.visual_facts().hovered());
        assert!(style.has_focus_ring());
        assert!(style.has_dashed_border());
        assert_eq!(style.cue(), OpenGpuiPortMarkerCue::InvalidCross);
        assert_eq!(style.border_color(), marker.theme.palette.port_invalid);
        assert!(
            marker
                .accessible_state_text()
                .contains("incompatible connection target")
        );
    }

    #[test]
    fn candidate_hover_and_relationship_do_not_replace_keyboard_focus() {
        let theme = OpenGpuiGraphTheme::default();
        let style = theme.resolve_port_marker_style(
            &OpenGpuiPortInteractionState::idle()
                .with_connected(true)
                .with_hovered(true)
                .with_focused(true)
                .with_candidate(OpenGpuiPortCandidateState::ValidTarget),
            OpenGpuiPortTone::Data,
        );

        assert!(style.visual_facts().connected());
        assert!(style.visual_facts().hovered());
        assert_eq!(
            style.visual_facts().candidate(),
            OpenGpuiPortCandidateVisual::ValidTarget
        );
        assert!(style.has_focus_ring());
        assert_eq!(style.focus_ring_color(), theme.palette.focus);
        assert_eq!(style.fill_color(), theme.palette.port_valid);
    }

    #[test]
    fn port_outline_metrics_are_explicit_screen_space_theme_values() {
        let mut theme = OpenGpuiGraphTheme::default();
        theme.metrics.port_outline_width = 1.25;
        theme.metrics.port_emphasized_outline_width = 2.75;
        theme.metrics.port_focus_ring_width = 1.5;

        let idle = theme.resolve_port_marker_style(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::Data,
        );
        let emphasized = theme.resolve_port_marker_style(
            &OpenGpuiPortInteractionState::idle()
                .with_hovered(true)
                .with_focused(true),
            OpenGpuiPortTone::Data,
        );

        assert_eq!(idle.border_width(), 1.25);
        assert_eq!(emphasized.border_width(), 2.75);
        assert_eq!(emphasized.focus_ring_width(), 1.5);
        assert_eq!(idle.layout_diameter(), emphasized.layout_diameter());
        assert_eq!(
            idle.minimum_hit_diameter(),
            emphasized.minimum_hit_diameter()
        );
    }

    #[test]
    fn all_finite_combinations_keep_geometry_and_batch_cardinality_bounded() {
        let theme = OpenGpuiGraphTheme::default();
        let mut batch_facts = BTreeSet::new();

        for tone in [
            OpenGpuiPortTone::Default,
            OpenGpuiPortTone::Data,
            OpenGpuiPortTone::Control,
        ] {
            for candidate in [
                OpenGpuiPortCandidateVisual::Idle,
                OpenGpuiPortCandidateVisual::Source,
                OpenGpuiPortCandidateVisual::ValidTarget,
                OpenGpuiPortCandidateVisual::InvalidTarget,
            ] {
                for disabled in [false, true] {
                    for connected in [false, true] {
                        for hovered in [false, true] {
                            for focused in [false, true] {
                                let state = state_with_candidate(candidate)
                                    .with_disabled(disabled)
                                    .with_connected(connected)
                                    .with_hovered(hovered)
                                    .with_focused(focused);
                                let style = theme.resolve_port_marker_style(&state, tone);

                                batch_facts.insert(style.visual_facts());
                                assert_eq!(style.visual_facts().tone(), tone);
                                assert_eq!(style.layout_diameter(), 12.0);
                                assert_eq!(style.layout_diameter() * 0.5, 6.0);
                                assert_eq!(style.minimum_hit_diameter(), 22.0);
                                assert!(style.visual_diameter() <= style.layout_diameter());
                                assert_eq!(style.has_focus_ring(), focused);
                                assert_eq!(
                                    style.has_dashed_border(),
                                    candidate == OpenGpuiPortCandidateVisual::InvalidTarget
                                );
                            }
                        }
                    }
                }
            }
        }

        assert_eq!(batch_facts.len(), 192);
    }

    #[test]
    fn idle_tones_match_edge_families_and_survive_hover_focus_and_relationship_layers() {
        let theme = OpenGpuiGraphTheme::default();
        for (tone, color) in [
            (OpenGpuiPortTone::Default, theme.palette.edge_default),
            (OpenGpuiPortTone::Data, theme.palette.edge_data),
            (OpenGpuiPortTone::Control, theme.palette.edge_control),
        ] {
            for connected in [false, true] {
                for hovered in [false, true] {
                    for focused in [false, true] {
                        let style = theme.resolve_port_marker_style(
                            &OpenGpuiPortInteractionState::idle()
                                .with_connected(connected)
                                .with_hovered(hovered)
                                .with_focused(focused),
                            tone,
                        );
                        assert_eq!(style.visual_facts().tone(), tone);
                        assert_eq!(style.border_color(), color);
                        assert_eq!(
                            style.fill_color(),
                            if connected {
                                color
                            } else {
                                theme.palette.canvas
                            }
                        );
                        assert_eq!(style.has_focus_ring(), focused);
                    }
                }
            }
        }

        assert_eq!(
            OpenGpuiPortTone::from(PortKind::Data),
            OpenGpuiPortTone::Data
        );
        assert_eq!(
            OpenGpuiPortTone::from(PortKind::Exec),
            OpenGpuiPortTone::Control
        );
        assert_eq!(
            OpenGpuiPortTone::from(OpenGpuiEdgeTone::Warning),
            OpenGpuiPortTone::Default
        );
    }

    #[test]
    fn registered_style_slot_controls_color_and_finite_port_cue() {
        let slot = OpenGpuiGraphStyleSlot::new(4).expect("in-range style slot");
        let color = open_gpui::rgb(0xdca1ff).into();
        let theme = OpenGpuiGraphTheme::default().with_style_slot(
            slot,
            crate::theme::OpenGpuiGraphStyleRecipe::new(color)
                .with_cue(OpenGpuiGraphStyleCue::Dotted),
        );
        let marker = OpenGpuiPortMarker::new(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::Data,
            theme,
        )
        .with_style_slot(slot);
        let style = marker.style();

        assert_eq!(style.visual_facts().style_slot(), Some(slot));
        assert_eq!(style.border_color(), color);
        assert_eq!(style.cue(), OpenGpuiPortMarkerCue::StyleDot);
    }

    #[test]
    fn marker_shape_precedence_is_explicit_then_style_slot_then_theme_family() {
        let slot = OpenGpuiGraphStyleSlot::new(5).expect("in-range style slot");
        let theme = OpenGpuiGraphTheme::default()
            .with_port_marker_shapes(crate::theme::OpenGpuiPortMarkerShapes {
                default: OpenGpuiPortMarkerShape::Circle,
                data: OpenGpuiPortMarkerShape::Diamond,
                control: OpenGpuiPortMarkerShape::Square,
            })
            .with_style_slot(
                slot,
                crate::theme::OpenGpuiGraphStyleRecipe::new(open_gpui::rgb(0x59c2a6).into())
                    .with_port_marker_shape(OpenGpuiPortMarkerShape::Triangle),
            );
        let state = OpenGpuiPortInteractionState::idle();

        assert_eq!(
            theme
                .resolve_port_marker_style(&state, OpenGpuiPortTone::Data)
                .shape(),
            OpenGpuiPortMarkerShape::Diamond
        );
        assert_eq!(
            theme
                .resolve_port_marker_style(&state, OpenGpuiPortTone::Control)
                .shape(),
            OpenGpuiPortMarkerShape::Square
        );
        assert_eq!(
            theme
                .resolve_port_marker_style_slot(&state, OpenGpuiPortTone::Data, Some(slot))
                .shape(),
            OpenGpuiPortMarkerShape::Triangle
        );
        let explicit = OpenGpuiPortMarker::new(&state, OpenGpuiPortTone::Data, theme)
            .with_style_slot(slot)
            .with_shape(OpenGpuiPortMarkerShape::Circle)
            .style();
        assert_eq!(explicit.shape(), OpenGpuiPortMarkerShape::Circle);
        assert_eq!(explicit.layout_diameter(), 12.0);
        assert_eq!(explicit.minimum_hit_diameter(), 22.0);
    }

    #[test]
    fn every_marker_shape_builds_a_bounded_fill_and_outline_plan() {
        let bounds =
            open_gpui::Bounds::new(point(px(0.0), px(0.0)), open_gpui::size(px(12.0), px(12.0)));
        for shape in [
            OpenGpuiPortMarkerShape::Circle,
            OpenGpuiPortMarkerShape::Triangle,
            OpenGpuiPortMarkerShape::Diamond,
            OpenGpuiPortMarkerShape::Square,
        ] {
            let (fill, outline) = build_port_marker_paths(shape, bounds, 1.0, false);
            let fill = fill.expect("finite marker fill");
            let outline = outline.expect("finite marker outline");
            assert!(!fill.vertices.is_empty());
            assert!(!outline.vertices.is_empty());
            assert!(fill.bounds.size.width.as_f32() <= 12.0);
            assert!(fill.bounds.size.height.as_f32() <= 12.0);
        }
    }

    #[test]
    fn unregistered_style_slot_and_invalid_state_have_stable_precedence() {
        let slot = OpenGpuiGraphStyleSlot::new(7).expect("in-range style slot");
        let theme = OpenGpuiGraphTheme::default();
        let fallback = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::Data,
            Some(slot),
        );
        assert_eq!(fallback.visual_facts().style_slot(), None);
        assert_eq!(fallback.border_color(), theme.palette.edge_data);
        assert_eq!(fallback.cue(), OpenGpuiPortMarkerCue::None);

        let typed_theme = theme.with_style_slot(
            slot,
            crate::theme::OpenGpuiGraphStyleRecipe::new(open_gpui::rgb(0xe8b95c).into())
                .with_cue(OpenGpuiGraphStyleCue::Dotted),
        );
        let invalid = typed_theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle()
                .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new())),
            OpenGpuiPortTone::Data,
            Some(slot),
        );
        assert_eq!(invalid.cue(), OpenGpuiPortMarkerCue::InvalidCross);
        assert!(invalid.has_dashed_border());
    }

    fn state_with_candidate(
        candidate: OpenGpuiPortCandidateVisual,
    ) -> OpenGpuiPortInteractionState {
        let candidate = match candidate {
            OpenGpuiPortCandidateVisual::Idle => OpenGpuiPortCandidateState::Idle,
            OpenGpuiPortCandidateVisual::Source => OpenGpuiPortCandidateState::Source,
            OpenGpuiPortCandidateVisual::ValidTarget => OpenGpuiPortCandidateState::ValidTarget,
            OpenGpuiPortCandidateVisual::InvalidTarget => {
                OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new())
            }
        };
        OpenGpuiPortInteractionState::idle().with_candidate(candidate)
    }
}
