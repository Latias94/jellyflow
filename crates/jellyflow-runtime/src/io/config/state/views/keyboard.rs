use crate::io::config::keys::NodeGraphDeleteKey;
use crate::io::config::types::NodeGraphNudgeStepMode;

use super::super::NodeGraphInteractionState;

/// Keyboard accessibility and nudge settings resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphKeyboardInteraction {
    pub nodes_focusable: bool,
    pub edges_focusable: bool,
    pub delete_key: NodeGraphDeleteKey,
    pub nudge_step_mode: NodeGraphNudgeStepMode,
    pub nudge_step_px: f32,
    pub nudge_fast_step_px: f32,
    pub disable_keyboard_a11y: bool,
}

impl NodeGraphInteractionState {
    pub fn keyboard_interaction(&self) -> NodeGraphKeyboardInteraction {
        NodeGraphKeyboardInteraction {
            nodes_focusable: self.nodes_focusable,
            edges_focusable: self.edges_focusable,
            delete_key: self.delete_key,
            nudge_step_mode: self.nudge_step_mode,
            nudge_step_px: self.nudge_step_px,
            nudge_fast_step_px: self.nudge_fast_step_px,
            disable_keyboard_a11y: self.disable_keyboard_a11y,
        }
    }
}
