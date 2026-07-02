use jellyflow::{
    core::CanvasSize,
    runtime::schema::{
        NodeKindViewDescriptor, NodeKitContentDensity, NodeSurfaceOverflowIndicator,
    },
};
use serde::{Deserialize, Serialize};

/// Quantized logical size used in serializable adapter reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OpenGpuiSizeEvidence {
    pub width: u32,
    pub height: u32,
}

impl OpenGpuiSizeEvidence {
    pub fn from_canvas_size(size: CanvasSize) -> Self {
        Self {
            width: size.width.max(0.0).round() as u32,
            height: size.height.max(0.0).round() as u32,
        }
    }

    pub fn contains(self, minimum: Self) -> bool {
        self.width >= minimum.width && self.height >= minimum.height
    }
}

/// Adapter-local style budget for product graph affordances.
///
/// Values stay renderer-neutral numbers so hosts can map them into GPUI, egui, screenshots,
/// or structured checks without importing widget types here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OpenGpuiSurfaceStyleBudget {
    pub handle_radius: f32,
    pub handle_hit_width: f32,
    pub edge_stroke_width: f32,
    pub edge_hit_width: f32,
    pub row_height: f32,
    pub control_row_height: f32,
    pub repeatable_row_height: f32,
}

impl Default for OpenGpuiSurfaceStyleBudget {
    fn default() -> Self {
        Self {
            handle_radius: 5.0,
            handle_hit_width: 22.0,
            edge_stroke_width: 2.0,
            edge_hit_width: 18.0,
            row_height: 26.0,
            control_row_height: 34.0,
            repeatable_row_height: 34.0,
        }
    }
}

impl OpenGpuiSurfaceStyleBudget {
    pub fn for_renderer_key(renderer_key: &str) -> Self {
        let mut budget = Self::default();
        match renderer_key {
            "shader-card" => {
                budget.handle_radius = 6.0;
                budget.handle_hit_width = 24.0;
                budget.edge_hit_width = 24.0;
            }
            "table-card" => {
                budget.row_height = 30.0;
                budget.handle_hit_width = 24.0;
            }
            "topic-card" | "source-card" => {
                budget.edge_hit_width = 20.0;
                budget.control_row_height = 32.0;
            }
            _ => {}
        }
        budget
    }

    pub fn evidence(self) -> OpenGpuiStyleBudgetEvidence {
        OpenGpuiStyleBudgetEvidence {
            handle_radius: self.handle_radius.round() as u32,
            handle_hit_width: self.handle_hit_width.round() as u32,
            edge_stroke_width: self.edge_stroke_width.round() as u32,
            edge_hit_width: self.edge_hit_width.round() as u32,
            row_height: self.row_height.round() as u32,
            control_row_height: self.control_row_height.round() as u32,
            repeatable_row_height: self.repeatable_row_height.round() as u32,
        }
    }
}

/// Integer style facts exposed in host reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OpenGpuiStyleBudgetEvidence {
    pub handle_radius: u32,
    pub handle_hit_width: u32,
    pub edge_stroke_width: u32,
    pub edge_hit_width: u32,
    pub row_height: u32,
    pub control_row_height: u32,
    pub repeatable_row_height: u32,
}

/// Widget-free budgets for host-local component fit evidence.
///
/// The host still owns concrete GPUI layout and widgets. These numbers keep the
/// evidence thresholds reviewable from the adapter crate instead of scattering
/// magic constants through the example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiComponentFitBudget {
    pub content_horizontal_padding: u32,
    pub text_region_height: u32,
    pub control_region_height: u32,
    pub full_text_line_budget: usize,
    pub compact_text_line_budget: usize,
}

impl OpenGpuiComponentFitBudget {
    pub fn for_renderer_key(
        renderer_key: &str,
        style: OpenGpuiSurfaceStyleBudget,
        slot_line_budget: Option<usize>,
        control_line_budget: Option<usize>,
    ) -> Self {
        let full_text_line_budget = slot_line_budget.unwrap_or(2).max(1);
        let compact_text_line_budget = control_line_budget
            .or(slot_line_budget)
            .unwrap_or(1)
            .max(1)
            .min(full_text_line_budget);
        let mut budget = Self {
            content_horizontal_padding: 20,
            text_region_height: style.control_row_height.round().max(1.0) as u32,
            control_region_height: style.control_row_height.round().max(1.0) as u32,
            full_text_line_budget,
            compact_text_line_budget,
        };

        match renderer_key {
            "source-card" => {
                budget.text_region_height = budget.text_region_height.max(40);
            }
            "shader-card" | "table-card" => {
                budget.control_region_height = budget.control_region_height.max(34);
            }
            _ => {}
        }

        budget
    }

