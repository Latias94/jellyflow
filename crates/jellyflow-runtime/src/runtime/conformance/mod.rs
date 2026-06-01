//! Headless conformance fixture vocabulary for runtime and adapter checks.
//!
//! These types describe renderer-free scenarios that can be replayed against the runtime store.

use std::cell::RefCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::auto_pan::AutoPanRequest;
use crate::runtime::drag::NodeDragRequest;
use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    NodeGraphStoreEvent, ViewChange, ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{ViewportPanRequest, ViewportZoomRequest};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, SelectionChange, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GroupId, NodeId};
use jellyflow_core::ops::{EdgeEndpoints, GraphTransaction};

pub const CONFORMANCE_FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceScenario {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub setup: ConformanceSetup,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ConformanceAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_trace: Vec<ConformanceTraceEvent>,
}

impl ConformanceScenario {
    pub fn new(name: impl Into<String>, graph: Graph) -> Self {
        Self {
            schema_version: CONFORMANCE_FIXTURE_SCHEMA_VERSION,
            name: name.into(),
            setup: ConformanceSetup::from_graph(graph),
            actions: Vec::new(),
            expected_trace: Vec::new(),
        }
    }

    pub fn with_setup(mut self, setup: ConformanceSetup) -> Self {
        self.setup = setup;
        self
    }

    pub fn with_view_state(mut self, view_state: NodeGraphViewState) -> Self {
        self.setup.view_state = view_state;
        self
    }

    pub fn with_editor_config(mut self, editor_config: NodeGraphEditorConfig) -> Self {
        self.setup.editor_config = editor_config;
        self
    }

    pub fn with_trace_config(mut self, trace: ConformanceTraceConfig) -> Self {
        self.setup.trace = trace;
        self
    }

    pub fn with_actions(mut self, actions: impl IntoIterator<Item = ConformanceAction>) -> Self {
        self.actions = actions.into_iter().collect();
        self
    }

