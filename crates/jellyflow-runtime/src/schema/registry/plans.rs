use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::NodeRegistry;
use crate::schema::migration::{
    CanonicalizeKindsPlan, MigrateNodesPlan, MigrateNodesReport, NodeKindRewrite,
    NodeMigrationErrorEntry, NodeMigrationMissingMigrator, NodeMigrationMissingSchema,
    NodeMigrationNewerThanSchema, NodeMigrationUpgraded,
};

impl NodeRegistry {
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
