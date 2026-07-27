//! Native retained Open GPUI adapter for Jellyflow.
//!
//! The headless runtime owns graph and view facts. This crate owns retained
//! Open GPUI entities, native input, measurement, lifecycle, and scene order.

#![deny(unsafe_code)]

pub mod accessibility;
pub mod activity;
pub mod authoring;
mod binding;
pub mod chrome;
mod command_journal;
mod draft;
mod draft_store;
pub mod host;
pub mod interaction;
pub mod navigation;
mod node_drag_handle;
mod panic_sink;
mod port_handle;
pub mod presentation;
pub mod registry;
pub mod retained_field;
pub mod scale_policy;
mod semantic_zoom;
pub mod size_policy;
pub mod theme;
