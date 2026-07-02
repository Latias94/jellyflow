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
}

impl OpenGpuiProductSurfacePreset {
    pub fn from_descriptor(descriptor: &NodeKindViewDescriptor) -> Self {
        let layout_budget = &descriptor.layout_budget;
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
            style: OpenGpuiSurfaceStyleBudget::for_renderer_key(&descriptor.renderer_key),
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
    }

    #[test]
    fn style_budget_evidence_exposes_clickable_affordance_numbers() {
        let style = OpenGpuiSurfaceStyleBudget::for_renderer_key("shader-card").evidence();

        assert!(style.handle_radius >= 5);
        assert!(style.handle_hit_width >= 24);
        assert!(style.edge_hit_width >= style.edge_stroke_width);
        assert!(serde_json::to_string(&style).is_ok());
    }
}
