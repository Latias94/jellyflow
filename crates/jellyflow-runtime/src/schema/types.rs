use serde::{Deserialize, Serialize};
use serde_json::Value;

use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
use jellyflow_core::types::TypeDesc;

fn port_view_descriptor_is_default(value: &PortViewDescriptor) -> bool {
    value.is_default()
}

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
    /// Adapter-facing handle and label presentation metadata.
    #[serde(default, skip_serializing_if = "port_view_descriptor_is_default")]
    pub view: PortViewDescriptor,
}

/// Adapter-facing side hint for a node port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortViewSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl PortViewSide {
    pub fn fallback_for_direction(dir: PortDirection) -> Self {
        match dir {
            PortDirection::In => Self::Left,
            PortDirection::Out => Self::Right,
        }
    }
}

/// Adapter-facing visibility hint for a node port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortHandleVisibility {
    Visible,
    Hidden,
    Collapsed,
}

/// Renderer-neutral metadata that helps adapters place and present handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PortViewDescriptor {
    /// Preferred side for the handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<PortViewSide>,
    /// Deterministic order within side/group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    /// Optional grouping key for adapters that cluster ports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Optional adapter anchor id, such as a table field or form row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Optional lane key inside a node renderer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    /// Optional slot key inside a lane or anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Optional label override for adapter handle labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional adapter icon key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    /// Optional handle visibility hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<PortHandleVisibility>,
}

impl PortViewDescriptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn side(side: PortViewSide) -> Self {
        Self {
            side: Some(side),
            ..Self::default()
        }
    }

    pub fn top() -> Self {
        Self::side(PortViewSide::Top)
    }

    pub fn right() -> Self {
        Self::side(PortViewSide::Right)
    }

    pub fn bottom() -> Self {
        Self::side(PortViewSide::Bottom)
    }

    pub fn left() -> Self {
        Self::side(PortViewSide::Left)
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub fn with_lane(mut self, lane: impl Into<String>) -> Self {
        self.lane = Some(lane.into());
        self
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_visibility(mut self, visibility: PortHandleVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn hidden(self) -> Self {
        self.with_visibility(PortHandleVisibility::Hidden)
    }

    pub fn collapsed(self) -> Self {
        self.with_visibility(PortHandleVisibility::Collapsed)
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
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

/// Builder for adapter-facing node schemas.
#[derive(Debug, Clone)]
pub struct NodeSchemaBuilder {
    schema: NodeSchema,
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
    /// Starts a node schema builder with renderer-neutral defaults.
    pub fn builder(kind: impl Into<NodeKindKey>, title: impl Into<String>) -> NodeSchemaBuilder {
        NodeSchemaBuilder {
            schema: NodeSchema {
                kind: kind.into(),
                latest_kind_version: 1,
                kind_aliases: Vec::new(),
                title: title.into(),
                category: Vec::new(),
                keywords: Vec::new(),
                renderer_key: None,
                default_size: None,
                ports: Vec::new(),
                default_data: Value::Null,
            },
        }
    }

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

impl NodeSchemaBuilder {
    /// Sets the latest schema version for this node kind.
    pub fn latest_kind_version(mut self, version: u32) -> Self {
        self.schema.latest_kind_version = version;
        self
    }

    /// Adds one alias for this node kind.
    pub fn alias(mut self, alias: impl Into<NodeKindKey>) -> Self {
        self.schema.kind_aliases.push(alias.into());
        self
    }

    /// Adds aliases for this node kind.
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<NodeKindKey>>) -> Self {
        self.schema
            .kind_aliases
            .extend(aliases.into_iter().map(Into::into));
        self
    }

    /// Sets the create-node category path.
    pub fn category(mut self, category: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.schema.category = category.into_iter().map(Into::into).collect();
        self
    }

    /// Adds one search keyword.
    pub fn keyword(mut self, keyword: impl Into<String>) -> Self {
        self.schema.keywords.push(keyword.into());
        self
    }

    /// Adds search keywords.
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.schema
            .keywords
            .extend(keywords.into_iter().map(Into::into));
        self
    }

    /// Sets the adapter-owned renderer lookup key.
    pub fn renderer_key(mut self, renderer_key: impl Into<String>) -> Self {
        self.schema.renderer_key = Some(renderer_key.into());
        self
    }

    /// Sets the fallback logical node size.
    pub fn default_size(mut self, size: CanvasSize) -> Self {
        self.schema.default_size = Some(size);
        self
    }

    /// Adds one declared port.
    pub fn port(mut self, port: PortDecl) -> Self {
        self.schema.ports.push(port);
        self
    }

    /// Adds declared ports.
    pub fn ports(mut self, ports: impl IntoIterator<Item = PortDecl>) -> Self {
        self.schema.ports.extend(ports);
        self
    }

    /// Sets the default node payload.
    pub fn default_data(mut self, data: Value) -> Self {
        self.schema.default_data = data;
        self
    }

    /// Builds the schema.
    pub fn build(self) -> NodeSchema {
        self.schema
    }
}

impl From<NodeSchemaBuilder> for NodeSchema {
    fn from(value: NodeSchemaBuilder) -> Self {
        value.build()
    }
}

impl PortDecl {
    /// Creates a port declaration.
    pub fn new(
        key: impl Into<PortKey>,
        dir: PortDirection,
        kind: PortKind,
        capacity: PortCapacity,
    ) -> Self {
        Self {
            key: key.into(),
            dir,
            kind,
            capacity,
            ty: None,
            label: None,
            view: PortViewDescriptor::default(),
        }
    }

    /// Creates a single-capacity data input port.
    pub fn data_input(key: impl Into<PortKey>) -> Self {
        Self::new(key, PortDirection::In, PortKind::Data, PortCapacity::Single)
    }

    /// Creates a multi-capacity data output port.
    pub fn data_output(key: impl Into<PortKey>) -> Self {
        Self::new(key, PortDirection::Out, PortKind::Data, PortCapacity::Multi)
    }

    /// Sets the port type descriptor.
    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }

    /// Sets the adapter-facing label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the adapter-facing port view descriptor.
    pub fn with_view(mut self, view: PortViewDescriptor) -> Self {
        self.view = view;
        self
    }

    /// Places this port on the top side.
    pub fn on_top(self) -> Self {
        self.with_view(PortViewDescriptor::top())
    }

    /// Places this port on the right side.
    pub fn on_right(self) -> Self {
        self.with_view(PortViewDescriptor::right())
    }

    /// Places this port on the bottom side.
    pub fn on_bottom(self) -> Self {
        self.with_view(PortViewDescriptor::bottom())
    }

    /// Places this port on the left side.
    pub fn on_left(self) -> Self {
        self.with_view(PortViewDescriptor::left())
    }

    /// Sets deterministic ordering within the selected side/group.
    pub fn with_view_order(mut self, order: i32) -> Self {
        self.view.order = Some(order);
        self
    }

    /// Groups this port with related handles for adapter presentation.
    pub fn with_view_group(mut self, group: impl Into<String>) -> Self {
        self.view.group = Some(group.into());
        self
    }

    /// Anchors this port to an adapter-owned region such as a field row.
    pub fn with_view_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.view.anchor = Some(anchor.into());
        self
    }

    /// Hides this handle from adapter hit testing without removing the semantic port.
    pub fn hidden_handle(mut self) -> Self {
        self.view.visibility = Some(PortHandleVisibility::Hidden);
        self
    }

    /// Sets the capacity.
    pub fn with_capacity(mut self, capacity: PortCapacity) -> Self {
        self.capacity = capacity;
        self
    }

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
