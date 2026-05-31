use jellyflow_core::core::{Graph, Node, NodeId, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
use serde_json::Value;

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
        MigrateNodesPlanner::new(self, graph).finish()
    }
}

struct MigrateNodesPlanner<'a> {
    registry: &'a NodeRegistry,
    graph: &'a Graph,
    tx: GraphTransaction,
    report: MigrateNodesReport,
}

impl<'a> MigrateNodesPlanner<'a> {
    fn new(registry: &'a NodeRegistry, graph: &'a Graph) -> Self {
        Self {
            registry,
            graph,
            tx: GraphTransaction::new().with_label("Migrate node kinds"),
            report: MigrateNodesReport::default(),
        }
    }

    fn finish(mut self) -> MigrateNodesPlan {
        for (id, node) in &self.graph.nodes {
            self.plan_node(*id, node);
        }

        MigrateNodesPlan {
            tx: self.tx,
            report: self.report,
        }
    }

    fn plan_node(&mut self, id: NodeId, node: &Node) {
        let canonical = self.registry.resolve_kind(&node.kind).clone();
        let Some(schema) = self.registry.get(&canonical) else {
            self.report.missing_schema.push(NodeMigrationMissingSchema {
                node: id,
                kind: node.kind.clone(),
            });
            return;
        };

        let latest_kind_version = schema.latest_kind_version;
        self.push_kind_canonicalization(id, node, &canonical);

        if node.kind_version == latest_kind_version {
            return;
        }
        if node.kind_version > latest_kind_version {
            self.report
                .newer_than_schema
                .push(NodeMigrationNewerThanSchema {
                    node: id,
                    kind: canonical,
                    node_kind_version: node.kind_version,
                    schema_latest_kind_version: latest_kind_version,
                });
            return;
        }

        let Some(migrator) = self.registry.migrators.get(&canonical) else {
            self.report
                .missing_migrator
                .push(NodeMigrationMissingMigrator {
                    node: id,
                    kind: canonical,
                    from: node.kind_version,
                    to: latest_kind_version,
                });
            return;
        };

        match migrator.migrate(node.kind_version, latest_kind_version, &node.data) {
            Ok(new_data) => {
                self.push_node_upgrade(id, node, canonical, latest_kind_version, new_data)
            }
            Err(err) => self.report.errors.push(NodeMigrationErrorEntry {
                node: id,
                kind: canonical,
                from: node.kind_version,
                to: latest_kind_version,
                message: err.to_string(),
            }),
        }
    }

    fn push_kind_canonicalization(&mut self, id: NodeId, node: &Node, canonical: &NodeKindKey) {
        if canonical != &node.kind {
            self.tx.push(GraphOp::SetNodeKind {
                id,
                from: node.kind.clone(),
                to: canonical.clone(),
            });
        }
    }

    fn push_node_upgrade(
        &mut self,
        id: NodeId,
        node: &Node,
        canonical: NodeKindKey,
        latest_kind_version: u32,
        new_data: Value,
    ) {
        self.tx.push(GraphOp::SetNodeData {
            id,
            from: node.data.clone(),
            to: new_data,
        });
        self.tx.push(GraphOp::SetNodeKindVersion {
            id,
            from: node.kind_version,
            to: latest_kind_version,
        });
        self.report.upgraded.push(NodeMigrationUpgraded {
            node: id,
            kind: canonical,
            from: node.kind_version,
            to: latest_kind_version,
        });
    }
}
