use jellyflow::{
    core::{Graph, GraphTransaction, NodeGraphConnectionMode},
    runtime::{
        io::NodeGraphInteractionState,
        runtime::conformance::{
            ConformanceCapabilityClaim, ConformanceCapabilityKind, ConformanceCapabilityMatrix,
            ConformanceSupportLevel,
        },
        schema::NodeKindViewDescriptor,
    },
};

use crate::{
    OpenGpuiConnectionSyncError, OpenGpuiConnectionSyncRequest, OpenGpuiMeasurementCoverage,
    OpenGpuiNodeTransformSnapshot, OpenGpuiProductSurfacePreset,
    connection::plan_connection_sync_transactions as plan_connection_sync_transaction_batch,
    sync::plan_transform_sync_transaction as plan_host_transform_sync_transaction,
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

/// Minimal adapter facade for capability and measurement-mode reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiAdapter {
    measurement_mode: OpenGpuiMeasurementMode,
    measurement_coverage: Option<OpenGpuiMeasurementCoverage>,
}

impl OpenGpuiAdapter {
    pub fn projection_fallback() -> Self {
        Self {
            measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
            measurement_coverage: None,
        }
    }

    pub fn layout_pass(coverage: OpenGpuiMeasurementCoverage) -> Self {
        Self {
            measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
            measurement_coverage: Some(coverage),
        }
    }

    pub fn measurement_mode(&self) -> OpenGpuiMeasurementMode {
        self.measurement_mode
    }

    pub fn measurement_coverage(&self) -> Option<&OpenGpuiMeasurementCoverage> {
        self.measurement_coverage.as_ref()
    }

    pub fn product_surface_preset(
        &self,
        descriptor: &NodeKindViewDescriptor,
    ) -> OpenGpuiProductSurfacePreset {
        OpenGpuiProductSurfacePreset::from_descriptor(descriptor)
    }

    pub fn plan_transform_sync_transaction(
        &self,
        graph: &Graph,
        snapshots: impl IntoIterator<Item = OpenGpuiNodeTransformSnapshot>,
    ) -> GraphTransaction {
        plan_host_transform_sync_transaction(graph, snapshots)
    }

    pub fn plan_connection_sync_transactions(
        &self,
        graph: &Graph,
        requests: impl IntoIterator<Item = OpenGpuiConnectionSyncRequest>,
        mode: NodeGraphConnectionMode,
        interaction: &NodeGraphInteractionState,
    ) -> Result<Vec<GraphTransaction>, OpenGpuiConnectionSyncError> {
        plan_connection_sync_transaction_batch(graph, requests, mode, interaction)
    }

    fn layout_pass_support_level(&self) -> ConformanceSupportLevel {
        match self.measurement_mode {
            OpenGpuiMeasurementMode::ProjectionFallback => ConformanceSupportLevel::Projection,
            OpenGpuiMeasurementMode::LayoutPass => {
                let Some(coverage) = &self.measurement_coverage else {
                    return ConformanceSupportLevel::Projection;
                };
                if coverage.is_full_layout_pass() {
                    ConformanceSupportLevel::Full
                } else if coverage.layout_pass_regions > 0 {
                    ConformanceSupportLevel::Partial
                } else {
                    ConformanceSupportLevel::Projection
                }
            }
        }
    }

    fn layout_pass_notes(&self) -> String {
        match self.measurement_mode {
            OpenGpuiMeasurementMode::ProjectionFallback => {
                "fallback projection; not a mature layout-pass measurement claim".to_string()
            }
            OpenGpuiMeasurementMode::LayoutPass => {
                if let Some(coverage) = &self.measurement_coverage {
                    format!(
                        "bounds reported by open-gpui measured elements; layout_pass={}, projection_fallback={}, missing={}, stale={}, partial={}, duplicate={}",
                        coverage.layout_pass_regions,
                        coverage.projection_fallback_regions,
                        coverage.missing_regions,
                        coverage.stale_regions,
                        coverage.partial_regions,
                        coverage.duplicate_regions
                    )
                } else {
                    "layout-pass mode without measured-element coverage evidence".to_string()
                }
            }
        }
    }

    pub fn capability_matrix(&self) -> ConformanceCapabilityMatrix {
        ConformanceCapabilityMatrix::for_adapter(OPEN_GPUI_ADAPTER_ID)
            .with_claim(
                ConformanceCapabilityClaim::new(
                    ConformanceCapabilityKind::LayoutPassMeasurement,
                    self.layout_pass_support_level(),
                )
                .with_notes(self.layout_pass_notes()),
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
    use jellyflow::{
        core::{CanvasPoint, GraphBuilder, GraphId, Node, NodeId, NodeKindKey},
        runtime::schema::NodeKitRegistry,
    };

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
        let matrix = OpenGpuiAdapter::layout_pass(OpenGpuiMeasurementCoverage {
            layout_pass_regions: 4,
            projection_fallback_regions: 0,
            missing_regions: 0,
            stale_regions: 0,
            partial_regions: 0,
            duplicate_regions: 0,
            measured_slots: 2,
            measured_anchors: 2,
        })
        .capability_matrix();

        assert!(matrix.satisfies(
            ConformanceCapabilityKind::LayoutPassMeasurement,
            ConformanceSupportLevel::Full
        ));
    }

    #[test]
    fn capability_matrix_downgrades_partial_layout_pass_coverage() {
        let matrix = OpenGpuiAdapter::layout_pass(OpenGpuiMeasurementCoverage {
            layout_pass_regions: 3,
            projection_fallback_regions: 1,
            missing_regions: 0,
            stale_regions: 0,
            partial_regions: 0,
            duplicate_regions: 0,
            measured_slots: 2,
            measured_anchors: 1,
        })
        .capability_matrix();

        assert_eq!(
            matrix.level(ConformanceCapabilityKind::LayoutPassMeasurement),
            ConformanceSupportLevel::Partial
        );
        assert!(!matrix.satisfies(
            ConformanceCapabilityKind::LayoutPassMeasurement,
            ConformanceSupportLevel::Full
        ));
    }

    #[test]
    fn adapter_facade_exposes_product_preset_and_transform_sync_planner() {
        let adapter = OpenGpuiAdapter::default();
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("builtin descriptor should exist");
        let preset = adapter.product_surface_preset(&descriptor);
        assert_eq!(preset.renderer_key, "decision-card");

        let node_id = NodeId::from_u128(1);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node_id, node_at(10.0, 20.0))
            .build_unchecked();
        let transaction = adapter.plan_transform_sync_transaction(
            &graph,
            [OpenGpuiNodeTransformSnapshot::new(
                node_id,
                CanvasPoint { x: 40.0, y: 60.0 },
            )],
        );

        assert_eq!(transaction.len(), 1);
    }

    fn node_at(x: f32, y: f32) -> Node {
        Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: CanvasPoint { x, y },
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }
}
