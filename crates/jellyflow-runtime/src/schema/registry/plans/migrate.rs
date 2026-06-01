use jellyflow_core::core::{Graph, Node, NodeId, NodeKindKey};
use serde_json::Value;

use crate::schema::migration::{MigrateNodesPlan, MigrateNodesReport};
use crate::schema::registry::NodeRegistry;

use super::writer::NodeKindTxWriter;

pub(super) struct MigrateNodesPlanner<'a> {
    registry: &'a NodeRegistry,
    graph: &'a Graph,
    tx: NodeKindTxWriter,
    report: MigrateNodesReport,
}

impl<'a> MigrateNodesPlanner<'a> {
    pub(super) fn new(registry: &'a NodeRegistry, graph: &'a Graph) -> Self {
        Self {
            registry,
            graph,
            tx: NodeKindTxWriter::new("Migrate node kinds"),
            report: MigrateNodesReport::default(),
        }
    }

    pub(super) fn finish(mut self) -> MigrateNodesPlan {
        for (id, node) in &self.graph.nodes {
            self.plan_node(*id, node);
        }

        MigrateNodesPlan::from_parts(self.tx.into_tx(), self.report)
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
