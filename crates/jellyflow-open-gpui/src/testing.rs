//! Test helpers and regression gates for GPUI adapter conformance.

use std::collections::BTreeSet;

use jellyflow::{
    core::{
        CanvasRect, CanvasSize, DefaultTypeCompatibility, Edge, EdgeId, EdgeKind, Graph,
        GraphBuilder, GraphId, GraphOp, GraphTransaction, Node, NodeId, NodeKindKey, PortId,
        PortKey,
    },
    runtime::{
        runtime::{
            conformance::{ConformanceCapabilityKind, ConformanceSupportLevel},
            measurement::{MeasuredSurfaceSlot, NodeMeasurement},
        },
        schema::{
            NodeKindViewDescriptor, NodeKitContentDensity, NodeKitKey, NodeKitRegistry,
            NodeRegistry,
        },
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    OpenGpuiActionSurface, OpenGpuiAdapter, OpenGpuiControlPrimitive, OpenGpuiDynamicPortPolicy,
    OpenGpuiGraphAffordanceEvidence, OpenGpuiInspectorSurface, OpenGpuiInspectorTargetSource,
    OpenGpuiMeasuredRegion, OpenGpuiMeasurementContext, OpenGpuiMeasurementCoverage,
    OpenGpuiMeasurementId, OpenGpuiMeasurementMode, OpenGpuiNodeSurfaceLayout,
    OpenGpuiRepeatableActionPlan, OpenGpuiRepeatableItemLayout, OpenGpuiRepeatablePortDiagnostic,
    OpenGpuiSizeEvidence, OpenGpuiStyleBudgetEvidence, OpenGpuiViewBounds, OpenGpuiViewPoint,
    OpenGpuiViewSize, layout_pass_measurement_from_regions, measured_surface_anchors,
    plan_repeatable_action, primitive_for_kind, project_actions_for_surface,
    project_blackboards_for_descriptor, project_node_measurement, project_slot_controls,
    projected_node_surface_graph_layout, repeatable_item_projection, repeatable_port_diagnostics,
    resolve_inspector_target_bounds,
};

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
        assert!(
            adapter
                .measurement_coverage()
                .is_some_and(|coverage| coverage.is_full_layout_pass()),
            "full layout-pass support requires measured-element source coverage"
        );
    }
}

/// A builtin product shape that GPUI adapter regression gates must keep covering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiProductFixtureKind {
    DifyWorkflow,
    ShaderBlueprint,
    ErdTable,
    MindMap,
}

/// Product family covered by one Open GPUI gallery fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiProductFixtureFamily {
    Workflow,
    ShaderGraph,
    Erd,
    MindMap,
}

/// Widget-free catalog entry for one product-shaped Open GPUI gallery fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiProductFixtureCase {
    pub id: String,
    pub kind: OpenGpuiProductFixtureKind,
    pub family: OpenGpuiProductFixtureFamily,
    pub kit_key: String,
    pub fixture_key: String,
    pub expected_renderer_keys: BTreeSet<String>,
    pub expected_capabilities: BTreeSet<String>,
}

/// Host-side source used to render one product surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiHostRendererSource {
    ProductRenderer,
    DescriptorFallback,
    MissingHostRenderer,
    UnregisteredRenderer,
}

/// Structured capability gap collected by an Open GPUI host report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiHostCapabilityGap {
    RendererFallback,
    UnsupportedControl,
    AdvancedControlStub,
    MissingMeasuredRegion,
    PartialOrHiddenRegion,
    MissingDynamicPort,
}

/// Host-level evidence for one rendered product node surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiHostSurfaceReportRow {
    pub fixture_id: String,
    pub fixture_kind: OpenGpuiProductFixtureKind,
    pub family: OpenGpuiProductFixtureFamily,
    pub node_kind: String,
    pub renderer_key: String,
    pub source: OpenGpuiHostRendererSource,
    pub measured_slots: usize,
    pub measured_anchors: usize,
    pub style_budget: Option<OpenGpuiStyleBudgetEvidence>,
    pub unsupported_controls: BTreeSet<String>,
    pub capability_gaps: BTreeSet<OpenGpuiHostCapabilityGap>,
}

impl OpenGpuiHostSurfaceReportRow {
    pub fn new(
        fixture: &OpenGpuiProductFixtureCase,
        node_kind: impl Into<String>,
        renderer_key: impl Into<String>,
        source: OpenGpuiHostRendererSource,
    ) -> Self {
        let mut row = Self {
            fixture_id: fixture.id.clone(),
            fixture_kind: fixture.kind,
            family: fixture.family,
            node_kind: node_kind.into(),
            renderer_key: renderer_key.into(),
            source,
            measured_slots: 0,
            measured_anchors: 0,
            style_budget: None,
            unsupported_controls: BTreeSet::new(),
            capability_gaps: BTreeSet::new(),
        };
        row.sync_source_gap();
        row
    }

    pub fn with_measurement(mut self, measured_slots: usize, measured_anchors: usize) -> Self {
        self.measured_slots = measured_slots;
        self.measured_anchors = measured_anchors;
        if measured_slots + measured_anchors == 0 {
            self.capability_gaps
                .insert(OpenGpuiHostCapabilityGap::MissingMeasuredRegion);
        } else {
            self.capability_gaps
                .remove(&OpenGpuiHostCapabilityGap::MissingMeasuredRegion);
        }
        self
    }

    pub fn with_style_budget(mut self, style_budget: OpenGpuiStyleBudgetEvidence) -> Self {
        self.style_budget = Some(style_budget);
        self
    }

    pub fn with_gap(mut self, gap: OpenGpuiHostCapabilityGap) -> Self {
        self.capability_gaps.insert(gap);
        self
    }

    pub fn with_unsupported_control(mut self, control_key: impl Into<String>) -> Self {
        self.unsupported_controls.insert(control_key.into());
        self.capability_gaps
            .insert(OpenGpuiHostCapabilityGap::UnsupportedControl);
        self
    }

    pub fn is_product_renderer(&self) -> bool {
        self.source == OpenGpuiHostRendererSource::ProductRenderer
    }

    pub fn uses_renderer_fallback(&self) -> bool {
        matches!(
            self.source,
            OpenGpuiHostRendererSource::DescriptorFallback
                | OpenGpuiHostRendererSource::MissingHostRenderer
                | OpenGpuiHostRendererSource::UnregisteredRenderer
        )
    }

    fn sync_source_gap(&mut self) {
        if self.uses_renderer_fallback() {
            self.capability_gaps
                .insert(OpenGpuiHostCapabilityGap::RendererFallback);
        }
    }
}

/// Host-level evidence for the real Open GPUI product gallery path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiHostSurfaceReport {
    pub rows: Vec<OpenGpuiHostSurfaceReportRow>,
}

impl OpenGpuiHostSurfaceReport {
    pub fn new(rows: impl IntoIterator<Item = OpenGpuiHostSurfaceReportRow>) -> Self {
        Self {
            rows: rows.into_iter().collect(),
        }
    }

    pub fn push(&mut self, row: OpenGpuiHostSurfaceReportRow) {
        self.rows.push(row);
    }

    pub fn rows_for_fixture(
        &self,
        fixture_id: &str,
    ) -> impl Iterator<Item = &OpenGpuiHostSurfaceReportRow> {
        self.rows
            .iter()
            .filter(move |row| row.fixture_id == fixture_id)
    }

    pub fn fallback_rows(&self) -> impl Iterator<Item = &OpenGpuiHostSurfaceReportRow> {
        self.rows.iter().filter(|row| row.uses_renderer_fallback())
    }
}

/// User-visible host regression gap collected from the concrete Open GPUI gallery path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiHostVisualInteractionGap {
    UnselectedContentHidden,
    ContentOverflow,
    NodeBoundsOverlap,
    HandleOverlap,
    StaleMeasuredRegion,
    MissingRepeatableAnchor,
    BelowReadableSize,
    TextOverflow,
    ControlClipping,
    HiddenRepeatableOverflow,
    RepeatableOverflowIndicatorMissing,
    InvalidHoverOutOfBounds,
    DroppedWireMenuOutOfBounds,
    EdgeEndpointNotFollowingMeasuredHandle,
}

/// Host-level visual/layout evidence for one rendered product node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiHostVisualSurfaceRow {
    pub fixture_id: String,
    pub fixture_kind: OpenGpuiProductFixtureKind,
    pub family: OpenGpuiProductFixtureFamily,
    pub node_kind: String,
    pub renderer_key: String,
    pub source: OpenGpuiHostRendererSource,
    pub selected: bool,
    pub content_visible: bool,
    pub content_readable: bool,
    pub content_within_node_bounds: bool,
    pub actual_size: Option<OpenGpuiSizeEvidence>,
    pub min_readable_size: Option<OpenGpuiSizeEvidence>,
    pub text_overflow_count: usize,
    pub clipped_control_count: usize,
    pub handle_overlap_count: usize,
    pub stale_measured_regions: usize,
    pub repeatable_rows: usize,
    pub repeatable_rows_with_anchors: usize,
    pub hidden_repeatable_overflow_count: usize,
    pub repeatable_overflow_indicator_count: usize,
    pub gaps: BTreeSet<OpenGpuiHostVisualInteractionGap>,
}

impl OpenGpuiHostVisualSurfaceRow {
    pub fn new(
        fixture: &OpenGpuiProductFixtureCase,
        node_kind: impl Into<String>,
        renderer_key: impl Into<String>,
        source: OpenGpuiHostRendererSource,
    ) -> Self {
        Self {
            fixture_id: fixture.id.clone(),
            fixture_kind: fixture.kind,
            family: fixture.family,
            node_kind: node_kind.into(),
            renderer_key: renderer_key.into(),
            source,
            selected: false,
            content_visible: false,
            content_readable: false,
            content_within_node_bounds: false,
            actual_size: None,
            min_readable_size: None,
            text_overflow_count: 0,
            clipped_control_count: 0,
            handle_overlap_count: 0,
            stale_measured_regions: 0,
            repeatable_rows: 0,
            repeatable_rows_with_anchors: 0,
            hidden_repeatable_overflow_count: 0,
            repeatable_overflow_indicator_count: 0,
            gaps: BTreeSet::new(),
        }
    }

    pub fn with_selection(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_content_bounds(
        mut self,
        visible: bool,
        readable: bool,
        within_node_bounds: bool,
    ) -> Self {
        self.content_visible = visible;
        self.content_readable = readable;
        self.content_within_node_bounds = within_node_bounds;
        if !visible && !self.selected {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::UnselectedContentHidden);
        }
        if !readable || !within_node_bounds {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::ContentOverflow);
        }
        self
    }

    pub fn with_readability_budget(
        mut self,
        actual_size: OpenGpuiSizeEvidence,
        min_readable_size: Option<OpenGpuiSizeEvidence>,
    ) -> Self {
        self.actual_size = Some(actual_size);
        self.min_readable_size = min_readable_size;
        if min_readable_size.is_some_and(|minimum| !actual_size.contains(minimum)) {
            self.content_readable = false;
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::BelowReadableSize);
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::ContentOverflow);
        }
        self
    }

    pub fn with_text_overflow_count(mut self, overflow_count: usize) -> Self {
        self.text_overflow_count = overflow_count;
        if overflow_count > 0 {
            self.content_readable = false;
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::TextOverflow);
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::ContentOverflow);
        }
        self
    }

    pub fn with_control_clipping_count(mut self, clipping_count: usize) -> Self {
        self.clipped_control_count = clipping_count;
        if clipping_count > 0 {
            self.content_readable = false;
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::ControlClipping);
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::ContentOverflow);
        }
        self
    }

    pub fn with_handle_overlap_count(mut self, overlap_count: usize) -> Self {
        self.handle_overlap_count = overlap_count;
        if overlap_count > 0 {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::HandleOverlap);
        }
        self
    }

    pub fn with_repeatable_overflow(
        mut self,
        hidden_overflow_count: usize,
        visible_indicator_count: usize,
    ) -> Self {
        self.hidden_repeatable_overflow_count = hidden_overflow_count;
        self.repeatable_overflow_indicator_count = visible_indicator_count;
        if hidden_overflow_count > 0 && visible_indicator_count == 0 {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::HiddenRepeatableOverflow);
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::RepeatableOverflowIndicatorMissing);
        }
        self
    }

    pub fn with_stale_measured_regions(mut self, stale_regions: usize) -> Self {
        self.stale_measured_regions = stale_regions;
        if stale_regions > 0 {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::StaleMeasuredRegion);
        }
        self
    }

    pub fn with_repeatable_anchor_coverage(
        mut self,
        repeatable_rows: usize,
        repeatable_rows_with_anchors: usize,
    ) -> Self {
        self.repeatable_rows = repeatable_rows;
        self.repeatable_rows_with_anchors = repeatable_rows_with_anchors;
        if repeatable_rows > 0 && repeatable_rows_with_anchors == 0 {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::MissingRepeatableAnchor);
        }
        self
    }
}

/// Host-level visual and interaction evidence for the real Open GPUI gallery path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiHostVisualInteractionReport {
    pub rows: Vec<OpenGpuiHostVisualSurfaceRow>,
    pub node_bounds_overlap_count: usize,
    pub invalid_hover_bounds_checked: bool,
    pub dropped_wire_menu_bounds_checked: bool,
    pub repeatable_edit_updates_anchors: bool,
    pub edge_endpoints_follow_measured_handles: bool,
    pub gaps: BTreeSet<OpenGpuiHostVisualInteractionGap>,
}

impl OpenGpuiHostVisualInteractionReport {
    pub fn push(&mut self, row: OpenGpuiHostVisualSurfaceRow) {
        self.gaps.extend(row.gaps.iter().copied());
        self.rows.push(row);
    }

    pub fn rows_for_fixture(
        &self,
        fixture_id: &str,
    ) -> impl Iterator<Item = &OpenGpuiHostVisualSurfaceRow> {
        self.rows
            .iter()
            .filter(move |row| row.fixture_id == fixture_id)
    }

