//! Test helpers for GPUI adapter conformance.

use jellyflow::runtime::runtime::conformance::{
    ConformanceCapabilityKind, ConformanceSupportLevel,
};

use crate::OpenGpuiAdapter;

pub fn assert_layout_pass_capability_requires_real_bounds(adapter: &OpenGpuiAdapter) {
    let matrix = adapter.capability_matrix();
    if matrix.satisfies(
        ConformanceCapabilityKind::LayoutPassMeasurement,
        ConformanceSupportLevel::Full,
    ) {
        assert_eq!(
            adapter.measurement_mode(),
            crate::OpenGpuiMeasurementMode::LayoutPass
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OpenGpuiMeasurementMode;

    #[test]
    fn helper_rejects_full_claim_without_layout_pass_mode() {
        let adapter = OpenGpuiAdapter::projection_fallback();
        assert_layout_pass_capability_requires_real_bounds(&adapter);
        assert_eq!(
            adapter.measurement_mode(),
            OpenGpuiMeasurementMode::ProjectionFallback
        );
    }
}
