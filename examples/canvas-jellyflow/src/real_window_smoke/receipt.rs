use open_gpui_canvas_jellyflow::smoke_receipt::{
    SMOKE_RECEIPT_SCHEMA_VERSION, SmokeBoundsV1, SmokeDemoV1, SmokeEvidenceV1, SmokeFailureV1,
    SmokeGeometryEvidenceV1, SmokeHostEvidenceV1, SmokeInteractionEvidenceV1, SmokeLodEvidenceV1,
    SmokePaintEvidenceV1, SmokeReceiptStatusV1, SmokeRunDescriptorV1, SmokeRunReceiptV1,
    SmokeScenarioV1, SmokeTopologyEvidenceV1, SmokeZoomV1,
};

use super::config::{SmokeBounds, SmokeScenario, SmokeZoom};
use super::{
    SmokeExpectations, SmokeGeometryEvidence, SmokeInputEvidence, SmokeObservation, SmokeOutcome,
    SmokePaintEvidence, SmokeScenarioEvidence, SmokeState,
};

pub(super) fn take_run_receipt(state: &mut SmokeState) -> Option<SmokeRunReceiptV1> {
    if state.receipt_emitted || state.outcome == SmokeOutcome::Pending {
        return None;
    }
    state.receipt_emitted = true;

    let run = run_descriptor(state);
    let presented_frames = state
        .last_observation
        .as_ref()
        .map_or_else(Vec::new, |observation| observation.presented_frames.clone());
    Some(match state.outcome {
        SmokeOutcome::Passed => SmokeRunReceiptV1 {
            schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
            status: SmokeReceiptStatusV1::Pass,
            run,
            failure: None,
            evidence: state
                .expectations
                .as_ref()
                .zip(state.last_observation.as_ref())
                .and_then(|(expectations, observation)| pass_evidence(expectations, observation)),
            presented_frames,
        },
        SmokeOutcome::Failed(code) => SmokeRunReceiptV1 {
            schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
            status: SmokeReceiptStatusV1::Fail,
            run,
            failure: Some(SmokeFailureV1 {
                code: code.to_owned(),
                message: failure_message(state.last_observation.as_ref()),
            }),
            evidence: None,
            presented_frames,
        },
        SmokeOutcome::Pending => unreachable!("pending outcomes do not emit receipts"),
    })
}

fn run_descriptor(state: &SmokeState) -> SmokeRunDescriptorV1 {
    let demo = match state.demo {
        "shader" => SmokeDemoV1::Shader,
        "workflow" => SmokeDemoV1::Workflow,
        other => panic!("unsupported real-window smoke demo: {other}"),
    };
    let bounds = match state.config.bounds() {
        SmokeBounds::Compact => SmokeBoundsV1::Compact,
        SmokeBounds::Normal => SmokeBoundsV1::Normal,
    };
    let scenario = match state.config.scenario() {
        SmokeScenario::Interaction => SmokeScenarioV1::Interaction,
        SmokeScenario::LodThreshold => SmokeScenarioV1::LodThreshold,
    };
    let target_zoom = match state.config.zoom() {
        SmokeZoom::Half => SmokeZoomV1::Half,
        SmokeZoom::One => SmokeZoomV1::One,
        SmokeZoom::Two => SmokeZoomV1::Two,
    };
    let mut run = SmokeRunDescriptorV1 {
        run_id: String::new(),
        demo,
        bounds,
        scenario,
        target_zoom,
    };
    run.run_id = run.expected_run_id();
    run
}

fn pass_evidence(
    expectations: &SmokeExpectations,
    observation: &SmokeObservation,
) -> Option<SmokeEvidenceV1> {
    let geometry = observation.geometry.as_ref()?;
    let paint = observation.paint.as_ref()?;
    let input = observation.input.as_ref()?;
    let (captures, interaction, lod) = scenario_evidence(input);
    Some(SmokeEvidenceV1 {
        captures,
        initial_graph: initial_graph(expectations),
        host: host_evidence(observation),
        geometry: geometry_evidence(geometry),
        paint: paint_evidence(paint),
        interaction,
        lod,
    })
}

fn initial_graph(expectations: &SmokeExpectations) -> SmokeTopologyEvidenceV1 {
    SmokeTopologyEvidenceV1 {
        node_ids: expectations
            .node_ports
            .keys()
            .map(|id| id.0.to_string())
            .collect(),
        port_ids: expectations
            .node_ports
            .values()
            .flatten()
            .map(|id| id.0.to_string())
            .collect(),
        edge_ids: expectations
            .edges
            .iter()
            .map(|id| id.0.to_string())
            .collect(),
    }
}