    pub fn add_node_bounds_overlap_count(&mut self, overlap_count: usize) {
        self.node_bounds_overlap_count += overlap_count;
        if overlap_count > 0 {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::NodeBoundsOverlap);
        }
    }

    pub fn mark_invalid_hover_bounds_checked(&mut self, inside_bounds: bool) {
        self.invalid_hover_bounds_checked = inside_bounds;
        if !inside_bounds {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::InvalidHoverOutOfBounds);
        }
    }

    pub fn mark_dropped_wire_menu_bounds_checked(&mut self, inside_bounds: bool) {
        self.dropped_wire_menu_bounds_checked = inside_bounds;
        if !inside_bounds {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::DroppedWireMenuOutOfBounds);
        }
    }

    pub fn mark_repeatable_edit_updates_anchors(&mut self, updates_anchors: bool) {
        self.repeatable_edit_updates_anchors = updates_anchors;
        if !updates_anchors {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::MissingRepeatableAnchor);
        }
    }

    pub fn mark_edge_endpoints_follow_measured_handles(&mut self, follows_handles: bool) {
        self.edge_endpoints_follow_measured_handles = follows_handles;
        if !follows_handles {
            self.gaps
                .insert(OpenGpuiHostVisualInteractionGap::EdgeEndpointNotFollowingMeasuredHandle);
        }
    }
}

/// User-visible interaction gap collected from the concrete Open GPUI gallery path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiHostProductInteractionGap {
    DragSurfaceMissingFullPointerSequence,
    ControlEventShieldingUnchecked,
    GraphAffordanceHitBudgetMissing,
    GraphAffordanceLayoutEvidenceMissing,
    GraphAffordanceRoutePolicyMissing,
    PortHotspotPathMissing,
    ToolSwitcherMissing,
    ConnectFlowNotStoreSynced,
    ReconnectAffordanceMissing,
    DroppedWireGestureDetached,
    RepeatableOverflowIndicatorMissing,
}

/// Characterization report for product-level Open GPUI interactions.
///
/// This report is intentionally allowed to contain gaps during characterization; later
/// productization units can promote selected fields into hard gates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiHostProductInteractionReport {
    pub product_drag_surface_count: usize,
    pub full_drag_pointer_sequence_checked: bool,
    pub control_event_shielding_checked: bool,
    pub port_hotspot_path_checked: bool,
    pub tool_switcher_visible: bool,
    pub connect_flow_store_synced: bool,
    pub reconnect_affordance_visible: bool,
    pub dropped_wire_gesture_connected: bool,
    pub graph_affordance_evidence: Option<OpenGpuiGraphAffordanceEvidence>,
    pub hidden_repeatable_overflow_count: usize,
    pub repeatable_overflow_indicator_count: usize,
    pub gaps: BTreeSet<OpenGpuiHostProductInteractionGap>,
}

impl OpenGpuiHostProductInteractionReport {
    pub fn mark_drag_surface_coverage(
        &mut self,
        product_drag_surface_count: usize,
        full_pointer_sequence_checked: bool,
    ) {
        self.product_drag_surface_count = product_drag_surface_count;
        self.full_drag_pointer_sequence_checked = full_pointer_sequence_checked;
        if product_drag_surface_count > 0 && !full_pointer_sequence_checked {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::DragSurfaceMissingFullPointerSequence);
        }
    }

    pub fn mark_control_event_shielding_checked(&mut self, checked: bool) {
        self.control_event_shielding_checked = checked;
        if !checked {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::ControlEventShieldingUnchecked);
        }
    }

    pub fn mark_port_hotspot_path_checked(&mut self, checked: bool) {
        self.port_hotspot_path_checked = checked;
        if !checked {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::PortHotspotPathMissing);
        }
    }

    pub fn mark_tool_switcher_visible(&mut self, visible: bool) {
        self.tool_switcher_visible = visible;
        if !visible {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::ToolSwitcherMissing);
        }
    }

    pub fn mark_connect_flow_store_synced(&mut self, synced: bool) {
        self.connect_flow_store_synced = synced;
        if !synced {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::ConnectFlowNotStoreSynced);
        }
    }

    pub fn mark_reconnect_affordance_visible(&mut self, visible: bool) {
        self.reconnect_affordance_visible = visible;
        if !visible {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::ReconnectAffordanceMissing);
        }
    }

    pub fn mark_dropped_wire_gesture_connected(&mut self, connected: bool) {
        self.dropped_wire_gesture_connected = connected;
        if !connected {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::DroppedWireGestureDetached);
        }
    }

    pub fn mark_graph_affordance_evidence(&mut self, evidence: OpenGpuiGraphAffordanceEvidence) {
        self.graph_affordance_evidence = Some(evidence);
        if !evidence.has_product_route_policy() {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::GraphAffordanceRoutePolicyMissing);
        }
        if !evidence.has_product_hit_budgets() {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::GraphAffordanceHitBudgetMissing);
        }
        if !evidence.has_layout_region_evidence() {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::GraphAffordanceLayoutEvidenceMissing);
        }
    }

    pub fn mark_repeatable_overflow(
        &mut self,
        hidden_overflow_count: usize,
        visible_indicator_count: usize,
    ) {
        self.hidden_repeatable_overflow_count = hidden_overflow_count;
        self.repeatable_overflow_indicator_count = visible_indicator_count;
        if hidden_overflow_count > 0 && visible_indicator_count == 0 {
            self.gaps
                .insert(OpenGpuiHostProductInteractionGap::RepeatableOverflowIndicatorMissing);
        }
    }
}

pub fn assert_product_interaction_characterization_report_contract(
    report: &OpenGpuiHostProductInteractionReport,
) {
    assert!(
        report.product_drag_surface_count > 0,
        "product interaction report must cover at least one product renderer surface: {report:?}"
    );
    assert!(
        report.hidden_repeatable_overflow_count >= report.repeatable_overflow_indicator_count,
        "overflow counters must be internally consistent: {report:?}"
    );
    assert!(
        report.full_drag_pointer_sequence_checked
            || report.control_event_shielding_checked
            || report.port_hotspot_path_checked
            || report.tool_switcher_visible
            || report.connect_flow_store_synced
            || report.reconnect_affordance_visible
            || report.dropped_wire_gesture_connected
            || report.graph_affordance_evidence.is_some(),
        "product interaction report must expose at least one checked interaction fact: {report:?}"
    );
    assert!(
        serde_json::to_string(report).is_ok(),
        "product interaction report must stay serializable: {report:?}"
    );
}

/// Assert concrete Open GPUI product interaction gates for the productized gallery path.
///
/// This gate is intentionally stricter than the characterization contract above: it is the
/// adapter-owned hard baseline that the concrete host report must satisfy after productization.
pub fn assert_product_interaction_report_gates(report: &OpenGpuiHostProductInteractionReport) {
    assert_product_interaction_characterization_report_contract(report);
    assert!(
        report.product_drag_surface_count >= product_fixture_catalog().len(),
        "product interaction report must cover drag surfaces across all product fixture families: {report:?}"
    );
    assert!(
        report.full_drag_pointer_sequence_checked,
        "product drag surfaces must be covered by full down/move/up/cancel pointer sequences: {report:?}"
    );
    assert!(
        report.control_event_shielding_checked,
        "product controls must prove they do not start node drags: {report:?}"
    );
    assert!(
        report.port_hotspot_path_checked,
        "product ports must be reachable through the concrete host hotspot path: {report:?}"
    );
    assert!(
        report.tool_switcher_visible,
        "Open GPUI product UI must expose tool switching: {report:?}"
    );
    assert!(
        report.connect_flow_store_synced,
        "Connect gestures must synchronize through Jellyflow store transactions: {report:?}"
    );
    assert!(
        report.reconnect_affordance_visible,
        "Allowed edge reconnect/switch affordances must be visible: {report:?}"
    );
    assert!(
        report.dropped_wire_gesture_connected,
        "Dropped-wire insertion must be connected to a real connect-release gesture: {report:?}"
    );
    let Some(graph_affordance) = report.graph_affordance_evidence else {
        panic!(
            "product interaction report must include widget-free graph affordance evidence: {report:?}"
        );
    };
    assert!(
        graph_affordance.has_product_route_policy(),
        "committed wires and connection previews must use product route policy: {report:?}"
    );
    assert!(
        graph_affordance.has_product_hit_budgets(),
        "ports, endpoints, and reconnect handles must expose product hit budgets: {report:?}"
    );
    assert!(
        graph_affordance.has_layout_region_evidence(),
        "product graph affordance evidence must include drag and readable layout regions: {report:?}"
    );
    if report.hidden_repeatable_overflow_count > 0 {
        assert!(
            report.repeatable_overflow_indicator_count > 0,
            "hidden repeatable overflow must have a visible indicator: {report:?}"
        );
    }
    assert!(
        report.gaps.is_empty(),
        "product interaction report has unresolved gaps: {report:?}"
    );
}

/// Native lifecycle gap collected by an Open GPUI product smoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiNativeLifecycleGap {
    ProductGalleryNotRendered,
    ProductDragUnchecked,
    LastWindowCloseUnchecked,
    CloseAutomationSkipped,
    LastWindowCloseRejected,
    NoWindowBeforeClose,
    WindowsRemainAfterClose,
    QuitNotObserved,
    CloseAutomationSkipReasonMissing,
}

/// Widget-free native lifecycle evidence for the Open GPUI product gallery.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiNativeLifecycleEvidence {
    pub rendered_product_fixture_id: Option<String>,
    pub rendered_product_node_count: usize,
    pub product_drag_checked: bool,
    pub last_window_close_attempted: bool,
    pub initial_window_count: usize,
    pub last_window_close_accepted: bool,
    pub remaining_window_count: usize,
    pub quit_observed: bool,
    pub close_automation_skip_reason: Option<String>,
    pub gaps: BTreeSet<OpenGpuiNativeLifecycleGap>,
}

impl OpenGpuiNativeLifecycleEvidence {
    pub fn skipped(reason: impl Into<String>) -> Self {
        let mut evidence = Self::default();
        evidence.mark_close_automation_skipped(reason);
        evidence
    }

    pub fn mark_product_gallery_rendered(
        &mut self,
        fixture_id: impl Into<String>,
        product_node_count: usize,
    ) {
        self.rendered_product_fixture_id = Some(fixture_id.into());
        self.rendered_product_node_count = product_node_count;
        self.sync_gaps();
    }

    pub fn mark_product_drag_checked(&mut self, checked: bool) {
        self.product_drag_checked = checked;
        self.sync_gaps();
    }

    pub fn mark_last_window_close(
        &mut self,
        initial_window_count: usize,
        accepted: bool,
        remaining_window_count: usize,
        quit_observed: bool,
    ) {
        self.last_window_close_attempted = true;
        self.initial_window_count = initial_window_count;
        self.last_window_close_accepted = accepted;
        self.remaining_window_count = remaining_window_count;
        self.quit_observed = quit_observed;
        self.close_automation_skip_reason = None;
        self.sync_gaps();
    }

    pub fn mark_close_automation_skipped(&mut self, reason: impl Into<String>) {
        self.last_window_close_attempted = false;
        self.close_automation_skip_reason = Some(reason.into());
        self.sync_gaps();
    }

    pub fn computed_gaps(&self) -> BTreeSet<OpenGpuiNativeLifecycleGap> {
        let mut gaps = BTreeSet::new();
        if self
            .rendered_product_fixture_id
            .as_ref()
            .is_none_or(|fixture| fixture.trim().is_empty())
            || self.rendered_product_node_count == 0
        {
            gaps.insert(OpenGpuiNativeLifecycleGap::ProductGalleryNotRendered);
        }
        if !self.product_drag_checked {
            gaps.insert(OpenGpuiNativeLifecycleGap::ProductDragUnchecked);
        }

        if let Some(reason) = &self.close_automation_skip_reason {
            if reason.trim().is_empty() {
                gaps.insert(OpenGpuiNativeLifecycleGap::CloseAutomationSkipReasonMissing);
            } else {
                gaps.insert(OpenGpuiNativeLifecycleGap::CloseAutomationSkipped);
            }
            return gaps;
        }

        if !self.last_window_close_attempted {
            gaps.insert(OpenGpuiNativeLifecycleGap::LastWindowCloseUnchecked);
            return gaps;
        }
        if self.initial_window_count == 0 {
            gaps.insert(OpenGpuiNativeLifecycleGap::NoWindowBeforeClose);
        }
        if !self.last_window_close_accepted {
            gaps.insert(OpenGpuiNativeLifecycleGap::LastWindowCloseRejected);
        }
        if self.remaining_window_count > 0 {
            gaps.insert(OpenGpuiNativeLifecycleGap::WindowsRemainAfterClose);
        }
        if !self.quit_observed {
            gaps.insert(OpenGpuiNativeLifecycleGap::QuitNotObserved);
        }
        gaps
    }

    fn sync_gaps(&mut self) {
        self.gaps = self.computed_gaps();
    }
}

pub fn assert_native_lifecycle_evidence_gates(evidence: &OpenGpuiNativeLifecycleEvidence) {
    let gaps = evidence.computed_gaps();
    assert!(
        serde_json::to_string(evidence).is_ok(),
        "native lifecycle evidence must stay serializable: {evidence:?}"
    );
    assert!(
        gaps.is_empty(),
        "native lifecycle evidence has unresolved gaps: {evidence:?}; computed gaps: {gaps:?}"
    );
    assert_eq!(
        evidence.gaps, gaps,
        "native lifecycle evidence must carry synchronized gaps: {evidence:?}"
    );
}

/// Coarse screenshot region categories used by product gallery visual smoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiScreenshotRegionKind {
    NodeBody,
    NodeInternalUi,
    WirePath,
    PortArea,
    FeedbackOverlay,
}

/// Pixel-space rectangle sampled from a rendered screenshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiScreenshotRegionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl OpenGpuiScreenshotRegionRect {
    pub fn is_positive(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// Coarse evidence for one screenshot ROI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiScreenshotRegionEvidence {
    pub kind: OpenGpuiScreenshotRegionKind,
    pub rect: OpenGpuiScreenshotRegionRect,
    pub non_transparent_pixels: usize,
    pub distinct_rgba_samples: usize,
}

impl OpenGpuiScreenshotRegionEvidence {
    pub fn new(
        kind: OpenGpuiScreenshotRegionKind,
        rect: OpenGpuiScreenshotRegionRect,
        non_transparent_pixels: usize,
        distinct_rgba_samples: usize,
    ) -> Self {
        Self {
            kind,
            rect,
            non_transparent_pixels,
            distinct_rgba_samples,
        }
    }

    pub fn is_present(&self) -> bool {
        self.rect.is_positive()
            && self.non_transparent_pixels > 0
            && self.distinct_rgba_samples >= 2
    }
}

/// Screenshot region gap reported by the adapter smoke gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpenGpuiScreenshotRegionGap {
    ScreenshotSkipped,
    ScreenshotBlankOrSingleColor,
    MissingFixture,
    RequiredRegionMissing(OpenGpuiScreenshotRegionKind),
    RequiredRegionBlank(OpenGpuiScreenshotRegionKind),
}

