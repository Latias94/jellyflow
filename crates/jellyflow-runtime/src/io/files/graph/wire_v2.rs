//! Frozen V2 graph-file wire schema.
//!
//! Keep conversions exhaustive: runtime model changes must require an explicit V2 compatibility
//! decision instead of entering persisted files through derived `Graph` serialization.

use std::{collections::BTreeMap, fmt, marker::PhantomData};

use jellyflow_core::core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId,
    EdgeKind, EdgeLabelAnchor, EdgeReconnectable, EdgeReconnectableEndpoint, EdgeRouteKind,
    EdgeViewDescriptor, GRAPH_VERSION, Graph, GraphBuilder, GraphId, GraphImport,
    GraphLocalBindingTarget, Group, GroupId, Node, NodeExtent, NodeId, NodeKindKey, NodeOrigin,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind, SourceAnchor, StickyNote,
    StickyNoteId, Symbol, SymbolId,
};
use jellyflow_core::types::{TypeDesc, TypeVarId};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{Error as _, MapAccess, Visitor},
};
use serde_json::Value;

use super::{GRAPH_FILE_VERSION, GraphFile, GraphFileError};
use crate::io::files::limits::{PersistenceLimits, PersistenceResource, validate_graph_resources};

fn is_false(value: &bool) -> bool {
    !*value
}

fn edge_view_is_default(value: &EdgeViewV2) -> bool {
    value == &EdgeViewV2::default()
}

fn deserialize_unique_entity_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
{
    struct UniqueEntityMapVisitor<K, V>(PhantomData<fn() -> BTreeMap<K, V>>);

    impl<'de, K, V> Visitor<'de> for UniqueEntityMapVisitor<K, V>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
    {
        type Value = BTreeMap<K, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map with unique entity identifiers")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut entities = BTreeMap::new();
            while let Some((id, entity)) = map.next_entry()? {
                if entities.insert(id, entity).is_some() {
                    return Err(M::Error::custom("duplicate entity identifier"));
                }
            }
            Ok(entities)
        }
    }

    deserializer.deserialize_map(UniqueEntityMapVisitor(PhantomData))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphFileWireV2 {
    graph_id: GraphId,
    graph_version: u32,
    graph: GraphWireV2,
}

impl GraphFileWireV2 {
    pub(super) fn from_file(file: &GraphFile) -> Self {
        Self {
            graph_id: file.graph_id,
            graph_version: file.graph_version,
            graph: GraphWireV2::from_graph(&file.graph),
        }
    }

