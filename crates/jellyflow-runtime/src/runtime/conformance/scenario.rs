use serde::{Deserialize, Serialize};

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::auto_pan::AutoPanRequest;
use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::viewport::{ViewportPanRequest, ViewportZoomRequest};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
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

fn default_schema_version() -> u32 {
    CONFORMANCE_FIXTURE_SCHEMA_VERSION
}

fn default_true() -> bool {
    true
}