/// Per-fixture screenshot evidence. This stays widget-free so hosts can map any renderer into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiScreenshotFixtureEvidence {
    pub fixture_id: String,
    pub width: u32,
    pub height: u32,
    pub non_transparent_pixels: usize,
    pub distinct_rgba_samples: usize,
    pub regions: Vec<OpenGpuiScreenshotRegionEvidence>,
    pub skipped_reason: Option<String>,
    pub gaps: BTreeSet<OpenGpuiScreenshotRegionGap>,
}

impl OpenGpuiScreenshotFixtureEvidence {
    pub fn captured(
        fixture_id: impl Into<String>,
        width: u32,
        height: u32,
        non_transparent_pixels: usize,
        distinct_rgba_samples: usize,
    ) -> Self {
        let mut evidence = Self {
            fixture_id: fixture_id.into(),
            width,
            height,
            non_transparent_pixels,
            distinct_rgba_samples,
            regions: Vec::new(),
            skipped_reason: None,
            gaps: BTreeSet::new(),
        };
        evidence.sync_gaps();
        evidence
    }

    pub fn skipped(fixture_id: impl Into<String>, reason: impl Into<String>) -> Self {
        let mut evidence = Self {
            fixture_id: fixture_id.into(),
            width: 0,
            height: 0,
            non_transparent_pixels: 0,
            distinct_rgba_samples: 0,
            regions: Vec::new(),
            skipped_reason: Some(reason.into()),
            gaps: BTreeSet::new(),
        };
        evidence.sync_gaps();
        evidence
    }

    pub fn push_region(&mut self, region: OpenGpuiScreenshotRegionEvidence) {
        self.regions.push(region);
        self.sync_gaps();
    }

    pub fn has_region(&self, kind: OpenGpuiScreenshotRegionKind) -> bool {
        self.regions
            .iter()
            .any(|region| region.kind == kind && region.is_present())
    }

    pub fn computed_gaps(&self) -> BTreeSet<OpenGpuiScreenshotRegionGap> {
        let mut gaps = BTreeSet::new();
        if self
            .skipped_reason
            .as_ref()
            .is_some_and(|reason| !reason.trim().is_empty())
        {
            gaps.insert(OpenGpuiScreenshotRegionGap::ScreenshotSkipped);
            return gaps;
        }
        if self.width == 0
            || self.height == 0
            || self.non_transparent_pixels == 0
            || self.distinct_rgba_samples < 2
        {
            gaps.insert(OpenGpuiScreenshotRegionGap::ScreenshotBlankOrSingleColor);
        }
        for kind in [
            OpenGpuiScreenshotRegionKind::NodeBody,
            OpenGpuiScreenshotRegionKind::NodeInternalUi,
            OpenGpuiScreenshotRegionKind::WirePath,
            OpenGpuiScreenshotRegionKind::PortArea,
        ] {
            match self.regions.iter().find(|region| region.kind == kind) {
                Some(region) if region.is_present() => {}
                Some(_) => {
                    gaps.insert(OpenGpuiScreenshotRegionGap::RequiredRegionBlank(kind));
                }
                None => {
                    gaps.insert(OpenGpuiScreenshotRegionGap::RequiredRegionMissing(kind));
                }
            }
        }
        gaps
    }

    fn sync_gaps(&mut self) {
        self.gaps = self.computed_gaps();
    }
}

/// Screenshot region evidence for a product gallery export run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenGpuiScreenshotRegionReport {
    pub fixtures: Vec<OpenGpuiScreenshotFixtureEvidence>,
    pub skipped_reason: Option<String>,
    pub gaps: BTreeSet<OpenGpuiScreenshotRegionGap>,
}

impl OpenGpuiScreenshotRegionReport {
    pub fn skipped(reason: impl Into<String>) -> Self {
        let mut report = Self {
            fixtures: Vec::new(),
            skipped_reason: Some(reason.into()),
            gaps: BTreeSet::new(),
        };
        report.sync_gaps();
        report
    }

    pub fn push_fixture(&mut self, fixture: OpenGpuiScreenshotFixtureEvidence) {
        self.fixtures.push(fixture);
        self.sync_gaps();
    }

    pub fn fixture(&self, fixture_id: &str) -> Option<&OpenGpuiScreenshotFixtureEvidence> {
        self.fixtures
            .iter()
            .find(|fixture| fixture.fixture_id == fixture_id)
    }

    pub fn computed_gaps(&self) -> BTreeSet<OpenGpuiScreenshotRegionGap> {
        let mut gaps = BTreeSet::new();
        if self
            .skipped_reason
            .as_ref()
            .is_some_and(|reason| !reason.trim().is_empty())
        {
            gaps.insert(OpenGpuiScreenshotRegionGap::ScreenshotSkipped);
            return gaps;
        }
        for fixture in product_fixture_catalog() {
            let Some(evidence) = self.fixture(&fixture.id) else {
                gaps.insert(OpenGpuiScreenshotRegionGap::MissingFixture);
                continue;
            };
            gaps.extend(evidence.computed_gaps());
        }
        gaps
    }

    fn sync_gaps(&mut self) {
        self.gaps = self.computed_gaps();
    }
}

pub fn assert_screenshot_region_report_gates(report: &OpenGpuiScreenshotRegionReport) {
    let gaps = report.computed_gaps();
    assert!(
        serde_json::to_string(report).is_ok(),
        "screenshot region report must stay serializable: {report:?}"
    );
    assert!(
        gaps.is_empty(),
        "screenshot region report has unresolved gaps: {report:?}; computed gaps: {gaps:?}"
    );
    assert_eq!(
        report.gaps, gaps,
        "screenshot region report must carry synchronized gaps: {report:?}"
    );
}

/// Stable widget-free product fixture catalog used by Open GPUI gallery/report tests.
pub fn product_fixture_catalog() -> Vec<OpenGpuiProductFixtureCase> {
    [
        OpenGpuiProductFixtureKind::DifyWorkflow,
        OpenGpuiProductFixtureKind::ShaderBlueprint,
        OpenGpuiProductFixtureKind::ErdTable,
        OpenGpuiProductFixtureKind::MindMap,
    ]
    .into_iter()
    .map(OpenGpuiProductFixtureCase::from_kind)
    .collect()
}

/// Assert that a host-level report is structurally valid and names all product cases.
pub fn assert_host_surface_report_contract(report: &OpenGpuiHostSurfaceReport) {
    assert!(
        !report.rows.is_empty(),
        "Open GPUI host surface report must contain rendered rows"
    );
    let catalog = product_fixture_catalog();
    let fixture_ids = catalog
        .iter()
        .map(|fixture| fixture.id.as_str())
        .collect::<BTreeSet<_>>();
    for fixture in &catalog {
        assert!(
            report.rows_for_fixture(&fixture.id).next().is_some(),
            "host report is missing product fixture `{}`: {report:?}",
            fixture.id
        );
    }
    for row in &report.rows {
        assert!(
            fixture_ids.contains(row.fixture_id.as_str()),
            "host report row uses unknown fixture id `{}`: {row:?}",
            row.fixture_id
        );
        assert!(
            !row.node_kind.is_empty(),
            "host report row must name a node kind: {row:?}"
        );
        assert!(
            !row.renderer_key.is_empty(),
            "host report row must name a renderer key: {row:?}"
        );
        if row.uses_renderer_fallback() {
            assert!(
                row.capability_gaps
                    .contains(&OpenGpuiHostCapabilityGap::RendererFallback),
                "fallback renderer rows must carry RendererFallback gap: {row:?}"
            );
        }
        if !row.unsupported_controls.is_empty() {
            assert!(
                row.capability_gaps
                    .contains(&OpenGpuiHostCapabilityGap::UnsupportedControl),
                "unsupported control rows must carry UnsupportedControl gap: {row:?}"
            );
        }
        if row.measured_slots + row.measured_anchors == 0 {
            assert!(
                row.capability_gaps
                    .contains(&OpenGpuiHostCapabilityGap::MissingMeasuredRegion),
                "unmeasured rows must carry MissingMeasuredRegion gap: {row:?}"
            );
        }
        if row.is_product_renderer() {
            assert!(
                row.style_budget.is_some(),
                "product renderer rows must expose adapter style budget facts: {row:?}"
            );
        }
    }
}

/// Assert that every product fixture has at least one host product renderer row.
pub fn assert_product_gallery_host_report_gates(report: &OpenGpuiHostSurfaceReport) {
    assert_host_surface_report_contract(report);
    for fixture in product_fixture_catalog() {
        assert!(
            report.rows_for_fixture(&fixture.id).any(|row| {
                row.is_product_renderer()
                    && fixture
                        .expected_renderer_keys
                        .contains(row.renderer_key.as_str())
            }),
            "fixture `{}` must be rendered by one of {:?}: {report:?}",
            fixture.id,
            fixture.expected_renderer_keys
        );
    }
}

/// Assert concrete Open GPUI host-level visual/interaction gates.
pub fn assert_host_visual_interaction_report_gates(report: &OpenGpuiHostVisualInteractionReport) {
    assert!(
        !report.rows.is_empty(),
        "Open GPUI host visual report must contain rendered product rows"
    );
    for fixture in product_fixture_catalog() {
        assert!(
            report.rows_for_fixture(&fixture.id).next().is_some(),
            "visual report is missing product fixture `{}`: {report:?}",
            fixture.id
        );
        assert!(
            report.rows_for_fixture(&fixture.id).any(|row| {
                row.source == OpenGpuiHostRendererSource::ProductRenderer
                    && fixture
                        .expected_renderer_keys
                        .contains(row.renderer_key.as_str())
            }),
            "fixture `{}` must have a product renderer visual row: {report:?}",
            fixture.id
        );
    }

    for row in &report.rows {
        assert!(
            !row.node_kind.is_empty(),
            "visual report row must name a node kind: {row:?}"
        );
        assert!(
            !row.renderer_key.is_empty(),
            "visual report row must name a renderer key: {row:?}"
        );
        assert!(
            row.content_visible,
            "node-internal content must remain visible even when unselected: {row:?}"
        );
        assert!(
            row.content_readable && row.content_within_node_bounds,
            "node-internal content must satisfy readable size and stay inside the node body: {row:?}"
        );
        assert_eq!(
            row.handle_overlap_count, 0,
            "controls/content must not overlap connection handle rails: {row:?}"
        );
        assert_eq!(
            row.stale_measured_regions, 0,
            "visual report must not carry stale measured regions: {row:?}"
        );
        if row.repeatable_rows > 0 {
            assert!(
                row.repeatable_rows_with_anchors > 0,
                "repeatable rows need measured/fallback anchor evidence: {row:?}"
            );
        }
    }

    assert_eq!(
        report.node_bounds_overlap_count, 0,
        "product fixture nodes must not initialize with overlapping bounds: {report:?}"
    );
    assert!(
        report.invalid_hover_bounds_checked,
        "invalid hover feedback must be checked against measured handle bounds: {report:?}"
    );
    assert!(
        report.dropped_wire_menu_bounds_checked,
        "dropped-wire menu bounds must be checked: {report:?}"
    );
    assert!(
        report.repeatable_edit_updates_anchors,
        "repeatable edits must update or downgrade anchor evidence: {report:?}"
    );
    assert!(
        report.edge_endpoints_follow_measured_handles,
        "edge endpoints must follow measured handle positions: {report:?}"
    );
    assert!(
        report.gaps.is_empty(),
        "host visual interaction report has unresolved gaps: {report:?}"
    );
}

impl OpenGpuiProductFixtureCase {
    pub fn from_kind(kind: OpenGpuiProductFixtureKind) -> Self {
        let spec = ProductFixtureSpec::for_kind(kind);
        Self {
            id: spec.id.to_owned(),
            kind,
            family: spec.family,
            kit_key: spec.kit_key.to_owned(),
            fixture_key: spec.fixture_key.to_owned(),
            expected_renderer_keys: spec
                .expected_renderer_keys
                .iter()
                .map(|key| (*key).to_owned())
                .collect(),
            expected_capabilities: spec
                .expected_capabilities
                .iter()
                .map(|capability| (*capability).to_owned())
                .collect(),
        }
    }
}

/// Adapter-level evidence collected for one builtin product fixture.
#[derive(Debug, Clone)]
pub struct OpenGpuiProductFixtureReport {
    pub kind: OpenGpuiProductFixtureKind,
    pub kit_key: String,
    pub fixture_key: String,
    pub density_modes: BTreeSet<&'static str>,
    pub region_sources: BTreeSet<&'static str>,
    pub resize_probe_count: usize,
    pub node_count: usize,
    pub measured_nodes: usize,
    pub slot_count: usize,
    pub anchor_count: usize,
    pub repeatable_item_count: usize,
    pub missing_dynamic_ports: Vec<OpenGpuiRepeatablePortDiagnostic>,
    pub actions: BTreeSet<String>,
    pub inspectors: BTreeSet<String>,
    pub blackboards: BTreeSet<String>,
    pub control_primitives: BTreeSet<&'static str>,
    pub measured_control_regions: usize,
    pub measurement_mode: OpenGpuiMeasurementMode,
    pub measurement_coverage: OpenGpuiMeasurementCoverage,
}

/// Adapter-level evidence for user-facing authoring interaction paths.
#[derive(Debug, Clone, Default)]
pub struct OpenGpuiAuthoringInteractionReport {
    pub dropped_wire_actions: BTreeSet<String>,
    pub node_actions: BTreeSet<String>,
    pub inspector_target_sources: BTreeSet<&'static str>,
    pub inspector_actions: BTreeSet<String>,
    pub blackboard_actions: BTreeSet<String>,
    pub repeatable_mutations: BTreeSet<&'static str>,
    pub dynamic_repeatable_lifecycle: OpenGpuiDynamicRepeatableLifecycleReport,
    pub invalid_hover_rejections: usize,
    pub editable_control_regions: usize,
}

/// Dynamic repeatable lifecycle gaps that must stay explicit in product-shaped nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OpenGpuiDynamicRepeatableLifecycleGap {
    AddMissingPortPolicyAbsent,
    AddPublishedFakeHandle,
    RemoveKeptGraphPort,
    RemoveKeptIncidentEdge,
    RemoveKeptRepeatableAnchor,
    ReorderChangedItemIdentity,
    ReorderChangedAnchorIdentity,
    ReorderChangedSlotIdentity,
    ReorderChangedPortBinding,
    ReorderMissingRowBounds,
    EditKeptStaleLabel,
    EditChangedItemIdentity,
    EditPublishedFakeHandle,
    DisplayOnlyPublishedHandle,
}

