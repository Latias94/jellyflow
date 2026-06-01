use serde::{Deserialize, Serialize};

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_core::core::Graph;

use super::constants::default_true;

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