    pub fn available_content_width(self, size: CanvasSize) -> f32 {
        (size.width - self.content_horizontal_padding as f32).max(1.0)
    }
}

/// Serializable route family evidence for product graph affordances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiWireRouteEvidence {
    Straight,
    Orthogonal,
    Bezier,
}

/// Serializable preview policy evidence for in-progress connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiConnectionPreviewPolicyEvidence {
    DirectLineFallback,
    MirrorsCommittedRoute,
}

/// Widget-free evidence that a host exposes graph affordances with product budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiGraphAffordanceEvidence {
    pub committed_wire_route: OpenGpuiWireRouteEvidence,
    pub connection_preview_policy: OpenGpuiConnectionPreviewPolicyEvidence,
    pub port_placement_budget: u32,
    pub endpoint_hit_budget: u32,
    pub reconnect_affordance_budget: u32,
    pub drag_region_count: usize,
    pub readable_layout_region_count: usize,
}

impl OpenGpuiGraphAffordanceEvidence {
    pub fn for_renderer_key(renderer_key: &str, style: OpenGpuiSurfaceStyleBudget) -> Self {
        let style = style.evidence();
        let mut evidence = Self {
            committed_wire_route: OpenGpuiWireRouteEvidence::Orthogonal,
            connection_preview_policy:
                OpenGpuiConnectionPreviewPolicyEvidence::MirrorsCommittedRoute,
            port_placement_budget: 12,
            endpoint_hit_budget: style.handle_hit_width,
            reconnect_affordance_budget: style.edge_hit_width.max(style.handle_hit_width),
            drag_region_count: 1,
            readable_layout_region_count: 3,
        };

        match renderer_key {
            "shader-card" => {
                evidence.committed_wire_route = OpenGpuiWireRouteEvidence::Bezier;
                evidence.port_placement_budget = 16;
                evidence.drag_region_count = 2;
                evidence.readable_layout_region_count = 5;
            }
            "table-card" => {
                evidence.port_placement_budget = 14;
                evidence.readable_layout_region_count = 6;
            }
            "topic-card" | "source-card" => {
                evidence.drag_region_count = 2;
                evidence.readable_layout_region_count = 4;
            }
            _ => {}
        }

        evidence
    }

    pub fn has_product_route_policy(self) -> bool {
        self.committed_wire_route != OpenGpuiWireRouteEvidence::Straight
            && self.connection_preview_policy
                == OpenGpuiConnectionPreviewPolicyEvidence::MirrorsCommittedRoute
    }

    pub fn has_product_hit_budgets(self) -> bool {
        self.port_placement_budget >= 12
            && self.endpoint_hit_budget >= 20
            && self.reconnect_affordance_budget >= 18
    }

    pub fn has_layout_region_evidence(self) -> bool {
        self.drag_region_count > 0 && self.readable_layout_region_count > 0
    }
}

/// Descriptor-derived product surface preset shared by host rendering and reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenGpuiProductSurfacePreset {
    pub renderer_key: String,
    pub default_size: Option<CanvasSize>,
    pub min_readable_size: Option<CanvasSize>,
    pub preferred_size: Option<CanvasSize>,
    pub slot_line_budget: Option<usize>,
    pub control_line_budget: Option<usize>,
    pub repeatable_visible_items: Option<usize>,
    pub overflow_indicator: Option<NodeSurfaceOverflowIndicator>,
    pub density_priority: Vec<NodeKitContentDensity>,
    pub style: OpenGpuiSurfaceStyleBudget,
    pub graph_affordance: OpenGpuiGraphAffordanceEvidence,
    pub component_fit_budget: OpenGpuiComponentFitBudget,
}

impl OpenGpuiProductSurfacePreset {
    pub fn from_descriptor(descriptor: &NodeKindViewDescriptor) -> Self {
        let layout_budget = &descriptor.layout_budget;
        let style = OpenGpuiSurfaceStyleBudget::for_renderer_key(&descriptor.renderer_key);
        let component_fit_budget = OpenGpuiComponentFitBudget::for_renderer_key(
            &descriptor.renderer_key,
            style,
            layout_budget.slot_line_budget,
            layout_budget.control_line_budget,
        );
        Self {
            renderer_key: descriptor.renderer_key.clone(),
            default_size: descriptor.default_size,
            min_readable_size: layout_budget.min_readable_size,
            preferred_size: layout_budget.preferred_size,
            slot_line_budget: layout_budget.slot_line_budget,
            control_line_budget: layout_budget.control_line_budget,
            repeatable_visible_items: layout_budget.repeatable_visible_items,
            overflow_indicator: layout_budget.overflow_indicator,
            density_priority: layout_budget.density_priority.clone(),
            style,
            graph_affordance: OpenGpuiGraphAffordanceEvidence::for_renderer_key(
                &descriptor.renderer_key,
                style,
            ),
            component_fit_budget,
        }
    }

