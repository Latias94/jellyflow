use serde::{Deserialize, Serialize};
use serde_json::Value;

use jellyflow_core::core::{
    CanvasSize, NodeKindKey, PortCapacity, PortDirection, PortKey, PortKind,
};
use jellyflow_core::types::TypeDesc;

/// Declares a port for a node kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDecl {
    /// Stable schema key for this port.
    pub key: PortKey,
    /// Direction.
    pub dir: PortDirection,
    /// Kind.
    pub kind: PortKind,
    /// Capacity.
    pub capacity: PortCapacity,
    /// Optional type descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,
    /// UI-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Schema for a node kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSchema {
    /// Canonical kind key.
    pub kind: NodeKindKey,
    /// Latest schema version for this kind.
    pub latest_kind_version: u32,
    /// Kind aliases (renames).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_aliases: Vec<NodeKindKey>,

    /// UI-facing title.
    pub title: String,
    /// Category path (for create-node search/palette).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Adapter-facing renderer key.
    ///
    /// Runtime keeps this as data instead of a component reference so React, Svelte, native, and
    /// future adapters can map the key to their own renderer registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    /// Default logical node size for adapters that need an initial rect before measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<CanvasSize>,

    /// Declared ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortDecl>,

    /// Default node payload.
    #[serde(default)]
    pub default_data: Value,
}

/// Renderer-neutral node-kind descriptor for adapter palettes and renderer lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKindViewDescriptor {
    /// Canonical kind key.
    pub kind: NodeKindKey,
    /// Adapter-owned renderer lookup key.
    pub renderer_key: String,
    /// UI-facing title.
    pub title: String,
    /// Category path for create-node search/palette grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Default logical node size for initial adapter layout before measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<CanvasSize>,
    /// Declared ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortDecl>,
    /// Default node payload.
    #[serde(default)]
    pub default_data: Value,
}

impl NodeKindViewDescriptor {
    pub(crate) fn from_schema(schema: &NodeSchema) -> Self {
        Self {
            kind: schema.kind.clone(),
            renderer_key: schema
                .renderer_key
                .clone()
                .unwrap_or_else(|| schema.kind.0.clone()),
            title: schema.title.clone(),
            category: schema.category.clone(),
            keywords: schema.keywords.clone(),
            default_size: schema.default_size,
            ports: schema.ports.clone(),
            default_data: schema.default_data.clone(),
        }
    }
}
