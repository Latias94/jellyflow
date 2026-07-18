/// Default zoom at or below which native detail enters canonical overview mode.
pub const OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM: f32 = 0.85;
/// Default zoom at or above which canonical overview re-enters native detail mode.
pub const OPEN_GPUI_DEFAULT_DETAIL_ENTER_ZOOM: f32 = 1.0;

/// Validated hysteresis thresholds for semantic level of detail.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSemanticLodPolicy {
    overview_enter_zoom: f32,
    detail_enter_zoom: f32,
}

impl OpenGpuiSemanticLodPolicy {
    /// Creates a semantic LOD policy with a non-empty hysteresis gap.
    ///
    /// # Errors
    ///
    /// Returns an error when either threshold is not positive and finite, or when the overview
    /// threshold is not strictly below the detail threshold.
    pub fn new(
        overview_enter_zoom: f32,
        detail_enter_zoom: f32,
    ) -> Result<Self, OpenGpuiSemanticLodConfigError> {
        if !overview_enter_zoom.is_finite() || overview_enter_zoom <= 0.0 {
            return Err(OpenGpuiSemanticLodConfigError::InvalidOverviewEnterZoom);
        }
        if !detail_enter_zoom.is_finite() || detail_enter_zoom <= 0.0 {
            return Err(OpenGpuiSemanticLodConfigError::InvalidDetailEnterZoom);
        }
        if overview_enter_zoom >= detail_enter_zoom {
            return Err(OpenGpuiSemanticLodConfigError::MissingHysteresisGap);
        }
        Ok(Self {
            overview_enter_zoom,
            detail_enter_zoom,
        })
    }

    /// Returns the zoom at or below which detail enters overview mode.
    #[must_use]
    pub const fn overview_enter_zoom(self) -> f32 {
        self.overview_enter_zoom
    }

    /// Returns the zoom at or above which overview enters detail mode.
    #[must_use]
    pub const fn detail_enter_zoom(self) -> f32 {
        self.detail_enter_zoom
    }

    fn initial_band(self, zoom: f32) -> OpenGpuiNodeScaleBand {
        if zoom.is_finite() && zoom > 0.0 && zoom <= self.overview_enter_zoom {
            OpenGpuiNodeScaleBand::Overview
        } else {
            OpenGpuiNodeScaleBand::Detail
        }
    }

    fn reduce(self, current: OpenGpuiNodeScaleBand, zoom: f32) -> OpenGpuiNodeScaleBand {
        if !zoom.is_finite() || zoom <= 0.0 {
            return current;
        }
        match current {
            OpenGpuiNodeScaleBand::Detail if zoom <= self.overview_enter_zoom => {
                OpenGpuiNodeScaleBand::Overview
            }
            OpenGpuiNodeScaleBand::Overview if zoom >= self.detail_enter_zoom => {
                OpenGpuiNodeScaleBand::Detail
            }
            _ => current,
        }
    }
}

impl Default for OpenGpuiSemanticLodPolicy {
    fn default() -> Self {
        Self {
            overview_enter_zoom: OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
            detail_enter_zoom: OPEN_GPUI_DEFAULT_DETAIL_ENTER_ZOOM,
        }
    }
}

/// Invalid semantic LOD threshold configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenGpuiSemanticLodConfigError {
    /// The overview-enter threshold is not positive and finite.
    #[error("overview-enter zoom must be positive and finite")]
    InvalidOverviewEnterZoom,
    /// The detail-enter threshold is not positive and finite.
    #[error("detail-enter zoom must be positive and finite")]
    InvalidDetailEnterZoom,
    /// The thresholds do not leave a hysteresis gap.
    #[error("overview-enter zoom must be strictly below detail-enter zoom")]
    MissingHysteresisGap,
}

/// Adapter-local scaling authority for one registered node renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpenGpuiNodeScalePolicy {
    /// The host chooses between native detail and canonical overview with hysteresis.
    SemanticLod(OpenGpuiSemanticLodPolicy),
    /// The component owns all scaling behavior and remains in the detail band.
    ///
    /// Selecting this policy does not imply that arbitrary retained descendants can be scaled by
    /// the host. The registered component is responsible for layout, paint, hit-testing, focus,
    /// and overlay consistency at every supported zoom.
    ComponentManaged,
}

impl Default for OpenGpuiNodeScalePolicy {
    fn default() -> Self {
        Self::SemanticLod(OpenGpuiSemanticLodPolicy::default())
    }
}

/// Base graph presentation selected by a node scale policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum OpenGpuiNodeScaleBand {
    /// Render the graph-interactive native detail presentation.
    #[default]
    Detail,
    /// Render canonical lightweight overview chrome.
    Overview,
}

