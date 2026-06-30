use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::registry::NodeRegistry;
use super::types::NodeSchema;
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, EdgeViewDescriptor, Graph, GraphBuilder, GraphId, NodeId,
    NodeKindKey, PortDirection, PortId,
};

mod builtins;

pub use builtins::{
    builtin_node_kits, erd_table_manifest, mind_map_knowledge_canvas_manifest,
    shader_blueprint_manifest, workflow_automation_manifest,
};

/// Stable identifier for a node kit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeKitKey(pub String);

impl NodeKitKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl From<&str> for NodeKitKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeKitKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Stable identifier for an adapter supported by a kit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeKitAdapterKey(pub String);

impl NodeKitAdapterKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
}

impl From<&str> for NodeKitAdapterKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for NodeKitAdapterKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Zoom and spacing hints a kit can publish to adapters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKitLayoutHints {
    /// Zoom threshold where full content should be visible.
    #[serde(default = "NodeKitLayoutHints::default_full_zoom_min")]
    pub full_zoom_min: f32,
    /// Zoom threshold where compact content should be visible.
    #[serde(default = "NodeKitLayoutHints::default_compact_zoom_min")]
    pub compact_zoom_min: f32,
    /// Default vertical spacing between field rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_spacing: Option<f32>,
    /// Default vertical spacing between action rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_spacing: Option<f32>,
    /// Human-readable measurement guidance for adapters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurement_note: Option<String>,
}

/// Coarse surface density tiers derived from a kit's zoom hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKitContentDensity {
    Compact,
    Regular,
    Full,
}

impl Default for NodeKitLayoutHints {
    fn default() -> Self {
        Self {
            full_zoom_min: Self::default_full_zoom_min(),
            compact_zoom_min: Self::default_compact_zoom_min(),
            field_spacing: None,
            action_spacing: None,
            measurement_note: None,
        }
    }
}

impl NodeKitLayoutHints {
    fn default_full_zoom_min() -> f32 {
        0.62
    }

    fn default_compact_zoom_min() -> f32 {
        0.18
    }

    pub fn with_full_zoom_min(mut self, zoom_min: f32) -> Self {
        self.full_zoom_min = zoom_min;
        self
    }

    pub fn with_compact_zoom_min(mut self, zoom_min: f32) -> Self {
        self.compact_zoom_min = zoom_min;
        self
    }

    pub fn with_zoom_range(mut self, compact_zoom_min: f32, full_zoom_min: f32) -> Self {
        self.compact_zoom_min = compact_zoom_min;
        self.full_zoom_min = full_zoom_min;
        self
    }

    pub fn with_field_spacing(mut self, spacing: f32) -> Self {
        self.field_spacing = Some(spacing);
        self
    }

    pub fn with_action_spacing(mut self, spacing: f32) -> Self {
        self.action_spacing = Some(spacing);
        self
    }

    pub fn with_measurement_note(mut self, note: impl Into<String>) -> Self {
        self.measurement_note = Some(note.into());
        self
    }

    pub fn content_density_for_zoom(&self, zoom: f32) -> NodeKitContentDensity {
        let compact_zoom_min = self.compact_zoom_min.min(self.full_zoom_min);
        let full_zoom_min = self.full_zoom_min.max(self.compact_zoom_min);

        if zoom >= full_zoom_min {
            NodeKitContentDensity::Full
        } else if zoom >= compact_zoom_min {
            NodeKitContentDensity::Regular
        } else {
            NodeKitContentDensity::Compact
        }
    }
}

/// A node instance inside a kit fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKitFixtureNode {
    pub alias: String,
    pub kind: NodeKindKey,
    pub pos: CanvasPoint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl NodeKitFixtureNode {
    pub fn new(alias: impl Into<String>, kind: impl Into<NodeKindKey>, pos: CanvasPoint) -> Self {
        Self {
            alias: alias.into(),
            kind: kind.into(),
            pos,
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// An edge instance inside a kit fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKitFixtureEdge {
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "EdgeViewDescriptor::is_default")]
    pub view: EdgeViewDescriptor,
}

