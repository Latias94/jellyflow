//! Versioned contract for real-window smoke evidence.
//!
//! The native smoke process owns this semantic contract.  External runners only transport the
//! JSON payload to the contract binary and persist its canonical output.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use schemars::JsonSchema;
use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use thiserror::Error;
use uuid::Uuid;

pub const SMOKE_RECEIPT_SCHEMA_VERSION: u16 = 1;
pub const SMOKE_RECEIPT_PREFIX: &str = "JELLYFLOW_REAL_WINDOW_SMOKE_RECEIPT ";

const INTERACTION_CAPTURES: [&str; 9] = [
    "initial",
    "selected",
    "connection-preview",
    "connection-committed",
    "reconnect-selected",
    "reconnect-preview",
    "reconnect-committed",
    "search-open",
    "final",
];
const LOD_CAPTURES: [&str; 3] = ["lod-0.86", "lod-0.85", "lod-0.84"];

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmokeReceiptStatusV1 {
    Pass,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmokeDemoV1 {
    Shader,
    Workflow,
}

impl SmokeDemoV1 {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Shader => "shader",
            Self::Workflow => "workflow",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmokeBoundsV1 {
    Compact,
    Normal,
}

impl SmokeBoundsV1 {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmokeScenarioV1 {
    Interaction,
    LodThreshold,
}

impl SmokeScenarioV1 {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Interaction => "interaction",
            Self::LodThreshold => "lod-threshold",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum SmokeZoomV1 {
    #[serde(rename = "0.5")]
    Half,
    #[serde(rename = "1")]
    One,
    #[serde(rename = "2")]
    Two,
}

impl SmokeZoomV1 {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Half => "0.5",
            Self::One => "1",
            Self::Two => "2",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeRunDescriptorV1 {
    pub run_id: String,
    pub demo: SmokeDemoV1,
    pub bounds: SmokeBoundsV1,
    pub scenario: SmokeScenarioV1,
    pub target_zoom: SmokeZoomV1,
}

impl SmokeRunDescriptorV1 {
    #[must_use]
    pub fn expected_run_id(&self) -> String {
        format!(
            "{}-{}-{}-z{}",
            self.scenario.label(),
            self.demo.label(),
            self.bounds.label(),
            self.target_zoom.label()
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeFailureV1 {
    pub code: String,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokePointV1 {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeViewportV1 {
    pub pan: SmokePointV1,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeRectV1 {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeNodeBoundsV1 {
    pub node_id: String,
    pub bounds: SmokeRectV1,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeRouteV1 {
    pub edge_id: String,
    pub source: SmokePointV1,
    pub target: SmokePointV1,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokePreviewTargetV1 {
    pub port_id: String,
    pub anchor: SmokePointV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmokeFrameStageV1 {
    Interaction,
    NodeDragPending,
    NodeDragAuthoritative,
    ConnectionExploring,
    ConnectionPending,
    ConnectionAuthoritative,
    ReconnectExploring,
    ReconnectPending,
    ReconnectAuthoritative,
    Search,
    Lod,
    Final,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokePresentedFrameV1 {
    /// Contiguous ordinal assigned by the continuously registered native frame callback.
    pub presented_sequence: u64,
    /// Monotonic identifier assigned by the host while building a native scene render.
    ///
    /// A platform frame may build the scene more than once, so this is diagnostic evidence rather
    /// than the presented-frame identity and may advance by more than one.
    pub render_sequence: u64,
    pub stage: SmokeFrameStageV1,
    pub phase: String,
    pub graph_revision: u64,
    pub viewport: SmokeViewportV1,
    pub node_bounds: Vec<SmokeNodeBoundsV1>,
    pub chrome_bounds: Vec<SmokeNodeBoundsV1>,
    pub preview_route: Option<SmokeRouteV1>,
    pub preview_target: Option<SmokePreviewTargetV1>,
    pub suppressed_edge_id: Option<String>,
    pub authoritative_routes: Vec<SmokeRouteV1>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeGeometryEvidenceV1 {
    pub layout_revision: u64,
    pub image_width: u32,
    pub image_height: u32,
    pub canvas_width: u32,
    pub surface_width: f32,
    pub surface_height: f32,
    pub expected_node_ids: Vec<String>,
    pub visible_node_ids: Vec<String>,
    pub fresh_nodes: usize,
    pub expected_port_ids: Vec<String>,
    pub rendered_ports: usize,
    pub expected_edge_ids: Vec<String>,
    pub routed_edges: usize,
    pub endpoint_geometry_verified: bool,
    pub offscreen_endpoint_fallbacks: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokePaintEvidenceV1 {
    pub image_width: u32,
    pub image_height: u32,
    pub canvas_width: u32,
    pub non_background_pixels: usize,
    pub distinct_colors: usize,
    pub painted_node_ids: Vec<String>,
    pub painted_edge_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeHostEvidenceV1 {
    pub visible_expected_nodes: usize,
    pub active_nodes: usize,
    pub parked_nodes: usize,
    pub mounts: u64,
    pub restores: u64,
    pub evictions: u64,
    pub hard_retirements: u64,
    pub protected_instances: usize,
    pub protected_leases: usize,
    pub protected_overflows: u64,
    pub eligible_parked_nodes: usize,
    pub strong_instances: usize,
    pub frame_materialization_overflows: u64,
    pub feedback_submissions: u64,
    pub feedback_failures: u64,
    pub layout_rejections: u64,
    pub measurement_failures: u64,
    pub native_probe_phase: Option<String>,
    pub native_probe_wait_ticks: Option<u16>,
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeTopologyEvidenceV1 {
    pub node_ids: Vec<String>,
    pub port_ids: Vec<String>,
    pub edge_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeInteractionEvidenceV1 {
    pub dispatched_events: u16,
    pub auto_pan_reframes: u16,
    pub node_dragged: bool,
    pub selection_chrome_fitted: bool,
    pub selection_chrome_sides: u8,
    pub selection_chrome_detached_run: usize,
    pub auto_pan_frames: u8,
    pub connection_auto_pan_frames: u8,
    pub tab_focus_moved: bool,
    pub tab_focus_restored: bool,
    pub viewport_panned: bool,
    pub viewport_zoomed: bool,
    pub connection_created: bool,
    pub connection_preview_wire_painted: bool,
    pub connection_release_frame_painted: bool,
    pub connection_release_continuous: bool,
    pub keyboard_edge_deleted: bool,
    pub reconnect_applied: bool,
    pub reconnect_preview_wire_painted: bool,
    pub reconnect_release_frame_painted: bool,
    pub reconnect_release_continuous: bool,
    pub search_opened: bool,
    pub search_focus_moved: bool,
    pub search_cancelled: bool,
    pub search_focus_restored: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeLodEvidenceV1 {
    pub continuous: bool,
    pub max_jump_in_view: f32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeEvidenceV1 {
    pub captures: Vec<String>,
    pub initial_graph: SmokeTopologyEvidenceV1,
    pub host: SmokeHostEvidenceV1,
    pub geometry: SmokeGeometryEvidenceV1,
    pub paint: SmokePaintEvidenceV1,
    pub interaction: Option<SmokeInteractionEvidenceV1>,
    pub lod: Option<SmokeLodEvidenceV1>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmokeRunReceiptV1 {
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u16,
    pub status: SmokeReceiptStatusV1,
    pub run: SmokeRunDescriptorV1,
    pub failure: Option<SmokeFailureV1>,
    pub evidence: Option<SmokeEvidenceV1>,
    pub presented_frames: Vec<SmokePresentedFrameV1>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SmokeReceiptValidationError {
    #[error("{0}")]
    Invalid(String),
}

impl SmokeRunReceiptV1 {
    /// Decodes one unambiguous JSON receipt before applying the typed contract.
    ///
    /// `serde_json::Value` normally keeps the last value for a duplicate object key. The smoke
    /// contract rejects duplicates recursively so canonicalization cannot hide an ambiguous
    /// producer payload.
    pub fn from_json_slice(input: &[u8]) -> serde_json::Result<Self> {
        let mut deserializer = serde_json::Deserializer::from_slice(input);
        let value = StrictJsonValueSeed.deserialize(&mut deserializer)?;
        deserializer.end()?;
        serde_json::from_value(value)
    }

    pub fn validate(&self) -> Result<(), SmokeReceiptValidationError> {
        if self.schema_version != SMOKE_RECEIPT_SCHEMA_VERSION {
            return invalid(format!(
                "schema_version is {}, expected {}",
                self.schema_version, SMOKE_RECEIPT_SCHEMA_VERSION
            ));
        }
        if self.run.run_id != self.run.expected_run_id() {
            return invalid(format!(
                "run_id is {:?}, expected {:?}",
                self.run.run_id,
                self.run.expected_run_id()
            ));
        }
        let complete = validate_status(self)?;
        if let Some(evidence) = &self.evidence {
            validate_evidence(evidence, &self.run.scenario)?;
        }
        validate_presented_frames(&self.presented_frames, &self.run.scenario, complete)?;
        Ok(())
    }
}

struct StrictJsonValueSeed;

impl<'de> DeserializeSeed<'de> for StrictJsonValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictJsonValueVisitor)
    }
}

struct StrictJsonValueVisitor;

impl<'de> Visitor<'de> for StrictJsonValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an unambiguous JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        StrictJsonValueSeed.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or_default());
        while let Some(value) = sequence.next_element_seed(StrictJsonValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom(format!(
                    "JSON object contains duplicate key {key:?}"
                )));
            }
            let value = object.next_value_seed(StrictJsonValueSeed)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn validate_status(receipt: &SmokeRunReceiptV1) -> Result<bool, SmokeReceiptValidationError> {
    match (&receipt.status, &receipt.failure, &receipt.evidence) {
        (SmokeReceiptStatusV1::Pass, None, Some(_)) => Ok(true),
        (SmokeReceiptStatusV1::Pass, Some(_), _) => invalid("pass receipt includes a failure"),
        (SmokeReceiptStatusV1::Pass, None, None) => invalid("pass receipt omits evidence"),
        (SmokeReceiptStatusV1::Fail, Some(failure), None) => {
            if valid_failure_code(&failure.code) {
                Ok(false)
            } else {
                invalid("failure.code must be a non-empty snake_case identifier")
            }
        }
        (SmokeReceiptStatusV1::Fail, Some(_), Some(_)) => {
            invalid("fail receipt must not include completed evidence")
        }
        (SmokeReceiptStatusV1::Fail, None, _) => invalid("fail receipt omits failure"),
    }
}

fn validate_evidence(
    evidence: &SmokeEvidenceV1,
    scenario: &SmokeScenarioV1,
) -> Result<(), SmokeReceiptValidationError> {
    let expected_captures: &[&str] = match scenario {
        SmokeScenarioV1::Interaction => &INTERACTION_CAPTURES,
        SmokeScenarioV1::LodThreshold => &LOD_CAPTURES,
    };
    if evidence
        .captures
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_captures
    {
        return invalid(format!(
            "captures are {:?}, expected {:?}",
            evidence.captures, expected_captures
        ));
    }
    validate_topology(&evidence.initial_graph)?;
    validate_host(&evidence.host, &evidence.initial_graph)?;
    validate_geometry(&evidence.geometry)?;
    validate_paint(&evidence.paint)?;
    validate_geometry_paint_consistency(
        &evidence.initial_graph,
        &evidence.host,
        &evidence.geometry,
        &evidence.paint,
    )?;
    match (scenario, &evidence.interaction, &evidence.lod) {
        (SmokeScenarioV1::Interaction, Some(interaction), None) => {
            validate_interaction(interaction)
        }
        (SmokeScenarioV1::LodThreshold, None, Some(lod)) => validate_lod(lod),
        (SmokeScenarioV1::Interaction, _, _) => {
            invalid("interaction receipt must contain interaction evidence only")
        }
        (SmokeScenarioV1::LodThreshold, _, _) => {
            invalid("LOD receipt must contain LOD evidence only")
        }
    }
}

fn validate_topology(
    topology: &SmokeTopologyEvidenceV1,
) -> Result<(), SmokeReceiptValidationError> {
    validate_identifier_set(&topology.node_ids, "initial_graph.node_ids")?;
    validate_identifier_set(&topology.port_ids, "initial_graph.port_ids")?;
    validate_identifier_set(&topology.edge_ids, "initial_graph.edge_ids")
}

fn validate_host(
    host: &SmokeHostEvidenceV1,
    topology: &SmokeTopologyEvidenceV1,
) -> Result<(), SmokeReceiptValidationError> {
    let node_count = topology.node_ids.len();
    if host.visible_expected_nodes > node_count
        || host.active_nodes.saturating_add(host.parked_nodes) != node_count
        || host.mounts > node_count as u64
        || host.protected_instances > host.active_nodes
        || host.eligible_parked_nodes > host.parked_nodes
        || host.strong_instances != node_count
        || host.evictions > 0
        || host.hard_retirements > 0
        || host.protected_overflows > 0
        || host.frame_materialization_overflows > 0
        || host.feedback_submissions < 2
        || host.feedback_failures > 0
        || host.layout_rejections > 0
        || host.measurement_failures > 0
    {
        return invalid("host resource evidence is inconsistent with the initial graph");
    }
    Ok(())
}

fn validate_geometry(
    geometry: &SmokeGeometryEvidenceV1,
) -> Result<(), SmokeReceiptValidationError> {
    if geometry.layout_revision == 0
        || geometry.image_width == 0
        || geometry.image_height == 0
        || geometry.canvas_width == 0
        || geometry.canvas_width > geometry.image_width
        || !positive_finite(geometry.surface_width)
        || !positive_finite(geometry.surface_height)
    {
        return invalid("geometry dimensions or revision are invalid");
    }
    validate_identifier_set(&geometry.expected_node_ids, "expected_node_ids")?;
    validate_optional_identifier_set(&geometry.visible_node_ids, "visible_node_ids")?;
    validate_identifier_set(&geometry.expected_port_ids, "expected_port_ids")?;
    validate_identifier_set(&geometry.expected_edge_ids, "expected_edge_ids")?;
    if geometry.fresh_nodes > geometry.expected_node_ids.len()
        || geometry.rendered_ports > geometry.expected_port_ids.len()
        || geometry.routed_edges > geometry.expected_edge_ids.len()
        || !geometry.endpoint_geometry_verified
    {
        return invalid("geometry counts or endpoint verification are invalid");
    }
    Ok(())
}

fn validate_paint(paint: &SmokePaintEvidenceV1) -> Result<(), SmokeReceiptValidationError> {
    if paint.image_width == 0
        || paint.image_height == 0
        || paint.canvas_width == 0
        || paint.canvas_width > paint.image_width
        || paint.non_background_pixels == 0
        || paint.distinct_colors == 0
    {
        return invalid("paint evidence is empty or has invalid dimensions");
    }
    validate_optional_identifier_set(&paint.painted_node_ids, "painted_node_ids")?;
    validate_optional_identifier_set(&paint.painted_edge_ids, "painted_edge_ids")?;
    if paint.painted_node_ids.is_empty() && paint.painted_edge_ids.is_empty() {
        return invalid("paint evidence contains neither nodes nor routes");
    }
    Ok(())
}

fn validate_geometry_paint_consistency(
    topology: &SmokeTopologyEvidenceV1,
    host: &SmokeHostEvidenceV1,
    geometry: &SmokeGeometryEvidenceV1,
    paint: &SmokePaintEvidenceV1,
) -> Result<(), SmokeReceiptValidationError> {
    let initial_nodes = topology.node_ids.iter().collect::<BTreeSet<_>>();
    let initial_ports = topology.port_ids.iter().collect::<BTreeSet<_>>();
    let expected_nodes = geometry.expected_node_ids.iter().collect::<BTreeSet<_>>();
    let expected_ports = geometry.expected_port_ids.iter().collect::<BTreeSet<_>>();
    let expected_edges = geometry.expected_edge_ids.iter().collect::<BTreeSet<_>>();
    let visible_nodes = geometry.visible_node_ids.iter().collect::<BTreeSet<_>>();
    let painted_nodes = paint.painted_node_ids.iter().collect::<BTreeSet<_>>();
    if geometry.image_width != paint.image_width
        || geometry.image_height != paint.image_height
        || geometry.canvas_width != paint.canvas_width
        || expected_nodes != initial_nodes
        || expected_ports != initial_ports
        || geometry.visible_node_ids.len() != host.visible_expected_nodes
        || !visible_nodes.is_subset(&expected_nodes)
        || geometry.fresh_nodes < host.visible_expected_nodes
        || paint.painted_node_ids.len() > geometry.fresh_nodes
        || paint.painted_edge_ids.len() > geometry.routed_edges
        || painted_nodes != visible_nodes
        || paint
            .painted_node_ids
            .iter()
            .any(|node| !expected_nodes.contains(node))
        || paint
            .painted_edge_ids
            .iter()
            .any(|edge| !expected_edges.contains(edge))
    {
        return invalid("geometry and paint evidence disagree");
    }
    Ok(())
}

fn validate_interaction(
    interaction: &SmokeInteractionEvidenceV1,
) -> Result<(), SmokeReceiptValidationError> {
    if ![
        interaction.node_dragged,
        interaction.selection_chrome_fitted,
        interaction.tab_focus_moved,
        interaction.tab_focus_restored,
        interaction.viewport_panned,
        interaction.viewport_zoomed,
        interaction.connection_created,
        interaction.connection_preview_wire_painted,
        interaction.connection_release_frame_painted,
        interaction.connection_release_continuous,
        interaction.keyboard_edge_deleted,
        interaction.reconnect_applied,
        interaction.reconnect_preview_wire_painted,
        interaction.reconnect_release_frame_painted,
        interaction.reconnect_release_continuous,
        interaction.search_opened,
        interaction.search_focus_moved,
        interaction.search_cancelled,
        interaction.search_focus_restored,
    ]
    .into_iter()
    .all(|complete| complete)
    {
        return invalid("interaction receipt has an incomplete required assertion");
    }
    if interaction.dispatched_events != 36
        || interaction.selection_chrome_sides != 4
        || interaction.auto_pan_frames < 3
        || interaction.connection_auto_pan_frames < 3
    {
        return invalid("interaction counters or selection chrome evidence are invalid");
    }
    Ok(())
}

fn validate_lod(lod: &SmokeLodEvidenceV1) -> Result<(), SmokeReceiptValidationError> {
    if !lod.continuous || !lod.max_jump_in_view.is_finite() || lod.max_jump_in_view < 0.0 {
        return invalid("LOD evidence is invalid");
    }
    Ok(())
}

fn validate_presented_frames(
    frames: &[SmokePresentedFrameV1],
    scenario: &SmokeScenarioV1,
    require_complete: bool,
) -> Result<(), SmokeReceiptValidationError> {
    if frames.is_empty() {
        return if require_complete {
            invalid("receipt omits presented frame evidence")
        } else {
            Ok(())
        };
    }
    let mut previous_revision = 0;
    let mut previous_presented_sequence: Option<u64> = None;
    let mut previous_render_sequence: Option<u64> = None;
    let mut connection_authoritative = false;
    let mut reconnect_authoritative = false;
    let mut saw_connection_pending = false;
    let mut saw_connection_authoritative = false;
    let mut saw_reconnect_pending = false;
    let mut saw_reconnect_authoritative = false;
    let mut saw_node_drag_pending = false;
    let mut saw_node_drag_authoritative = false;
    let mut saw_chrome = false;
    let mut saw_search = false;
    let mut saw_final = false;
    let mut lod_frames = 0usize;
    let mut connection_pending_routes: Option<PendingRouteSwap> = None;
    let mut reconnect_pending_routes: Option<PendingRouteSwap> = None;
    let mut node_drag_pending: Option<PendingNodeDrag> = None;
    for (index, frame) in frames.iter().enumerate() {
        let invalid_presented_sequence = match previous_presented_sequence {
            Some(previous) => frame.presented_sequence != previous.saturating_add(1),
            None => frame.presented_sequence != 0,
        };
        let invalid_render_sequence =
            previous_render_sequence.is_some_and(|previous| frame.render_sequence < previous);
        if frame.phase.is_empty()
            || frame.graph_revision < previous_revision
            || invalid_presented_sequence
            || invalid_render_sequence
            || !finite_point(frame.viewport.pan)
            || !positive_finite(frame.viewport.zoom)
        {
            return invalid("presented frame phase or graph revision is invalid");
        }
        previous_revision = frame.graph_revision;
        previous_presented_sequence = Some(frame.presented_sequence);
        previous_render_sequence = Some(frame.render_sequence);
        if !matches!(
            frame.stage,
            SmokeFrameStageV1::ConnectionPending | SmokeFrameStageV1::ConnectionAuthoritative
        ) && let Some(pending) = connection_pending_routes.take()
            && pending.committing
        {
            return invalid("committing connection frame span was interrupted");
        }
        if !matches!(
            frame.stage,
            SmokeFrameStageV1::ReconnectPending | SmokeFrameStageV1::ReconnectAuthoritative
        ) && let Some(pending) = reconnect_pending_routes.take()
            && pending.committing
        {
            return invalid("committing reconnect frame span was interrupted");
        }
        if node_drag_pending.is_some()
            && !matches!(
                frame.stage,
                SmokeFrameStageV1::NodeDragPending | SmokeFrameStageV1::NodeDragAuthoritative
            )
        {
            return invalid("node drag pending-to-authoritative frame span was interrupted");
        }
        validate_node_bounds(&frame.node_bounds, "node_bounds")?;
        validate_node_bounds(&frame.chrome_bounds, "chrome_bounds")?;
        validate_chrome_alignment(&frame.node_bounds, &frame.chrome_bounds)?;
        saw_chrome |= !frame.chrome_bounds.is_empty();
        validate_routes(&frame.authoritative_routes, "authoritative_routes")?;
        if let Some(route) = &frame.preview_route {
            validate_route(route, "preview_route")?;
        }
        if let Some(target) = &frame.preview_target
            && (!canonical_uuid(&target.port_id) || !finite_point(target.anchor))
        {
            return invalid("preview target is invalid");
        }
        match frame.stage {
            SmokeFrameStageV1::NodeDragPending => {
                validate_authoritative_only(frame)?;
                node_drag_pending = Some(PendingNodeDrag::from_frame(frame)?);
                saw_node_drag_pending = true;
            }
            SmokeFrameStageV1::NodeDragAuthoritative => {
                validate_authoritative_only(frame)?;
                let pending = node_drag_pending.take();
                if saw_node_drag_authoritative
                    || !pending.is_some_and(|pending| pending.aligns_with(frame))
                {
                    return invalid("node drag authoritative frame jumps from pending geometry");
                }
                saw_node_drag_authoritative = true;
            }
            SmokeFrameStageV1::ConnectionExploring => {
                validate_exploring_frame(frame, false)?;
            }
            SmokeFrameStageV1::ConnectionPending => {
                if connection_authoritative
                    || frame.preview_route.is_none()
                    || frame.preview_target.is_none()
                    || frame.suppressed_edge_id.is_some()
                {
                    return invalid("connection pending frame lacks a continuous preview route");
                }
                validate_preview_target_alignment(frame)?;
                match connection_pending_routes.as_mut() {
                    Some(pending) => pending.observe(frame, None)?,
                    None => {
                        connection_pending_routes =
                            Some(PendingRouteSwap::from_frame(frame, None)?);
                    }
                }
                saw_connection_pending = true;
            }
            SmokeFrameStageV1::ConnectionAuthoritative => {
                let pending = connection_pending_routes.take();
                if frame.preview_route.is_some()
                    || frame.preview_target.is_some()
                    || frame.suppressed_edge_id.is_some()
                    || !atomic_route_addition(pending.as_ref(), frame)
                {
                    return invalid("connection authoritative frame is not an atomic route swap");
                }
                connection_authoritative = true;
                saw_connection_authoritative = true;
            }
            SmokeFrameStageV1::ReconnectExploring => {
                validate_exploring_frame(frame, true)?;
            }
            SmokeFrameStageV1::ReconnectPending => {
                if reconnect_authoritative
                    || frame.preview_route.is_none()
                    || frame.preview_target.is_none()
                    || frame
                        .suppressed_edge_id
                        .as_deref()
                        .is_none_or(|edge| !canonical_uuid(edge))
                {
                    return invalid("reconnect pending frame lacks a continuous preview route");
                }
                validate_preview_target_alignment(frame)?;
                let expected_edge_id = frame.suppressed_edge_id.clone();
                match reconnect_pending_routes.as_mut() {
                    Some(pending) => pending.observe(frame, expected_edge_id)?,
                    None => {
                        reconnect_pending_routes =
                            Some(PendingRouteSwap::from_frame(frame, expected_edge_id)?);
                    }
                }
                saw_reconnect_pending = true;
            }
            SmokeFrameStageV1::ReconnectAuthoritative => {
                let pending = reconnect_pending_routes.take();
                if frame.preview_route.is_some()
                    || frame.preview_target.is_some()
                    || frame.suppressed_edge_id.is_some()
                    || !atomic_route_addition(pending.as_ref(), frame)
                {
                    return invalid("reconnect authoritative frame is not an atomic route swap");
                }
                reconnect_authoritative = true;
                saw_reconnect_authoritative = true;
            }
            SmokeFrameStageV1::Search => {
                if frame.suppressed_edge_id.is_some() || frame.preview_target.is_some() {
                    return invalid("search frame contains an active target or suppressed edge");
                }
                saw_search = true;
            }
            SmokeFrameStageV1::Final => {
                validate_authoritative_only(frame)?;
                if index + 1 != frames.len() {
                    return invalid("final presented frame is not the last observed frame");
                }
                saw_final = true;
            }
            SmokeFrameStageV1::Lod => {
                validate_authoritative_only(frame)?;
                lod_frames = lod_frames.saturating_add(1);
            }
            SmokeFrameStageV1::Interaction => validate_authoritative_only(frame)?,
        }
    }
    if !require_complete {
        return Ok(());
    }
    match scenario {
        SmokeScenarioV1::Interaction
            if !saw_connection_pending
                || !saw_connection_authoritative
                || !saw_reconnect_pending
                || !saw_reconnect_authoritative
                || !saw_node_drag_pending
                || !saw_node_drag_authoritative
                || !saw_chrome
                || !saw_search
                || !saw_final =>
        {
            invalid("interaction receipt omits required continuous-frame evidence")
        }
        SmokeScenarioV1::LodThreshold if lod_frames < LOD_CAPTURES.len() => {
            invalid("LOD receipt omits LOD presented frames")
        }
        SmokeScenarioV1::Interaction | SmokeScenarioV1::LodThreshold => Ok(()),
    }
}

#[derive(Clone)]
struct PendingRouteSwap {
    routes: BTreeMap<String, SmokeRouteV1>,
    preview: SmokeRouteV1,
    target: SmokePreviewTargetV1,
    expected_edge_id: Option<String>,
    graph_revision: u64,
    zoom: f32,
    committing: bool,
}

impl PendingRouteSwap {
    fn from_frame(
        frame: &SmokePresentedFrameV1,
        expected_edge_id: Option<String>,
    ) -> Result<Self, SmokeReceiptValidationError> {
        let routes = frame
            .authoritative_routes
            .iter()
            .map(|route| {
                canvas_route(route, frame.viewport).map(|route| (route.edge_id.clone(), route))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .ok_or_else(|| {
                SmokeReceiptValidationError::Invalid(
                    "pending route viewport projection is invalid".to_owned(),
                )
            })?;
        if expected_edge_id
            .as_ref()
            .is_some_and(|edge| routes.contains_key(edge))
        {
            return invalid("pending reconnect frame did not suppress its operation edge");
        }
        let preview = canvas_route(
            frame
                .preview_route
                .as_ref()
                .expect("pending frame preview presence was validated"),
            frame.viewport,
        )
        .ok_or_else(|| {
            SmokeReceiptValidationError::Invalid(
                "pending preview viewport projection is invalid".to_owned(),
            )
        })?;
        let target = frame
            .preview_target
            .as_ref()
            .expect("pending frame target presence was validated");
        let target = SmokePreviewTargetV1 {
            port_id: target.port_id.clone(),
            anchor: canvas_point(target.anchor, frame.viewport).ok_or_else(|| {
                SmokeReceiptValidationError::Invalid(
                    "pending target viewport projection is invalid".to_owned(),
                )
            })?,
        };
        Ok(Self {
            routes,
            preview,
            target,
            expected_edge_id,
            graph_revision: frame.graph_revision,
            zoom: frame.viewport.zoom,
            committing: frame.phase == "Committing",
        })
    }

    fn observe(
        &mut self,
        frame: &SmokePresentedFrameV1,
        expected_edge_id: Option<String>,
    ) -> Result<(), SmokeReceiptValidationError> {
        let current = Self::from_frame(frame, expected_edge_id)?;
        if self.committing && !current.committing {
            return invalid("committing pending route regressed to an editable frame");
        }
        if self.expected_edge_id != current.expected_edge_id
            || self.target.port_id != current.target.port_id
        {
            if self.committing {
                return invalid("committing pending route changed target identity");
            }
            *self = current;
            return Ok(());
        }
        let zoom = self.zoom.max(current.zoom);
        if !points_align_at_zoom(self.target.anchor, current.target.anchor, zoom)
            || !route_endpoints_align_at_zoom(&self.preview, &current.preview, zoom)
            || !route_maps_overlap_align_at_zoom(&self.routes, &current.routes, zoom)
        {
            return invalid("pending route or target changed between presented frames");
        }
        *self = current;
        Ok(())
    }
}

fn atomic_route_addition(
    pending: Option<&PendingRouteSwap>,
    frame: &SmokePresentedFrameV1,
) -> bool {
    let Some(pending) = pending else {
        return false;
    };
    if frame.graph_revision <= pending.graph_revision {
        return false;
    }
    let Some(authoritative_routes) = frame
        .authoritative_routes
        .iter()
        .map(|route| {
            canvas_route(route, frame.viewport).map(|route| (route.edge_id.clone(), route))
        })
        .collect::<Option<BTreeMap<_, _>>>()
    else {
        return false;
    };
    let zoom = pending.zoom.max(frame.viewport.zoom);
    if !route_maps_overlap_align_at_zoom(&pending.routes, &authoritative_routes, zoom) {
        return false;
    }
    let operation_routes = authoritative_routes
        .values()
        .filter(|route| !pending.routes.contains_key(&route.edge_id))
        .filter(|route| {
            pending
                .expected_edge_id
                .as_ref()
                .is_none_or(|expected| route.edge_id == *expected)
        })
        .filter(|route| route_endpoints_align_at_zoom(&pending.preview, route, zoom))
        .filter(|route| {
            points_align_at_zoom(pending.target.anchor, route.source, zoom)
                || points_align_at_zoom(pending.target.anchor, route.target, zoom)
        })
        .count();
    operation_routes == 1
}

fn route_maps_align_at_zoom(
    left: &BTreeMap<String, SmokeRouteV1>,
    right: &BTreeMap<String, SmokeRouteV1>,
    zoom: f32,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(edge, route)| {
            right
                .get(edge)
                .is_some_and(|current| route_endpoints_align_at_zoom(route, current, zoom))
        })
}

fn route_maps_overlap_align_at_zoom(
    left: &BTreeMap<String, SmokeRouteV1>,
    right: &BTreeMap<String, SmokeRouteV1>,
    zoom: f32,
) -> bool {
    left.iter().all(|(edge, route)| {
        right
            .get(edge)
            .is_none_or(|current| route_endpoints_align_at_zoom(route, current, zoom))
    })
}

fn route_endpoints_align_at_zoom(left: &SmokeRouteV1, right: &SmokeRouteV1, zoom: f32) -> bool {
    (points_align_at_zoom(left.source, right.source, zoom)
        && points_align_at_zoom(left.target, right.target, zoom))
        || (points_align_at_zoom(left.source, right.target, zoom)
            && points_align_at_zoom(left.target, right.source, zoom))
}

fn points_align_at_zoom(left: SmokePointV1, right: SmokePointV1, zoom: f32) -> bool {
    (left.x - right.x).hypot(left.y - right.y) * zoom <= 0.5
}

fn validate_authoritative_only(
    frame: &SmokePresentedFrameV1,
) -> Result<(), SmokeReceiptValidationError> {
    if frame.preview_route.is_some()
        || frame.preview_target.is_some()
        || frame.suppressed_edge_id.is_some()
    {
        return invalid("authoritative-only frame contains preview state");
    }
    Ok(())
}

fn validate_exploring_frame(
    frame: &SmokePresentedFrameV1,
    reconnect: bool,
) -> Result<(), SmokeReceiptValidationError> {
    if frame.preview_route.is_none() || frame.preview_target.is_some() {
        return invalid("exploring frame has invalid preview state");
    }
    match (reconnect, frame.suppressed_edge_id.as_deref()) {
        (false, None) => Ok(()),
        (true, Some(edge)) if canonical_uuid(edge) => {
            if frame
                .authoritative_routes
                .iter()
                .any(|route| route.edge_id == edge)
            {
                return invalid("reconnect exploring frame did not suppress its operation edge");
            }
            Ok(())
        }
        _ => invalid("exploring frame has invalid suppressed edge identity"),
    }
}

#[derive(Clone)]
struct PendingNodeDrag {
    node_id: String,
    node_bounds: SmokeRectV1,
    routes: BTreeMap<String, SmokeRouteV1>,
    zoom: f32,
}

impl PendingNodeDrag {
    fn from_frame(frame: &SmokePresentedFrameV1) -> Result<Self, SmokeReceiptValidationError> {
        let [chrome] = frame.chrome_bounds.as_slice() else {
            return invalid("node drag frame does not contain exactly one chrome surface");
        };
        let Some(node) = frame
            .node_bounds
            .iter()
            .find(|node| node.node_id == chrome.node_id)
        else {
            return invalid("node drag chrome does not belong to a presented node");
        };
        let Some(node_bounds) = canvas_rect(node.bounds, frame.viewport) else {
            return invalid("node drag frame viewport projection is invalid");
        };
        let Some(routes) = frame
            .authoritative_routes
            .iter()
            .map(|route| {
                canvas_route(route, frame.viewport).map(|route| (route.edge_id.clone(), route))
            })
            .collect::<Option<BTreeMap<_, _>>>()
        else {
            return invalid("node drag route viewport projection is invalid");
        };
        Ok(Self {
            node_id: node.node_id.clone(),
            node_bounds,
            routes,
            zoom: frame.viewport.zoom,
        })
    }

    fn aligns_with(&self, frame: &SmokePresentedFrameV1) -> bool {
        let Ok(current) = Self::from_frame(frame) else {
            return false;
        };
        let zoom = self.zoom.max(current.zoom);
        self.node_id == current.node_id
            && rects_align_at_zoom(self.node_bounds, current.node_bounds, zoom)
            && route_maps_align_at_zoom(&self.routes, &current.routes, zoom)
    }
}

fn canvas_route(route: &SmokeRouteV1, viewport: SmokeViewportV1) -> Option<SmokeRouteV1> {
    Some(SmokeRouteV1 {
        edge_id: route.edge_id.clone(),
        source: canvas_point(route.source, viewport)?,
        target: canvas_point(route.target, viewport)?,
    })
}

fn canvas_rect(rect: SmokeRectV1, viewport: SmokeViewportV1) -> Option<SmokeRectV1> {
    let origin = canvas_point(
        SmokePointV1 {
            x: rect.x,
            y: rect.y,
        },
        viewport,
    )?;
    let rect = SmokeRectV1 {
        x: origin.x,
        y: origin.y,
        width: rect.width / viewport.zoom,
        height: rect.height / viewport.zoom,
    };
    finite_rect(rect).then_some(rect)
}

fn canvas_point(point: SmokePointV1, viewport: SmokeViewportV1) -> Option<SmokePointV1> {
    if !finite_point(point) || !finite_point(viewport.pan) || !positive_finite(viewport.zoom) {
        return None;
    }
    let point = SmokePointV1 {
        x: point.x / viewport.zoom - viewport.pan.x,
        y: point.y / viewport.zoom - viewport.pan.y,
    };
    finite_point(point).then_some(point)
}

fn rects_align_at_zoom(left: SmokeRectV1, right: SmokeRectV1, zoom: f32) -> bool {
    [
        (left.x - right.x).abs(),
        (left.y - right.y).abs(),
        (left.width - right.width).abs(),
        (left.height - right.height).abs(),
    ]
    .into_iter()
    .all(|difference| difference * zoom <= 0.5)
}

fn validate_preview_target_alignment(
    frame: &SmokePresentedFrameV1,
) -> Result<(), SmokeReceiptValidationError> {
    let route = frame
        .preview_route
        .as_ref()
        .expect("pending frames checked preview route presence");
    let target = frame
        .preview_target
        .as_ref()
        .expect("pending frames checked preview target presence");
    let distance =
        |point: SmokePointV1| (point.x - target.anchor.x).hypot(point.y - target.anchor.y);
    if distance(route.source).min(distance(route.target)) > 0.5 {
        return invalid("preview target is more than 0.5 logical pixels from its route endpoint");
    }
    Ok(())
}

fn validate_node_bounds(
    bounds: &[SmokeNodeBoundsV1],
    field: &str,
) -> Result<(), SmokeReceiptValidationError> {
    let mut ids = BTreeSet::new();
    for entry in bounds {
        if !canonical_uuid(&entry.node_id)
            || !ids.insert(entry.node_id.as_str())
            || !finite_rect(entry.bounds)
        {
            return invalid(format!("{field} contains invalid or duplicate node bounds"));
        }
    }
    Ok(())
}

fn validate_chrome_alignment(
    nodes: &[SmokeNodeBoundsV1],
    chrome: &[SmokeNodeBoundsV1],
) -> Result<(), SmokeReceiptValidationError> {
    for chrome_entry in chrome {
        let Some(node_entry) = nodes
            .iter()
            .find(|node_entry| node_entry.node_id == chrome_entry.node_id)
        else {
            return invalid("chrome bounds do not belong to a presented node");
        };
        let node = node_entry.bounds;
        let actual = chrome_entry.bounds;
        if [
            (node.x - actual.x).abs(),
            (node.y - actual.y).abs(),
            (node.width - actual.width).abs(),
            (node.height - actual.height).abs(),
        ]
        .into_iter()
        .any(|difference| difference > 0.5)
        {
            return invalid("selection chrome is more than 0.5 logical pixels from node bounds");
        }
    }
    Ok(())
}

fn validate_routes(
    routes: &[SmokeRouteV1],
    field: &str,
) -> Result<(), SmokeReceiptValidationError> {
    let mut ids = BTreeSet::new();
    for route in routes {
        if !canonical_uuid(&route.edge_id) || !ids.insert(route.edge_id.as_str()) {
            return invalid(format!("{field} contains duplicate edge ids"));
        }
        validate_route(route, field)?;
    }
    Ok(())
}

fn validate_route(route: &SmokeRouteV1, field: &str) -> Result<(), SmokeReceiptValidationError> {
    if route.edge_id.is_empty() || !finite_point(route.source) || !finite_point(route.target) {
        return invalid(format!("{field} contains an invalid route"));
    }
    Ok(())
}

fn validate_identifier_set(
    values: &[String],
    field: &str,
) -> Result<(), SmokeReceiptValidationError> {
    if values.is_empty() || values.iter().any(|value| !canonical_uuid(value)) {
        return invalid(format!("{field} must contain canonical UUID values"));
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return invalid(format!("{field} contains duplicate values"));
    }
    Ok(())
}

fn validate_optional_identifier_set(
    values: &[String],
    field: &str,
) -> Result<(), SmokeReceiptValidationError> {
    if values.iter().any(|value| !canonical_uuid(value)) {
        return invalid(format!("{field} must contain canonical UUID values"));
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return invalid(format!("{field} contains duplicate values"));
    }
    Ok(())
}

fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|uuid| uuid.hyphenated().to_string() == value)
}

const fn finite_point(point: SmokePointV1) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

const fn finite_rect(rect: SmokeRectV1) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && positive_finite(rect.width)
        && positive_finite(rect.height)
}

const fn positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn valid_failure_code(code: &str) -> bool {
    !code.is_empty()
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn invalid<T>(message: impl Into<String>) -> Result<T, SmokeReceiptValidationError> {
    Err(SmokeReceiptValidationError::Invalid(message.into()))
}

/// Returns the complete structural JSON Schema for the current receipt version.
#[must_use]
pub fn schema_json() -> String {
    serde_json::to_string(&schemars::schema_for!(SmokeRunReceiptV1))
        .expect("the derived smoke receipt schema is serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_ID: &str = "00000000-0000-0000-0000-000000000001";
    const PORT_ID: &str = "00000000-0000-0000-0000-000000000002";
    const EDGE_ID: &str = "00000000-0000-0000-0000-000000000003";
    const OTHER_EDGE_ID: &str = "00000000-0000-0000-0000-000000000004";

    fn point(x: f32, y: f32) -> SmokePointV1 {
        SmokePointV1 { x, y }
    }

    fn rect(x: f32, y: f32) -> SmokeRectV1 {
        SmokeRectV1 {
            x,
            y,
            width: 120.0,
            height: 80.0,
        }
    }

    fn route(id: &str) -> SmokeRouteV1 {
        SmokeRouteV1 {
            edge_id: id.to_owned(),
            source: point(10.0, 20.0),
            target: point(80.0, 50.0),
        }
    }

    fn node_bounds() -> Vec<SmokeNodeBoundsV1> {
        vec![SmokeNodeBoundsV1 {
            node_id: NODE_ID.to_owned(),
            bounds: rect(10.0, 20.0),
        }]
    }

    fn frame(
        render_sequence: u64,
        stage: SmokeFrameStageV1,
        preview: bool,
        authoritative: bool,
        chrome: bool,
    ) -> SmokePresentedFrameV1 {
        SmokePresentedFrameV1 {
            presented_sequence: render_sequence,
            render_sequence,
            stage,
            phase: "fixture".to_owned(),
            graph_revision: render_sequence,
            viewport: SmokeViewportV1 {
                pan: point(0.0, 0.0),
                zoom: 1.0,
            },
            node_bounds: node_bounds(),
            chrome_bounds: chrome.then(node_bounds).unwrap_or_default(),
            preview_route: preview.then(|| route("preview")),
            preview_target: preview.then(|| SmokePreviewTargetV1 {
                port_id: PORT_ID.to_owned(),
                anchor: point(80.0, 50.0),
            }),
            suppressed_edge_id: (stage == SmokeFrameStageV1::ReconnectPending)
                .then(|| EDGE_ID.to_owned()),
            authoritative_routes: if authoritative {
                vec![route(EDGE_ID)]
            } else {
                Vec::new()
            },
        }
    }

    fn interaction() -> SmokeInteractionEvidenceV1 {
        SmokeInteractionEvidenceV1 {
            dispatched_events: 36,
            auto_pan_reframes: 0,
            node_dragged: true,
            selection_chrome_fitted: true,
            selection_chrome_sides: 4,
            selection_chrome_detached_run: 0,
            auto_pan_frames: 3,
            connection_auto_pan_frames: 3,
            tab_focus_moved: true,
            tab_focus_restored: true,
            viewport_panned: true,
            viewport_zoomed: true,
            connection_created: true,
            connection_preview_wire_painted: true,
            connection_release_frame_painted: true,
            connection_release_continuous: true,
            keyboard_edge_deleted: true,
            reconnect_applied: true,
            reconnect_preview_wire_painted: true,
            reconnect_release_frame_painted: true,
            reconnect_release_continuous: true,
            search_opened: true,
            search_focus_moved: true,
            search_cancelled: true,
            search_focus_restored: true,
        }
    }

    fn host() -> SmokeHostEvidenceV1 {
        SmokeHostEvidenceV1 {
            visible_expected_nodes: 1,
            active_nodes: 1,
            parked_nodes: 0,
            mounts: 1,
            restores: 0,
            evictions: 0,
            hard_retirements: 0,
            protected_instances: 0,
            protected_leases: 0,
            protected_overflows: 0,
            eligible_parked_nodes: 0,
            strong_instances: 1,
            frame_materialization_overflows: 0,
            feedback_submissions: 2,
            feedback_failures: 0,
            layout_rejections: 0,
            measurement_failures: 0,
            native_probe_phase: Some("Complete".to_owned()),
            native_probe_wait_ticks: Some(0),
            diagnostic: None,
        }
    }

    fn evidence() -> SmokeEvidenceV1 {
        SmokeEvidenceV1 {
            captures: INTERACTION_CAPTURES
                .iter()
                .map(ToString::to_string)
                .collect(),
            initial_graph: SmokeTopologyEvidenceV1 {
                node_ids: vec![NODE_ID.to_owned()],
                port_ids: vec![PORT_ID.to_owned()],
                edge_ids: vec![EDGE_ID.to_owned()],
            },
            host: host(),
            geometry: SmokeGeometryEvidenceV1 {
                layout_revision: 1,
                image_width: 960,
                image_height: 640,
                canvas_width: 680,
                surface_width: 680.0,
                surface_height: 640.0,
                expected_node_ids: vec![NODE_ID.to_owned()],
                visible_node_ids: vec![NODE_ID.to_owned()],
                fresh_nodes: 1,
                expected_port_ids: vec![PORT_ID.to_owned()],
                rendered_ports: 1,
                expected_edge_ids: vec![EDGE_ID.to_owned()],
                routed_edges: 1,
                endpoint_geometry_verified: true,
                offscreen_endpoint_fallbacks: 0,
            },
            paint: SmokePaintEvidenceV1 {
                image_width: 960,
                image_height: 640,
                canvas_width: 680,
                non_background_pixels: 1024,
                distinct_colors: 4,
                painted_node_ids: vec![NODE_ID.to_owned()],
                painted_edge_ids: vec![EDGE_ID.to_owned()],
            },
            interaction: Some(interaction()),
            lod: None,
        }
    }

    fn valid_interaction_receipt() -> SmokeRunReceiptV1 {
        SmokeRunReceiptV1 {
            schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
            status: SmokeReceiptStatusV1::Pass,
            run: SmokeRunDescriptorV1 {
                run_id: "interaction-shader-compact-z1".to_owned(),
                demo: SmokeDemoV1::Shader,
                bounds: SmokeBoundsV1::Compact,
                scenario: SmokeScenarioV1::Interaction,
                target_zoom: SmokeZoomV1::One,
            },
            failure: None,
            evidence: Some(evidence()),
            presented_frames: vec![
                frame(0, SmokeFrameStageV1::Interaction, false, false, true),
                frame(1, SmokeFrameStageV1::NodeDragPending, false, false, true),
                frame(
                    2,
                    SmokeFrameStageV1::NodeDragAuthoritative,
                    false,
                    false,
                    true,
                ),
                frame(3, SmokeFrameStageV1::ConnectionPending, true, false, false),
                frame(
                    4,
                    SmokeFrameStageV1::ConnectionAuthoritative,
                    false,
                    true,
                    false,
                ),
                frame(5, SmokeFrameStageV1::ReconnectPending, true, false, false),
                frame(
                    6,
                    SmokeFrameStageV1::ReconnectAuthoritative,
                    false,
                    true,
                    false,
                ),
                frame(7, SmokeFrameStageV1::Search, false, true, false),
                frame(8, SmokeFrameStageV1::Final, false, true, false),
            ],
        }
    }

    #[test]
    fn valid_interaction_receipt_round_trips_and_validates() {
        let receipt = valid_interaction_receipt();
        receipt.validate().expect("fixture receipt is valid");
        let encoded = serde_json::to_string(&receipt).expect("receipt serializes");
        let decoded: SmokeRunReceiptV1 = serde_json::from_str(&encoded).expect("receipt parses");
        assert_eq!(decoded, receipt);
    }

    #[test]
    fn strict_json_decoder_rejects_duplicate_and_unknown_nested_fields() {
        let receipt = valid_interaction_receipt();
        let encoded = serde_json::to_string(&receipt).expect("receipt serializes");
        let duplicate_root = encoded.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        );
        let error = SmokeRunReceiptV1::from_json_slice(duplicate_root.as_bytes())
            .expect_err("duplicate root field must fail");
        assert!(error.to_string().contains("duplicate key"));

        let duplicate_nested = encoded.replacen(
            "\"run_id\":\"interaction-shader-compact-z1\"",
            "\"run_id\":\"interaction-shader-compact-z1\",\"run_id\":\"interaction-shader-compact-z1\"",
            1,
        );
        let error = SmokeRunReceiptV1::from_json_slice(duplicate_nested.as_bytes())
            .expect_err("duplicate nested field must fail");
        assert!(error.to_string().contains("duplicate key"));

        let mut value = serde_json::to_value(receipt).expect("receipt converts to JSON");
        value["presented_frames"][0]["unknown"] = serde_json::json!(true);
        assert!(
            SmokeRunReceiptV1::from_json_slice(&serde_json::to_vec(&value).expect("JSON encodes"))
                .is_err()
        );
    }

    #[test]
    fn validation_rejects_gapped_frames_and_missing_preview() {
        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[1].presented_sequence = 3;
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[1].presented_sequence = 0;
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[3].preview_route = None;
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn validation_rejects_unknown_schema_and_misaligned_chrome() {
        let mut receipt = valid_interaction_receipt();
        receipt.schema_version = 2;
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[0].chrome_bounds[0].bounds.x += 0.6;
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn validation_rejects_geometry_identity_and_host_evidence_drift() {
        let mut receipt = valid_interaction_receipt();
        receipt
            .evidence
            .as_mut()
            .expect("evidence")
            .geometry
            .fresh_nodes = 0;
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt
            .evidence
            .as_mut()
            .expect("evidence")
            .geometry
            .expected_node_ids = vec![OTHER_EDGE_ID.to_owned()];
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt
            .evidence
            .as_mut()
            .expect("evidence")
            .host
            .feedback_submissions = 1;
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn validation_rejects_intermediate_route_discontinuity_and_release_jump() {
        let mut receipt = valid_interaction_receipt();
        let mut second_pending = receipt.presented_frames[3].clone();
        receipt.presented_frames[3].authoritative_routes = vec![route(OTHER_EDGE_ID)];
        second_pending.authoritative_routes = vec![route(OTHER_EDGE_ID)];
        second_pending.authoritative_routes[0].target.x += 1.0;
        receipt.presented_frames.insert(4, second_pending);
        resequence_frames(&mut receipt.presented_frames);
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[4].authoritative_routes[0].target.x += 1.0;
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[2].node_bounds[0].bounds.x += 1.0;
        receipt.presented_frames[2].chrome_bounds[0].bounds.x += 1.0;
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn validation_normalizes_viewport_motion_and_ignores_unrelated_route_visibility() {
        let mut receipt = valid_interaction_receipt();
        translate_frame_view(&mut receipt.presented_frames[3], 12.0, -7.0);
        translate_frame_view(&mut receipt.presented_frames[4], 12.0, -7.0);
        receipt.presented_frames[3].viewport.pan = point(12.0, -7.0);
        receipt.presented_frames[4].viewport.pan = point(12.0, -7.0);
        let mut unrelated = route(OTHER_EDGE_ID);
        unrelated.source.x += 200.0;
        unrelated.target.x += 200.0;
        receipt.presented_frames[4]
            .authoritative_routes
            .push(unrelated);

        receipt
            .validate()
            .expect("viewport motion and unrelated route admission preserve the operation swap");
    }

    #[test]
    fn validation_allows_target_reacquisition_before_commit_but_not_after_commit() {
        let mut receipt = valid_interaction_receipt();
        let mut exploring = receipt.presented_frames[3].clone();
        exploring.stage = SmokeFrameStageV1::ConnectionExploring;
        exploring.preview_target = None;
        let reacquired = receipt.presented_frames[3].clone();
        receipt.presented_frames.insert(4, exploring.clone());
        receipt.presented_frames.insert(5, reacquired);
        resequence_frames(&mut receipt.presented_frames);
        receipt
            .validate()
            .expect("an editable drag may reacquire its target before release");

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[3].phase = "Committing".to_owned();
        receipt.presented_frames.insert(4, exploring);
        resequence_frames(&mut receipt.presented_frames);
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn validation_rejects_missing_drag_chrome_and_reversed_render_sequence() {
        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[1].chrome_bounds.clear();
        assert!(receipt.validate().is_err());

        let mut receipt = valid_interaction_receipt();
        receipt.presented_frames[2].render_sequence = 0;
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn failure_receipt_accepts_partial_or_empty_frame_evidence() {
        let valid = valid_interaction_receipt();
        for frames in [Vec::new(), valid.presented_frames[..2].to_vec()] {
            let receipt = SmokeRunReceiptV1 {
                schema_version: SMOKE_RECEIPT_SCHEMA_VERSION,
                status: SmokeReceiptStatusV1::Fail,
                run: valid.run.clone(),
                failure: Some(SmokeFailureV1 {
                    code: "native_probe_failed".to_owned(),
                    message: None,
                }),
                evidence: None,
                presented_frames: frames,
            };
            receipt
                .validate()
                .expect("partial failure receipt is valid");
        }
    }

    #[test]
    fn schema_output_is_deterministic_and_valid_json() {
        assert_eq!(schema_json(), schema_json());
        let value: serde_json::Value = serde_json::from_str(&schema_json()).expect("schema JSON");
        assert_eq!(value["properties"]["schema_version"]["minimum"], 1);
        assert_eq!(value["properties"]["schema_version"]["maximum"], 1);
        assert_eq!(
            value["properties"]["presented_frames"]["items"]["$ref"],
            "#/$defs/SmokePresentedFrameV1"
        );
        assert!(value["$defs"]["SmokePresentedFrameV1"]["properties"].is_object());
    }

    fn resequence_frames(frames: &mut [SmokePresentedFrameV1]) {
        for (render_sequence, frame) in frames.iter_mut().enumerate() {
            frame.presented_sequence = render_sequence as u64;
            frame.render_sequence = render_sequence as u64;
            frame.graph_revision = render_sequence as u64;
        }
    }

    fn translate_frame_view(frame: &mut SmokePresentedFrameV1, x: f32, y: f32) {
        for node in frame
            .node_bounds
            .iter_mut()
            .chain(frame.chrome_bounds.iter_mut())
        {
            node.bounds.x += x;
            node.bounds.y += y;
        }
        if let Some(route) = frame.preview_route.as_mut() {
            translate_route(route, x, y);
        }
        if let Some(target) = frame.preview_target.as_mut() {
            target.anchor.x += x;
            target.anchor.y += y;
        }
        for route in &mut frame.authoritative_routes {
            translate_route(route, x, y);
        }
    }

    fn translate_route(route: &mut SmokeRouteV1, x: f32, y: f32) {
        route.source.x += x;
        route.source.y += y;
        route.target.x += x;
        route.target.y += y;
    }
}
