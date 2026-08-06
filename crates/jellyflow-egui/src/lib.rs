//! Immediate-mode egui protocol witness for Jellyflow.
//!
//! The adapter consumes renderer-neutral [`jellyflow_runtime::view::ViewSession`] frames, renders
//! ordinary egui widgets, and returns measured node and real-port geometry through ordered
//! feedback. Product UI and application state remain outside this crate.

#![deny(unsafe_code)]

pub use egui;

mod interaction;
mod native_host;
mod native_input;

pub use interaction::{
    EguiCanvasInteraction, EguiInteractionError, EguiInteractionOutput,
    EguiInteractionSettlementResult,
};
pub use native_host::{
    EguiNativeFrameError, EguiNativeFrameHost, EguiNativeFrameOutcome, EguiNativeNode,
    EguiNativeNodeDiagnostic, EguiNativePortError,
};
