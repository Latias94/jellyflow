use std::collections::BTreeMap;
use std::sync::Arc;

use jellyflow_core::core::{Graph, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::migration::{
    CanonicalizeKindsPlan, MigrateNodesPlan, MigrateNodesReport, NodeKindMigrator, NodeKindRewrite,
    NodeMigrationErrorEntry, NodeMigrationMissingMigrator, NodeMigrationMissingSchema,
    NodeMigrationNewerThanSchema, NodeMigrationUpgraded,
};
use super::types::NodeSchema;

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

    /// Plans a transaction that rewrites aliased node kinds to their canonical kind.
    pub fn plan_canonicalize_kinds(&self, graph: &Graph) -> CanonicalizeKindsPlan {
        let mut tx = GraphTransaction::new().with_label("Canonicalize node kinds");
        let mut rewrites: Vec<NodeKindRewrite> = Vec::new();

        for (id, node) in &graph.nodes {
            let canonical = self.resolve_kind(&node.kind);
            if canonical == &node.kind {
                continue;
            }
            tx.push(GraphOp::SetNodeKind {
                id: *id,
                from: node.kind.clone(),
                to: canonical.clone(),
            });
            rewrites.push(NodeKindRewrite {
                node: *id,
                from: node.kind.clone(),
                to: canonical.clone(),
            });
        }

        CanonicalizeKindsPlan { tx, rewrites }
    }

    /// Plans a transaction that upgrades node payloads to the latest registered kind version.
    ///
    /// This plan is intentionally best-effort:
    /// - missing schema -> recorded as a report entry, no edits are produced
    /// - missing migrator -> recorded as a report entry, no edits are produced
    /// - migrator errors -> recorded as a report entry, no edits are produced for that node
    ///
    /// The returned transaction may also include `SetNodeKind` ops for aliased kinds.
    pub fn plan_migrate_nodes(&self, graph: &Graph) -> MigrateNodesPlan {
        let mut tx = GraphTransaction::new().with_label("Migrate node kinds");
        let mut report = MigrateNodesReport::default();

        for (id, node) in &graph.nodes {
            let canonical = self.resolve_kind(&node.kind);
            let schema = self.get(canonical);
            let Some(schema) = schema else {
                report.missing_schema.push(NodeMigrationMissingSchema {
                    node: *id,
                    kind: node.kind.clone(),
                });
                continue;
            };

            if canonical != &node.kind {
                tx.push(GraphOp::SetNodeKind {
                    id: *id,
                    from: node.kind.clone(),
                    to: canonical.clone(),
                });
            }

            if node.kind_version == schema.latest_kind_version {
                continue;
            }
            if node.kind_version > schema.latest_kind_version {
                report.newer_than_schema.push(NodeMigrationNewerThanSchema {
                    node: *id,
                    kind: canonical.clone(),
                    node_kind_version: node.kind_version,
                    schema_latest_kind_version: schema.latest_kind_version,
                });
                continue;
            }

            let Some(migrator) = self.migrators.get(canonical) else {
                report.missing_migrator.push(NodeMigrationMissingMigrator {
                    node: *id,
                    kind: canonical.clone(),
                    from: node.kind_version,
                    to: schema.latest_kind_version,
                });
                continue;
            };

            match migrator.migrate(node.kind_version, schema.latest_kind_version, &node.data) {
                Ok(new_data) => {
                    tx.push(GraphOp::SetNodeData {
                        id: *id,
                        from: node.data.clone(),
                        to: new_data,
                    });
                    tx.push(GraphOp::SetNodeKindVersion {
                        id: *id,
                        from: node.kind_version,
                        to: schema.latest_kind_version,
                    });
                    report.upgraded.push(NodeMigrationUpgraded {
                        node: *id,
                        kind: canonical.clone(),
                        from: node.kind_version,
                        to: schema.latest_kind_version,
                    });
                }
                Err(err) => report.errors.push(NodeMigrationErrorEntry {
                    node: *id,
                    kind: canonical.clone(),
                    from: node.kind_version,
                    to: schema.latest_kind_version,
                    message: err.to_string(),
                }),
            }
        }

        MigrateNodesPlan { tx, report }
    }
}
