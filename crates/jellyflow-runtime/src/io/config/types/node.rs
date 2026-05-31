use serde::{Deserialize, Serialize};

/// Node origin (anchor) used to interpret `Node.pos` (XyFlow `nodeOrigin`).
///
/// This is expressed as a normalized fraction of the node rect:
/// - `(0.0, 0.0)` means `Node.pos` is the node's top-left.
/// - `(0.5, 0.5)` means `Node.pos` is the node's center.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphNodeOrigin {
    pub x: f32,
    pub y: f32,
}

impl NodeGraphNodeOrigin {
    pub fn normalized(self) -> Self {
        let mut out = self;
        if !out.x.is_finite() {
            out.x = 0.0;
        }
        if !out.y.is_finite() {
            out.y = 0.0;
        }
        out.x = out.x.clamp(0.0, 1.0);
        out.y = out.y.clamp(0.0, 1.0);
        out
    }
}

/// Nudge step semantics for keyboard-driven movement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphNudgeStepMode {
    /// Interprets the step as screen-space pixels (converted to canvas units by dividing by zoom).
    #[default]
    ScreenPx,
    /// Uses the editor snap grid (`snap_grid`) as the step (canvas-space).
    Grid,
}