/// Structured evidence for add/remove/reorder/edit semantics on repeatable product rows.
#[derive(Debug, Clone, Default)]
pub struct OpenGpuiDynamicRepeatableLifecycleReport {
    pub exercised_collections: BTreeSet<String>,
    pub mutations: BTreeSet<&'static str>,
    pub add_missing_port_diagnostics: usize,
    pub add_created_graph_ports: usize,
    pub add_fake_handle_count: usize,
    pub remove_removed_graph_ports: usize,
    pub remove_removed_incident_edges: usize,
    pub remove_cleared_repeatable_anchor: bool,
    pub reorder_preserved_item_identity: bool,
    pub reorder_preserved_anchor_identity: bool,
    pub reorder_preserved_slot_identity: bool,
    pub reorder_preserved_port_binding: bool,
    pub reorder_has_row_bounds: bool,
    pub edit_refreshed_row_label: bool,
    pub edit_preserved_item_identity: bool,
    pub edit_missing_port_downgrade: bool,
    pub display_only_rows_without_handles: usize,
    pub display_only_fake_handle_count: usize,
    pub gaps: BTreeSet<OpenGpuiDynamicRepeatableLifecycleGap>,
}

/// Build adapter-level regression evidence for one builtin product fixture.
pub fn product_fixture_report(
    kind: OpenGpuiProductFixtureKind,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let spec = ProductFixtureSpec::for_kind(kind);
    let kit_registry = NodeKitRegistry::builtin();
    let node_registry = kit_registry.node_registry();
    let graph = kit_registry
        .fixture_graph(&NodeKitKey::new(spec.kit_key), spec.fixture_key)
        .map_err(|error| error.to_string())?;
    let mut report = OpenGpuiProductFixtureReport {
        kind,
        kit_key: spec.kit_key.to_owned(),
        fixture_key: spec.fixture_key.to_owned(),
        density_modes: BTreeSet::new(),
        region_sources: BTreeSet::new(),
        resize_probe_count: 0,
        node_count: graph.nodes().len(),
        measured_nodes: 0,
        slot_count: 0,
        anchor_count: 0,
        repeatable_item_count: 0,
        missing_dynamic_ports: Vec::new(),
        actions: BTreeSet::new(),
        inspectors: BTreeSet::new(),
        blackboards: BTreeSet::new(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        measurement_coverage: OpenGpuiMeasurementCoverage::default(),
    };

    for (node_id, node) in graph.nodes() {
        let Some(descriptor) = node_registry.view_descriptor(&node.kind) else {
            continue;
        };
        collect_density_modes(&mut report, kit_registry.layout_hints_for_kind(&node.kind));
        report.region_sources.insert("projection_fallback");
        let size = node_size(node);
        let layout = projected_node_surface_graph_layout(&descriptor, node, &graph, node_id, size);
        assert_layout_regions_inside_node(&layout, size, &descriptor.kind.0);
        collect_resize_probe(&mut report, &descriptor, node, &graph, node_id, size);
        let measurement = project_node_measurement(node_id, node, &graph, &descriptor);
        assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);

        report.measured_nodes += 1;
        report.slot_count += measurement.slots.len();
        report.anchor_count += measurement.anchors.len();
        report.measurement_coverage.projection_fallback_regions +=
            measurement.slots.len() + measurement.anchors.len();
        report.measurement_coverage.measured_slots += measurement.slots.len();
        report.measurement_coverage.measured_anchors += measurement.anchors.len();

        let repeatable_items = repeatable_item_projection(&descriptor, node, &graph, node_id);
        report.repeatable_item_count += repeatable_items.len();
        report
            .missing_dynamic_ports
            .extend(repeatable_port_diagnostics(&repeatable_items));
        report
            .actions
            .extend(descriptor.actions.iter().map(|action| action.key.clone()));
        report.inspectors.extend(
            descriptor
                .inspectors
                .iter()
                .map(|inspector| inspector.key.clone()),
        );
        report.blackboards.extend(
            descriptor
                .blackboards
                .iter()
                .map(|blackboard| blackboard.key.clone()),
        );
        collect_control_primitives(&mut report, &descriptor, node);
    }

    Ok(report)
}

/// Build product-shaped regression evidence from layout-pass semantic regions.
pub fn layout_pass_product_fixture_report(
    kind: OpenGpuiProductFixtureKind,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let spec = ProductFixtureSpec::for_kind(kind);
    let kit_registry = NodeKitRegistry::builtin();
    let node_registry = kit_registry.node_registry();
    let graph = kit_registry
        .fixture_graph(&NodeKitKey::new(spec.kit_key), spec.fixture_key)
        .map_err(|error| error.to_string())?;
    let mut report = OpenGpuiProductFixtureReport {
        kind,
        kit_key: spec.kit_key.to_owned(),
        fixture_key: spec.fixture_key.to_owned(),
        density_modes: BTreeSet::new(),
        region_sources: BTreeSet::new(),
        resize_probe_count: 0,
        node_count: graph.nodes().len(),
        measured_nodes: 0,
        slot_count: 0,
        anchor_count: 0,
        repeatable_item_count: 0,
        missing_dynamic_ports: Vec::new(),
        actions: BTreeSet::new(),
        inspectors: BTreeSet::new(),
        blackboards: BTreeSet::new(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
        measurement_coverage: OpenGpuiMeasurementCoverage::default(),
    };

    for (node_id, node) in graph.nodes() {
        let Some(descriptor) = node_registry.view_descriptor(&node.kind) else {
            continue;
        };
        collect_density_modes(&mut report, kit_registry.layout_hints_for_kind(&node.kind));
        let size = node_size(node);
        let layout = projected_node_surface_graph_layout(&descriptor, node, &graph, node_id, size);
        collect_resize_probe(&mut report, &descriptor, node, &graph, node_id, size);
        let (regions, control_regions) = layout_pass_regions_for_node(node_id, node, &layout);
        let fallback_anchors = measured_surface_anchors(&descriptor, &graph, node_id, &layout);
        if regions.is_empty() && fallback_anchors.is_empty() {
            continue;
        }
        let context = OpenGpuiMeasurementContext::new(*node_id, node_view_origin(node), 1.0, size)
            .with_revision(2);
        let (measurement, coverage) =
            layout_pass_measurement_from_regions(context, regions, fallback_anchors);

        assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);
        assert!(
            coverage.is_full_layout_pass(),
            "layout-pass fixture for `{}` must not depend on projection fallback: {coverage:?}",
            descriptor.kind.0
        );

        report.measured_nodes += 1;
        report.slot_count += measurement.slots.len();
        report.anchor_count += measurement.anchors.len();
        report.measured_control_regions += control_regions;
        accumulate_coverage(&mut report.measurement_coverage, coverage);
        sync_region_sources(&mut report);

        let repeatable_items = repeatable_item_projection(&descriptor, node, &graph, node_id);
        report.repeatable_item_count += repeatable_items.len();
        report
            .missing_dynamic_ports
            .extend(repeatable_port_diagnostics(&repeatable_items));
        report
            .actions
            .extend(descriptor.actions.iter().map(|action| action.key.clone()));
        report.inspectors.extend(
            descriptor
                .inspectors
                .iter()
                .map(|inspector| inspector.key.clone()),
        );
        report.blackboards.extend(
            descriptor
                .blackboards
                .iter()
                .map(|blackboard| blackboard.key.clone()),
        );
        collect_control_primitives(&mut report, &descriptor, node);
    }

    Ok(report)
}

/// Assert the product fixtures that approximate Dify, shader/blueprint, ERD, and mind-map usage.
pub fn assert_product_fixture_regression_gates() {
    let workflow = product_fixture_report(OpenGpuiProductFixtureKind::DifyWorkflow)
        .expect("Dify-style workflow fixture");
    assert!(workflow.node_count >= 4);
    assert!(workflow.measured_nodes >= 4);
    assert!(workflow.slot_count >= 1);
    assert_density_and_resize_evidence(&workflow);
    assert!(workflow.region_sources.contains("projection_fallback"));
    assert_eq!(
        workflow.measurement_mode,
        OpenGpuiMeasurementMode::ProjectionFallback
    );
    assert!(!workflow.measurement_coverage.is_full_layout_pass());
    assert_eq!(workflow.measurement_coverage.layout_pass_regions, 0);
    assert!(
        workflow.measurement_coverage.projection_fallback_regions
            >= workflow.slot_count + workflow.anchor_count
    );

    let dify_node = schema_node_report(OpenGpuiProductFixtureKind::DifyWorkflow, "demo.llm")
        .expect("Dify-style LLM schema");
    assert!(dify_node.slot_count >= 8);
    assert!(dify_node.anchor_count >= 2);
    assert!(dify_node.repeatable_item_count >= 2);
    assert!(dify_node.actions.contains("action.llm.run"));
    assert!(dify_node.actions.contains("action.insert.llm"));
    assert!(dify_node.inspectors.contains("inspector.llm"));
    assert!(dify_node.control_primitives.contains("textarea"));
    assert!(dify_node.control_primitives.contains("select"));
    assert!(
        dify_node
            .control_primitives
            .contains("variable_picker_stub")
    );
    assert_eq!(
        dify_node.measurement_mode,
        OpenGpuiMeasurementMode::ProjectionFallback
    );
    assert!(!dify_node.measurement_coverage.is_full_layout_pass());
    assert_eq!(dify_node.measurement_coverage.layout_pass_regions, 0);

    let dify_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::DifyWorkflow)
        .expect("Dify-style layout-pass fixture");
    assert_eq!(
        dify_layout.measurement_mode,
        OpenGpuiMeasurementMode::LayoutPass
    );
    assert_density_and_resize_evidence(&dify_layout);
    assert!(dify_layout.region_sources.contains("layout_pass"));
    assert!(dify_layout.measurement_coverage.is_full_layout_pass());
    assert_layout_pass_capability_requires_real_bounds(&OpenGpuiAdapter::layout_pass(
        dify_layout.measurement_coverage.clone(),
    ));
    let dify_node_layout =
        layout_pass_schema_node_report(OpenGpuiProductFixtureKind::DifyWorkflow, "demo.llm")
            .expect("Dify-style LLM layout-pass schema");
    assert!(
        dify_node_layout.measurement_coverage.is_full_layout_pass(),
        "Dify LLM layout-pass coverage must be full: {:?}",
        dify_node_layout.measurement_coverage
    );
    assert!(dify_node_layout.measured_control_regions >= 3);

    let shader = product_fixture_report(OpenGpuiProductFixtureKind::ShaderBlueprint)
        .expect("shader fixture");
    assert!(shader.node_count >= 2);
    assert!(shader.measured_nodes >= 2);
    assert!(shader.slot_count >= 6);
    assert!(shader.anchor_count >= 3);
    assert_density_and_resize_evidence(&shader);

    let shader_node = schema_node_report(
        OpenGpuiProductFixtureKind::ShaderBlueprint,
        "demo.shader.mix",
    )
    .expect("shader mix schema");
    assert!(shader_node.repeatable_item_count >= 3);
    assert!(shader_node.anchor_count >= 3);
    assert!(shader_node.actions.contains("action.shader_input.add"));
    assert!(shader_node.actions.contains("action.shader_input.remove"));
    assert!(
        shader_node
            .blackboards
            .contains("blackboard.shader.properties")
    );
    assert!(shader_node.control_primitives.contains("slider"));
    assert!(shader_node.control_primitives.contains("select"));
    assert!(shader_node.missing_dynamic_ports.is_empty());
    let shader_layout =
        layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::ShaderBlueprint)
            .expect("shader layout-pass fixture");
    assert!(shader_layout.measurement_coverage.is_full_layout_pass());
    let shader_node_layout = layout_pass_schema_node_report(
        OpenGpuiProductFixtureKind::ShaderBlueprint,
        "demo.shader.mix",
    )
    .expect("shader mix layout-pass schema");
    assert_density_and_resize_evidence(&shader_node_layout);
    assert!(
        shader_node_layout
            .measurement_coverage
            .is_full_layout_pass()
    );
    assert!(shader_node_layout.repeatable_item_count >= 3);
    assert!(shader_node_layout.anchor_count >= 3);

    let erd = product_fixture_report(OpenGpuiProductFixtureKind::ErdTable).expect("ERD fixture");
    assert!(erd.node_count >= 2);
    assert!(erd.repeatable_item_count >= 3);
    assert!(erd.anchor_count >= 2);
    assert!(erd.actions.contains("action.column.add"));
    assert!(erd.inspectors.contains("inspector.column.email"));
    assert!(erd.control_primitives.contains("text_input"));
    assert_density_and_resize_evidence(&erd);
    let erd_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::ErdTable)
        .expect("ERD layout-pass fixture");
    assert_density_and_resize_evidence(&erd_layout);
    assert!(erd_layout.measurement_coverage.is_full_layout_pass());
    assert!(erd_layout.repeatable_item_count >= 3);
    assert!(erd_layout.measured_control_regions >= 3);

    let mind =
        product_fixture_report(OpenGpuiProductFixtureKind::MindMap).expect("mind-map fixture");
    assert!(mind.node_count >= 3);
    assert!(mind.measured_nodes >= 3);
    assert!(mind.slot_count >= 3);
    assert!(mind.control_primitives.contains("text_input"));
    assert_density_and_resize_evidence(&mind);
    let mind_layout = layout_pass_product_fixture_report(OpenGpuiProductFixtureKind::MindMap)
        .expect("mind-map layout-pass fixture");
    assert_density_and_resize_evidence(&mind_layout);
    assert!(mind_layout.measurement_coverage.is_full_layout_pass());
    assert!(mind_layout.slot_count >= 3);
}

