//! Open GPUI adapter boundary for Jellyflow.
//!
//! Jellyflow runtime stays headless. This crate owns the GPUI-specific mapping
//! from semantic node descriptors to retained components, layout bounds, and
//! adapter capability facts.

#![deny(unsafe_code)]

pub mod actions;
pub mod adapter;
pub mod controls;
pub mod measurement;
pub mod projection;
pub mod repeatable;
pub mod testing;

pub use actions::{
    OpenGpuiActionDispatchPlan, OpenGpuiActionPlan, OpenGpuiActionSurface,
    OpenGpuiDroppedWireInsertPlan, OpenGpuiMenuPlan, plan_action_dispatch,
    plan_dropped_wire_insert, project_action, project_actions_for_surface,
    project_dropped_wire_menu, project_menu,
};
pub use adapter::{OPEN_GPUI_ADAPTER_ID, OpenGpuiAdapter, OpenGpuiMeasurementMode};
pub use controls::{
    OpenGpuiControlEditPlan, OpenGpuiControlOptionPlan, OpenGpuiControlPlan,
    OpenGpuiControlPrimitive, OpenGpuiControlProjectionContext, OpenGpuiControlSupport,
    plan_control_edit, primitive_for_kind, project_control, project_slot_controls,
    support_for_primitive,
};
pub use measurement::{
    OpenGpuiBoundsCollector, OpenGpuiMeasuredRegion, OpenGpuiMeasuredRegionKind,
    OpenGpuiMeasurementContext, OpenGpuiViewBounds, OpenGpuiViewPoint, OpenGpuiViewSize,
};
pub use projection::{
    OpenGpuiNodeSurfaceLayout, OpenGpuiNodeSurfaceSlotLayout, OpenGpuiRepeatableSurfaceLayout,
    OpenGpuiRepeatableSurfaceProjection, measured_surface_anchors, measured_surface_slots,
    project_node_measurement, projected_node_surface_component_layout,
    projected_node_surface_graph_layout, repeatable_surface_projection,
    semantic_component_priority, slot_anchor_rect, slot_projection_visibility, slot_row_rect,
    slot_row_y,
};
pub use repeatable::{
    OpenGpuiDynamicPortPolicy, OpenGpuiRepeatableItemLayout, OpenGpuiRepeatableItemProjection,
    OpenGpuiRepeatablePortDiagnostic, measured_repeatable_item_anchors,
    measured_repeatable_item_slots, repeatable_item_control_count, repeatable_item_label,
    repeatable_item_projection, repeatable_port_diagnostics,
};
