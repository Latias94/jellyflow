use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{EdgeId, GroupId, NodeId, PortId, StickyNoteId};

/// Persisted relationship between graph-local content and a knowledge source anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// Stable local endpoint inside this graph.
    pub subject: BindingEndpoint,
    /// Stable target endpoint, usually an opaque host-owned source anchor.
    pub target: BindingEndpoint,
    /// Optional domain-specific relationship label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Arbitrary domain metadata.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub meta: Value,
}

/// One side of a binding relationship.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BindingEndpoint {
    /// A graph-local object that Jellyflow can validate structurally.
    GraphLocal { target: GraphLocalBindingTarget },
    /// A host-owned source anchor that Jellyflow stores opaquely.
    Source { anchor: SourceAnchor },
}

/// A graph-local binding target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphLocalBindingTarget {
    Graph,
    Node { id: NodeId },
    Port { id: PortId },
    Edge { id: EdgeId },
    Group { id: GroupId },
    StickyNote { id: StickyNoteId },
}

/// Opaque host-owned source anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAnchor {
    /// Stable source document/resource identifier owned by the host product.
    pub source_id: String,
    /// Host-defined anchor payload, such as PDF coordinates or text ranges.
    #[serde(default)]
    pub payload: Value,
}

impl Binding {
    pub fn new(subject: BindingEndpoint, target: BindingEndpoint) -> Self {
        Self {
            subject,
            target,
            kind: None,
            meta: Value::Null,
        }
    }

    pub fn graph_local_to_source(
        subject: GraphLocalBindingTarget,
        source_id: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self::new(
            BindingEndpoint::graph_local(subject),
            BindingEndpoint::source_payload(source_id, payload),
        )
    }

    pub fn node_to_source(node: NodeId, source_id: impl Into<String>, payload: Value) -> Self {
        Self::graph_local_to_source(GraphLocalBindingTarget::node(node), source_id, payload)
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    pub fn with_meta(mut self, meta: Value) -> Self {
        self.meta = meta;
        self
    }
}

impl BindingEndpoint {
    pub fn graph_local(target: GraphLocalBindingTarget) -> Self {
        Self::GraphLocal { target }
    }

    pub fn graph() -> Self {
        Self::graph_local(GraphLocalBindingTarget::Graph)
    }

    pub fn node(id: NodeId) -> Self {
        Self::graph_local(GraphLocalBindingTarget::node(id))
    }

    pub fn port(id: PortId) -> Self {
        Self::graph_local(GraphLocalBindingTarget::port(id))
    }

    pub fn edge(id: EdgeId) -> Self {
        Self::graph_local(GraphLocalBindingTarget::edge(id))
    }

    pub fn group(id: GroupId) -> Self {
        Self::graph_local(GraphLocalBindingTarget::group(id))
    }

    pub fn sticky_note(id: StickyNoteId) -> Self {
        Self::graph_local(GraphLocalBindingTarget::sticky_note(id))
    }

    pub fn source(anchor: SourceAnchor) -> Self {
        Self::Source { anchor }
    }

    pub fn source_payload(source_id: impl Into<String>, payload: Value) -> Self {
        Self::source(SourceAnchor::new(source_id, payload))
    }

    pub fn graph_local_target(&self) -> Option<GraphLocalBindingTarget> {
        match self {
            Self::GraphLocal { target } => Some(*target),
            Self::Source { .. } => None,
        }
    }
}

impl GraphLocalBindingTarget {
    pub fn node(id: NodeId) -> Self {
        Self::Node { id }
    }

    pub fn port(id: PortId) -> Self {
        Self::Port { id }
    }

    pub fn edge(id: EdgeId) -> Self {
        Self::Edge { id }
    }

    pub fn group(id: GroupId) -> Self {
        Self::Group { id }
    }

    pub fn sticky_note(id: StickyNoteId) -> Self {
        Self::StickyNote { id }
    }
}

impl SourceAnchor {
    pub fn new(source_id: impl Into<String>, payload: Value) -> Self {
        Self {
            source_id: source_id.into(),
            payload,
        }
    }
}
