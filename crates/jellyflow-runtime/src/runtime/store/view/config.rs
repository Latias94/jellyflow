use crate::io::{
    NodeGraphEditorConfig, NodeGraphInteractionConfig, NodeGraphInteractionState,
    NodeGraphRuntimeTuning,
};

use super::super::NodeGraphStore;

impl NodeGraphStore {
    pub fn interaction(&self) -> &NodeGraphInteractionConfig {
        &self.interaction
    }

    pub fn runtime_tuning(&self) -> &NodeGraphRuntimeTuning {
        &self.runtime_tuning
    }

    pub fn editor_config(&self) -> NodeGraphEditorConfig {
        NodeGraphEditorConfig {
            interaction: self.interaction.clone(),
            runtime_tuning: self.runtime_tuning,
        }
    }

    pub fn resolved_interaction_state(&self) -> NodeGraphInteractionState {
        NodeGraphInteractionState::from_parts(&self.interaction, &self.runtime_tuning)
    }

    pub fn replace_editor_config(&mut self, editor_config: NodeGraphEditorConfig) {
        self.install_editor_config_if_changed(editor_config);
    }

    pub fn update_editor_config(&mut self, f: impl FnOnce(&mut NodeGraphEditorConfig)) {
        let mut next = self.editor_config();
        f(&mut next);
        self.install_editor_config_if_changed(next);
    }

    fn install_editor_config_if_changed(&mut self, editor_config: NodeGraphEditorConfig) {
        if self.interaction == editor_config.interaction
            && self.runtime_tuning == editor_config.runtime_tuning
        {
            return;
        }

        self.interaction = editor_config.interaction;
        self.runtime_tuning = editor_config.runtime_tuning;
        self.notify_selectors();
    }
}