    pub(super) fn into_file_with_limits(
        self,
        limits: PersistenceLimits,
    ) -> Result<GraphFile, GraphFileError> {
        if self.graph_version != GRAPH_FILE_VERSION {
            return Err(GraphFileError::UnsupportedEnvelopeVersion {
                found: self.graph_version,
                supported: GRAPH_FILE_VERSION,
            });
        }
        limits.ensure(PersistenceResource::Entities, self.graph.entity_count())?;

        let file = GraphFile {
            graph_id: self.graph_id,
            graph_version: self.graph_version,
            graph: self.graph.into_graph()?,
        };
        validate_graph_resources(&file.graph, limits)?;
        file.validate()?;
        Ok(file)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphWireV2 {
    graph_id: GraphId,
    graph_version: u32,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    imports: BTreeMap<GraphId, GraphImportV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    symbols: BTreeMap<SymbolId, SymbolV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    nodes: BTreeMap<NodeId, NodeV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    ports: BTreeMap<PortId, PortV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    edges: BTreeMap<EdgeId, EdgeV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    groups: BTreeMap<GroupId, GroupV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    sticky_notes: BTreeMap<StickyNoteId, StickyNoteV2>,
    #[serde(
        default,
        deserialize_with = "deserialize_unique_entity_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    bindings: BTreeMap<BindingId, BindingV2>,
}

impl GraphWireV2 {
    fn entity_count(&self) -> usize {
        let map_entities = [
            self.imports.len(),
            self.symbols.len(),
            self.nodes.len(),
            self.ports.len(),
            self.edges.len(),
            self.groups.len(),
            self.sticky_notes.len(),
            self.bindings.len(),
        ]
        .into_iter()
        .fold(0_usize, usize::saturating_add);
        let ordered_port_references = self
            .nodes
            .values()
            .map(|node| node.ports.len())
            .fold(0_usize, usize::saturating_add);
        map_entities.saturating_add(ordered_port_references)
    }

    fn from_graph(graph: &Graph) -> Self {
        Self {
            graph_id: graph.graph_id(),
            graph_version: graph.graph_version(),
            imports: graph
                .imports()
                .iter()
                .map(|(id, import)| (*id, GraphImportV2::from(import)))
                .collect(),
            symbols: graph
                .symbols()
                .iter()
                .map(|(id, symbol)| (*id, SymbolV2::from(symbol)))
                .collect(),
            nodes: graph
                .nodes()
                .iter()
                .map(|(id, node)| (*id, NodeV2::from(node)))
                .collect(),
            ports: graph
                .ports()
                .iter()
                .map(|(id, port)| (*id, PortV2::from(port)))
                .collect(),
            edges: graph
                .edges()
                .iter()
                .map(|(id, edge)| (*id, EdgeV2::from(edge)))
                .collect(),
            groups: graph
                .groups()
                .iter()
                .map(|(id, group)| (*id, GroupV2::from(group)))
                .collect(),
            sticky_notes: graph
                .sticky_notes()
                .iter()
                .map(|(id, note)| (*id, StickyNoteV2::from(note)))
                .collect(),
            bindings: graph
                .bindings()
                .iter()
                .map(|(id, binding)| (*id, BindingV2::from(binding)))
                .collect(),
        }
    }

    fn into_graph(self) -> Result<Graph, GraphFileError> {
        if self.graph_version != GRAPH_VERSION {
            return Err(GraphFileError::UnsupportedGraphModelVersion {
                found: self.graph_version,
                supported: GRAPH_VERSION,
            });
        }

        let mut builder = GraphBuilder::new(self.graph_id);
        for (id, import) in self.imports {
            builder.insert_import(id, import.into());
        }
        for (id, symbol) in self.symbols {
            builder.insert_symbol(id, symbol.into());
        }
        for (id, node) in self.nodes {
            builder.insert_node(id, node.into());
        }
        for (id, port) in self.ports {
            builder.insert_port(id, port.into());
        }
        for (id, edge) in self.edges {
            builder.insert_edge(id, edge.into());
        }
        for (id, group) in self.groups {
            builder.insert_group(id, group.into());
        }
        for (id, note) in self.sticky_notes {
            builder.insert_sticky_note(id, note.into());
        }
        for (id, binding) in self.bindings {
            builder.insert_binding(id, binding.into());
        }
        Ok(builder.build_unchecked())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphImportV2 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alias: Option<String>,
}

impl From<&GraphImport> for GraphImportV2 {
    fn from(value: &GraphImport) -> Self {
        Self {
            alias: value.alias.clone(),
        }
    }
}

impl From<GraphImportV2> for GraphImport {
    fn from(value: GraphImportV2) -> Self {
        Self { alias: value.alias }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolV2 {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ty: Option<TypeDescV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    meta: Value,
}

impl From<&Symbol> for SymbolV2 {
    fn from(value: &Symbol) -> Self {
        Self {
            name: value.name.clone(),
            ty: value.ty.as_ref().map(TypeDescV2::from),
            default_value: value.default_value.clone(),
            meta: value.meta.clone(),
        }
    }
}

impl From<SymbolV2> for Symbol {
    fn from(value: SymbolV2) -> Self {
        Self {
            name: value.name,
            ty: value.ty.map(TypeDesc::from),
            default_value: value.default_value,
            meta: value.meta,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TypeDescV2 {
    Any {},
    Unknown {},
    Never {},
    Null {},
    Bool {},
    Int {},
    Float {},
    String {},
    Bytes {},
    List {
        of: Box<TypeDescV2>,
    },
    Map {
        key: Box<TypeDescV2>,
        value: Box<TypeDescV2>,
    },
    Object {
        fields: BTreeMap<String, TypeDescV2>,
        open: bool,
    },
    Union {
        types: Vec<TypeDescV2>,
    },
    Option {
        of: Box<TypeDescV2>,
    },
    Var {
        id: u32,
    },
    Opaque {
        key: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        params: Vec<TypeDescV2>,
    },
}

impl From<&TypeDesc> for TypeDescV2 {
    fn from(value: &TypeDesc) -> Self {
        match value {
            TypeDesc::Any => Self::Any {},
            TypeDesc::Unknown => Self::Unknown {},
            TypeDesc::Never => Self::Never {},
            TypeDesc::Null => Self::Null {},
            TypeDesc::Bool => Self::Bool {},
            TypeDesc::Int => Self::Int {},
            TypeDesc::Float => Self::Float {},
            TypeDesc::String => Self::String {},
            TypeDesc::Bytes => Self::Bytes {},
            TypeDesc::List { of } => Self::List {
                of: Box::new(Self::from(of.as_ref())),
            },
            TypeDesc::Map { key, value } => Self::Map {
                key: Box::new(Self::from(key.as_ref())),
                value: Box::new(Self::from(value.as_ref())),
            },
            TypeDesc::Object { fields, open } => Self::Object {
                fields: fields
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from(value)))
                    .collect(),
                open: *open,
            },
            TypeDesc::Union { types } => Self::Union {
                types: types.iter().map(Self::from).collect(),
            },
            TypeDesc::Option { of } => Self::Option {
                of: Box::new(Self::from(of.as_ref())),
            },
            TypeDesc::Var { id } => Self::Var { id: id.0 },
            TypeDesc::Opaque { key, params } => Self::Opaque {
                key: key.clone(),
                params: params.iter().map(Self::from).collect(),
            },
        }
    }
}

impl From<TypeDescV2> for TypeDesc {
    fn from(value: TypeDescV2) -> Self {
        match value {
            TypeDescV2::Any {} => Self::Any,
            TypeDescV2::Unknown {} => Self::Unknown,
            TypeDescV2::Never {} => Self::Never,
            TypeDescV2::Null {} => Self::Null,
            TypeDescV2::Bool {} => Self::Bool,
            TypeDescV2::Int {} => Self::Int,
            TypeDescV2::Float {} => Self::Float,
            TypeDescV2::String {} => Self::String,
            TypeDescV2::Bytes {} => Self::Bytes,
            TypeDescV2::List { of } => Self::List {
                of: Box::new(Self::from(*of)),
            },
            TypeDescV2::Map { key, value } => Self::Map {
                key: Box::new(Self::from(*key)),
                value: Box::new(Self::from(*value)),
            },
            TypeDescV2::Object { fields, open } => Self::Object {
                fields: fields
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
                open,
            },
            TypeDescV2::Union { types } => Self::Union {
                types: types.into_iter().map(Self::from).collect(),
            },
            TypeDescV2::Option { of } => Self::Option {
                of: Box::new(Self::from(*of)),
            },
            TypeDescV2::Var { id } => Self::Var { id: TypeVarId(id) },
            TypeDescV2::Opaque { key, params } => Self::Opaque {
                key,
                params: params.into_iter().map(Self::from).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeV2 {
    kind: String,
    kind_version: u32,
    pos: CanvasPointV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<NodeOriginV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    draggable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deletable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<GroupId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extent: Option<NodeExtentV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expand_parent: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<CanvasSizeV2>,
    #[serde(default, skip_serializing_if = "is_false")]
    hidden: bool,
    #[serde(default)]
    collapsed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ports: Vec<PortId>,
    #[serde(default)]
    data: Value,
}

impl From<&Node> for NodeV2 {
    fn from(value: &Node) -> Self {
        Self {
            kind: value.kind.0.clone(),
            kind_version: value.kind_version,
            pos: value.pos.into(),
            origin: value.origin.map(NodeOriginV2::from),
            selectable: value.selectable,
            draggable: value.draggable,
            connectable: value.connectable,
            deletable: value.deletable,
            parent: value.parent,
            extent: value.extent.map(NodeExtentV2::from),
            expand_parent: value.expand_parent,
            size: value.size.map(CanvasSizeV2::from),
            hidden: value.hidden,
            collapsed: value.collapsed,
            ports: value.ports.clone(),
            data: value.data.clone(),
        }
    }
}

impl From<NodeV2> for Node {
    fn from(value: NodeV2) -> Self {
        Self {
            kind: NodeKindKey(value.kind),
            kind_version: value.kind_version,
            pos: value.pos.into(),
            origin: value.origin.map(NodeOrigin::from),
            selectable: value.selectable,
            draggable: value.draggable,
            connectable: value.connectable,
            deletable: value.deletable,
            parent: value.parent,
            extent: value.extent.map(NodeExtent::from),
            expand_parent: value.expand_parent,
            size: value.size.map(CanvasSize::from),
            hidden: value.hidden,
            collapsed: value.collapsed,
            ports: value.ports,
            data: value.data,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasPointV2 {
    x: f32,
    y: f32,
}

impl From<CanvasPoint> for CanvasPointV2 {
    fn from(value: CanvasPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<CanvasPointV2> for CanvasPoint {
    fn from(value: CanvasPointV2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasSizeV2 {
    width: f32,
    height: f32,
}

impl From<CanvasSize> for CanvasSizeV2 {
    fn from(value: CanvasSize) -> Self {
        Self {
            width: value.width,
            height: value.height,
        }
    }
}

impl From<CanvasSizeV2> for CanvasSize {
    fn from(value: CanvasSizeV2) -> Self {
        Self {
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasRectV2 {
    origin: CanvasPointV2,
    size: CanvasSizeV2,
}

impl From<CanvasRect> for CanvasRectV2 {
    fn from(value: CanvasRect) -> Self {
        Self {
            origin: value.origin.into(),
            size: value.size.into(),
        }
    }
}

impl From<CanvasRectV2> for CanvasRect {
    fn from(value: CanvasRectV2) -> Self {
        Self {
            origin: value.origin.into(),
            size: value.size.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeOriginV2 {
    x: f32,
    y: f32,
}

impl From<NodeOrigin> for NodeOriginV2 {
    fn from(value: NodeOrigin) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<NodeOriginV2> for NodeOrigin {
    fn from(value: NodeOriginV2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NodeExtentV2 {
    Parent {},
    Rect { rect: CanvasRectV2 },
}

impl From<NodeExtent> for NodeExtentV2 {
    fn from(value: NodeExtent) -> Self {
        match value {
            NodeExtent::Parent => Self::Parent {},
            NodeExtent::Rect { rect } => Self::Rect { rect: rect.into() },
        }
    }
}

impl From<NodeExtentV2> for NodeExtent {
    fn from(value: NodeExtentV2) -> Self {
        match value {
            NodeExtentV2::Parent {} => Self::Parent,
            NodeExtentV2::Rect { rect } => Self::Rect { rect: rect.into() },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortV2 {
    node: NodeId,
    key: String,
    dir: PortDirectionV2,
    kind: PortKindV2,
    capacity: PortCapacityV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connectable_start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    connectable_end: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ty: Option<TypeDescV2>,
    #[serde(default)]
    data: Value,
}

impl From<&Port> for PortV2 {
    fn from(value: &Port) -> Self {
        Self {
            node: value.node,
            key: value.key.0.clone(),
            dir: value.dir.into(),
            kind: value.kind.into(),
            capacity: value.capacity.into(),
            connectable: value.connectable,
            connectable_start: value.connectable_start,
            connectable_end: value.connectable_end,
            ty: value.ty.as_ref().map(TypeDescV2::from),
            data: value.data.clone(),
        }
    }
}

impl From<PortV2> for Port {
    fn from(value: PortV2) -> Self {
        Self {
            node: value.node,
            key: PortKey(value.key),
            dir: value.dir.into(),
            kind: value.kind.into(),
            capacity: value.capacity.into(),
            connectable: value.connectable,
            connectable_start: value.connectable_start,
            connectable_end: value.connectable_end,
            ty: value.ty.map(TypeDesc::from),
            data: value.data,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortDirectionV2 {
    In,
    Out,
}

impl From<PortDirection> for PortDirectionV2 {
    fn from(value: PortDirection) -> Self {
        match value {
            PortDirection::In => Self::In,
            PortDirection::Out => Self::Out,
        }
    }
}

impl From<PortDirectionV2> for PortDirection {
    fn from(value: PortDirectionV2) -> Self {
        match value {
            PortDirectionV2::In => Self::In,
            PortDirectionV2::Out => Self::Out,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortKindV2 {
    Data,
    Exec,
}

impl From<PortKind> for PortKindV2 {
    fn from(value: PortKind) -> Self {
        match value {
            PortKind::Data => Self::Data,
            PortKind::Exec => Self::Exec,
        }
    }
}

impl From<PortKindV2> for PortKind {
    fn from(value: PortKindV2) -> Self {
        match value {
            PortKindV2::Data => Self::Data,
            PortKindV2::Exec => Self::Exec,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortCapacityV2 {
    Single,
    Multi,
}

impl From<PortCapacity> for PortCapacityV2 {
    fn from(value: PortCapacity) -> Self {
        match value {
            PortCapacity::Single => Self::Single,
            PortCapacity::Multi => Self::Multi,
        }
    }
}

impl From<PortCapacityV2> for PortCapacity {
    fn from(value: PortCapacityV2) -> Self {
        match value {
            PortCapacityV2::Single => Self::Single,
            PortCapacityV2::Multi => Self::Multi,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeV2 {
    kind: EdgeKindV2,
    from: PortId,
    to: PortId,
    #[serde(default, skip_serializing_if = "is_false")]
    hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interaction_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deletable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reconnectable: Option<EdgeReconnectableV2>,
    #[serde(default)]
    data: Value,
    #[serde(default, skip_serializing_if = "edge_view_is_default")]
    view: EdgeViewV2,
}

impl From<&Edge> for EdgeV2 {
    fn from(value: &Edge) -> Self {
        Self {
            kind: value.kind.into(),
            from: value.from,
            to: value.to,
            hidden: value.hidden,
            selectable: value.selectable,
            interaction_width: value.interaction_width,
            deletable: value.deletable,
            reconnectable: value.reconnectable.map(EdgeReconnectableV2::from),
            data: value.data.clone(),
            view: EdgeViewV2::from(&value.view),
        }
    }
}

impl From<EdgeV2> for Edge {
    fn from(value: EdgeV2) -> Self {
        Self {
            kind: value.kind.into(),
            from: value.from,
            to: value.to,
            hidden: value.hidden,
            selectable: value.selectable,
            interaction_width: value.interaction_width,
            deletable: value.deletable,
            reconnectable: value.reconnectable.map(EdgeReconnectable::from),
            data: value.data,
            view: value.view.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EdgeKindV2 {
    Data,
    Exec,
}

impl From<EdgeKind> for EdgeKindV2 {
    fn from(value: EdgeKind) -> Self {
        match value {
            EdgeKind::Data => Self::Data,
            EdgeKind::Exec => Self::Exec,
        }
    }
}

impl From<EdgeKindV2> for EdgeKind {
    fn from(value: EdgeKindV2) -> Self {
        match value {
            EdgeKindV2::Data => Self::Data,
            EdgeKindV2::Exec => Self::Exec,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
enum EdgeReconnectableV2 {
    Bool(bool),
    Endpoint(EdgeReconnectableEndpointV2),
}

impl From<EdgeReconnectable> for EdgeReconnectableV2 {
    fn from(value: EdgeReconnectable) -> Self {
        match value {
            EdgeReconnectable::Bool(value) => Self::Bool(value),
            EdgeReconnectable::Endpoint(endpoint) => Self::Endpoint(endpoint.into()),
        }
    }
}

impl From<EdgeReconnectableV2> for EdgeReconnectable {
    fn from(value: EdgeReconnectableV2) -> Self {
        match value {
            EdgeReconnectableV2::Bool(value) => Self::Bool(value),
            EdgeReconnectableV2::Endpoint(endpoint) => Self::Endpoint(endpoint.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EdgeReconnectableEndpointV2 {
    Source,
    Target,
}

impl From<EdgeReconnectableEndpoint> for EdgeReconnectableEndpointV2 {
    fn from(value: EdgeReconnectableEndpoint) -> Self {
        match value {
            EdgeReconnectableEndpoint::Source => Self::Source,
            EdgeReconnectableEndpoint::Target => Self::Target,
        }
    }
}

impl From<EdgeReconnectableEndpointV2> for EdgeReconnectableEndpoint {
    fn from(value: EdgeReconnectableEndpointV2) -> Self {
        match value {
            EdgeReconnectableEndpointV2::Source => Self::Source,
            EdgeReconnectableEndpointV2::Target => Self::Target,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeViewV2 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    renderer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label_anchor: Option<EdgeLabelAnchorV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_marker_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_marker_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route_kind: Option<EdgeRouteKindV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hit_target_width: Option<f32>,
}

impl From<&EdgeViewDescriptor> for EdgeViewV2 {
    fn from(value: &EdgeViewDescriptor) -> Self {
        Self {
            renderer_key: value.renderer_key.clone(),
            label: value.label.clone(),
            label_anchor: value.label_anchor.map(EdgeLabelAnchorV2::from),
            source_marker_key: value.source_marker_key.clone(),
            target_marker_key: value.target_marker_key.clone(),
            style_token: value.style_token.clone(),
            route_kind: value.route_kind.map(EdgeRouteKindV2::from),
            hit_target_width: value.hit_target_width,
        }
    }
}

impl From<EdgeViewV2> for EdgeViewDescriptor {
    fn from(value: EdgeViewV2) -> Self {
        Self {
            renderer_key: value.renderer_key,
            label: value.label,
            label_anchor: value.label_anchor.map(EdgeLabelAnchor::from),
            source_marker_key: value.source_marker_key,
            target_marker_key: value.target_marker_key,
            style_token: value.style_token,
            route_kind: value.route_kind.map(EdgeRouteKind::from),
            hit_target_width: value.hit_target_width,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EdgeLabelAnchorV2 {
    Source,
    Center,
    Target,
}

impl From<EdgeLabelAnchor> for EdgeLabelAnchorV2 {
    fn from(value: EdgeLabelAnchor) -> Self {
        match value {
            EdgeLabelAnchor::Source => Self::Source,
            EdgeLabelAnchor::Center => Self::Center,
            EdgeLabelAnchor::Target => Self::Target,
        }
    }
}

impl From<EdgeLabelAnchorV2> for EdgeLabelAnchor {
    fn from(value: EdgeLabelAnchorV2) -> Self {
        match value {
            EdgeLabelAnchorV2::Source => Self::Source,
            EdgeLabelAnchorV2::Center => Self::Center,
            EdgeLabelAnchorV2::Target => Self::Target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EdgeRouteKindV2 {
    Straight,
    Orthogonal,
    Bezier,
    SmoothStep,
}

impl From<EdgeRouteKind> for EdgeRouteKindV2 {
    fn from(value: EdgeRouteKind) -> Self {
        match value {
            EdgeRouteKind::Straight => Self::Straight,
            EdgeRouteKind::Orthogonal => Self::Orthogonal,
            EdgeRouteKind::Bezier => Self::Bezier,
            EdgeRouteKind::SmoothStep => Self::SmoothStep,
        }
    }
}

impl From<EdgeRouteKindV2> for EdgeRouteKind {
    fn from(value: EdgeRouteKindV2) -> Self {
        match value {
            EdgeRouteKindV2::Straight => Self::Straight,
            EdgeRouteKindV2::Orthogonal => Self::Orthogonal,
            EdgeRouteKindV2::Bezier => Self::Bezier,
            EdgeRouteKindV2::SmoothStep => Self::SmoothStep,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupV2 {
    title: String,
    rect: CanvasRectV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

impl From<&Group> for GroupV2 {
    fn from(value: &Group) -> Self {
        Self {
            title: value.title.clone(),
            rect: value.rect.into(),
            color: value.color.clone(),
        }
    }
}

impl From<GroupV2> for Group {
    fn from(value: GroupV2) -> Self {
        Self {
            title: value.title,
            rect: value.rect.into(),
            color: value.color,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StickyNoteV2 {
    text: String,
    rect: CanvasRectV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

impl From<&StickyNote> for StickyNoteV2 {
    fn from(value: &StickyNote) -> Self {
        Self {
            text: value.text.clone(),
            rect: value.rect.into(),
            color: value.color.clone(),
        }
    }
}

impl From<StickyNoteV2> for StickyNote {
    fn from(value: StickyNoteV2) -> Self {
        Self {
            text: value.text,
            rect: value.rect.into(),
            color: value.color,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingV2 {
    subject: BindingEndpointV2,
    target: BindingEndpointV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    meta: Value,
}

impl From<&Binding> for BindingV2 {
    fn from(value: &Binding) -> Self {
        Self {
            subject: BindingEndpointV2::from(&value.subject),
            target: BindingEndpointV2::from(&value.target),
            kind: value.kind.clone(),
            meta: value.meta.clone(),
        }
    }
}

impl From<BindingV2> for Binding {
    fn from(value: BindingV2) -> Self {
        Self {
            subject: value.subject.into(),
            target: value.target.into(),
            kind: value.kind,
            meta: value.meta,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum BindingEndpointV2 {
    GraphLocal { target: GraphLocalBindingTargetV2 },
    Source { anchor: SourceAnchorV2 },
}

impl From<&BindingEndpoint> for BindingEndpointV2 {
    fn from(value: &BindingEndpoint) -> Self {
        match value {
            BindingEndpoint::GraphLocal { target } => Self::GraphLocal {
                target: (*target).into(),
            },
            BindingEndpoint::Source { anchor } => Self::Source {
                anchor: SourceAnchorV2::from(anchor),
            },
        }
    }
}

impl From<BindingEndpointV2> for BindingEndpoint {
    fn from(value: BindingEndpointV2) -> Self {
        match value {
            BindingEndpointV2::GraphLocal { target } => Self::GraphLocal {
                target: target.into(),
            },
            BindingEndpointV2::Source { anchor } => Self::Source {
                anchor: anchor.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum GraphLocalBindingTargetV2 {
    Graph {},
    Node { id: NodeId },
    Port { id: PortId },
    Edge { id: EdgeId },
    Group { id: GroupId },
    StickyNote { id: StickyNoteId },
}

impl From<GraphLocalBindingTarget> for GraphLocalBindingTargetV2 {
    fn from(value: GraphLocalBindingTarget) -> Self {
        match value {
            GraphLocalBindingTarget::Graph => Self::Graph {},
            GraphLocalBindingTarget::Node { id } => Self::Node { id },
            GraphLocalBindingTarget::Port { id } => Self::Port { id },
            GraphLocalBindingTarget::Edge { id } => Self::Edge { id },
            GraphLocalBindingTarget::Group { id } => Self::Group { id },
            GraphLocalBindingTarget::StickyNote { id } => Self::StickyNote { id },
        }
    }
}

impl From<GraphLocalBindingTargetV2> for GraphLocalBindingTarget {
    fn from(value: GraphLocalBindingTargetV2) -> Self {
        match value {
            GraphLocalBindingTargetV2::Graph {} => Self::Graph,
            GraphLocalBindingTargetV2::Node { id } => Self::Node { id },
            GraphLocalBindingTargetV2::Port { id } => Self::Port { id },
            GraphLocalBindingTargetV2::Edge { id } => Self::Edge { id },
            GraphLocalBindingTargetV2::Group { id } => Self::Group { id },
            GraphLocalBindingTargetV2::StickyNote { id } => Self::StickyNote { id },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceAnchorV2 {
    source_id: String,
    #[serde(default)]
    payload: Value,
}

impl From<&SourceAnchor> for SourceAnchorV2 {
    fn from(value: &SourceAnchor) -> Self {
        Self {
            source_id: value.source_id.clone(),
            payload: value.payload.clone(),
        }
    }
}

impl From<SourceAnchorV2> for SourceAnchor {
    fn from(value: SourceAnchorV2) -> Self {
        Self {
            source_id: value.source_id,
            payload: value.payload,
        }
    }
}