    pub fn initial_size_or(&self, fallback: CanvasSize) -> CanvasSize {
        self.preferred_size
            .or(self.min_readable_size)
            .or(self.default_size)
            .unwrap_or(fallback)
    }

    pub fn readable_size_for_request(&self, requested_size: CanvasSize) -> CanvasSize {
        let Some(minimum) = self.min_readable_size else {
            return requested_size;
        };
        CanvasSize {
            width: requested_size.width.max(minimum.width),
            height: requested_size.height.max(minimum.height),
        }
    }

    pub fn repeatable_visible_items_or(&self, fallback: usize) -> usize {
        self.repeatable_visible_items.unwrap_or(fallback)
    }

    pub fn density_priority_labels(&self) -> Vec<&'static str> {
        self.density_priority
            .iter()
            .map(|density| match density {
                NodeKitContentDensity::Compact => "compact",
                NodeKitContentDensity::Regular => "regular",
                NodeKitContentDensity::Full => "full",
            })
            .collect()
    }

    pub fn min_readable_size_evidence(&self) -> Option<OpenGpuiSizeEvidence> {
        self.min_readable_size
            .map(OpenGpuiSizeEvidence::from_canvas_size)
    }
}

#[cfg(test)]
mod tests {
    use jellyflow::{
        core::NodeKindKey,
        runtime::schema::{NodeKitContentDensity, NodeKitRegistry, NodeSurfaceOverflowIndicator},
    };

    use super::*;

    #[test]
    fn product_surface_preset_uses_runtime_layout_budget() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("builtin llm descriptor");

        let preset = OpenGpuiProductSurfacePreset::from_descriptor(&descriptor);

        assert_eq!(preset.renderer_key, "decision-card");
        assert_eq!(
            preset.min_readable_size,
            Some(CanvasSize {
                width: 340.0,
                height: 288.0,
            })
        );
        assert_eq!(preset.repeatable_visible_items, Some(3));
        assert_eq!(
            preset.overflow_indicator,
            Some(NodeSurfaceOverflowIndicator::Count)
        );
        assert_eq!(
            preset.density_priority.as_slice(),
            &[
                NodeKitContentDensity::Full,
                NodeKitContentDensity::Regular,
                NodeKitContentDensity::Compact,
            ]
        );
        assert_eq!(
            preset.density_priority_labels(),
            ["full", "regular", "compact"]
        );
        assert!(preset.graph_affordance.has_product_route_policy());
        assert!(preset.graph_affordance.has_product_hit_budgets());
        assert!(preset.graph_affordance.has_layout_region_evidence());
        assert_eq!(preset.component_fit_budget.content_horizontal_padding, 20);
        assert!(preset.component_fit_budget.control_region_height >= 32);
        assert!(serde_json::to_string(&preset.component_fit_budget).is_ok());
    }

    #[test]
    fn style_budget_evidence_exposes_clickable_affordance_numbers() {
        let style = OpenGpuiSurfaceStyleBudget::for_renderer_key("shader-card").evidence();

        assert!(style.handle_radius >= 5);
        assert!(style.handle_hit_width >= 24);
        assert!(style.edge_hit_width >= style.edge_stroke_width);
        assert!(serde_json::to_string(&style).is_ok());
    }

    #[test]
    fn graph_affordance_evidence_serializes_route_hit_and_layout_budgets() {
        let evidence = OpenGpuiGraphAffordanceEvidence::for_renderer_key(
            "shader-card",
            OpenGpuiSurfaceStyleBudget::for_renderer_key("shader-card"),
        );

        assert_eq!(
            evidence.committed_wire_route,
            OpenGpuiWireRouteEvidence::Bezier
        );
        assert_eq!(
            evidence.connection_preview_policy,
            OpenGpuiConnectionPreviewPolicyEvidence::MirrorsCommittedRoute
        );
        assert!(evidence.has_product_route_policy());
        assert!(evidence.has_product_hit_budgets());
        assert!(evidence.has_layout_region_evidence());
        assert!(serde_json::to_string(&evidence).is_ok());
    }
}