    pub fn with_expected_trace(
        mut self,
        expected_trace: impl IntoIterator<Item = ConformanceTraceEvent>,
    ) -> Self {
        self.expected_trace = expected_trace.into_iter().collect();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuite {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<ConformanceScenario>,
}

impl ConformanceSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CONFORMANCE_FIXTURE_SCHEMA_VERSION,
            name: name.into(),
            scenarios: Vec::new(),
        }
    }

    pub fn with_scenarios(
        mut self,
        scenarios: impl IntoIterator<Item = ConformanceScenario>,
    ) -> Self {
        self.scenarios = scenarios.into_iter().collect();
        self
    }

    pub fn push_scenario(&mut self, scenario: ConformanceScenario) {
        self.scenarios.push(scenario);
    }

    pub fn run(&self) -> ConformanceSuiteReport {
        run_conformance_suite(self)
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| ConformanceFixtureFileError::Read {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| ConformanceFixtureFileError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn load_json_if_exists(
        path: impl AsRef<Path>,
    ) -> Result<Option<Self>, ConformanceFixtureFileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::load_json(path).map(Some)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), ConformanceFixtureFileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                ConformanceFixtureFileError::Write {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| {
            ConformanceFixtureFileError::Serialize {
                path: path.display().to_string(),
                source,
            }
        })?;
        std::fs::write(path, bytes).map_err(|source| ConformanceFixtureFileError::Write {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn approve_actual_traces(&self) -> ConformanceSuiteApproval {
        let mut approved = self.clone();
        let mut scenario_reports = Vec::new();
        let mut errors = Vec::new();

        for scenario in &mut approved.scenarios {
            match run_conformance_scenario(scenario) {
                Ok(report) => {
                    let expected_event_count = scenario.expected_trace.len();
                    let actual_event_count = report.actual_trace.len();
                    let changed = scenario.expected_trace != report.actual_trace;
                    scenario.expected_trace = report.actual_trace;
                    scenario_reports.push(ConformanceScenarioApprovalReport {
                        scenario: scenario.name.clone(),
                        changed,
                        expected_event_count,
                        actual_event_count,
                        mismatches: report.mismatches,
                    });
                }
                Err(error) => errors.push(error),
            }
        }

        ConformanceSuiteApproval {
            suite: approved,
            report: ConformanceSuiteApprovalReport {
                suite: self.name.clone(),
                scenario_reports,
                errors,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectory {
    pub root: PathBuf,
    pub files: Vec<ConformanceSuiteFile>,
}

impl ConformanceFixtureDirectory {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let root = path.as_ref();
        let mut suite_paths = Vec::new();
        collect_conformance_json_files(root, &mut suite_paths)?;
        suite_paths.sort();
        let files = suite_paths
            .into_iter()
            .map(ConformanceSuiteFile::load_json)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            root: root.to_path_buf(),
            files,
        })
    }

    pub fn load_json_if_exists(
        path: impl AsRef<Path>,
    ) -> Result<Option<Self>, ConformanceFixtureFileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::load_json(path).map(Some)
    }

    pub fn run(&self) -> ConformanceFixtureDirectoryReport {
        ConformanceFixtureDirectoryReport {
            root: self.root.clone(),
            reports: self.files.iter().map(ConformanceSuiteFile::run).collect(),
        }
    }

    pub fn approve_actual_traces_to_json(
        &self,
    ) -> Result<ConformanceFixtureDirectoryApprovalReport, ConformanceFixtureFileError> {
        let mut approvals = Vec::new();
        for file in &self.files {
            let approval = file.suite.approve_actual_traces();
            if !approval.is_approvable() {
                return Err(ConformanceFixtureFileError::Approve {
                    path: file.path.display().to_string(),
                    source: ConformanceApprovalError::from_report(approval.report),
                });
            }
            approvals.push((file.path.clone(), approval));
        }

        for (path, approval) in &approvals {
            approval.suite.save_json(path)?;
        }

        Ok(ConformanceFixtureDirectoryApprovalReport {
            root: self.root.clone(),
            reports: approvals
                .into_iter()
                .map(|(path, approval)| ConformanceSuiteFileApprovalReport {
                    path,
                    report: approval.report,
                })
                .collect(),
        })
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFile {
    pub path: PathBuf,
    pub suite: ConformanceSuite,
}

impl ConformanceSuiteFile {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let path = path.as_ref();
        Ok(Self {
            path: path.to_path_buf(),
            suite: ConformanceSuite::load_json(path)?,
        })
    }

    pub fn run(&self) -> ConformanceSuiteFileReport {
        ConformanceSuiteFileReport {
            path: self.path.clone(),
            report: self.suite.run(),
        }
    }

    pub fn approve_actual_traces_to_json(
        &self,
    ) -> Result<ConformanceSuiteApproval, ConformanceFixtureFileError> {
        let approval = self.suite.approve_actual_traces();
        if !approval.is_approvable() {
            return Err(ConformanceFixtureFileError::Approve {
                path: self.path.display().to_string(),
                source: ConformanceApprovalError::from_report(approval.report),
            });
        }
        approval.suite.save_json(&self.path)?;
        Ok(approval)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFileReport {
    pub path: PathBuf,
    pub report: ConformanceSuiteReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectoryReport {
    pub root: PathBuf,
    pub reports: Vec<ConformanceSuiteFileReport>,
}

impl ConformanceFixtureDirectoryReport {
    pub fn is_match(&self) -> bool {
        self.reports.iter().all(|report| report.report.is_match())
    }

    pub fn file_count(&self) -> usize {
        self.reports.len()
    }

    pub fn scenario_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.scenario_count())
            .sum()
    }

    pub fn failed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| !report.report.is_match())
            .count()
    }

    pub fn failed_scenarios(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.failed_scenarios())
            .sum()
    }

    pub fn error_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.errors.len())
            .sum()
    }
}