/// Assert that descriptor-driven interaction states exist for the GPUI adapter to render locally.
pub fn assert_authoring_interaction_regression_gates() {
    let report = authoring_interaction_report().expect("authoring interaction report");
    assert!(
        report.dropped_wire_actions.contains("action.insert.llm"),
        "dropped-wire insert action must remain visible and dispatchable: {report:?}"
    );
    assert!(
        report.node_actions.contains("action.llm.run"),
        "node action dispatch evidence is missing: {report:?}"
    );
    assert!(
        report
            .inspector_target_sources
            .is_superset(&BTreeSet::from(["measured", "fallback", "missing"])),
        "inspector target evidence must cover measured/fallback/missing: {report:?}"
    );
    assert!(
        report.inspector_actions.contains("action.column.remove"),
        "repeatable item inspector action evidence is missing: {report:?}"
    );
    assert!(
        report
            .blackboard_actions
            .contains("action.shader_property.add"),
        "blackboard action dispatch evidence is missing: {report:?}"
    );
    assert!(
        report.repeatable_mutations.is_superset(&BTreeSet::from([
            "add",
            "remove",
            "reorder",
            "edit",
            "missing_dynamic_port",
            "removed_port"
        ])),
        "repeatable mutation evidence must cover add/remove/reorder/edit and port lifecycle: {report:?}"
    );
    assert_dynamic_repeatable_lifecycle_report_gates(&report.dynamic_repeatable_lifecycle);
    assert!(
        report.invalid_hover_rejections >= 1,
        "shader invalid hover must reject incompatible typed targets: {report:?}"
    );
    assert!(
        report.editable_control_regions >= 3,
        "editable in-node/inspector control evidence is missing: {report:?}"
    );
}

/// Assert dynamic repeatable rows stay honest about graph ports, anchors, and row identity.
pub fn assert_dynamic_repeatable_lifecycle_report_gates(
    report: &OpenGpuiDynamicRepeatableLifecycleReport,
) {
    assert!(
        report.exercised_collections.is_superset(&BTreeSet::from([
            "shader.inputs".to_owned(),
            "table.columns".to_owned(),
            "llm.params".to_owned()
        ])),
        "dynamic repeatable lifecycle must cover shader, ERD, and Dify-like params: {report:?}"
    );
    assert!(
        report
            .mutations
            .is_superset(&BTreeSet::from(["add", "remove", "reorder", "edit"])),
        "dynamic repeatable lifecycle must cover add/remove/reorder/edit: {report:?}"
    );
    assert!(
        report.add_created_graph_ports > 0 || report.add_missing_port_diagnostics > 0,
        "repeatable add must either create graph port facts or emit missing-port diagnostics: {report:?}"
    );
    assert_eq!(
        report.add_fake_handle_count, 0,
        "repeatable add must not publish fake handles for missing graph ports: {report:?}"
    );
    assert!(
        report.remove_removed_graph_ports > 0,
        "repeatable remove must clear graph port facts for bound dynamic ports: {report:?}"
    );
    assert!(
        report.remove_removed_incident_edges > 0,
        "repeatable remove must clear incident edges for removed dynamic ports: {report:?}"
    );
    assert!(
        report.remove_cleared_repeatable_anchor,
        "repeatable remove must stop publishing removed item anchors: {report:?}"
    );
    assert!(
        report.reorder_preserved_item_identity
            && report.reorder_preserved_anchor_identity
            && report.reorder_preserved_slot_identity
            && report.reorder_preserved_port_binding,
        "repeatable reorder must preserve item, anchor, slot, and port identity: {report:?}"
    );
    assert!(
        report.reorder_has_row_bounds,
        "repeatable reorder must keep measured row bounds attached to the stable item identity: {report:?}"
    );
    assert!(
        report.edit_refreshed_row_label && report.edit_preserved_item_identity,
        "repeatable edit must refresh row data without changing item identity: {report:?}"
    );
    assert!(
        report.edit_missing_port_downgrade,
        "ERD field edits must stay downgraded when graph ports are missing: {report:?}"
    );
    assert!(
        report.display_only_rows_without_handles > 0,
        "Dify-style display-only params must be exercised: {report:?}"
    );
    assert_eq!(
        report.display_only_fake_handle_count, 0,
        "display-only repeatable params must not publish handles: {report:?}"
    );
    assert!(
        report.gaps.is_empty(),
        "dynamic repeatable lifecycle report has unresolved gaps: {report:?}"
    );
}

/// Collect structured evidence for GPUI-local authoring interactions.
pub fn authoring_interaction_report() -> Result<OpenGpuiAuthoringInteractionReport, String> {
    let registry = NodeKitRegistry::builtin().node_registry();
    let kit_registry = NodeKitRegistry::builtin();
    let mut report = OpenGpuiAuthoringInteractionReport::default();
    let llm = descriptor(&registry, "demo.llm");
    let dropped = crate::project_dropped_wire_menu(
        &registry,
        jellyflow::runtime::runtime::connection::ConnectionHandleRef::new(
            NodeId::from_u128(1),
            jellyflow::core::PortId::from_u128(2),
            jellyflow::core::PortDirection::Out,
        ),
        Some(&PortKey::new("completion")),
        jellyflow::core::CanvasPoint { x: 320.0, y: 160.0 },
    );
    report.dropped_wire_actions.extend(
        dropped
            .actions
            .iter()
            .filter(|action| action.dispatchable())
            .map(|action| action.key.clone()),
    );

    let node_menu = project_actions_for_surface(
        &llm,
        &OpenGpuiActionSurface::Node {
            node_kind: "demo.llm".to_owned(),
        },
    );
    report
        .node_actions
        .extend(node_menu.actions.iter().map(|action| action.key.clone()));

    let inspectors = crate::project_inspectors_for_surface(
        &llm,
        &llm.default_data,
        &OpenGpuiInspectorSurface::Node {
            node_kind: "demo.llm".to_owned(),
        },
    );
    report.editable_control_regions += inspectors
        .iter()
        .flat_map(|inspector| inspector.editable_controls())
        .count();
    report.editable_control_regions += llm
        .surface_slots
        .iter()
        .flat_map(|slot| project_slot_controls(&llm.default_data, slot))
        .filter(|control| control.is_editable())
        .count();

    let (table_descriptor, table_node_id, table_node, _table_graph) =
        schema_node_graph("demo.table")?;
    let column_inspector = crate::project_inspectors_for_surface(
        &table_descriptor,
        &table_node.data,
        &OpenGpuiInspectorSurface::RepeatableItem {
            collection_key: "table.columns".to_owned(),
            item_id: "email".to_owned(),
        },
    )
    .into_iter()
    .find(|inspector| inspector.key == "inspector.column.email")
    .ok_or_else(|| "missing table column inspector".to_owned())?;
    report.inspector_actions.extend(
        column_inspector
            .action_menu
            .actions
            .iter()
            .map(|action| action.key.clone()),
    );
    let measured = CanvasRect {
        origin: jellyflow::core::CanvasPoint { x: 8.0, y: 52.0 },
        size: CanvasSize {
            width: 144.0,
            height: 24.0,
        },
    };
    let fallback = CanvasRect {
        origin: jellyflow::core::CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 32.0,
            height: 16.0,
        },
    };
    let target_region_key = column_inspector
        .target_region_key
        .clone()
        .ok_or_else(|| "column inspector missing target region".to_owned())?;
    let measurement = NodeMeasurement::new(table_node_id)
        .with_slots([MeasuredSurfaceSlot::new(target_region_key, measured)]);
    for target in [
        resolve_inspector_target_bounds(&column_inspector, Some(&measurement), Some(fallback)),
        resolve_inspector_target_bounds(&column_inspector, None, Some(fallback)),
        resolve_inspector_target_bounds(&column_inspector, None, None),
    ] {
        report
            .inspector_target_sources
            .insert(inspector_target_source_name(target.source));
    }

    let (shader_descriptor, _, shader_node, _) = schema_node_graph("demo.shader.mix")?;
    report.blackboard_actions.extend(
        project_blackboards_for_descriptor(&shader_descriptor, &shader_node.data)
            .into_iter()
            .flat_map(|blackboard| blackboard.action_menu.actions.into_iter())
            .filter(|action| action.dispatchable())
            .map(|action| action.key),
    );
    report.dynamic_repeatable_lifecycle = dynamic_repeatable_lifecycle_report()?;
    report.repeatable_mutations.extend(
        report
            .dynamic_repeatable_lifecycle
            .mutations
            .iter()
            .copied(),
    );
    if report
        .dynamic_repeatable_lifecycle
        .add_missing_port_diagnostics
        > 0
    {
        report.repeatable_mutations.insert("missing_dynamic_port");
    }
    if report
        .dynamic_repeatable_lifecycle
        .remove_removed_graph_ports
        > 0
    {
        report.repeatable_mutations.insert("removed_port");
    }
    report.invalid_hover_rejections += shader_invalid_hover_rejections(&kit_registry)?;

    Ok(report)
}

fn schema_node_report(
    fixture_kind: OpenGpuiProductFixtureKind,
    kind: &str,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let kit_registry = NodeKitRegistry::builtin();
    let registry = kit_registry.node_registry();
    let descriptor = descriptor(&registry, kind);
    let schema = registry
        .get(&descriptor.kind)
        .ok_or_else(|| format!("missing schema `{}`", descriptor.kind.0))?;
    let instantiation = schema.instantiate(jellyflow::core::CanvasPoint::default());
    let (node_id, node, ports) = instantiation.into_parts();
    let mut graph_builder =
        jellyflow::core::GraphBuilder::new(jellyflow::core::GraphId::from_u128(0x67_70_75_69))
            .with_node(node_id, node.clone());
    for (port_id, port) in ports {
        graph_builder = graph_builder.with_port(port_id, port);
    }
    let graph = graph_builder.build_unchecked();
    let layout =
        projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, node_size(&node));
    assert_layout_regions_inside_node(&layout, node_size(&node), &descriptor.kind.0);
    let measurement = project_node_measurement(&node_id, &node, &graph, &descriptor);
    assert_measurement_inside_node(&measurement, node_size(&node), &descriptor.kind.0);

    let repeatable_items = repeatable_item_projection(&descriptor, &node, &graph, &node_id);
    let mut report = OpenGpuiProductFixtureReport {
        kind: fixture_kind,
        kit_key: kit_registry
            .manifest_for_kind(&descriptor.kind)
            .map(|manifest| manifest.key.0.clone())
            .unwrap_or_default(),
        fixture_key: kind.to_owned(),
        density_modes: BTreeSet::new(),
        region_sources: BTreeSet::from(["projection_fallback"]),
        resize_probe_count: 0,
        node_count: 1,
        measured_nodes: 1,
        slot_count: measurement.slots.len(),
        anchor_count: measurement.anchors.len(),
        repeatable_item_count: repeatable_items.len(),
        missing_dynamic_ports: repeatable_port_diagnostics(&repeatable_items),
        actions: descriptor
            .actions
            .iter()
            .map(|action| action.key.clone())
            .collect(),
        inspectors: descriptor
            .inspectors
            .iter()
            .map(|inspector| inspector.key.clone())
            .collect(),
        blackboards: descriptor
            .blackboards
            .iter()
            .map(|blackboard| blackboard.key.clone())
            .collect(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: 0,
        measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        measurement_coverage: OpenGpuiMeasurementCoverage {
            layout_pass_regions: 0,
            projection_fallback_regions: measurement.slots.len() + measurement.anchors.len(),
            missing_regions: 0,
            stale_regions: 0,
            partial_regions: 0,
            duplicate_regions: 0,
            measured_slots: measurement.slots.len(),
            measured_anchors: measurement.anchors.len(),
        },
    };
    collect_density_modes(
        &mut report,
        kit_registry.layout_hints_for_kind(&descriptor.kind),
    );
    collect_resize_probe(
        &mut report,
        &descriptor,
        &node,
        &graph,
        &node_id,
        node_size(&node),
    );
    collect_control_primitives(&mut report, &descriptor, &node);
    Ok(report)
}

fn layout_pass_schema_node_report(
    fixture_kind: OpenGpuiProductFixtureKind,
    kind: &str,
) -> Result<OpenGpuiProductFixtureReport, String> {
    let kit_registry = NodeKitRegistry::builtin();
    let registry = kit_registry.node_registry();
    let descriptor = descriptor(&registry, kind);
    let schema = registry
        .get(&descriptor.kind)
        .ok_or_else(|| format!("missing schema `{}`", descriptor.kind.0))?;
    let instantiation = schema.instantiate(jellyflow::core::CanvasPoint::default());
    let (node_id, node, ports) = instantiation.into_parts();
    let mut graph_builder =
        jellyflow::core::GraphBuilder::new(jellyflow::core::GraphId::from_u128(0x67_70_75_69))
            .with_node(node_id, node.clone());
    for (port_id, port) in ports {
        graph_builder = graph_builder.with_port(port_id, port);
    }
    let graph = graph_builder.build_unchecked();
    let size = node_size(&node);
    let layout = projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, size);
    let (regions, control_regions) = layout_pass_regions_for_node(&node_id, &node, &layout);
    let fallback_anchors = measured_surface_anchors(&descriptor, &graph, &node_id, &layout);
    let context = OpenGpuiMeasurementContext::new(node_id, node_view_origin(&node), 1.0, size)
        .with_revision(2);
    let (measurement, coverage) =
        layout_pass_measurement_from_regions(context, regions, fallback_anchors);
    assert_measurement_inside_node(&measurement, size, &descriptor.kind.0);

    let repeatable_items = repeatable_item_projection(&descriptor, &node, &graph, &node_id);
    let mut report = OpenGpuiProductFixtureReport {
        kind: fixture_kind,
        kit_key: kit_registry
            .manifest_for_kind(&descriptor.kind)
            .map(|manifest| manifest.key.0.clone())
            .unwrap_or_default(),
        fixture_key: kind.to_owned(),
        density_modes: BTreeSet::new(),
        region_sources: BTreeSet::new(),
        resize_probe_count: 0,
        node_count: 1,
        measured_nodes: 1,
        slot_count: measurement.slots.len(),
        anchor_count: measurement.anchors.len(),
        repeatable_item_count: repeatable_items.len(),
        missing_dynamic_ports: repeatable_port_diagnostics(&repeatable_items),
        actions: descriptor
            .actions
            .iter()
            .map(|action| action.key.clone())
            .collect(),
        inspectors: descriptor
            .inspectors
            .iter()
            .map(|inspector| inspector.key.clone())
            .collect(),
        blackboards: descriptor
            .blackboards
            .iter()
            .map(|blackboard| blackboard.key.clone())
            .collect(),
        control_primitives: BTreeSet::new(),
        measured_control_regions: control_regions,
        measurement_mode: OpenGpuiMeasurementMode::LayoutPass,
        measurement_coverage: coverage,
    };
    collect_density_modes(
        &mut report,
        kit_registry.layout_hints_for_kind(&descriptor.kind),
    );
    collect_resize_probe(
        &mut report,
        &descriptor,
        &node,
        &graph,
        &node_id,
        node_size(&node),
    );
    sync_region_sources(&mut report);
    collect_control_primitives(&mut report, &descriptor, &node);
    Ok(report)
}

