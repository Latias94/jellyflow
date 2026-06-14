mod canonicalize;
mod migrate;
mod writer;

use jellyflow_core::core::Graph;

use super::NodeRegistry;
use crate::schema::migration::{CanonicalizeKindsPlan, MigrateNodesPlan};

impl NodeRegistry {
    /// Plans a transaction that rewrites aliased node kinds to their canonical kind.
    pub fn plan_canonicalize_kinds(&self, graph: &Graph) -> CanonicalizeKindsPlan {
        let mut planner = canonicalize::CanonicalizeKindsPlanner::new();

        for (id, node) in graph.nodes() {
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
        migrate::MigrateNodesPlanner::new(self, graph).finish()
    }
}