impl fmt::Display for ConformanceFixtureDirectoryReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            return write!(
                f,
                "conformance fixture directory `{}` matched {} file(s), {} scenario(s)",
                self.root.display(),
                self.file_count(),
                self.scenario_count()
            );
        }

        writeln!(
            f,
            "conformance fixture directory `{}` failed: {} file(s), {} scenario(s), {} execution error(s)",
            self.root.display(),
            self.failed_files(),
            self.failed_scenarios(),
            self.error_count()
        )?;
        for report in self
            .reports
            .iter()
            .filter(|report| !report.report.is_match())
            .take(8)
        {
            writeln!(f, "  file `{}`: {}", report.path.display(), report.report)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteApproval {
    pub suite: ConformanceSuite,
    pub report: ConformanceSuiteApprovalReport,
}

impl ConformanceSuiteApproval {
    pub fn is_approvable(&self) -> bool {
        self.report.is_approvable()
    }

    pub fn has_changes(&self) -> bool {
        self.report.has_changes()
    }

    pub fn changed_scenarios(&self) -> usize {
        self.report.changed_scenarios()
    }

    pub fn error_count(&self) -> usize {
        self.report.error_count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteApprovalReport {
    pub suite: String,
    pub scenario_reports: Vec<ConformanceScenarioApprovalReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceSuiteApprovalReport {
    pub fn is_approvable(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_changes(&self) -> bool {
        self.scenario_reports.iter().any(|report| report.changed)
    }

    pub fn changed_scenarios(&self) -> usize {
        self.scenario_reports
            .iter()
            .filter(|report| report.changed)
            .count()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceScenarioApprovalReport {
    pub scenario: String,
    pub changed: bool,
    pub expected_event_count: usize,
    pub actual_event_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConformanceTraceMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFileApprovalReport {
    pub path: PathBuf,
    pub report: ConformanceSuiteApprovalReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectoryApprovalReport {
    pub root: PathBuf,
    pub reports: Vec<ConformanceSuiteFileApprovalReport>,
}

impl ConformanceFixtureDirectoryApprovalReport {
    pub fn is_approvable(&self) -> bool {
        self.reports
            .iter()
            .all(|report| report.report.is_approvable())
    }

    pub fn has_changes(&self) -> bool {
        self.reports
            .iter()
            .any(|report| report.report.has_changes())
    }

    pub fn file_count(&self) -> usize {
        self.reports.len()
    }

    pub fn changed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| report.report.has_changes())
            .count()
    }

    pub fn changed_scenarios(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.changed_scenarios())
            .sum()
    }

    pub fn error_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.error_count())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("failed to approve conformance suite `{suite}` because {error_count} scenario(s) errored")]
pub struct ConformanceApprovalError {
    pub suite: String,
    pub error_count: usize,
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceApprovalError {
    pub fn from_report(report: ConformanceSuiteApprovalReport) -> Self {
        Self {
            suite: report.suite,
            error_count: report.errors.len(),
            errors: report.errors,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConformanceFixtureFileError {
    #[error("failed to read conformance fixture directory: {path}")]
    ReadDirectory {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture directory entry: {path}")]
    ReadDirectoryEntry {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture directory entry type: {path}")]
    ReadDirectoryEntryType {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture file: {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse conformance fixture JSON: {path}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to write conformance fixture file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to serialize conformance fixture JSON: {path}")]
    Serialize {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to approve conformance fixture file: {path}")]
    Approve {
        path: String,
        source: ConformanceApprovalError,
    },
}

fn collect_conformance_json_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), ConformanceFixtureFileError> {
    let entries =
        std::fs::read_dir(dir).map_err(|source| ConformanceFixtureFileError::ReadDirectory {
            path: dir.display().to_string(),
            source,
        })?;

    for entry in entries {
        let entry = entry.map_err(|source| ConformanceFixtureFileError::ReadDirectoryEntry {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| {
            ConformanceFixtureFileError::ReadDirectoryEntryType {
                path: path.display().to_string(),
                source,
            }
        })?;

        if file_type.is_dir() {
            collect_conformance_json_files(&path, files)?;
        } else if file_type.is_file() && is_conformance_json_path(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn is_conformance_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSetup {
    #[serde(default)]
    pub graph: Graph,
    #[serde(default)]
    pub view_state: NodeGraphViewState,
    #[serde(default)]
    pub editor_config: NodeGraphEditorConfig,
    #[serde(default)]
    pub trace: ConformanceTraceConfig,
}

impl Default for ConformanceSetup {
    fn default() -> Self {
        Self::from_graph(Graph::default())
    }
}

impl ConformanceSetup {
    pub fn from_graph(graph: Graph) -> Self {
        Self {
            graph,
            view_state: NodeGraphViewState::default(),
            editor_config: NodeGraphEditorConfig::default(),
            trace: ConformanceTraceConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceTraceConfig {
    #[serde(default = "default_true")]
    pub record_store_events: bool,
    #[serde(default = "default_true")]
    pub record_gesture_events: bool,
    #[serde(default)]
    pub record_xyflow_callbacks: bool,
}

impl Default for ConformanceTraceConfig {
    fn default() -> Self {
        Self {
            record_store_events: true,
            record_gesture_events: true,
            record_xyflow_callbacks: false,
        }
    }
}

impl ConformanceTraceConfig {
    pub fn with_xyflow_callbacks() -> Self {
        Self {
            record_xyflow_callbacks: true,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceAction {
    DispatchTransaction {
        transaction: GraphTransaction,
    },
    ApplyNodeDrag {
        node: NodeId,
        to: CanvasPoint,
    },
    ApplyAutoPan {
        request: AutoPanRequest,
    },
    ApplyViewportPan {
        request: ViewportPanRequest,
    },
    ApplyViewportZoom {
        request: ViewportZoomRequest,
    },
    SetViewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    SetSelection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
    EmitGesture {
        event: NodeGraphGestureEvent,
    },
}

impl ConformanceAction {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DispatchTransaction { .. } => "dispatch_transaction",
            Self::ApplyNodeDrag { .. } => "apply_node_drag",
            Self::ApplyAutoPan { .. } => "apply_auto_pan",
            Self::ApplyViewportPan { .. } => "apply_viewport_pan",
            Self::ApplyViewportZoom { .. } => "apply_viewport_zoom",
            Self::SetViewport { .. } => "set_viewport",
            Self::SetSelection { .. } => "set_selection",
            Self::EmitGesture { .. } => "emit_gesture",
        }
    }

    pub fn dispatch_transaction(transaction: GraphTransaction) -> Self {
        Self::DispatchTransaction { transaction }
    }

    pub fn apply_node_drag(node: NodeId, to: CanvasPoint) -> Self {
        Self::ApplyNodeDrag { node, to }
    }

    pub fn apply_auto_pan(request: AutoPanRequest) -> Self {
        Self::ApplyAutoPan { request }
    }

    pub fn apply_viewport_pan(request: ViewportPanRequest) -> Self {
        Self::ApplyViewportPan { request }
    }

    pub fn apply_viewport_zoom(request: ViewportZoomRequest) -> Self {
        Self::ApplyViewportZoom { request }
    }

    pub fn set_viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::SetViewport { pan, zoom }
    }

    pub fn set_selection(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::SetSelection {
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
            groups: groups.into_iter().collect(),
        }
    }

    pub fn emit_gesture(event: NodeGraphGestureEvent) -> Self {
        Self::EmitGesture { event }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceTraceEvent {
    DocumentReplaced {
        before_revision: u64,
        after_revision: u64,
    },
    GraphCommitted {
        label: Option<String>,
        op_kinds: Vec<String>,
    },
    ViewChanged {
        changes: Vec<ConformanceViewChange>,
    },
    Gesture(NodeGraphGestureEvent),
    Callback(ConformanceCallbackEvent),
}

impl ConformanceTraceEvent {
    pub fn graph_commit(
        label: Option<impl Into<String>>,
        op_kinds: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::GraphCommitted {
            label: label.map(Into::into),
            op_kinds: op_kinds.into_iter().map(Into::into).collect(),
        }
    }

    pub fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::ViewChanged {
            changes: vec![ConformanceViewChange::Viewport { pan, zoom }],
        }
    }

    pub fn selection(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::ViewChanged {
            changes: vec![ConformanceViewChange::Selection {
                nodes: nodes.into_iter().collect(),
                edges: edges.into_iter().collect(),
                groups: groups.into_iter().collect(),
            }],
        }
    }

    pub fn gesture(event: NodeGraphGestureEvent) -> Self {
        Self::Gesture(event)
    }

    pub fn callback(event: ConformanceCallbackEvent) -> Self {
        Self::Callback(event)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConformanceViewChange {
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Selection {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        nodes: Vec<NodeId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        edges: Vec<EdgeId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        groups: Vec<GroupId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceCallbackEvent {
    ViewChange {
        changes: Vec<ConformanceViewChange>,
    },
    ViewportChange {
        pan: CanvasPoint,
        zoom: f32,
    },
    SelectionChange {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
    GraphCommit {
        label: Option<String>,
    },
    NodeEdgeChanges {
        nodes: usize,
        edges: usize,
    },
    NodesChange {
        count: usize,
    },
    EdgesChange {
        count: usize,
    },
    ConnectionChange(ConnectionChange),
    Connect(EdgeConnection),
    Disconnect(EdgeConnection),
    Reconnect {
        edge: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    },
    NodeDragStart(NodeDragStart),
    NodeDrag(NodeDragUpdate),
    NodeDragEnd(NodeDragEnd),
    ViewportMoveStart(ViewportMoveStart),
    ViewportMove(ViewportMove),
    ViewportMoveEnd(ViewportMoveEnd),
    ConnectStart(ConnectStart),
    ConnectEnd(ConnectEnd),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceRunReport {
    pub scenario: String,
    pub actual_trace: Vec<ConformanceTraceEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConformanceTraceMismatch>,
}

impl ConformanceRunReport {
    pub fn new(
        scenario: impl Into<String>,
        actual_trace: Vec<ConformanceTraceEvent>,
        expected_trace: &[ConformanceTraceEvent],
    ) -> Self {
        let mismatches = trace_mismatches(expected_trace, &actual_trace);
        Self {
            scenario: scenario.into(),
            actual_trace,
            mismatches,
        }
    }

    pub fn is_match(&self) -> bool {
        self.mismatches.is_empty()
    }

    pub fn actual_trace(&self) -> &[ConformanceTraceEvent] {
        &self.actual_trace
    }

    pub fn mismatches(&self) -> &[ConformanceTraceMismatch] {
        &self.mismatches
    }
}

impl fmt::Display for ConformanceRunReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            return write!(
                f,
                "conformance scenario `{}` matched {} trace events",
                self.scenario,
                self.actual_trace.len()
            );
        }

        writeln!(
            f,
            "conformance trace mismatch for scenario `{}` ({} mismatch(es))",
            self.scenario,
            self.mismatches.len()
        )?;
        for mismatch in self.mismatches.iter().take(8) {
            writeln!(
                f,
                "  [{}] expected: {:?}; actual: {:?}",
                mismatch.index, mismatch.expected, mismatch.actual
            )?;
        }
        if self.mismatches.len() > 8 {
            writeln!(f, "  ... {} more", self.mismatches.len() - 8)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteReport {
    pub suite: String,
    pub scenario_reports: Vec<ConformanceRunReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ConformanceRunError>,
}

impl ConformanceSuiteReport {
    pub fn is_match(&self) -> bool {
        self.errors.is_empty()
            && self
                .scenario_reports
                .iter()
                .all(ConformanceRunReport::is_match)
    }

    pub fn failed_scenarios(&self) -> usize {
        self.errors.len()
            + self
                .scenario_reports
                .iter()
                .filter(|report| !report.is_match())
                .count()
    }

    pub fn scenario_count(&self) -> usize {
        self.scenario_reports.len() + self.errors.len()
    }
}

impl fmt::Display for ConformanceSuiteReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            return write!(
                f,
                "conformance suite `{}` matched {} scenario(s)",
                self.suite,
                self.scenario_count()
            );
        }

        writeln!(
            f,
            "conformance suite `{}` failed: {} scenario(s), {} execution error(s)",
            self.suite,
            self.failed_scenarios(),
            self.errors.len()
        )?;
        for report in self
            .scenario_reports
            .iter()
            .filter(|report| !report.is_match())
            .take(8)
        {
            writeln!(
                f,
                "  scenario `{}` mismatched {} trace event(s)",
                report.scenario,
                report.mismatches.len()
            )?;
        }
        for error in self.errors.iter().take(8) {
            writeln!(
                f,
                "  scenario `{}` errored at action {} ({}): {}",
                error.scenario, error.action_index, error.action_kind, error.message
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConformanceTraceMismatch {
    pub index: usize,
    pub expected: Option<ConformanceTraceEvent>,
    pub actual: Option<ConformanceTraceEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error(
    "conformance scenario `{scenario}` failed at action {action_index} ({action_kind}): {message}"
)]
pub struct ConformanceRunError {
    pub scenario: String,
    pub action_index: usize,
    pub action_kind: String,
    pub message: String,
}

pub fn run_conformance_scenario(
    scenario: &ConformanceScenario,
) -> Result<ConformanceRunReport, ConformanceRunError> {
    ConformanceRunner::new(scenario).run()
}

pub fn run_conformance_suite(suite: &ConformanceSuite) -> ConformanceSuiteReport {
    let mut scenario_reports = Vec::new();
    let mut errors = Vec::new();

    for scenario in &suite.scenarios {
        match run_conformance_scenario(scenario) {
            Ok(report) => scenario_reports.push(report),
            Err(error) => errors.push(error),
        }
    }

    ConformanceSuiteReport {
        suite: suite.name.clone(),
        scenario_reports,
        errors,
    }
}

#[derive(Debug)]
pub struct ConformanceRunner<'a> {
    scenario: &'a ConformanceScenario,
}

impl<'a> ConformanceRunner<'a> {
    pub fn new(scenario: &'a ConformanceScenario) -> Self {
        Self { scenario }
    }

    pub fn run(&self) -> Result<ConformanceRunReport, ConformanceRunError> {
        let mut store = NodeGraphStore::new(
            self.scenario.setup.graph.clone(),
            self.scenario.setup.view_state.clone(),
            self.scenario.setup.editor_config.clone(),
        );
        let trace = Rc::new(RefCell::new(Vec::new()));
        install_trace_recorders(&mut store, self.scenario.setup.trace, trace.clone());

        for (index, action) in self.scenario.actions.iter().enumerate() {
            execute_action(&mut store, action).map_err(|message| ConformanceRunError {
                scenario: self.scenario.name.clone(),
                action_index: index,
                action_kind: action.kind().to_owned(),
                message,
            })?;
        }

        Ok(ConformanceRunReport::new(
            self.scenario.name.clone(),
            trace.borrow().clone(),
            &self.scenario.expected_trace,
        ))
    }
}

fn install_trace_recorders(
    store: &mut NodeGraphStore,
    config: ConformanceTraceConfig,
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
) {
    if config.record_store_events || config.record_gesture_events {
        let store_trace = trace.clone();
        let token = store.subscribe(move |event| {
            if config.record_store_events {
                store_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::from_store_event(event));
            }
        });

        if config.record_gesture_events {
            let gesture_trace = trace.clone();
            store.subscribe_gesture_with_token(token, move |event| {
                gesture_trace
                    .borrow_mut()
                    .push(ConformanceTraceEvent::Gesture(event));
            });
        }
    }

    if config.record_xyflow_callbacks {
        let _ = install_callbacks(&mut *store, CallbackTraceRecorder { trace });
    }
}

fn execute_action(store: &mut NodeGraphStore, action: &ConformanceAction) -> Result<(), String> {
    match action {
        ConformanceAction::DispatchTransaction { transaction } => store
            .dispatch_transaction(transaction)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyNodeDrag { node, to } => store
            .apply_node_drag(NodeDragRequest {
                node: *node,
                to: *to,
            })
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyAutoPan { request } => store
            .apply_auto_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "auto-pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportPan { request } => store
            .apply_viewport_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportZoom { request } => store
            .apply_viewport_zoom(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport zoom request was rejected".to_owned()),
        ConformanceAction::SetViewport { pan, zoom } => {
            store.set_viewport(*pan, *zoom);
            Ok(())
        }
        ConformanceAction::SetSelection {
            nodes,
            edges,
            groups,
        } => {
            store.set_selection(nodes.clone(), edges.clone(), groups.clone());
            Ok(())
        }
        ConformanceAction::EmitGesture { event } => {
            store.emit_gesture(event.clone());
            Ok(())
        }
    }
}

fn trace_mismatches(
    expected: &[ConformanceTraceEvent],
    actual: &[ConformanceTraceEvent],
) -> Vec<ConformanceTraceMismatch> {
    let len = expected.len().max(actual.len());
    (0..len)
        .filter_map(|index| {
            let expected = expected.get(index);
            let actual = actual.get(index);
            (expected != actual).then(|| ConformanceTraceMismatch {
                index,
                expected: expected.cloned(),
                actual: actual.cloned(),
            })
        })
        .collect()
}

impl ConformanceTraceEvent {
    fn from_store_event(event: NodeGraphStoreEvent<'_>) -> Self {
        match event {
            NodeGraphStoreEvent::DocumentReplaced { before, after } => Self::DocumentReplaced {
                before_revision: before.graph_revision,
                after_revision: after.graph_revision,
            },
            NodeGraphStoreEvent::GraphCommitted { patch } => Self::GraphCommitted {
                label: patch.transaction().label().map(str::to_owned),
                op_kinds: patch
                    .transaction()
                    .ops()
                    .iter()
                    .map(serialized_graph_op_kind)
                    .collect(),
            },
            NodeGraphStoreEvent::ViewChanged { changes, .. } => Self::ViewChanged {
                changes: changes
                    .iter()
                    .map(ConformanceViewChange::from_view_change)
                    .collect(),
            },
        }
    }
}

impl ConformanceViewChange {
    fn from_view_change(change: &ViewChange) -> Self {
        match change {
            ViewChange::Viewport { pan, zoom } => Self::Viewport {
                pan: *pan,
                zoom: *zoom,
            },
            ViewChange::Selection {
                nodes,
                edges,
                groups,
            } => Self::Selection {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: groups.clone(),
            },
        }
    }
}

#[derive(Clone)]
struct CallbackTraceRecorder {
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
}

impl CallbackTraceRecorder {
    fn push(&self, event: ConformanceCallbackEvent) {
        self.trace
            .borrow_mut()
            .push(ConformanceTraceEvent::Callback(event));
    }
}

impl NodeGraphCommitCallbacks for CallbackTraceRecorder {
    fn on_graph_commit(&mut self, patch: &crate::runtime::commit::NodeGraphPatch) {
        self.push(ConformanceCallbackEvent::GraphCommit {
            label: patch.transaction().label().map(str::to_owned),
        });
    }

    fn on_node_edge_changes(&mut self, changes: &NodeGraphChanges) {
        self.push(ConformanceCallbackEvent::NodeEdgeChanges {
            nodes: changes.nodes().len(),
            edges: changes.edges().len(),
        });
    }

    fn on_nodes_change(&mut self, changes: &[NodeChange]) {
        self.push(ConformanceCallbackEvent::NodesChange {
            count: changes.len(),
        });
    }

    fn on_edges_change(&mut self, changes: &[EdgeChange]) {
        self.push(ConformanceCallbackEvent::EdgesChange {
            count: changes.len(),
        });
    }

    fn on_connection_change(&mut self, change: ConnectionChange) {
        self.push(ConformanceCallbackEvent::ConnectionChange(change));
    }

    fn on_connect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Connect(conn));
    }

    fn on_disconnect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Disconnect(conn));
    }

    fn on_reconnect(&mut self, edge: EdgeId, from: EdgeEndpoints, to: EdgeEndpoints) {
        self.push(ConformanceCallbackEvent::Reconnect { edge, from, to });
    }
}

impl NodeGraphViewCallbacks for CallbackTraceRecorder {
    fn on_view_change(&mut self, changes: &[ViewChange]) {
        self.push(ConformanceCallbackEvent::ViewChange {
            changes: changes
                .iter()
                .map(ConformanceViewChange::from_view_change)
                .collect(),
        });
    }

    fn on_viewport_change(&mut self, pan: CanvasPoint, zoom: f32) {
        self.push(ConformanceCallbackEvent::ViewportChange { pan, zoom });
    }

    fn on_selection_change(&mut self, sel: SelectionChange) {
        let (nodes, edges, groups) = sel.into_parts();
        self.push(ConformanceCallbackEvent::SelectionChange {
            nodes,
            edges,
            groups,
        });
    }
}

impl NodeGraphGestureCallbacks for CallbackTraceRecorder {
    fn on_move_start(&mut self, ev: ViewportMoveStart) {
        self.push(ConformanceCallbackEvent::ViewportMoveStart(ev));
    }

    fn on_move(&mut self, ev: ViewportMove) {
        self.push(ConformanceCallbackEvent::ViewportMove(ev));
    }

    fn on_move_end(&mut self, ev: ViewportMoveEnd) {
        self.push(ConformanceCallbackEvent::ViewportMoveEnd(ev));
    }

    fn on_node_drag_start(&mut self, ev: NodeDragStart) {
        self.push(ConformanceCallbackEvent::NodeDragStart(ev));
    }

    fn on_node_drag(&mut self, ev: NodeDragUpdate) {
        self.push(ConformanceCallbackEvent::NodeDrag(ev));
    }

    fn on_node_drag_end(&mut self, ev: NodeDragEnd) {
        self.push(ConformanceCallbackEvent::NodeDragEnd(ev));
    }

    fn on_connect_start(&mut self, ev: ConnectStart) {
        self.push(ConformanceCallbackEvent::ConnectStart(ev));
    }

    fn on_connect_end(&mut self, ev: ConnectEnd) {
        self.push(ConformanceCallbackEvent::ConnectEnd(ev));
    }
}

fn serialized_graph_op_kind(op: &jellyflow_core::ops::GraphOp) -> String {
    serde_json::to_value(op)
        .ok()
        .and_then(|value| {
            value
                .get("op")
                .and_then(|op| op.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

fn default_schema_version() -> u32 {
    CONFORMANCE_FIXTURE_SCHEMA_VERSION
}

fn default_true() -> bool {
    true
}
