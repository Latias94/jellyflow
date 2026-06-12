use serde::{Deserialize, Serialize};
use serde_json::Value;

use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
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

/// Error returned when a node cannot be instantiated from schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeInstantiationError {
    /// The requested node kind is not registered.
    #[error("node kind schema not found: {0:?}")]
    MissingSchema(NodeKindKey),
    /// The caller supplied a different number of port ids than the schema declares.
    #[error("port id count mismatch: expected {expected}, got {actual}")]
    PortIdCountMismatch { expected: usize, actual: usize },
}

/// Concrete graph records produced from a node schema.
#[derive(Debug, Clone)]
pub struct NodeInstantiation {
    /// Allocated node id.
    pub node_id: NodeId,
    /// Node record to add to the graph.
    pub node: Node,
    /// Port records to add to the graph, in schema/UI order.
    pub ports: Vec<(PortId, Port)>,
}

impl NodeInstantiation {
    /// Consumes this instantiation into graph records.
    pub fn into_parts(self) -> (NodeId, Node, Vec<(PortId, Port)>) {
        (self.node_id, self.node, self.ports)
    }

    /// Consumes this instantiation into add-node/add-port operations.
    pub fn into_ops(self) -> Vec<GraphOp> {
        let port_order = self.node.ports.clone();
        let mut node = self.node;
        node.ports = Vec::new();

        let mut ops =
            Vec::with_capacity(self.ports.len() + usize::from(!port_order.is_empty()) + 1);
        ops.push(GraphOp::AddNode {
            id: self.node_id,
            node,
        });
        ops.extend(
            self.ports
                .into_iter()
                .map(|(id, port)| GraphOp::AddPort { id, port }),
        );
        if !port_order.is_empty() {
            ops.push(GraphOp::SetNodePorts {
                id: self.node_id,
                from: Vec::new(),
                to: port_order,
            });
        }
        ops
    }

    /// Consumes this instantiation into an unlabeled graph transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        GraphTransaction::from_ops(self.into_ops())
    }

    /// Consumes this instantiation into a labeled graph transaction.
    pub fn into_labeled_transaction(self, label: impl Into<String>) -> GraphTransaction {
        self.into_transaction().with_label(label)
    }
}

impl NodeSchema {
    /// Instantiates a node and its declared ports with freshly allocated ids.
    pub fn instantiate(&self, pos: CanvasPoint) -> NodeInstantiation {
        let node_id = NodeId::new();
        let port_ids = std::iter::repeat_with(PortId::new)
            .take(self.ports.len())
            .collect();
        self.instantiate_from_port_ids(node_id, pos, port_ids)
    }

    /// Instantiates a node and its declared ports with caller-provided ids.
    pub fn instantiate_with_ids(
        &self,
        node_id: NodeId,
        pos: CanvasPoint,
        port_ids: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeInstantiation, NodeInstantiationError> {
        let port_ids: Vec<PortId> = port_ids.into_iter().collect();
        if port_ids.len() != self.ports.len() {
            return Err(NodeInstantiationError::PortIdCountMismatch {
                expected: self.ports.len(),
                actual: port_ids.len(),
            });
        }

        Ok(self.instantiate_from_port_ids(node_id, pos, port_ids))
    }

    fn instantiate_from_port_ids(
        &self,
        node_id: NodeId,
        pos: CanvasPoint,
        port_ids: Vec<PortId>,
    ) -> NodeInstantiation {
        let ports = self
            .ports
            .iter()
            .zip(port_ids.iter().copied())
            .map(|(decl, port_id)| (port_id, decl.instantiate(node_id)))
            .collect();

        NodeInstantiation {
            node_id,
            node: Node {
                kind: self.kind.clone(),
                kind_version: self.latest_kind_version,
                pos,
                origin: None,
                selectable: None,
                focusable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: self.default_size,
                hidden: false,
                collapsed: false,
                ports: port_ids,
                data: self.default_data.clone(),
            },
            ports,
        }
    }
}

impl PortDecl {
    fn instantiate(&self, node: NodeId) -> Port {
        Port {
            node,
            key: self.key.clone(),
            dir: self.dir,
            kind: self.kind,
            capacity: self.capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: self.ty.clone(),
            data: Value::Null,
        }
    }
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