fn schema_node_graph(kind: &str) -> Result<(NodeKindViewDescriptor, NodeId, Node, Graph), String> {
    let registry = NodeKitRegistry::builtin().node_registry();
    let descriptor = descriptor(&registry, kind);
    let schema = registry
        .get(&descriptor.kind)
        .ok_or_else(|| format!("missing schema `{}`", descriptor.kind.0))?;
    let instantiation = schema.instantiate(jellyflow::core::CanvasPoint::default());
    let (node_id, node, ports) = instantiation.into_parts();
    let mut graph_builder =
        GraphBuilder::new(GraphId::from_u128(0x67_70_75_69)).with_node(node_id, node.clone());
    for (port_id, port) in ports {
        graph_builder = graph_builder.with_port(port_id, port);
    }
    Ok((descriptor, node_id, node, graph_builder.build_unchecked()))
}

/// Collect structured dynamic repeatable lifecycle evidence across product-shaped fixtures.
pub fn dynamic_repeatable_lifecycle_report()
-> Result<OpenGpuiDynamicRepeatableLifecycleReport, String> {
    let mut report = OpenGpuiDynamicRepeatableLifecycleReport::default();
    collect_shader_repeatable_lifecycle(&mut report)?;
    collect_table_repeatable_lifecycle(&mut report)?;
    collect_llm_param_lifecycle(&mut report)?;
    finalize_dynamic_repeatable_lifecycle_report(&mut report);
    Ok(report)
}

fn collect_shader_repeatable_lifecycle(
    report: &mut OpenGpuiDynamicRepeatableLifecycleReport,
) -> Result<(), String> {
    let (descriptor, node_id, node, graph) = schema_node_graph("demo.shader.mix")?;
    report
        .exercised_collections
        .insert("shader.inputs".to_owned());

    let add = plan_repeatable_action(
        &descriptor,
        &graph,
        node_id,
        &node,
        OpenGpuiRepeatableActionPlan::Add {
            collection_key: "shader.inputs".to_owned(),
            item: serde_json::json!({
                "name": "Input 4",
                "ty": "vec4",
                "port": "input_4"
            }),
        },
    )
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "shader repeatable add produced no plan".to_owned())?;
    report.mutations.insert("add");
    report.add_missing_port_diagnostics += add
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.collection_key == "shader.inputs"
                && diagnostic.item_id == "input_4"
                && diagnostic.port_key == PortKey::new("input_4")
                && diagnostic.policy == OpenGpuiDynamicPortPolicy::MissingGraphPort
        })
        .count();
    report.add_created_graph_ports += add
        .transaction
        .ops()
        .iter()
        .filter(|op| matches!(op, GraphOp::AddPort { .. }))
        .count();

    let mut added_graph = graph.clone();
    add.transaction
        .apply_to(&mut added_graph)
        .map_err(|error| error.to_string())?;
    let added_node = graph_node(&added_graph, node_id)?;
    let added_items = repeatable_item_projection(&descriptor, added_node, &added_graph, &node_id);
    let added_input = added_items
        .iter()
        .find(|item| item.collection_key == "shader.inputs" && item.item_id == "input_4")
        .ok_or_else(|| "missing added shader input row".to_owned())?;
    let added_measurement =
        project_node_measurement(&node_id, added_node, &added_graph, &descriptor);
    if added_input.dynamic_port_policy == OpenGpuiDynamicPortPolicy::MissingGraphPort
        && (added_input.port_id.is_some()
            || has_measured_anchor_for_port_key(&added_measurement, &PortKey::new("input_4")))
    {
        report.add_fake_handle_count += 1;
    }

    let before_layout =
        projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, node_size(&node));
    let before_factor = repeatable_layout_by_item(&before_layout, "shader.inputs", "factor")?;
    let reorder = plan_repeatable_action(
        &descriptor,
        &graph,
        node_id,
        &node,
        OpenGpuiRepeatableActionPlan::Reorder {
            collection_key: "shader.inputs".to_owned(),
            item_id: "factor".to_owned(),
            to_index: 0,
        },
    )
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "shader repeatable reorder produced no plan".to_owned())?;
    report.mutations.insert("reorder");
    let mut reordered_graph = graph.clone();
    reorder
        .transaction
        .apply_to(&mut reordered_graph)
        .map_err(|error| error.to_string())?;
    let reordered_node = graph_node(&reordered_graph, node_id)?;
    let after_layout = projected_node_surface_graph_layout(
        &descriptor,
        reordered_node,
        &reordered_graph,
        &node_id,
        node_size(reordered_node),
    );
    let after_factor = repeatable_layout_by_item(&after_layout, "shader.inputs", "factor")?;
    report.reorder_preserved_item_identity = reorder.item_id.as_deref() == Some("factor")
        && after_factor.projection.item_id == before_factor.projection.item_id
        && after_factor.projection.item_index == 0;
    report.reorder_preserved_anchor_identity =
        after_factor.projection.anchor == before_factor.projection.anchor;
    report.reorder_preserved_slot_identity =
        after_factor.projection.slot_key == before_factor.projection.slot_key;
    report.reorder_preserved_port_binding = after_factor.projection.port_key
        == before_factor.projection.port_key
        && after_factor.projection.port_id == before_factor.projection.port_id
        && after_factor.projection.port_direction == before_factor.projection.port_direction
        && after_factor.projection.dynamic_port_policy
            == OpenGpuiDynamicPortPolicy::BoundToGraphPort;
    report.reorder_has_row_bounds = after_factor.rect.size.width > 0.0
        && after_factor.rect.size.height > 0.0
        && after_factor.anchor_rect.size.width > 0.0
        && after_factor.anchor_rect.size.height > 0.0;

    let mut remove_graph = graph.clone();
    let factor_port = find_port_by_key(&remove_graph, node_id, "factor")?;
    let result_port = find_port_by_key(&remove_graph, node_id, "result")?;
    let incident_edge_id = EdgeId::from_u128(0x67_70_75_69_06_00);
    GraphTransaction::from_ops([GraphOp::AddEdge {
        id: incident_edge_id,
        edge: Edge::new(EdgeKind::Data, result_port, factor_port),
    }])
    .apply_to(&mut remove_graph)
    .map_err(|error| error.to_string())?;
    let remove_node = graph_node(&remove_graph, node_id)?;

    let remove = plan_repeatable_action(
        &descriptor,
        &remove_graph,
        node_id,
        remove_node,
        OpenGpuiRepeatableActionPlan::Remove {
            collection_key: "shader.inputs".to_owned(),
            item_id: "factor".to_owned(),
        },
    )
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "shader repeatable remove produced no plan".to_owned())?;
    report.mutations.insert("remove");
    report.remove_removed_graph_ports += remove
        .transaction
        .ops()
        .iter()
        .filter(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == factor_port))
        .count();
    report.remove_removed_incident_edges += remove
        .transaction
        .ops()
        .iter()
        .filter_map(|op| match op {
            GraphOp::RemovePort { edges, .. } => Some(edges),
            _ => None,
        })
        .flat_map(|edges| edges.iter())
        .filter(|(edge_id, _)| *edge_id == incident_edge_id)
        .count();

    let mut removed_graph = remove_graph.clone();
    remove
        .transaction
        .apply_to(&mut removed_graph)
        .map_err(|error| error.to_string())?;
    let removed_node = graph_node(&removed_graph, node_id)?;
    let removed_items =
        repeatable_item_projection(&descriptor, removed_node, &removed_graph, &node_id);
    let removed_measurement =
        project_node_measurement(&node_id, removed_node, &removed_graph, &descriptor);
    report.remove_cleared_repeatable_anchor = !removed_items
        .iter()
        .any(|item| item.collection_key == "shader.inputs" && item.item_id == "factor")
        && !removed_graph.ports().contains_key(&factor_port)
        && !removed_graph.edges().contains_key(&incident_edge_id)
        && !has_measured_anchor(&removed_measurement, "rail.inputs.factor");

    Ok(())
}

fn collect_table_repeatable_lifecycle(
    report: &mut OpenGpuiDynamicRepeatableLifecycleReport,
) -> Result<(), String> {
    let (descriptor, node_id, node, graph) = schema_node_graph("demo.table")?;
    report
        .exercised_collections
        .insert("table.columns".to_owned());

    let before_layout =
        projected_node_surface_graph_layout(&descriptor, &node, &graph, &node_id, node_size(&node));
    let before_email = repeatable_layout_by_item(&before_layout, "table.columns", "email")?;
    let edit = plan_repeatable_action(
        &descriptor,
        &graph,
        node_id,
        &node,
        OpenGpuiRepeatableActionPlan::Edit {
            collection_key: "table.columns".to_owned(),
            item_id: "email".to_owned(),
            control_key: "control.column.name".to_owned(),
            value: serde_json::json!("email_address"),
        },
    )
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "table repeatable edit produced no plan".to_owned())?;
    report.mutations.insert("edit");

    let mut edited_graph = graph.clone();
    edit.transaction
        .apply_to(&mut edited_graph)
        .map_err(|error| error.to_string())?;
    let edited_node = graph_node(&edited_graph, node_id)?;
    let after_layout = projected_node_surface_graph_layout(
        &descriptor,
        edited_node,
        &edited_graph,
        &node_id,
        node_size(edited_node),
    );
    let after_email = repeatable_layout_by_item(&after_layout, "table.columns", "email")?;
    let edited_measurement =
        project_node_measurement(&node_id, edited_node, &edited_graph, &descriptor);
    report.edit_refreshed_row_label = after_email.projection.label == "email_address";
    report.edit_preserved_item_identity = edit.item_id.as_deref() == Some("email")
        && after_email.projection.item_id == before_email.projection.item_id
        && after_email.projection.slot_key == before_email.projection.slot_key
        && after_email.projection.anchor == before_email.projection.anchor
        && after_email.rect == before_email.rect
        && after_email.anchor_rect == before_email.anchor_rect;
    report.edit_missing_port_downgrade = after_email.projection.dynamic_port_policy
        == OpenGpuiDynamicPortPolicy::MissingGraphPort
        && after_email.projection.port_id.is_none();
    if after_email.projection.dynamic_port_policy == OpenGpuiDynamicPortPolicy::MissingGraphPort
        && after_email
            .projection
            .port_key
            .as_ref()
            .is_some_and(|port_key| has_measured_anchor_for_port_key(&edited_measurement, port_key))
    {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::EditPublishedFakeHandle);
    }

    Ok(())
}

fn collect_llm_param_lifecycle(
    report: &mut OpenGpuiDynamicRepeatableLifecycleReport,
) -> Result<(), String> {
    let (descriptor, node_id, node, graph) = schema_node_graph("demo.llm")?;
    report.exercised_collections.insert("llm.params".to_owned());

    let add = plan_repeatable_action(
        &descriptor,
        &graph,
        node_id,
        &node,
        OpenGpuiRepeatableActionPlan::Add {
            collection_key: "llm.params".to_owned(),
            item: serde_json::json!({
                "name": "locale",
                "value": "{{ customer.locale }}"
            }),
        },
    )
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "llm param add produced no plan".to_owned())?;
    let mut updated_graph = graph.clone();
    add.transaction
        .apply_to(&mut updated_graph)
        .map_err(|error| error.to_string())?;
    let updated_node = graph_node(&updated_graph, node_id)?;
    let items = repeatable_item_projection(&descriptor, updated_node, &updated_graph, &node_id);
    let locale = items
        .iter()
        .find(|item| item.collection_key == "llm.params" && item.item_id == "param_3")
        .ok_or_else(|| "missing added llm param row".to_owned())?;
    if locale.dynamic_port_policy == OpenGpuiDynamicPortPolicy::DisplayOnly
        && locale.port_key.is_none()
        && locale.port_id.is_none()
    {
        report.display_only_rows_without_handles += 1;
    } else {
        report.display_only_fake_handle_count += 1;
    }
    Ok(())
}

fn finalize_dynamic_repeatable_lifecycle_report(
    report: &mut OpenGpuiDynamicRepeatableLifecycleReport,
) {
    if report.add_created_graph_ports == 0 && report.add_missing_port_diagnostics == 0 {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::AddMissingPortPolicyAbsent);
    }
    if report.add_fake_handle_count > 0 {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::AddPublishedFakeHandle);
    }
    if report.remove_removed_graph_ports == 0 {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::RemoveKeptGraphPort);
    }
    if report.remove_removed_incident_edges == 0 {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::RemoveKeptIncidentEdge);
    }
    if !report.remove_cleared_repeatable_anchor {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::RemoveKeptRepeatableAnchor);
    }
    if !report.reorder_preserved_item_identity {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::ReorderChangedItemIdentity);
    }
    if !report.reorder_preserved_anchor_identity {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::ReorderChangedAnchorIdentity);
    }
    if !report.reorder_preserved_slot_identity {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::ReorderChangedSlotIdentity);
    }
    if !report.reorder_preserved_port_binding {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::ReorderChangedPortBinding);
    }
    if !report.reorder_has_row_bounds {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::ReorderMissingRowBounds);
    }
    if !report.edit_refreshed_row_label {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::EditKeptStaleLabel);
    }
    if !report.edit_preserved_item_identity {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::EditChangedItemIdentity);
    }
    if !report.edit_missing_port_downgrade {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::EditPublishedFakeHandle);
    }
    if report.display_only_fake_handle_count > 0 || report.display_only_rows_without_handles == 0 {
        report
            .gaps
            .insert(OpenGpuiDynamicRepeatableLifecycleGap::DisplayOnlyPublishedHandle);
    }
}

fn graph_node(graph: &Graph, node_id: NodeId) -> Result<&Node, String> {
    graph
        .nodes()
        .get(&node_id)
        .ok_or_else(|| format!("missing node `{node_id:?}`"))
}

fn repeatable_layout_by_item<'a>(
    layout: &'a OpenGpuiNodeSurfaceLayout,
    collection_key: &str,
    item_id: &str,
) -> Result<&'a OpenGpuiRepeatableItemLayout, String> {
    layout
        .repeatable_items
        .iter()
        .find(|item| {
            item.projection.collection_key == collection_key && item.projection.item_id == item_id
        })
        .ok_or_else(|| format!("missing repeatable item `{collection_key}:{item_id}`"))
}

fn has_measured_anchor(measurement: &NodeMeasurement, anchor_key: &str) -> bool {
    measurement
        .anchors
        .iter()
        .any(|anchor| anchor.anchor == anchor_key)
}

