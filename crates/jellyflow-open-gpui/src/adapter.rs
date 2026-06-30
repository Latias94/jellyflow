use jellyflow::runtime::runtime::conformance::{
    ConformanceCapabilityClaim, ConformanceCapabilityKind, ConformanceCapabilityMatrix,
    ConformanceSupportLevel,
};

/// Stable adapter identifier used in conformance reports.
pub const OPEN_GPUI_ADAPTER_ID: &str = "open-gpui";

/// How the adapter produced node-internal measurement facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiMeasurementMode {
    /// Geometry came from fallback projection rather than GPUI layout-pass bounds.
    ProjectionFallback,
    /// Geometry came from GPUI measured element bounds.
    LayoutPass,
}

impl OpenGpuiMeasurementMode {
    pub fn support_level(self) -> ConformanceSupportLevel {
        match self {
            Self::ProjectionFallback => ConformanceSupportLevel::Projection,
            Self::LayoutPass => ConformanceSupportLevel::Full,
        }
    }
}

/// Minimal adapter facade for capability and measurement-mode reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiAdapter {
    measurement_mode: OpenGpuiMeasurementMode,
}

impl OpenGpuiAdapter {
    pub fn projection_fallback() -> Self {
        Self {
            measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        }
    }

    pub fn layout_pass() -> Self {
        Self {
            measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
        }
    }

    pub fn measurement_mode(&self) -> OpenGpuiMeasurementMode {
        self.measurement_mode
    }

    pub fn capability_matrix(&self) -> ConformanceCapabilityMatrix {
        ConformanceCapabilityMatrix::for_adapter(OPEN_GPUI_ADAPTER_ID)
            .with_claim(
                ConformanceCapabilityClaim::new(
                    ConformanceCapabilityKind::LayoutPassMeasurement,
                    self.measurement_mode.support_level(),
                )
                .with_notes(match self.measurement_mode {
                    OpenGpuiMeasurementMode::ProjectionFallback => {
                        "fallback projection; not a mature layout-pass measurement claim"
                    }
                    OpenGpuiMeasurementMode::LayoutPass => {
                        "bounds reported by open-gpui measured elements"
                    }
                }),
            )
            .with_claim(ConformanceCapabilityClaim::partial(
                ConformanceCapabilityKind::ControlProjection,
            ))
            .with_claim(ConformanceCapabilityClaim::partial(
                ConformanceCapabilityKind::RepeatableCollections,
            ))
            .with_claim(ConformanceCapabilityClaim::partial(
                ConformanceCapabilityKind::Actions,
            ))
            .with_claim(ConformanceCapabilityClaim::partial(
                ConformanceCapabilityKind::Menus,
            ))
            .with_claim(ConformanceCapabilityClaim::partial(
                ConformanceCapabilityKind::Inspector,
            ))
    }
}

impl Default for OpenGpuiAdapter {
    fn default() -> Self {
        Self::projection_fallback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_matrix_keeps_projection_fallback_honest() {
        let matrix = OpenGpuiAdapter::projection_fallback().capability_matrix();

        assert_eq!(
            matrix.level(ConformanceCapabilityKind::LayoutPassMeasurement),
            ConformanceSupportLevel::Projection
        );
        assert_eq!(
            matrix.level(ConformanceCapabilityKind::ControlProjection),
            ConformanceSupportLevel::Partial
        );
        assert!(!matrix.satisfies(
            ConformanceCapabilityKind::LayoutPassMeasurement,
            ConformanceSupportLevel::Full
        ));
    }

    #[test]
    fn capability_matrix_can_claim_full_layout_pass_after_real_bounds() {
        let matrix = OpenGpuiAdapter::layout_pass().capability_matrix();

        assert!(matrix.satisfies(
            ConformanceCapabilityKind::LayoutPassMeasurement,
            ConformanceSupportLevel::Full
        ));
    }
}
