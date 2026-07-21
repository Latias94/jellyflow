//! Stateless native port-marker chrome.

use open_gpui::{App, Hsla, IntoElement, ParentElement, RenderOnce, Styled, Window, div, px};

use crate::presentation::{OpenGpuiPortCandidateState, OpenGpuiPortInteractionState};
use crate::theme::{
    OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER, OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER,
    OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER, OpenGpuiGraphTheme,
};

const PORT_MARKER_BORDER_WIDTH: f32 = 1.0;
const PORT_MARKER_EMPHASIZED_BORDER_WIDTH: f32 = 2.0;
const PORT_MARKER_CUE_THICKNESS: f32 = 1.5;
const PORT_MARKER_CUE_SCALE: f32 = 0.5;

/// Exclusive visual state used by the default native port marker.
///
/// Candidate states deliberately outrank focus, hover, and connectivity. A disabled port outranks
/// every other state because the semantic wrapper will reject interaction for that port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenGpuiPortVisualState {
    /// Idle, unconnected port.
    #[default]
    Idle,
    /// Connected port with no stronger interaction state.
    Connected,
    /// Port currently under the pointer.
    Hovered,
    /// Port currently owned by keyboard or accessibility focus.
    Focused,
    /// Fixed source of an active connection gesture.
    Source,
    /// Compatible target of an active connection gesture.
    ValidTarget,
    /// Incompatible target of an active connection gesture.
    InvalidTarget,
    /// Port whose native interaction is disabled.
    Disabled,
}

impl OpenGpuiPortVisualState {
    /// Resolves composable presentation facts into the default marker's visual priority.
    #[must_use]
    pub fn resolve(state: &OpenGpuiPortInteractionState) -> Self {
        if state.disabled() {
            return Self::Disabled;
        }

        match state.candidate() {
            OpenGpuiPortCandidateState::InvalidTarget { .. } => return Self::InvalidTarget,
            OpenGpuiPortCandidateState::ValidTarget => return Self::ValidTarget,
            OpenGpuiPortCandidateState::Source => return Self::Source,
            OpenGpuiPortCandidateState::Idle => {}
        }

        if state.focused() {
            Self::Focused
        } else if state.hovered() {
            Self::Hovered
        } else if state.connected() {
            Self::Connected
        } else {
            Self::Idle
        }
    }

    const fn is_emphasized(self) -> bool {
        matches!(
            self,
            Self::Hovered | Self::Focused | Self::Source | Self::ValidTarget | Self::InvalidTarget
        )
    }
}

/// Non-color cue painted inside the default native port marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpenGpuiPortMarkerCue {
    /// No internal cue.
    #[default]
    None,
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
    visual_state: OpenGpuiPortVisualState,
    fill_color: Hsla,
    border_color: Hsla,
    cue_color: Hsla,
    layout_diameter: f32,
    visual_diameter: f32,
    minimum_hit_diameter: f32,
    border_width: f32,
    dashed_border: bool,
    cue: OpenGpuiPortMarkerCue,
}

impl OpenGpuiPortMarkerStyle {
    /// Returns the resolved exclusive visual state.
    #[must_use]
    pub const fn visual_state(self) -> OpenGpuiPortVisualState {
        self.visual_state
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
    /// Apply this to the surrounding [`crate::OpenGpuiPortHandle`]. It is deliberately not part of
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
    ) -> OpenGpuiPortMarkerStyle {
        let visual_state = OpenGpuiPortVisualState::resolve(state);
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
        let visual_diameter = if visual_state.is_emphasized() {
            layout_diameter
        } else {
            idle_diameter
        };
        let minimum_hit_diameter = positive_or(
            self.metrics.port_hit_diameter,
            OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
        )
        .max(layout_diameter);
        let (fill_color, border_color) = match visual_state {
            OpenGpuiPortVisualState::Idle => (self.palette.canvas, self.palette.port_idle),
            OpenGpuiPortVisualState::Connected => {
                (self.palette.port_connected, self.palette.port_connected)
            }
            OpenGpuiPortVisualState::Hovered => {
                (self.palette.port_hovered, self.palette.port_hovered)
            }
            OpenGpuiPortVisualState::Focused => (self.palette.canvas, self.palette.focus),
            OpenGpuiPortVisualState::Source => (self.palette.canvas, self.palette.port_source),
            OpenGpuiPortVisualState::ValidTarget => {
                (self.palette.port_valid, self.palette.port_valid)
            }
            OpenGpuiPortVisualState::InvalidTarget => {
                (self.palette.canvas, self.palette.port_invalid)
            }
            OpenGpuiPortVisualState::Disabled => (self.palette.canvas, self.palette.port_disabled),
        };
        let invalid = visual_state == OpenGpuiPortVisualState::InvalidTarget;

        OpenGpuiPortMarkerStyle {
            visual_state,
            fill_color,
            border_color,
            cue_color: self.palette.port_invalid,
            layout_diameter,
            visual_diameter,
            minimum_hit_diameter,
            border_width: if visual_state.is_emphasized() {
                PORT_MARKER_EMPHASIZED_BORDER_WIDTH
            } else {
                PORT_MARKER_BORDER_WIDTH
            },
            dashed_border: invalid,
            cue: if invalid {
                OpenGpuiPortMarkerCue::InvalidCross
            } else {
                OpenGpuiPortMarkerCue::None
            },
        }
    }
}