impl NodeKitFixtureEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>, kind: EdgeKind) -> Self {
        Self {
            from: from.into(),
            from_port: None,
            to: to.into(),
            to_port: None,
            kind,
            data: None,
            view: EdgeViewDescriptor::default(),
        }
    }

    pub fn with_from_port(mut self, from_port: impl Into<String>) -> Self {
        self.from_port = Some(from_port.into());
        self
    }

    pub fn with_to_port(mut self, to_port: impl Into<String>) -> Self {
        self.to_port = Some(to_port.into());
        self
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_view(mut self, view: EdgeViewDescriptor) -> Self {
        self.view = view;
        self
    }
}

/// A reusable fixture graph for a node kit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKitFixture {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeKitFixtureNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<NodeKitFixtureEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_node_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_edge_count: Option<usize>,
}

impl NodeKitFixture {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            expected_node_count: None,
            expected_edge_count: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn node(mut self, node: NodeKitFixtureNode) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn edge(mut self, edge: NodeKitFixtureEdge) -> Self {
        self.edges.push(edge);
        self
    }

    pub fn expect_counts(mut self, nodes: usize, edges: usize) -> Self {
        self.expected_node_count = Some(nodes);
        self.expected_edge_count = Some(edges);
        self
    }

    pub fn build_graph(
        &self,
        kit_key: &NodeKitKey,
        registry: &NodeRegistry,
    ) -> Result<Graph, NodeKitFixtureError> {
        let mut builder = GraphBuilder::new(GraphId::from_u128(stable_u128(&format!(
            "kit.fixture.graph::{}::{}",
            kit_key.0, self.key
        ))));
        let mut alias_to_kind = BTreeMap::new();
        let mut alias_to_ports: BTreeMap<String, BTreeMap<String, PortId>> = BTreeMap::new();

        for node in &self.nodes {
            let schema =
                registry
                    .get(&node.kind)
                    .ok_or_else(|| NodeKitFixtureError::MissingSchema {
                        fixture: self.key.clone(),
                        kind: node.kind.clone(),
                    })?;
            let node_id = NodeId::from_u128(stable_u128(&format!(
                "kit.fixture.node::{}::{}",
                self.key, node.alias
            )));
            let port_ids = schema
                .ports
                .iter()
                .map(|decl| {
                    PortId::from_u128(stable_u128(&format!(
                        "kit.fixture.port::{}::{}::{}",
                        self.key, node.alias, decl.key.0
                    )))
                })
                .collect::<Vec<_>>();
            let mut inst = schema
                .instantiate_with_ids(node_id, node.pos, port_ids)
                .map_err(|error| NodeKitFixtureError::Instantiation {
                    fixture: self.key.clone(),
                    kind: node.kind.clone(),
                    error,
                })?;

            if let Some(data) = &node.data {
                inst.node.data = data.clone();
            }

            alias_to_kind.insert(node.alias.clone(), node.kind.clone());
            let mut port_map = BTreeMap::new();
            for ((port_id, port), decl) in inst.ports.into_iter().zip(schema.ports.iter()) {
                port_map.insert(decl.key.0.clone(), port_id);
                builder = builder.with_port(port_id, port);
            }
            alias_to_ports.insert(node.alias.clone(), port_map);
            builder = builder.with_node(node_id, inst.node);
        }

        for edge in &self.edges {
            let from_kind = alias_to_kind.get(&edge.from).ok_or_else(|| {
                NodeKitFixtureError::MissingNodeAlias {
                    fixture: self.key.clone(),
                    alias: edge.from.clone(),
                }
            })?;
            let to_kind = alias_to_kind.get(&edge.to).ok_or_else(|| {
                NodeKitFixtureError::MissingNodeAlias {
                    fixture: self.key.clone(),
                    alias: edge.to.clone(),
                }
            })?;
            let from_schema =
                registry
                    .get(from_kind)
                    .ok_or_else(|| NodeKitFixtureError::MissingSchema {
                        fixture: self.key.clone(),
                        kind: from_kind.clone(),
                    })?;
            let to_schema =
                registry
                    .get(to_kind)
                    .ok_or_else(|| NodeKitFixtureError::MissingSchema {
                        fixture: self.key.clone(),
                        kind: to_kind.clone(),
                    })?;
            let from_port_key = resolve_fixture_port_key(
                from_schema,
                PortDirection::Out,
                edge.from_port.as_deref(),
            )?;
            let to_port_key =
                resolve_fixture_port_key(to_schema, PortDirection::In, edge.to_port.as_deref())?;
            let from_port_id = alias_to_ports
                .get(&edge.from)
                .and_then(|ports| ports.get(&from_port_key))
                .copied()
                .ok_or_else(|| NodeKitFixtureError::MissingPort {
                    fixture: self.key.clone(),
                    alias: edge.from.clone(),
                    port: from_port_key.clone(),
                })?;
            let to_port_id = alias_to_ports
                .get(&edge.to)
                .and_then(|ports| ports.get(&to_port_key))
                .copied()
                .ok_or_else(|| NodeKitFixtureError::MissingPort {
                    fixture: self.key.clone(),
                    alias: edge.to.clone(),
                    port: to_port_key.clone(),
                })?;

            let edge_id = EdgeId::from_u128(stable_u128(&format!(
                "kit.fixture.edge::{}::{}::{}:{}->{}:{}::{:?}",
                kit_key.0, self.key, edge.from, from_port_key, edge.to, to_port_key, edge.kind
            )));
            let mut record = Edge::new(edge.kind, from_port_id, to_port_id);
            if let Some(data) = &edge.data {
                record.data = data.clone();
            }
            record.view = edge.view.clone();
            builder = builder.with_edge(edge_id, record);
        }

        Ok(builder.build_unchecked())
    }
}

