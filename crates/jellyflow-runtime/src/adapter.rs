//! Opt-in support for testing native adapter semantics.

#[cfg(feature = "adapter-conformance")]
pub use crate::runtime::adapter_protocol::conformance;