/// Persistent reducer state for one node's scale policy.
///
/// Interaction-island presentation is intentionally not represented here. It is a native
/// lifecycle concern layered over this base graph band and must remain graph-inert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiNodeScaleState {
    band: OpenGpuiNodeScaleBand,
}

impl OpenGpuiNodeScaleState {
    /// Creates scale state for the first observed zoom.
    #[must_use]
    pub fn new(policy: OpenGpuiNodeScalePolicy, zoom: f32) -> Self {
        let band = match policy {
            OpenGpuiNodeScalePolicy::SemanticLod(policy) => policy.initial_band(zoom),
            OpenGpuiNodeScalePolicy::ComponentManaged => OpenGpuiNodeScaleBand::Detail,
        };
        Self { band }
    }

    /// Returns the current base graph presentation.
    #[must_use]
    pub const fn band(self) -> OpenGpuiNodeScaleBand {
        self.band
    }

    /// Reduces one zoom observation and reports whether the graph presentation changed.
    ///
    /// Invalid zoom observations preserve the previous band. Component-managed nodes always
    /// reduce to detail, including when the policy changes at runtime.
    pub fn reconcile(&mut self, policy: OpenGpuiNodeScalePolicy, zoom: f32) -> bool {
        let next = match policy {
            OpenGpuiNodeScalePolicy::SemanticLod(policy) => policy.reduce(self.band, zoom),
            OpenGpuiNodeScalePolicy::ComponentManaged => OpenGpuiNodeScaleBand::Detail,
        };
        let changed = next != self.band;
        self.band = next;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_lod_is_the_default_scale_authority() {
        assert_eq!(
            OpenGpuiNodeScalePolicy::default(),
            OpenGpuiNodeScalePolicy::SemanticLod(OpenGpuiSemanticLodPolicy::default())
        );
    }

    #[test]
    fn thresholds_must_be_positive_finite_and_leave_a_gap() {
        assert_eq!(
            OpenGpuiSemanticLodPolicy::new(0.0, 1.0),
            Err(OpenGpuiSemanticLodConfigError::InvalidOverviewEnterZoom)
        );
        assert_eq!(
            OpenGpuiSemanticLodPolicy::new(0.8, f32::INFINITY),
            Err(OpenGpuiSemanticLodConfigError::InvalidDetailEnterZoom)
        );
        assert_eq!(
            OpenGpuiSemanticLodPolicy::new(1.0, 1.0),
            Err(OpenGpuiSemanticLodConfigError::MissingHysteresisGap)
        );
    }

    #[test]
    fn semantic_lod_holds_its_band_inside_the_hysteresis_gap() {
        let policy = OpenGpuiNodeScalePolicy::SemanticLod(
            OpenGpuiSemanticLodPolicy::new(0.8, 1.0).expect("valid thresholds"),
        );
        let mut state = OpenGpuiNodeScaleState::new(policy, 1.2);

        for zoom in [0.99, 0.91, 0.81, 0.95, 0.82] {
            assert!(!state.reconcile(policy, zoom));
            assert_eq!(state.band(), OpenGpuiNodeScaleBand::Detail);
        }

        assert!(state.reconcile(policy, 0.8));
        for zoom in [0.81, 0.99, 0.9, 0.82] {
            assert!(!state.reconcile(policy, zoom));
            assert_eq!(state.band(), OpenGpuiNodeScaleBand::Overview);
        }
        assert!(state.reconcile(policy, 1.0));
        assert_eq!(state.band(), OpenGpuiNodeScaleBand::Detail);
    }

    #[test]
    fn invalid_zoom_is_inert_and_component_managed_always_returns_to_detail() {
        let semantic = OpenGpuiNodeScalePolicy::SemanticLod(
            OpenGpuiSemanticLodPolicy::new(0.8, 1.0).expect("valid thresholds"),
        );
        let mut state = OpenGpuiNodeScaleState::new(semantic, 0.5);
        assert_eq!(state.band(), OpenGpuiNodeScaleBand::Overview);
        assert!(!state.reconcile(semantic, f32::NAN));
        assert_eq!(state.band(), OpenGpuiNodeScaleBand::Overview);

        assert!(state.reconcile(OpenGpuiNodeScalePolicy::ComponentManaged, 0.1));
        assert_eq!(state.band(), OpenGpuiNodeScaleBand::Detail);
        assert!(!state.reconcile(OpenGpuiNodeScalePolicy::ComponentManaged, 0.01));
    }
}
