use jellyflow_core::core::{Graph, Node, NodeId, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
use serde_json::Value;

use super::NodeRegistry;
use crate::schema::migration::{
    CanonicalizeKindsPlan, MigrateNodesPlan, MigrateNodesReport, NodeKindRewrite,
};

impl NodeRegistry {
    /// Plans a transaction that rewrites aliased node kinds to their canonical kind.
    pub fn plan_canonicalize_kinds(&self, graph: &Graph) -> CanonicalizeKindsPlan {
        let mut planner = CanonicalizeKindsPlanner::new();

        for (id, node) in &graph.nodes {
            let canonical = self.resolve_kind(&node.kind);
            planner.rewrite_node_kind(*id, node, canonical);
        }

        planner.finish()
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

struct NodeKindTxWriter {
    tx: GraphTransaction,
}

impl NodeKindTxWriter {
    fn new(label: &str) -> Self {
        Self {
            tx: GraphTransaction::new().with_label(label),
        }
    }

    fn rewrite_node_kind(&mut self, id: NodeId, node: &Node, canonical: &NodeKindKey) -> bool {
        if canonical == &node.kind {
            return false;
        }

        self.tx.push(GraphOp::SetNodeKind {
            id,
            from: node.kind.clone(),
            to: canonical.clone(),
        });
        true
    }

    fn update_node_data(&mut self, id: NodeId, node: &Node, new_data: Value) {
        self.tx.push(GraphOp::SetNodeData {
            id,
            from: node.data.clone(),
            to: new_data,
        });
    }

    fn update_node_kind_version(&mut self, id: NodeId, node: &Node, latest_kind_version: u32) {
        self.tx.push(GraphOp::SetNodeKindVersion {
            id,
            from: node.kind_version,
            to: latest_kind_version,
        });
    }

    fn into_tx(self) -> GraphTransaction {
        self.tx
    }
}

struct CanonicalizeKindsPlanner {
    tx: NodeKindTxWriter,
    rewrites: Vec<NodeKindRewrite>,
}

impl CanonicalizeKindsPlanner {
    fn new() -> Self {
        Self {
            tx: NodeKindTxWriter::new("Canonicalize node kinds"),
            rewrites: Vec::new(),
        }
    }

    fn rewrite_node_kind(&mut self, id: NodeId, node: &Node, canonical: &NodeKindKey) {
        if !self.tx.rewrite_node_kind(id, node, canonical) {
            return;
        }

        self.rewrites.push(NodeKindRewrite {
            node: id,
            from: node.kind.clone(),
            to: canonical.clone(),
        });
    }

    fn finish(self) -> CanonicalizeKindsPlan {
        CanonicalizeKindsPlan {
            tx: self.tx.into_tx(),
            rewrites: self.rewrites,
        }
    }
}

struct MigrateNodesPlanner<'a> {
    registry: &'a NodeRegistry,
    graph: &'a Graph,
    tx: NodeKindTxWriter,
    report: MigrateNodesReport,
}

impl<'a> MigrateNodesPlanner<'a> {
    fn new(registry: &'a NodeRegistry, graph: &'a Graph) -> Self {
        Self {
            registry,
            graph,
            tx: NodeKindTxWriter::new("Migrate node kinds"),
            report: MigrateNodesReport::default(),
        }
    }

    fn finish(mut self) -> MigrateNodesPlan {
        for (id, node) in &self.graph.nodes {
            self.plan_node(*id, node);
        }

        MigrateNodesPlan {
            tx: self.tx.into_tx(),
            report: self.report,
        }
    }

    fn plan_node(&mut self, id: NodeId, node: &Node) {
        let canonical = self.registry.resolve_kind(&node.kind).clone();
        let Some(schema) = self.registry.get(&canonical) else {
            self.report.push_missing_schema(id, node.kind.clone());
            return;
        };

        let latest_kind_version = schema.latest_kind_version;
        self.push_kind_canonicalization(id, node, &canonical);

        if node.kind_version == latest_kind_version {
            return;
        }
        if node.kind_version > latest_kind_version {
            self.report.push_newer_than_schema(
                id,
                canonical,
                node.kind_version,
                latest_kind_version,
            );
            return;
        }

        let Some(migrator) = self.registry.migrators.get(&canonical) else {
            self.report.push_missing_migrator(
                id,
                canonical,
                node.kind_version,
                latest_kind_version,
            );
            return;
        };

        match migrator.migrate(node.kind_version, latest_kind_version, &node.data) {
            Ok(new_data) => {
                self.push_node_upgrade(id, node, canonical, latest_kind_version, new_data)
            }
            Err(err) => self.report.push_error(
                id,
                canonical,
                node.kind_version,
                latest_kind_version,
                err.to_string(),
            ),
        }
    }

    fn push_kind_canonicalization(&mut self, id: NodeId, node: &Node, canonical: &NodeKindKey) {
        self.tx.rewrite_node_kind(id, node, canonical);
    }

    fn push_node_upgrade(
        &mut self,
        id: NodeId,
        node: &Node,
        canonical: NodeKindKey,
        latest_kind_version: u32,
        new_data: Value,
    ) {
        self.tx.update_node_data(id, node, new_data);
        self.tx
            .update_node_kind_version(id, node, latest_kind_version);
        self.report
            .push_upgraded(id, canonical, node.kind_version, latest_kind_version);
    }
}