/// Stateless default Open GPUI marker for one real graph port.
///
/// This component paints visual chrome only. Wrap it with
/// [`crate::OpenGpuiPortHandle::interactive`] or [`crate::OpenGpuiPortHandle::measure`] so the
/// headless runtime receives geometry and the existing wrapper owns pointer and accessibility
/// semantics.
#[derive(Debug, Clone, IntoElement)]
pub struct OpenGpuiPortMarker {
    presentation: OpenGpuiPortInteractionState,
    theme: OpenGpuiGraphTheme,
}

impl OpenGpuiPortMarker {
    /// Creates a marker from the latest immutable port presentation and graph theme.
    #[must_use]
    pub fn new(presentation: &OpenGpuiPortInteractionState, theme: OpenGpuiGraphTheme) -> Self {
        Self {
            presentation: presentation.clone(),
            theme,
        }
    }

    /// Resolves the immutable visual style used by this marker.
    #[must_use]
    pub fn style(&self) -> OpenGpuiPortMarkerStyle {
        self.theme.resolve_port_marker_style(&self.presentation)
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
        let style = self.theme.resolve_port_marker_style(&self.presentation);
        let layout_diameter = style.layout_diameter();
        let visual_diameter = style.visual_diameter();
        let mut visual = div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w(px(visual_diameter))
            .h(px(visual_diameter))
            .flex_none()
            .rounded_full()
            .border(px(style.border_width()))
            .border_color(style.border_color())
            .bg(style.fill_color());
        if style.has_dashed_border() {
            visual = visual.border_dashed();
        }

        visual = match style.cue() {
            OpenGpuiPortMarkerCue::None => visual,
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

        div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .w(px(layout_diameter))
            .h(px(layout_diameter))
            .flex_none()
            .child(visual)
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
    use jellyflow_runtime::Diagnostic;

    use super::*;

    #[test]
    fn visual_state_priority_preserves_candidate_and_disabled_feedback() {
        let invalid = OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new());
        let state = OpenGpuiPortInteractionState::idle()
            .with_connected(true)
            .with_hovered(true)
            .with_focused(true)
            .with_candidate(invalid.clone())
            .with_disabled(true);
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state),
            OpenGpuiPortVisualState::Disabled
        );

        let state = state.with_disabled(false);
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state),
            OpenGpuiPortVisualState::InvalidTarget
        );

        let state = state.with_candidate(OpenGpuiPortCandidateState::ValidTarget);
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state),
            OpenGpuiPortVisualState::ValidTarget
        );

        let state = state.with_candidate(OpenGpuiPortCandidateState::Source);
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state),
            OpenGpuiPortVisualState::Source
        );

        let state = state.with_candidate(OpenGpuiPortCandidateState::Idle);
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state),
            OpenGpuiPortVisualState::Focused
        );
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state.clone().with_focused(false)),
            OpenGpuiPortVisualState::Hovered
        );
        assert_eq!(
            OpenGpuiPortVisualState::resolve(&state.with_focused(false).with_hovered(false)),
            OpenGpuiPortVisualState::Connected
        );
    }

    #[test]
    fn invalid_target_has_non_color_cues_and_accessible_state_text() {
        let state = OpenGpuiPortInteractionState::idle()
            .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new()));
        let marker = OpenGpuiPortMarker::new(&state, OpenGpuiGraphTheme::default());
        let style = marker.style();

        assert_eq!(style.visual_state(), OpenGpuiPortVisualState::InvalidTarget);
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
    fn visual_and_hit_diameters_remain_separate_fixed_screen_metrics() {
        let theme = OpenGpuiGraphTheme::default();
        let idle = theme.resolve_port_marker_style(&OpenGpuiPortInteractionState::idle());
        let hovered = theme
            .resolve_port_marker_style(&OpenGpuiPortInteractionState::idle().with_hovered(true));

        assert_eq!(idle.layout_diameter(), 12.0);
        assert_eq!(hovered.layout_diameter(), 12.0);
        assert_eq!(idle.visual_diameter(), 10.0);
        assert_eq!(hovered.visual_diameter(), 12.0);
        assert_eq!(idle.minimum_hit_diameter(), 22.0);
        assert_eq!(hovered.minimum_hit_diameter(), 22.0);
    }

    #[test]
    fn every_interaction_state_keeps_one_layout_slot_and_external_hit_target() {
        let theme = OpenGpuiGraphTheme::default();
        let invalid = OpenGpuiPortCandidateState::invalid(Vec::<Diagnostic>::new());
        let states = [
            OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortInteractionState::idle().with_connected(true),
            OpenGpuiPortInteractionState::idle().with_hovered(true),
            OpenGpuiPortInteractionState::idle().with_focused(true),
            OpenGpuiPortInteractionState::idle().with_candidate(OpenGpuiPortCandidateState::Source),
            OpenGpuiPortInteractionState::idle()
                .with_candidate(OpenGpuiPortCandidateState::ValidTarget),
            OpenGpuiPortInteractionState::idle().with_candidate(invalid),
            OpenGpuiPortInteractionState::idle().with_disabled(true),
        ];
        let styles = states.map(|state| theme.resolve_port_marker_style(&state));

        assert!(styles.iter().all(|style| style.layout_diameter() == 12.0));
        assert!(
            styles
                .iter()
                .all(|style| style.minimum_hit_diameter() == 22.0)
        );
        assert_eq!(
            styles.map(OpenGpuiPortMarkerStyle::visual_diameter),
            [10.0, 10.0, 12.0, 12.0, 12.0, 12.0, 12.0, 10.0]
        );
    }
}
