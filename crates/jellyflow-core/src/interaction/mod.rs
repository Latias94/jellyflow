//! Renderer-neutral connection semantics.

use serde::{Deserialize, Serialize};

/// Connection mode for selecting/validating target ports during connection gestures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum NodeGraphConnectionMode {
    #[default]
    Strict,
    Loose,
}

// Adapter-specific pointer and keyboard configuration intentionally lives outside jellyflow-core.
