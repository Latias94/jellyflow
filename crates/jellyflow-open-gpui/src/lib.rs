//! Open GPUI adapter boundary for Jellyflow.
//!
//! Jellyflow runtime stays headless. This crate owns the GPUI-specific mapping
//! from semantic node descriptors to retained components, layout bounds, and
//! adapter capability facts.

#![deny(unsafe_code)]

pub mod adapter;
pub mod measurement;
pub mod testing;

pub use adapter::{OPEN_GPUI_ADAPTER_ID, OpenGpuiAdapter, OpenGpuiMeasurementMode};
pub use measurement::{
    OpenGpuiBoundsCollector, OpenGpuiMeasuredRegion, OpenGpuiMeasuredRegionKind,
    OpenGpuiMeasurementContext, OpenGpuiViewBounds, OpenGpuiViewPoint, OpenGpuiViewSize,
};
