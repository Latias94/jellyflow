use std::collections::BTreeMap;
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, NodeId, NodeKindKey, PortId};

use super::migration::NodeKindMigrator;
use super::types::{NodeInstantiation, NodeInstantiationError, NodeKindViewDescriptor, NodeSchema};

mod plans;

/// Registry for node kinds.
#[derive(Default, Clone)]
pub struct NodeRegistry {
    by_kind: BTreeMap<NodeKindKey, NodeSchema>,
    by_alias: BTreeMap<NodeKindKey, NodeKindKey>,
    migrators: BTreeMap<NodeKindKey, Arc<dyn NodeKindMigrator>>,
}

impl std::fmt::Debug for NodeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRegistry")
            .field("schema_count", &self.by_kind.len())
            .field("alias_count", &self.by_alias.len())
            .field("migrator_count", &self.migrators.len())
            .finish()
    }
}

impl NodeRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a schema.
    ///
    /// Aliases are mapped to the schema's canonical kind.
    pub fn register(&mut self, schema: NodeSchema) {
        for alias in &schema.kind_aliases {
            self.by_alias.insert(alias.clone(), schema.kind.clone());
        }
        self.by_kind.insert(schema.kind.clone(), schema);
    }

    /// Registers a per-kind data migrator.
    ///
    /// The migrator is stored as an in-memory hook (not serialized as part of the schema data).
    pub fn register_migrator(
        &mut self,
        kind: NodeKindKey,
        migrator: Arc<dyn NodeKindMigrator>,
    ) -> &mut Self {
        self.migrators.insert(kind, migrator);
        self
    }

    /// Resolves an input kind to a canonical kind (via aliases).
    pub fn resolve_kind<'a>(&'a self, kind: &'a NodeKindKey) -> &'a NodeKindKey {
        self.by_alias.get(kind).unwrap_or(kind)
    }

    /// Looks up a schema by canonical kind key.
    pub fn get(&self, kind: &NodeKindKey) -> Option<&NodeSchema> {
        self.by_kind.get(kind)
    }

    /// Iterates all registered schemas in deterministic order (by kind key).
    pub fn schemas(&self) -> impl Iterator<Item = &NodeSchema> {
        self.by_kind.values()
    }

    /// Returns the adapter-facing descriptor for a node kind or alias.
    pub fn view_descriptor(&self, kind: &NodeKindKey) -> Option<NodeKindViewDescriptor> {
        let canonical = self.resolve_kind(kind);
        self.get(canonical).map(NodeKindViewDescriptor::from_schema)
    }

    /// Returns adapter-facing node-kind descriptors in deterministic order.
    pub fn view_descriptors(&self) -> Vec<NodeKindViewDescriptor> {
        self.schemas()
            .map(NodeKindViewDescriptor::from_schema)
            .collect()
    }

    /// Instantiates a node kind or alias with freshly allocated node and port ids.
    pub fn instantiate_node(
        &self,
        kind: &NodeKindKey,
        pos: CanvasPoint,
    ) -> Result<NodeInstantiation, NodeInstantiationError> {
        let canonical = self.resolve_kind(kind);
        self.get(canonical)
            .map(|schema| schema.instantiate(pos))
            .ok_or_else(|| NodeInstantiationError::MissingSchema(kind.clone()))
    }

    /// Instantiates a node kind or alias with caller-provided ids.
    pub fn instantiate_node_with_ids(
        &self,
        kind: &NodeKindKey,
        node_id: NodeId,
        pos: CanvasPoint,
        port_ids: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeInstantiation, NodeInstantiationError> {
        let canonical = self.resolve_kind(kind);
        self.get(canonical)
            .ok_or_else(|| NodeInstantiationError::MissingSchema(kind.clone()))?
            .instantiate_with_ids(node_id, pos, port_ids)
    }
}