fn host_evidence(observation: &SmokeObservation) -> SmokeHostEvidenceV1 {
    SmokeHostEvidenceV1 {
        visible_expected_nodes: observation.visible_expected_nodes.unwrap_or(0),
        active_nodes: observation.active_nodes,
        parked_nodes: observation.parked_nodes,
        mounts: observation.mounts,
        restores: observation.restores,
        evictions: observation.evictions,
        hard_retirements: observation.hard_retirements,
        protected_instances: observation.protected_instances,
        protected_leases: observation.protected_leases,
        protected_overflows: observation.protected_overflows,
        eligible_parked_nodes: observation.eligible_parked_nodes,
        strong_instances: observation.strong_instances,
        frame_materialization_overflows: observation.frame_materialization_overflows,
        feedback_submissions: observation.feedback_submissions,
        feedback_failures: observation.feedback_failures,
        layout_rejections: observation.layout_rejections,
        measurement_failures: observation.measurement_failures,
        native_probe_phase: observation.native_probe_phase.clone(),
        native_probe_wait_ticks: observation.native_probe_wait_ticks,
        diagnostic: observation
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.message.clone()),
    }
}

fn geometry_evidence(geometry: &SmokeGeometryEvidence) -> SmokeGeometryEvidenceV1 {
    SmokeGeometryEvidenceV1 {
        layout_revision: geometry.layout_revision,
        image_width: geometry.image_width,
        image_height: geometry.image_height,
        canvas_width: geometry.canvas_width,
        surface_width: geometry.surface_size.width,
        surface_height: geometry.surface_size.height,
        expected_node_ids: geometry
            .expected_node_ids
            .iter()
            .map(|id| id.0.to_string())
            .collect(),
        visible_node_ids: geometry
            .node_regions
            .keys()
            .map(|id| id.0.to_string())
            .collect(),
        fresh_nodes: geometry.fresh_nodes,
        expected_port_ids: geometry
            .expected_port_ids
            .iter()
            .map(|id| id.0.to_string())
            .collect(),
        rendered_ports: geometry.rendered_ports,
        expected_edge_ids: geometry
            .expected_edge_ids
            .iter()
            .map(|id| id.0.to_string())
            .collect(),
        routed_edges: geometry.routed_edges,
        endpoint_geometry_verified: geometry.endpoint_geometry_verified,
        offscreen_endpoint_fallbacks: geometry.offscreen_endpoint_fallbacks,
    }
}

fn paint_evidence(paint: &SmokePaintEvidence) -> SmokePaintEvidenceV1 {
    SmokePaintEvidenceV1 {
        image_width: paint.image_width,
        image_height: paint.image_height,
        canvas_width: paint.canvas_width,
        non_background_pixels: paint.non_background_pixels,
        distinct_colors: paint.distinct_colors,
        painted_node_ids: paint.nodes.keys().map(|id| id.0.to_string()).collect(),
        painted_edge_ids: paint.wires.keys().map(|id| id.0.to_string()).collect(),
    }
}

fn scenario_evidence(
    input: &SmokeInputEvidence,
) -> (
    Vec<String>,
    Option<SmokeInteractionEvidenceV1>,
    Option<SmokeLodEvidenceV1>,
) {
    match &input.scenario {
        SmokeScenarioEvidence::Interaction(evidence) => (
            evidence
                .captures
                .iter()
                .map(|capture| capture.file_label().to_owned())
                .collect(),
            Some(SmokeInteractionEvidenceV1 {
                dispatched_events: evidence.dispatched_events,
                auto_pan_reframes: evidence.auto_pan_reframes,
                node_dragged: evidence.node_dragged,
                selection_chrome_fitted: evidence.selection_chrome_fitted,
                selection_chrome_sides: evidence.selection_chrome_sides,
                selection_chrome_detached_run: evidence.selection_chrome_detached_run,
                auto_pan_frames: evidence.auto_pan_frames,
                connection_auto_pan_frames: evidence.connection_auto_pan_frames,
                tab_focus_moved: evidence.tab_focus_moved,
                tab_focus_restored: evidence.tab_focus_restored,
                viewport_panned: evidence.viewport_panned,
                viewport_zoomed: evidence.viewport_zoomed,
                connection_created: evidence.connection_created,
                connection_preview_wire_painted: evidence.connection_preview_wire_painted,
                connection_release_frame_painted: evidence.connection_release_frame_painted,
                connection_release_continuous: evidence.connection_release_continuous,
                keyboard_edge_deleted: evidence.keyboard_edge_deleted,
                reconnect_applied: evidence.reconnect_applied,
                reconnect_preview_wire_painted: evidence.reconnect_preview_wire_painted,
                reconnect_release_frame_painted: evidence.reconnect_release_frame_painted,
                reconnect_release_continuous: evidence.reconnect_release_continuous,
                search_opened: evidence.search_opened,
                search_focus_moved: evidence.search_focus_moved,
                search_cancelled: evidence.search_cancelled,
                search_focus_restored: evidence.search_focus_restored,
            }),
            None,
        ),
        SmokeScenarioEvidence::LodThreshold(evidence) => (
            evidence
                .captures
                .iter()
                .map(|capture| capture.file_label().to_owned())
                .collect(),
            None,
            Some(SmokeLodEvidenceV1 {
                continuous: evidence.lod_continuous,
                max_jump_in_view: evidence.lod_max_jump_in_view,
            }),
        ),
    }
}

fn failure_message(observation: Option<&SmokeObservation>) -> Option<String> {
    observation.and_then(|observation| {
        observation
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.message.clone())
            .or_else(|| observation.native_probe_phase.clone())
    })
}