/// Error returned when a kit fixture cannot be materialized.
#[derive(Debug, thiserror::Error)]
pub enum NodeKitFixtureError {
    #[error("node kit fixture `{fixture}` missing node schema for `{kind:?}`")]
    MissingSchema { fixture: String, kind: NodeKindKey },
    #[error("node kit `{kit}` missing fixture `{fixture}`")]
    MissingFixture { kit: String, fixture: String },
    #[error("node kit registry missing kit `{kit:?}`")]
    MissingKit { kit: NodeKitKey },
    #[error("node kit fixture `{fixture}` missing node alias `{alias}`")]
    MissingNodeAlias { fixture: String, alias: String },
    #[error("node kit fixture `{fixture}` missing port `{port}` on node `{alias}`")]
    MissingPort {
        fixture: String,
        alias: String,
        port: String,
    },
    #[error("node kit fixture `{fixture}` failed to instantiate `{kind:?}`: {error}")]
    Instantiation {
        fixture: String,
        kind: NodeKindKey,
        error: super::NodeInstantiationError,
    },
}

/// A versioned package of semantic node families, layout hints, and fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKitManifest {
    pub key: NodeKitKey,
    pub title: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_adapters: Vec<NodeKitAdapterKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub layout_hints: NodeKitLayoutHints,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<NodeSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixtures: Vec<NodeKitFixture>,
}

impl NodeKitManifest {
    pub fn new(key: impl Into<NodeKitKey>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            version: 1,
            supported_adapters: Vec::new(),
            capabilities: Vec::new(),
            layout_hints: NodeKitLayoutHints::default(),
            recipes: Vec::new(),
            fixtures: Vec::new(),
        }
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn with_supported_adapter(mut self, adapter: impl Into<NodeKitAdapterKey>) -> Self {
        self.supported_adapters.push(adapter.into());
        self
    }

    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    pub fn with_layout_hints(mut self, layout_hints: NodeKitLayoutHints) -> Self {
        self.layout_hints = layout_hints;
        self
    }

    pub fn recipe(mut self, recipe: NodeSchema) -> Self {
        self.recipes.push(recipe);
        self
    }

    pub fn recipes(mut self, recipes: impl IntoIterator<Item = NodeSchema>) -> Self {
        self.recipes.extend(recipes);
        self
    }

    pub fn fixture(mut self, fixture: NodeKitFixture) -> Self {
        self.fixtures.push(fixture);
        self
    }

    pub fn fixtures(mut self, fixtures: impl IntoIterator<Item = NodeKitFixture>) -> Self {
        self.fixtures.extend(fixtures);
        self
    }

