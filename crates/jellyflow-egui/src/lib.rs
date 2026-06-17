//! Immediate-mode egui adapter for Jellyflow.
//!
//! This crate keeps rendering, windowing, and pointer capture in egui while delegating graph
//! semantics, layout, and mutation rules to the headless Jellyflow crates.

#![deny(unsafe_code)]

pub use eframe::egui;

pub mod app;
pub mod bridge;
pub mod input;
pub mod state;
pub mod ui;

pub use app::JellyflowEguiApp;
pub use bridge::{DemoGraphError, JellyflowEguiBridge, NodeRendererStyle, RendererCatalog};
pub use state::{
    ActiveCanvasInteraction, CanvasSnapshot, CanvasTool, HoverTarget, InspectorState,
    JellyflowEguiState, LayoutPresetChoice,
};
