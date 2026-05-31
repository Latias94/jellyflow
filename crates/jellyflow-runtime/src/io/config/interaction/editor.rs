use serde::{Deserialize, Serialize};

use crate::io::tuning::NodeGraphRuntimeTuning;

use super::super::state::NodeGraphInteractionState;
use super::NodeGraphInteractionConfig;

/// Persisted editor configuration stored alongside pure view state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphEditorConfig {
    #[serde(
        default,
        skip_serializing_if = "NodeGraphInteractionConfig::is_default"
    )]
    pub interaction: NodeGraphInteractionConfig,
    #[serde(default, skip_serializing_if = "NodeGraphRuntimeTuning::is_default")]
    pub runtime_tuning: NodeGraphRuntimeTuning,
}

impl NodeGraphEditorConfig {
    pub fn from_parts(
        interaction: NodeGraphInteractionConfig,
        runtime_tuning: NodeGraphRuntimeTuning,
    ) -> Self {
        Self {
            interaction,
            runtime_tuning,
        }
    }

    pub fn into_parts(self) -> (NodeGraphInteractionConfig, NodeGraphRuntimeTuning) {
        (self.interaction, self.runtime_tuning)
    }

    pub fn is_default(this: &Self) -> bool {
        this == &Self::default()
    }

    pub fn resolved_interaction_state(&self) -> NodeGraphInteractionState {
        NodeGraphInteractionState::from_parts(&self.interaction, &self.runtime_tuning)
    }
}
