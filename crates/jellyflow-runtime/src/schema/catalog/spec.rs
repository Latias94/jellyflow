use std::collections::BTreeSet;

use jellyflow_core::core::{
    CanvasRect, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_core::types::TypeDesc;
use serde_json::Value;

use super::{NodeCatalogInstantiationError, NodeCreation};

/// Semantic declaration for one real graph port created with a node kind.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePortSpec {
    key: PortKey,
    direction: PortDirection,
    kind: PortKind,
    capacity: PortCapacity,
    ty: Option<TypeDesc>,
    default_data: Value,
}

impl NodePortSpec {
    pub fn new(
        key: impl Into<PortKey>,
        direction: PortDirection,
        kind: PortKind,
        capacity: PortCapacity,
    ) -> Self {
        Self {
            key: key.into(),
            direction,
            kind,
            capacity,
            ty: None,
            default_data: Value::Null,
        }
    }

    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }

    pub fn with_default_data(mut self, data: Value) -> Self {
        self.default_data = data;
        self
    }

    pub fn key(&self) -> &PortKey {
        &self.key
    }

    pub fn direction(&self) -> PortDirection {
        self.direction
    }

    pub fn kind(&self) -> PortKind {
        self.kind
    }

    pub fn capacity(&self) -> PortCapacity {
        self.capacity
    }

    pub fn ty(&self) -> Option<&TypeDesc> {
        self.ty.as_ref()
    }

    pub fn default_data(&self) -> &Value {
        &self.default_data
    }

    pub(super) fn instantiate(&self, node: NodeId) -> Port {
        Port {
            node,
            key: self.key.clone(),
            dir: self.direction,
            kind: self.kind,
            capacity: self.capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: self.ty.clone(),
            data: self.default_data.clone(),
        }
    }
}

/// Minimal in-memory definition for one semantic node kind.
///
/// This type intentionally has no serde implementation. Graph documents persist concrete `Node`
/// and `Port` facts through their own versioned DTOs; catalogs are application configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeKindSpec {
    kind: NodeKindKey,
    latest_kind_version: u32,
    aliases: Vec<NodeKindKey>,
    ports: Vec<NodePortSpec>,
    default_data: Value,
}

impl NodeKindSpec {
    pub fn builder(kind: impl Into<NodeKindKey>) -> NodeKindSpecBuilder {
        NodeKindSpecBuilder {
            spec: Self {
                kind: kind.into(),
                latest_kind_version: 1,
                aliases: Vec::new(),
                ports: Vec::new(),
                default_data: Value::Null,
            },
        }
    }

    pub fn with_alias(mut self, alias: impl Into<NodeKindKey>) -> Self {
        self.aliases.push(alias.into());
        self
    }

    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    pub fn latest_kind_version(&self) -> u32 {
        self.latest_kind_version
    }

    pub fn aliases(&self) -> &[NodeKindKey] {
        &self.aliases
    }

    pub fn ports(&self) -> &[NodePortSpec] {
        &self.ports
    }

    pub fn default_data(&self) -> &Value {
        &self.default_data
    }

    pub(super) fn instantiate(&self, bounds: CanvasRect) -> NodeCreation {
        let node = NodeId::new();
        let ports = std::iter::repeat_with(PortId::new)
            .take(self.ports.len())
            .collect::<Vec<_>>();
        self.instantiate_from_port_ids(node, bounds, ports)
    }

    pub(super) fn instantiate_with_ids(
        &self,
        node: NodeId,
        bounds: CanvasRect,
        ports: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeCreation, NodeCatalogInstantiationError> {
        let ports = ports.into_iter().collect::<Vec<_>>();
        if ports.len() != self.ports.len() {
            return Err(NodeCatalogInstantiationError::PortIdCountMismatch {
                expected: self.ports.len(),
                actual: ports.len(),
            });
        }
        let mut unique = BTreeSet::new();
        if let Some(port) = ports.iter().copied().find(|port| !unique.insert(*port)) {
            return Err(NodeCatalogInstantiationError::DuplicatePortId { port });
        }
        Ok(self.instantiate_from_port_ids(node, bounds, ports))
    }

    fn instantiate_from_port_ids(
        &self,
        node_id: NodeId,
        bounds: CanvasRect,
        port_ids: Vec<PortId>,
    ) -> NodeCreation {
        let ports = self
            .ports
            .iter()
            .zip(port_ids.iter().copied())
            .map(|(spec, port)| (port, spec.instantiate(node_id)))
            .collect();
        NodeCreation::new(
            node_id,
            Node {
                kind: self.kind.clone(),
                kind_version: self.latest_kind_version,
                pos: bounds.origin,
                origin: None,
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(bounds.size),
                hidden: false,
                collapsed: false,
                ports: port_ids,
                data: self.default_data.clone(),
            },
            ports,
        )
    }
}

#[derive(Debug, Clone)]
pub struct NodeKindSpecBuilder {
    spec: NodeKindSpec,
}

impl NodeKindSpecBuilder {
    pub fn latest_kind_version(mut self, version: u32) -> Self {
        self.spec.latest_kind_version = version;
        self
    }

    pub fn alias(mut self, alias: impl Into<NodeKindKey>) -> Self {
        self.spec.aliases.push(alias.into());
        self
    }

    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<NodeKindKey>>) -> Self {
        self.spec
            .aliases
            .extend(aliases.into_iter().map(Into::into));
        self
    }

    pub fn port(mut self, port: NodePortSpec) -> Self {
        self.spec.ports.push(port);
        self
    }

    pub fn ports(mut self, ports: impl IntoIterator<Item = NodePortSpec>) -> Self {
        self.spec.ports.extend(ports);
        self
    }

    pub fn default_data(mut self, data: Value) -> Self {
        self.spec.default_data = data;
        self
    }

    pub fn build(self) -> NodeKindSpec {
        self.spec
    }
}
