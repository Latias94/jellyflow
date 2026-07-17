use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::types::TypeDesc;
use serde_json::Value;

use super::{NodeCatalogInstantiationError, NodeCreation};

/// Small renderer-neutral side hint for a real port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl PortSide {
    pub fn for_direction(direction: PortDirection) -> Self {
        match direction {
            PortDirection::In => Self::Left,
            PortDirection::Out => Self::Right,
        }
    }
}

/// Optional placement hints for one real port.
///
/// These values help a native adapter place a semantic handle. They do not describe a widget,
/// control tree, slot, binding, action, or layout budget.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortPresentationHint {
    side: Option<PortSide>,
    order: Option<i32>,
    group: Option<String>,
}

impl PortPresentationHint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_side(mut self, side: PortSide) -> Self {
        self.side = Some(side);
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn side(&self) -> Option<PortSide> {
        self.side
    }

    pub fn order(&self) -> Option<i32> {
        self.order
    }

    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }

    pub fn resolved_side(&self, direction: PortDirection) -> PortSide {
        self.side
            .unwrap_or_else(|| PortSide::for_direction(direction))
    }
}

/// Semantic declaration for one real graph port created with a node kind.
#[derive(Debug, Clone, PartialEq)]
pub struct NodePortSpec {
    key: PortKey,
    direction: PortDirection,
    kind: PortKind,
    capacity: PortCapacity,
    ty: Option<TypeDesc>,
    label: Option<String>,
    presentation: PortPresentationHint,
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
            label: None,
            presentation: PortPresentationHint::default(),
        }
    }

    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_presentation(mut self, presentation: PortPresentationHint) -> Self {
        self.presentation = presentation;
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

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn presentation(&self) -> &PortPresentationHint {
        &self.presentation
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
            data: Value::Null,
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
    title: String,
    category: Vec<String>,
    keywords: Vec<String>,
    renderer_key: String,
    default_size: Option<CanvasSize>,
    ports: Vec<NodePortSpec>,
    default_data: Value,
}

impl NodeKindSpec {
    pub fn builder(
        kind: impl Into<NodeKindKey>,
        title: impl Into<String>,
        renderer_key: impl Into<String>,
    ) -> NodeKindSpecBuilder {
        NodeKindSpecBuilder {
            spec: Self {
                kind: kind.into(),
                latest_kind_version: 1,
                aliases: Vec::new(),
                title: title.into(),
                category: Vec::new(),
                keywords: Vec::new(),
                renderer_key: renderer_key.into(),
                default_size: None,
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

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn category(&self) -> &[String] {
        &self.category
    }

    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    pub fn renderer_key(&self) -> &str {
        &self.renderer_key
    }

    pub fn default_size(&self) -> Option<CanvasSize> {
        self.default_size
    }

    pub fn ports(&self) -> &[NodePortSpec] {
        &self.ports
    }

    pub fn default_data(&self) -> &Value {
        &self.default_data
    }

    pub(super) fn instantiate(&self, position: CanvasPoint) -> NodeCreation {
        let node = NodeId::new();
        let ports = std::iter::repeat_with(PortId::new)
            .take(self.ports.len())
            .collect::<Vec<_>>();
        self.instantiate_from_port_ids(node, position, ports)
    }

    pub(super) fn instantiate_with_ids(
        &self,
        node: NodeId,
        position: CanvasPoint,
        ports: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeCreation, NodeCatalogInstantiationError> {
        let ports = ports.into_iter().collect::<Vec<_>>();
        if ports.len() != self.ports.len() {
            return Err(NodeCatalogInstantiationError::PortIdCountMismatch {
                expected: self.ports.len(),
                actual: ports.len(),
            });
        }
        Ok(self.instantiate_from_port_ids(node, position, ports))
    }

    fn instantiate_from_port_ids(
        &self,
        node_id: NodeId,
        position: CanvasPoint,
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
                pos: position,
                origin: None,
                selectable: None,
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

    pub fn category(mut self, category: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.spec.category = category.into_iter().map(Into::into).collect();
        self
    }

    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.spec.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    pub fn default_size(mut self, size: CanvasSize) -> Self {
        self.spec.default_size = Some(size);
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
