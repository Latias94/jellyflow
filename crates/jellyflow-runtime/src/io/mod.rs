//! On-disk wrapper formats, view state, and editor configuration.

mod config;
mod files;
mod tuning;
mod view_state;

pub use config::{
    NodeGraphBoxSelectEdges, NodeGraphDeleteKey, NodeGraphEditorConfig, NodeGraphInteractionConfig,
    NodeGraphInteractionState, NodeGraphKeyCode, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, NodeGraphViewportInterpolate,
};
pub use files::{
    EDITOR_STATE_FILE_VERSION, GRAPH_FILE_VERSION, GraphFileError, GraphFileV1,
    NodeGraphEditorStateFile, NodeGraphEditorStateFileError,
};
pub use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey, NodeGraphModifiers,
    NodeGraphZoomActivationKey,
};
pub use tuning::{
    NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
    NodeGraphRuntimeTuning, NodeGraphSpatialIndexTuning,
};
pub use view_state::{NodeGraphPureViewState, NodeGraphViewState};

#[cfg(test)]
mod tests;
