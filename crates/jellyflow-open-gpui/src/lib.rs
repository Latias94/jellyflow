//! Open GPUI adapter boundary for Jellyflow.
//!
//! Jellyflow runtime stays headless. This crate owns the GPUI-specific mapping
//! from semantic node descriptors to retained components, layout bounds, and
//! adapter capability facts.

#![deny(unsafe_code)]

pub mod actions;
pub mod adapter;
pub mod authoring;
pub mod connection;
pub mod controls;
pub mod element_ids;
pub mod inspector;
mod json_binding;
pub mod measurement;
pub mod presets;
pub mod projection;
pub mod renderer;
pub mod repeatable;
pub mod sync;
pub mod testing;

pub use actions::{
    OpenGpuiActionDispatchPlan, OpenGpuiActionPlan, OpenGpuiActionSurface,
    OpenGpuiBlackboardItemPlan, OpenGpuiBlackboardPlan, OpenGpuiDroppedWireInsertError,
    OpenGpuiDroppedWireInsertOutcome, OpenGpuiDroppedWireInsertPlan,
    OpenGpuiDroppedWireInsertTransactionPlan, OpenGpuiMenuPlan, apply_dropped_wire_insert,
    plan_action_dispatch, plan_dropped_wire_insert, plan_dropped_wire_insert_transaction,
    project_action, project_actions_for_surface, project_blackboard,
    project_blackboards_for_descriptor, project_dropped_wire_menu, project_menu,
};
pub use adapter::{OPEN_GPUI_ADAPTER_ID, OpenGpuiAdapter, OpenGpuiMeasurementMode};
pub use authoring::{
    OpenGpuiAuthoringController, OpenGpuiAuthoringOutcome, OpenGpuiAuthoringSkipReason,
    OpenGpuiControlEventValue, OpenGpuiRepeatableAddItemContext, control_option_key,
    control_option_value_key, control_selected_option_key,
};
pub use connection::{
    OpenGpuiConnectionSyncError, OpenGpuiConnectionSyncRequest, plan_connection_sync_transaction,
    plan_connection_sync_transactions,
};
pub use controls::{
    OpenGpuiControlEditPlan, OpenGpuiControlOptionPlan, OpenGpuiControlPlan,
    OpenGpuiControlPrimitive, OpenGpuiControlProjectionContext, OpenGpuiControlSupport,
    plan_control_edit, primitive_for_kind, project_control, project_slot_controls,
    support_for_primitive,
};
pub use element_ids::{
    open_gpui_action_button_element_id, open_gpui_action_menu_element_id,
    open_gpui_action_summary_element_id, open_gpui_blackboard_item_element_id,
    open_gpui_blackboard_status_element_id, open_gpui_chrome_fallback_button_element_id,
    open_gpui_control_element_id, open_gpui_custom_action_missing_element_id,
    open_gpui_custom_renderer_badge_element_id, open_gpui_custom_repeatables_badge_element_id,
    open_gpui_custom_slots_badge_element_id, open_gpui_node_element_scope,
    open_gpui_node_surface_wrapper_element_id, open_gpui_product_card_element_id,
    open_gpui_repeatable_add_action_element_id, open_gpui_repeatable_collection_element_id,
    open_gpui_repeatable_item_element_id, open_gpui_repeatable_remove_action_element_id,
    open_gpui_repeatable_reorder_action_element_id, open_gpui_slot_action_button_element_id,
    open_gpui_slot_action_label_element_id, open_gpui_slot_badge_element_id,
    open_gpui_slot_preview_progress_element_id, open_gpui_slot_status_label_element_id,
    open_gpui_slot_value_element_id,
};
pub use inspector::{
    OpenGpuiInspectorPlan, OpenGpuiInspectorSurface, OpenGpuiInspectorTargetBounds,
    OpenGpuiInspectorTargetSource, plan_inspector_control_edit, project_inspector,
    project_inspectors_for_surface, resolve_inspector_target_bounds,
};
pub use measurement::{
    OpenGpuiBoundsCollector, OpenGpuiMeasuredRegion, OpenGpuiMeasuredRegionKind,
    OpenGpuiMeasurementContext, OpenGpuiMeasurementCoverage, OpenGpuiMeasurementId,
    OpenGpuiMeasurementRevisionDecision, OpenGpuiMeasurementSource,
    OpenGpuiProjectionFallbackStoreEvidence, OpenGpuiProjectionFallbackStoreSummary,
    OpenGpuiProjectionMeasurementSource, OpenGpuiViewBounds, OpenGpuiViewPoint, OpenGpuiViewSize,
    assign_layout_pass_measurement_revision, layout_pass_measurement_from_regions,
    open_gpui_measurement_regions_match,
};
pub use presets::{
    OpenGpuiConnectionPreviewPolicyEvidence, OpenGpuiGraphAffordanceEvidence,
    OpenGpuiProductSurfacePreset, OpenGpuiSizeEvidence, OpenGpuiStyleBudgetEvidence,
    OpenGpuiSurfaceStyleBudget, OpenGpuiWireRouteEvidence,
};
pub use projection::{
    OpenGpuiNodeSurfaceLayout, OpenGpuiNodeSurfaceSlotLayout, OpenGpuiRepeatableSurfaceLayout,
    OpenGpuiRepeatableSurfaceProjection, measured_surface_anchors, measured_surface_slots,
    project_node_measurement, projected_node_surface_component_layout,
    projected_node_surface_graph_layout, repeatable_surface_projection,
    semantic_component_priority, slot_anchor_rect, slot_projection_visibility, slot_row_rect,
    slot_row_y,
};
pub use renderer::{
    OpenGpuiNodeRendererContext, OpenGpuiNodeRendererFallback, OpenGpuiNodeRendererFallbackReason,
    OpenGpuiNodeRendererHostContext, OpenGpuiNodeRendererOutput, OpenGpuiNodeRendererOutputSource,
    OpenGpuiNodeRendererRegistration, OpenGpuiNodeRendererRegistry, OpenGpuiNodeRendererResolution,
    OpenGpuiNodeRendererState, OpenGpuiNodeRendererTable, open_gpui_node_renderer_context,
    open_gpui_renderer_repeatable_items, open_gpui_renderer_repeatable_surfaces,
};
pub use repeatable::{
    OpenGpuiDynamicPortPolicy, OpenGpuiRepeatableActionPlan, OpenGpuiRepeatableEditError,
    OpenGpuiRepeatableEditPlan, OpenGpuiRepeatableItemLayout, OpenGpuiRepeatableItemProjection,
    OpenGpuiRepeatablePortDiagnostic, measured_repeatable_item_anchors,
    measured_repeatable_item_slots, plan_repeatable_action, repeatable_item_control_count,
    repeatable_item_label, repeatable_item_projection, repeatable_port_diagnostics,
};
pub use sync::{OpenGpuiNodeTransformSnapshot, plan_transform_sync_transaction};
pub use testing::{
    OPEN_GPUI_MIN_PRODUCT_PORT_HIT_SIZE, OpenGpuiConnectionReleaseEvidence,
    OpenGpuiFirstPointerEvidence, OpenGpuiHostProductInteractionGap,
    OpenGpuiHostProductInteractionReport, OpenGpuiHostRendererSource, OpenGpuiHostSurfaceReport,
    OpenGpuiHostSurfaceReportRow, OpenGpuiHostVisualInteractionGap,
    OpenGpuiHostVisualInteractionReport, OpenGpuiHostVisualSurfaceRow,
    OpenGpuiMeasuredContentEvidence, OpenGpuiMeasuredInternalsEvidence,
    OpenGpuiMeasuredInternalsEvidenceInput, OpenGpuiMeasuredInternalsSource,
    OpenGpuiNativeLifecycleEvidence, OpenGpuiNativeLifecycleGap, OpenGpuiPortHandleEvidence,
    OpenGpuiProductFixtureCase, OpenGpuiProductFixtureFamily, OpenGpuiProductFixtureKind,
    OpenGpuiReconnectSequenceEvidence, OpenGpuiScreenshotFixtureEvidence,
    OpenGpuiScreenshotRegionEvidence, OpenGpuiScreenshotRegionGap, OpenGpuiScreenshotRegionKind,
    OpenGpuiScreenshotRegionRect, OpenGpuiScreenshotRegionReport,
    assert_host_surface_report_contract, assert_host_visual_interaction_report_gates,
    assert_native_lifecycle_evidence_gates, assert_product_gallery_host_report_gates,
    assert_product_interaction_characterization_report_contract,
    assert_product_interaction_report_gates, assert_screenshot_region_report_gates,
    open_gpui_measured_content_evidence_from_slots, open_gpui_measured_internals_evidence,
    open_gpui_measured_region_kind_evidence, product_fixture_catalog,
};