fn has_measured_anchor_for_port_key(measurement: &NodeMeasurement, port_key: &PortKey) -> bool {
    measurement
        .anchors
        .iter()
        .any(|anchor| anchor.port_key.as_ref() == Some(port_key))
}

fn shader_invalid_hover_rejections(kit_registry: &NodeKitRegistry) -> Result<usize, String> {
    let graph = kit_registry
        .fixture_graph(&NodeKitKey::new("shader.blueprint"), "shader.material_mix")
        .map_err(|error| error.to_string())?;
    let texture = find_node_by_kind(&graph, "demo.shader.texture_sample")?;
    let mix = find_node_by_kind(&graph, "demo.shader.mix")?;
    let color = find_port_by_key(&graph, texture, "color")?;
    let factor = find_port_by_key(&graph, mix, "factor")?;
    let mut compatibility = DefaultTypeCompatibility;
    let plan = jellyflow::runtime::rules::plan_connect_typed(
        &graph,
        color,
        factor,
        |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
        &mut compatibility,
    );
    Ok(usize::from(plan.is_reject()))
}

fn find_node_by_kind(graph: &Graph, kind: &str) -> Result<NodeId, String> {
    graph
        .nodes()
        .iter()
        .find_map(|(id, node)| (node.kind == NodeKindKey::new(kind)).then_some(*id))
        .ok_or_else(|| format!("missing node kind `{kind}`"))
}

fn find_port_by_key(graph: &Graph, node_id: NodeId, key: &str) -> Result<PortId, String> {
    let node = graph
        .nodes()
        .get(&node_id)
        .ok_or_else(|| format!("missing node `{node_id:?}`"))?;
    node.ports
        .iter()
        .copied()
        .find(|port_id| {
            graph
                .ports()
                .get(port_id)
                .is_some_and(|port| port.key == PortKey::new(key))
        })
        .ok_or_else(|| format!("missing port `{key}` on `{node_id:?}`"))
}

fn layout_pass_regions_for_node(
    node_id: &NodeId,
    node: &Node,
    layout: &OpenGpuiNodeSurfaceLayout,
) -> (Vec<OpenGpuiMeasuredRegion>, usize) {
    let node_view_origin = node_view_origin(node);
    let mut regions = Vec::new();
    let mut control_regions = 0;

    for slot in &layout.slots {
        regions.push(
            OpenGpuiMeasurementId::slot(*node_id, slot.slot.key.clone())
                .into_region(view_bounds_from_rect(node_view_origin, slot.rect)),
        );
        if let Some(anchor) = slot
            .descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.anchor.as_ref())
        {
            regions.push(
                OpenGpuiMeasurementId::anchor(*node_id, anchor.clone())
                    .into_region(view_bounds_from_rect(node_view_origin, slot.anchor_rect)),
            );
        }
        if let Some(descriptor) = &slot.descriptor {
            let controls = project_slot_controls(&node.data, descriptor);
            let control_count = controls.len();
            control_regions += control_count;
            for (index, control) in controls.into_iter().enumerate() {
                regions.push(
                    OpenGpuiMeasurementId::control_in_slot(
                        *node_id,
                        descriptor.key.as_str(),
                        control.key,
                    )
                    .into_region(view_bounds_from_rect(
                        node_view_origin,
                        control_region_rect(slot.rect, index, control_count.max(1)),
                    )),
                );
            }
        }
    }

    for repeatable in &layout.repeatables {
        regions.push(
            OpenGpuiMeasurementId::slot(*node_id, repeatable.projection.key.clone())
                .into_region(view_bounds_from_rect(node_view_origin, repeatable.rect)),
        );
    }

    for item in &layout.repeatable_items {
        regions.push(
            OpenGpuiMeasurementId::repeatable_item(
                *node_id,
                item.projection.slot_key.clone(),
                item.projection.item_id.clone(),
            )
            .into_region(view_bounds_from_rect(node_view_origin, item.rect)),
        );
        regions.push(
            OpenGpuiMeasurementId::anchor(*node_id, item.projection.anchor.clone())
                .into_region(view_bounds_from_rect(node_view_origin, item.anchor_rect)),
        );
    }

    (regions, control_regions)
}

fn control_region_rect(slot: CanvasRect, index: usize, count: usize) -> CanvasRect {
    let count = count.max(1) as f32;
    let track_width = (slot.size.width - 8.0).max(count);
    let segment_width = track_width / count;
    CanvasRect {
        origin: jellyflow::core::CanvasPoint {
            x: slot.origin.x + 4.0 + segment_width * index as f32,
            y: slot.origin.y + 4.0,
        },
        size: CanvasSize {
            width: (segment_width - 2.0).max(1.0),
            height: (slot.size.height - 8.0).max(1.0),
        },
    }
}

fn node_view_origin(node: &Node) -> OpenGpuiViewPoint {
    OpenGpuiViewPoint::new(node.pos.x + 13.0, node.pos.y + 29.0)
}

fn view_bounds_from_rect(origin: OpenGpuiViewPoint, rect: CanvasRect) -> OpenGpuiViewBounds {
    OpenGpuiViewBounds::new(
        OpenGpuiViewPoint::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
        OpenGpuiViewSize::new(rect.size.width, rect.size.height),
    )
}

fn accumulate_coverage(
    target: &mut OpenGpuiMeasurementCoverage,
    coverage: OpenGpuiMeasurementCoverage,
) {
    target.layout_pass_regions += coverage.layout_pass_regions;
    target.projection_fallback_regions += coverage.projection_fallback_regions;
    target.missing_regions += coverage.missing_regions;
    target.stale_regions += coverage.stale_regions;
    target.partial_regions += coverage.partial_regions;
    target.duplicate_regions += coverage.duplicate_regions;
    target.measured_slots += coverage.measured_slots;
    target.measured_anchors += coverage.measured_anchors;
}

fn sync_region_sources(report: &mut OpenGpuiProductFixtureReport) {
    if report.measurement_coverage.layout_pass_regions > 0 {
        report.region_sources.insert("layout_pass");
    }
    if report.measurement_coverage.projection_fallback_regions > 0 {
        report.region_sources.insert("projection_fallback");
    }
    if report.measurement_coverage.missing_regions > 0 {
        report.region_sources.insert("missing");
    }
    if report.measurement_coverage.stale_regions > 0 {
        report.region_sources.insert("stale");
    }
    if report.measurement_coverage.partial_regions > 0 {
        report.region_sources.insert("partial");
    }
    if report.measurement_coverage.duplicate_regions > 0 {
        report.region_sources.insert("duplicate");
    }
}

fn collect_density_modes(
    report: &mut OpenGpuiProductFixtureReport,
    hints: Option<&jellyflow::runtime::schema::NodeKitLayoutHints>,
) {
    let hints = hints.cloned().unwrap_or_default();
    for zoom in [
        (hints.compact_zoom_min * 0.5).max(0.01),
        (hints.compact_zoom_min + hints.full_zoom_min) * 0.5,
        hints.full_zoom_min + 0.25,
    ] {
        report
            .density_modes
            .insert(density_name(hints.content_density_for_zoom(zoom)));
    }
}

fn density_name(density: NodeKitContentDensity) -> &'static str {
    match density {
        NodeKitContentDensity::Compact => "compact",
        NodeKitContentDensity::Regular => "regular",
        NodeKitContentDensity::Full => "full",
    }
}

fn collect_resize_probe(
    report: &mut OpenGpuiProductFixtureReport,
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
    graph: &Graph,
    node_id: &NodeId,
    size: CanvasSize,
) {
    let resized = CanvasSize {
        width: size.width + 96.0,
        height: size.height + 64.0,
    };
    let resized_layout =
        projected_node_surface_graph_layout(descriptor, node, graph, node_id, resized);
    assert_layout_regions_inside_node(&resized_layout, resized, &descriptor.kind.0);
    report.resize_probe_count += 1;
}

fn assert_density_and_resize_evidence(report: &OpenGpuiProductFixtureReport) {
    assert!(
        report
            .density_modes
            .is_superset(&BTreeSet::from(["compact", "regular", "full"])),
        "{:?} fixture `{}` must cover compact/regular/full density modes: {:?}",
        report.kind,
        report.fixture_key,
        report.density_modes
    );
    assert!(
        report.resize_probe_count >= report.measured_nodes.max(1),
        "{:?} fixture `{}` must include resize geometry probes: {} probes for {} measured nodes",
        report.kind,
        report.fixture_key,
        report.resize_probe_count,
        report.measured_nodes
    );
}

fn inspector_target_source_name(source: OpenGpuiInspectorTargetSource) -> &'static str {
    match source {
        OpenGpuiInspectorTargetSource::Measured => "measured",
        OpenGpuiInspectorTargetSource::Fallback => "fallback",
        OpenGpuiInspectorTargetSource::Missing => "missing",
    }
}

fn collect_control_primitives(
    report: &mut OpenGpuiProductFixtureReport,
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
) {
    for slot in &descriptor.surface_slots {
        for control in project_slot_controls(&node.data, slot) {
            report
                .control_primitives
                .insert(control_primitive_name(control.primitive));
        }
    }
    for collection in &descriptor.repeatable_collections {
        for slot in &collection.item_template_slots {
            for control in &slot.controls {
                report
                    .control_primitives
                    .insert(control_primitive_name(primitive_for_kind(control.kind)));
            }
        }
    }
}

fn control_primitive_name(primitive: OpenGpuiControlPrimitive) -> &'static str {
    match primitive {
        OpenGpuiControlPrimitive::TextInput => "text_input",
        OpenGpuiControlPrimitive::TextArea => "textarea",
        OpenGpuiControlPrimitive::NumberInput => "number_input",
        OpenGpuiControlPrimitive::Select => "select",
        OpenGpuiControlPrimitive::MultiSelect => "multi_select",
        OpenGpuiControlPrimitive::Switch => "switch",
        OpenGpuiControlPrimitive::Slider => "slider",
        OpenGpuiControlPrimitive::CodeEditor => "code_editor",
        OpenGpuiControlPrimitive::ColorSwatch => "color_swatch",
        OpenGpuiControlPrimitive::AssetPickerStub => "asset_picker_stub",
        OpenGpuiControlPrimitive::VariablePickerStub => "variable_picker_stub",
        OpenGpuiControlPrimitive::PortBindingDisplay => "port_binding_display",
    }
}

fn descriptor(registry: &NodeRegistry, kind: &str) -> NodeKindViewDescriptor {
    registry
        .view_descriptor(&NodeKindKey::new(kind))
        .unwrap_or_else(|| panic!("missing descriptor `{kind}`"))
}

fn node_size(node: &Node) -> CanvasSize {
    node.size.unwrap_or(CanvasSize {
        width: 228.0,
        height: 168.0,
    })
}

fn assert_layout_regions_inside_node(
    layout: &OpenGpuiNodeSurfaceLayout,
    size: CanvasSize,
    label: &str,
) {
    for slot in &layout.slots {
        assert_rect_inside_node(slot.rect, size, label, &slot.slot.key);
        assert_rect_inside_node(slot.anchor_rect, size, label, &slot.slot.key);
    }
    for repeatable in &layout.repeatables {
        assert_rect_inside_node(repeatable.rect, size, label, &repeatable.projection.key);
        assert_rect_inside_node(
            repeatable.anchor_rect,
            size,
            label,
            &repeatable.projection.key,
        );
    }
    for item in &layout.repeatable_items {
        assert_rect_inside_node(item.rect, size, label, &item.projection.slot_key);
        assert_rect_inside_node(item.anchor_rect, size, label, &item.projection.anchor);
    }
}

fn assert_measurement_inside_node(measurement: &NodeMeasurement, size: CanvasSize, label: &str) {
    for slot in &measurement.slots {
        assert_rect_inside_node(slot.rect, size, label, &slot.key);
    }
    for anchor in &measurement.anchors {
        assert_rect_inside_node(anchor.rect, size, label, &anchor.anchor);
    }
}

fn assert_rect_inside_node(rect: CanvasRect, size: CanvasSize, node_label: &str, region: &str) {
    assert!(
        rect.is_positive_finite(),
        "{node_label}:{region} must have positive finite bounds: {rect:?}"
    );
    assert!(
        rect.origin.x >= 0.0
            && rect.origin.y >= 0.0
            && rect.origin.x + rect.size.width <= size.width + 0.01
            && rect.origin.y + rect.size.height <= size.height + 0.01,
        "{node_label}:{region} must stay inside node size {size:?}: {rect:?}"
    );
}

struct ProductFixtureSpec {
    id: &'static str,
    family: OpenGpuiProductFixtureFamily,
    kit_key: &'static str,
    fixture_key: &'static str,
    expected_renderer_keys: &'static [&'static str],
    expected_capabilities: &'static [&'static str],
}