    pub fn recipe_for_kind(&self, kind: &NodeKindKey) -> Option<&NodeSchema> {
        self.recipes.iter().find(|recipe| &recipe.kind == kind)
    }

    pub fn fixture_for_key(&self, key: &str) -> Option<&NodeKitFixture> {
        self.fixtures.iter().find(|fixture| fixture.key == key)
    }

    pub fn layout_hints(&self) -> &NodeKitLayoutHints {
        &self.layout_hints
    }

    pub fn node_registry(&self) -> NodeRegistry {
        let mut registry = NodeRegistry::new();
        for recipe in &self.recipes {
            registry.register(recipe.clone());
        }
        registry
    }

    pub fn build_fixture_graph(&self, key: &str) -> Result<Graph, NodeKitFixtureError> {
        let fixture =
            self.fixture_for_key(key)
                .ok_or_else(|| NodeKitFixtureError::MissingFixture {
                    kit: self.key.0.clone(),
                    fixture: key.to_owned(),
                })?;
        fixture.build_graph(&self.key, &self.node_registry())
    }
}

/// Registry for reusable node kits.
#[derive(Default, Clone)]
pub struct NodeKitRegistry {
    kits: BTreeMap<NodeKitKey, NodeKitManifest>,
}

impl std::fmt::Debug for NodeKitRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeKitRegistry")
            .field("kit_count", &self.kits.len())
            .finish()
    }
}

impl NodeKitRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builtin() -> Self {
        builtin_node_kits()
    }

    pub fn register(&mut self, manifest: NodeKitManifest) -> &mut Self {
        self.kits.insert(manifest.key.clone(), manifest);
        self
    }

    pub fn manifest(&self, key: &NodeKitKey) -> Option<&NodeKitManifest> {
        self.kits.get(key)
    }

    pub fn manifest_for_kind(&self, kind: &NodeKindKey) -> Option<&NodeKitManifest> {
        self.manifests()
            .find(|manifest| manifest.recipe_for_kind(kind).is_some())
    }

    pub fn manifests(&self) -> impl Iterator<Item = &NodeKitManifest> {
        self.kits.values()
    }

    pub fn node_registry(&self) -> NodeRegistry {
        let mut registry = NodeRegistry::new();
        for manifest in self.manifests() {
            for recipe in &manifest.recipes {
                registry.register(recipe.clone());
            }
        }
        registry
    }

    pub fn recipe_for_kind(&self, kind: &NodeKindKey) -> Option<&NodeSchema> {
        self.manifests()
            .find_map(|manifest| manifest.recipe_for_kind(kind))
    }

    pub fn layout_hints_for_kind(&self, kind: &NodeKindKey) -> Option<&NodeKitLayoutHints> {
        self.manifest_for_kind(kind)
            .map(NodeKitManifest::layout_hints)
    }

    pub fn fixture_graph(
        &self,
        kit_key: &NodeKitKey,
        fixture_key: &str,
    ) -> Result<Graph, NodeKitFixtureError> {
        let manifest = self
            .manifest(kit_key)
            .ok_or_else(|| NodeKitFixtureError::MissingKit {
                kit: kit_key.clone(),
            })?;
        manifest.build_fixture_graph(fixture_key)
    }
}

fn resolve_fixture_port_key(
    schema: &NodeSchema,
    direction: PortDirection,
    requested: Option<&str>,
) -> Result<String, NodeKitFixtureError> {
    if let Some(requested) = requested {
        if schema.ports.iter().any(|decl| decl.key.0 == requested) {
            return Ok(requested.to_owned());
        }
        return Err(NodeKitFixtureError::MissingPort {
            fixture: schema.kind.0.clone(),
            alias: schema.title.clone(),
            port: requested.to_owned(),
        });
    }

    schema
        .ports
        .iter()
        .find(|decl| decl.dir == direction)
        .map(|decl| decl.key.0.clone())
        .ok_or_else(|| NodeKitFixtureError::MissingPort {
            fixture: schema.kind.0.clone(),
            alias: schema.title.clone(),
            port: format!("{direction:?}"),
        })
}

fn stable_u128(text: &str) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = OFFSET;
    for byte in text.as_bytes() {
        hash ^= *byte as u128;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
