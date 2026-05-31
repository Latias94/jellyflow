use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::TypeDesc;

use super::geometry::CanvasRect;

/// Graph-scoped symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Display name.
    pub name: String,
    /// Type descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,
    /// Default value (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    /// Arbitrary domain metadata.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub meta: Value,
}

/// A node group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Display name.
    pub title: String,
    /// Group bounds in canvas space.
    pub rect: CanvasRect,
    /// Group color (domain/theme-owned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A sticky note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickyNote {
    /// Markdown/plain text body.
    pub text: String,
    /// Note bounds in canvas space.
    pub rect: CanvasRect,
    /// Note color (domain/theme-owned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}