impl ProductFixtureSpec {
    fn for_kind(kind: OpenGpuiProductFixtureKind) -> Self {
        match kind {
            OpenGpuiProductFixtureKind::DifyWorkflow => Self {
                id: "workflow.review",
                family: OpenGpuiProductFixtureFamily::Workflow,
                kit_key: "workflow.automation",
                fixture_key: "workflow.review",
                expected_renderer_keys: &["decision-card"],
                expected_capabilities: &[
                    "controls",
                    "actions",
                    "repeatables",
                    "inspector",
                    "dropped-wire",
                ],
            },
            OpenGpuiProductFixtureKind::ShaderBlueprint => Self {
                id: "shader.material_mix",
                family: OpenGpuiProductFixtureFamily::ShaderGraph,
                kit_key: "shader.blueprint",
                fixture_key: "shader.material_mix",
                expected_renderer_keys: &["shader-card"],
                expected_capabilities: &[
                    "controls",
                    "repeatables",
                    "dynamic-ports",
                    "blackboard",
                    "invalid-hover",
                ],
            },
            OpenGpuiProductFixtureKind::ErdTable => Self {
                id: "erd.customer_orders",
                family: OpenGpuiProductFixtureFamily::Erd,
                kit_key: "erd.table",
                fixture_key: "erd.customer_orders",
                expected_renderer_keys: &["table-card"],
                expected_capabilities: &[
                    "controls",
                    "repeatables",
                    "dynamic-ports",
                    "inspector",
                    "resize",
                ],
            },
            OpenGpuiProductFixtureKind::MindMap => Self {
                id: "mind-map.strategy",
                family: OpenGpuiProductFixtureFamily::MindMap,
                kit_key: "mind-map.knowledge-canvas",
                fixture_key: "mind-map.strategy",
                expected_renderer_keys: &["topic-card", "source-card"],
                expected_capabilities: &["controls", "preview", "shell", "actions"],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        OpenGpuiConnectionPreviewPolicyEvidence, OpenGpuiGraphAffordanceEvidence,
        OpenGpuiMeasurementMode, OpenGpuiSurfaceStyleBudget,
    };

    fn productized_graph_affordance_evidence() -> OpenGpuiGraphAffordanceEvidence {
        OpenGpuiGraphAffordanceEvidence::for_renderer_key(
            "shader-card",
            OpenGpuiSurfaceStyleBudget::for_renderer_key("shader-card"),
        )
    }

    #[test]
    fn helper_rejects_full_claim_without_layout_pass_mode() {
        let adapter = OpenGpuiAdapter::projection_fallback();
        assert_layout_pass_capability_requires_real_bounds(&adapter);
        assert_eq!(
            adapter.measurement_mode(),
            OpenGpuiMeasurementMode::ProjectionFallback
        );
    }

    #[test]
    fn product_fixtures_cover_gpui_authoring_regressions() {
        assert_product_fixture_regression_gates();
    }

    #[test]
    fn interaction_fixtures_cover_gpui_authoring_states() {
        assert_authoring_interaction_regression_gates();
    }

    #[test]
    fn product_fixture_catalog_lists_stable_gallery_cases() {
        let catalog = product_fixture_catalog();
        assert_eq!(catalog.len(), 4);
        assert!(catalog.iter().any(|fixture| {
            fixture.id == "workflow.review"
                && fixture.kind == OpenGpuiProductFixtureKind::DifyWorkflow
                && fixture.family == OpenGpuiProductFixtureFamily::Workflow
                && fixture.kit_key == "workflow.automation"
                && fixture.expected_renderer_keys.contains("decision-card")
        }));
        assert!(catalog.iter().any(|fixture| {
            fixture.id == "shader.material_mix"
                && fixture.family == OpenGpuiProductFixtureFamily::ShaderGraph
                && fixture.expected_renderer_keys.contains("shader-card")
                && fixture.expected_capabilities.contains("dynamic-ports")
        }));
        assert!(catalog.iter().any(|fixture| {
            fixture.id == "erd.customer_orders"
                && fixture.family == OpenGpuiProductFixtureFamily::Erd
                && fixture.expected_renderer_keys.contains("table-card")
        }));
        assert!(catalog.iter().any(|fixture| {
            fixture.id == "mind-map.strategy"
                && fixture.family == OpenGpuiProductFixtureFamily::MindMap
                && fixture.expected_renderer_keys.contains("topic-card")
                && fixture.expected_renderer_keys.contains("source-card")
        }));

        let encoded = serde_json::to_string(&catalog[0]).expect("catalog case serializes");
        assert!(encoded.contains("workflow.review"));
    }

    #[test]
    fn host_surface_report_contract_marks_fallback_and_gaps() {
        let fixtures = product_fixture_catalog();
        let mut report = OpenGpuiHostSurfaceReport::default();
        for fixture in &fixtures {
            let mut row = OpenGpuiHostSurfaceReportRow::new(
                fixture,
                format!("node.{}", fixture.id),
                fixture
                    .expected_renderer_keys
                    .iter()
                    .next()
                    .expect("expected renderer")
                    .clone(),
                OpenGpuiHostRendererSource::ProductRenderer,
            )
            .with_measurement(2, 1)
            .with_style_budget(
                OpenGpuiSurfaceStyleBudget::for_renderer_key(
                    fixture
                        .expected_renderer_keys
                        .iter()
                        .next()
                        .expect("expected renderer"),
                )
                .evidence(),
            );
            if fixture.kind == OpenGpuiProductFixtureKind::ShaderBlueprint {
                row = OpenGpuiHostSurfaceReportRow::new(
                    fixture,
                    "demo.shader.mix",
                    "shader-card",
                    OpenGpuiHostRendererSource::MissingHostRenderer,
                )
                .with_measurement(0, 0)
                .with_unsupported_control("control.color");
            }
            report.push(row);
        }

        assert_host_surface_report_contract(&report);
        let shader = report
            .rows_for_fixture("shader.material_mix")
            .next()
            .expect("shader report row");
        assert!(shader.uses_renderer_fallback());
        assert!(
            shader
                .capability_gaps
                .contains(&OpenGpuiHostCapabilityGap::RendererFallback)
        );
        assert!(
            shader
                .capability_gaps
                .contains(&OpenGpuiHostCapabilityGap::UnsupportedControl)
        );
        assert!(
            shader
                .capability_gaps
                .contains(&OpenGpuiHostCapabilityGap::MissingMeasuredRegion)
        );
        assert!(serde_json::to_string(&report).is_ok());
    }

    #[test]
    fn strict_gallery_gate_rejects_product_renderer_fallbacks() {
        let fixtures = product_fixture_catalog();
        let report = OpenGpuiHostSurfaceReport::new(fixtures.iter().map(|fixture| {
            let source = if fixture.kind == OpenGpuiProductFixtureKind::DifyWorkflow {
                OpenGpuiHostRendererSource::ProductRenderer
            } else {
                OpenGpuiHostRendererSource::DescriptorFallback
            };
            let mut row = OpenGpuiHostSurfaceReportRow::new(
                fixture,
                format!("node.{}", fixture.id),
                fixture
                    .expected_renderer_keys
                    .iter()
                    .next()
                    .expect("expected renderer")
                    .clone(),
                source,
            )
            .with_measurement(1, 1);
            if source == OpenGpuiHostRendererSource::ProductRenderer {
                let renderer_key = row.renderer_key.clone();
                row = row.with_style_budget(
                    OpenGpuiSurfaceStyleBudget::for_renderer_key(&renderer_key).evidence(),
                );
            }
            row
        }));

        let result = std::panic::catch_unwind(|| assert_product_gallery_host_report_gates(&report));
        assert!(
            result.is_err(),
            "strict gallery gate must fail while product renderers are missing"
        );
    }

    #[test]
    fn product_interaction_gate_accepts_productized_report() {
        let mut report = OpenGpuiHostProductInteractionReport::default();
        report.mark_drag_surface_coverage(product_fixture_catalog().len(), true);
        report.mark_control_event_shielding_checked(true);
        report.mark_port_hotspot_path_checked(true);
        report.mark_tool_switcher_visible(true);
        report.mark_connect_flow_store_synced(true);
        report.mark_reconnect_affordance_visible(true);
        report.mark_dropped_wire_gesture_connected(true);
        report.mark_graph_affordance_evidence(productized_graph_affordance_evidence());
        report.mark_repeatable_overflow(3, 1);

        assert_product_interaction_report_gates(&report);
    }

    #[test]
    fn product_interaction_gate_rejects_unresolved_product_gaps() {
        let mut report = OpenGpuiHostProductInteractionReport::default();
        report.mark_drag_surface_coverage(product_fixture_catalog().len(), true);
        report.mark_control_event_shielding_checked(true);
        report.mark_port_hotspot_path_checked(true);
        report.mark_tool_switcher_visible(true);
        report.mark_connect_flow_store_synced(false);
        report.mark_reconnect_affordance_visible(true);
        report.mark_dropped_wire_gesture_connected(true);
        report.mark_graph_affordance_evidence(productized_graph_affordance_evidence());
        report.mark_repeatable_overflow(3, 0);

        let result = std::panic::catch_unwind(|| assert_product_interaction_report_gates(&report));
        assert!(
            result.is_err(),
            "product interaction hard gate must fail while connect sync or overflow indicators are missing"
        );
    }

    #[test]
    fn product_interaction_gate_rejects_missing_graph_affordance_evidence() {
        let mut report = OpenGpuiHostProductInteractionReport::default();
        report.mark_drag_surface_coverage(product_fixture_catalog().len(), true);
        report.mark_control_event_shielding_checked(true);
        report.mark_port_hotspot_path_checked(true);
        report.mark_tool_switcher_visible(true);
        report.mark_connect_flow_store_synced(true);
        report.mark_reconnect_affordance_visible(true);
        report.mark_dropped_wire_gesture_connected(true);
        report.mark_repeatable_overflow(0, 0);

        let result = std::panic::catch_unwind(|| assert_product_interaction_report_gates(&report));
        assert!(
            result.is_err(),
            "product interaction hard gate must fail until route, preview, port-hit, drag, and measurement affordance evidence is reported"
        );
    }

    #[test]
    fn product_interaction_gate_rejects_direct_line_preview_fallback() {
        let mut report = OpenGpuiHostProductInteractionReport::default();
        report.mark_drag_surface_coverage(product_fixture_catalog().len(), true);
        report.mark_control_event_shielding_checked(true);
        report.mark_port_hotspot_path_checked(true);
        report.mark_tool_switcher_visible(true);
        report.mark_connect_flow_store_synced(true);
        report.mark_reconnect_affordance_visible(true);
        report.mark_dropped_wire_gesture_connected(true);

        let mut evidence = productized_graph_affordance_evidence();
        evidence.connection_preview_policy =
            OpenGpuiConnectionPreviewPolicyEvidence::DirectLineFallback;
        report.mark_graph_affordance_evidence(evidence);
        report.mark_repeatable_overflow(0, 0);

        assert!(
            report
                .gaps
                .contains(&OpenGpuiHostProductInteractionGap::GraphAffordanceRoutePolicyMissing)
        );
        let result = std::panic::catch_unwind(|| assert_product_interaction_report_gates(&report));
        assert!(
            result.is_err(),
            "product interaction hard gate must fail when previews fall back to direct lines"
        );
    }

    #[test]
    fn native_lifecycle_gate_accepts_product_gallery_close_evidence() {
        let mut evidence = OpenGpuiNativeLifecycleEvidence::default();
        evidence.mark_product_gallery_rendered("workflow.review", 4);
        evidence.mark_product_drag_checked(true);
        evidence.mark_last_window_close(1, true, 0, true);

        assert_native_lifecycle_evidence_gates(&evidence);
    }

    #[test]
    fn native_lifecycle_gate_rejects_blank_window_evidence() {
        let mut evidence = OpenGpuiNativeLifecycleEvidence::default();
        evidence.mark_last_window_close(1, true, 0, true);

        let result = std::panic::catch_unwind(|| assert_native_lifecycle_evidence_gates(&evidence));
        assert!(
            result.is_err(),
            "native lifecycle gate must reject evidence that never rendered product gallery content"
        );
    }

    #[test]
    fn native_lifecycle_gate_rejects_close_automation_skip() {
        let mut evidence = OpenGpuiNativeLifecycleEvidence::default();
        evidence.mark_product_gallery_rendered("workflow.review", 4);
        evidence.mark_product_drag_checked(true);
        evidence.mark_close_automation_skipped("test platform does not support close");

        let result = std::panic::catch_unwind(|| assert_native_lifecycle_evidence_gates(&evidence));
        assert!(
            result.is_err(),
            "native lifecycle gate must reject skipped close automation"
        );
    }

    #[test]
    fn screenshot_region_gate_accepts_every_product_region() {
        let mut report = OpenGpuiScreenshotRegionReport::default();
        for fixture in product_fixture_catalog() {
            let mut evidence =
                OpenGpuiScreenshotFixtureEvidence::captured(fixture.id, 1140, 650, 4096, 8);
            for kind in [
                OpenGpuiScreenshotRegionKind::NodeBody,
                OpenGpuiScreenshotRegionKind::NodeInternalUi,
                OpenGpuiScreenshotRegionKind::WirePath,
                OpenGpuiScreenshotRegionKind::PortArea,
            ] {
                evidence.push_region(OpenGpuiScreenshotRegionEvidence::new(
                    kind,
                    OpenGpuiScreenshotRegionRect {
                        x: 8,
                        y: 8,
                        width: 64,
                        height: 48,
                    },
                    1024,
                    8,
                ));
            }
            report.push_fixture(evidence);
        }

        assert_screenshot_region_report_gates(&report);
    }

    #[test]
    fn screenshot_region_gate_rejects_missing_or_single_color_regions() {
        let mut report = OpenGpuiScreenshotRegionReport::default();
        let fixture = product_fixture_catalog()
            .into_iter()
            .next()
            .expect("product fixture");
        let mut evidence =
            OpenGpuiScreenshotFixtureEvidence::captured(fixture.id, 1140, 650, 4096, 8);
        evidence.push_region(OpenGpuiScreenshotRegionEvidence::new(
            OpenGpuiScreenshotRegionKind::NodeBody,
            OpenGpuiScreenshotRegionRect {
                x: 8,
                y: 8,
                width: 64,
                height: 48,
            },
            1024,
            1,
        ));
        report.push_fixture(evidence);

        let result = std::panic::catch_unwind(|| assert_screenshot_region_report_gates(&report));
        assert!(
            result.is_err(),
            "screenshot region gate must reject single-color or incomplete ROI evidence"
        );
    }

    #[test]
    fn screenshot_region_gate_rejects_skipped_capture() {
        let report = OpenGpuiScreenshotRegionReport::skipped("headless renderer unavailable");
        let result = std::panic::catch_unwind(|| assert_screenshot_region_report_gates(&report));
        assert!(
            result.is_err(),
            "screenshot region gate must reject skipped capture as hard evidence"
        );
    }

    #[test]
    fn visual_gate_rejects_unreadable_product_content() {
        let fixture = product_fixture_catalog()
            .into_iter()
            .find(|fixture| fixture.kind == OpenGpuiProductFixtureKind::ErdTable)
            .expect("ERD fixture");
        let mut report = OpenGpuiHostVisualInteractionReport::default();
        report.push(
            OpenGpuiHostVisualSurfaceRow::new(
                &fixture,
                "demo.table",
                "table-card",
                OpenGpuiHostRendererSource::ProductRenderer,
            )
            .with_content_bounds(true, false, true)
            .with_readability_budget(
                OpenGpuiSizeEvidence {
                    width: 220,
                    height: 160,
                },
                Some(OpenGpuiSizeEvidence {
                    width: 420,
                    height: 330,
                }),
            )
            .with_text_overflow_count(2)
            .with_handle_overlap_count(0)
            .with_stale_measured_regions(0)
            .with_repeatable_anchor_coverage(3, 3),
        );
        report.mark_invalid_hover_bounds_checked(true);
        report.mark_dropped_wire_menu_bounds_checked(true);
        report.mark_repeatable_edit_updates_anchors(true);
        report.mark_edge_endpoints_follow_measured_handles(true);

        let result =
            std::panic::catch_unwind(|| assert_host_visual_interaction_report_gates(&report));
        assert!(
            result.is_err(),
            "visual gate must fail when product content is present but below readable budget"
        );
    }
}
