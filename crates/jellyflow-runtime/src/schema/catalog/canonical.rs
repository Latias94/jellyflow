use crate::graph_kernel::{GraphOp, GraphTransaction};
use std::sync::Arc;

use jellyflow_core::core::{
    Graph, GraphValidationError, NodeId, NodeKindKey, validate_graph_storage,
    validate_graph_structural,
};

use crate::io::{
    PayloadCounter, PersistenceLimitError, PersistenceLimits, PersistenceResource,
    validate_graph_resources,
};

use super::NodeCatalog;

/// One known-kind incompatibility that prevents canonical graph publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKindMigrationIssue {
    MissingMigrator {
        node: NodeId,
        kind: NodeKindKey,
        from: u32,
        to: u32,
    },
    FutureVersion {
        node: NodeId,
        kind: NodeKindKey,
        found: u32,
        supported: u32,
    },
    MigrationFailed {
        node: NodeId,
        kind: NodeKindKey,
        from: u32,
        to: u32,
        message: String,
    },
}

/// Failure to produce a fully validated canonical graph.
#[derive(Debug, thiserror::Error)]
pub enum NodeCatalogMigrationError {
    #[error(transparent)]
    ResourceLimit(#[from] PersistenceLimitError),
    #[error("source graph failed storage validation: {errors:?}")]
    InvalidSource { errors: Vec<GraphValidationError> },
    #[error(
        "registered node kinds are incompatible with the document: {issues:?}; {omitted_count} additional issues omitted"
    )]
    IncompatibleKinds {
        issues: Vec<NodeKindMigrationIssue>,
        omitted_count: usize,
    },
    #[error("failed to apply node-kind migration to scratch graph: {message}")]
    InternalMutation { message: String },
    #[error("canonical graph failed storage validation: {errors:?}")]
    InvalidResult { errors: Vec<GraphValidationError> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeKindUpgrade {
    node: NodeId,
    kind: NodeKindKey,
    from: u32,
    to: u32,
}

impl NodeKindUpgrade {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    pub fn from(&self) -> u32 {
        self.from
    }

    pub fn to(&self) -> u32 {
        self.to
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeKindCanonicalization {
    node: NodeId,
    from: NodeKindKey,
    to: NodeKindKey,
}

impl NodeKindCanonicalization {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn from(&self) -> &NodeKindKey {
        &self.from
    }

    pub fn to(&self) -> &NodeKindKey {
        &self.to
    }
}

/// Unknown kind retained without interpretation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueNodeKind {
    node: NodeId,
    kind: NodeKindKey,
    version: u32,
}

impl OpaqueNodeKind {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn kind(&self) -> &NodeKindKey {
        &self.kind
    }

    pub fn version(&self) -> u32 {
        self.version
    }
}

/// Evidence produced while canonicalizing one graph document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeCatalogMigrationReport {
    upgraded: Vec<NodeKindUpgrade>,
    canonicalized: Vec<NodeKindCanonicalization>,
    opaque: Vec<OpaqueNodeKind>,
}

impl NodeCatalogMigrationReport {
    pub fn upgraded(&self) -> &[NodeKindUpgrade] {
        &self.upgraded
    }

    pub fn canonicalized(&self) -> &[NodeKindCanonicalization] {
        &self.canonicalized
    }

    pub fn opaque(&self) -> &[OpaqueNodeKind] {
        &self.opaque
    }

    pub fn has_changes(&self) -> bool {
        !self.upgraded.is_empty() || !self.canonicalized.is_empty()
    }
}

struct MigrationIssueCollector {
    issues: Vec<NodeKindMigrationIssue>,
    omitted_count: usize,
    max_issues: usize,
    max_message_bytes: usize,
    message_bytes: usize,
}

impl MigrationIssueCollector {
    fn new(limits: PersistenceLimits) -> Self {
        Self {
            issues: Vec::new(),
            omitted_count: 0,
            max_issues: limits.max_migration_issues(),
            max_message_bytes: limits.max_migration_issue_message_bytes(),
            message_bytes: 0,
        }
    }

    fn push(&mut self, issue: NodeKindMigrationIssue) {
        let message_bytes = match &issue {
            NodeKindMigrationIssue::MigrationFailed { message, .. } => message.len(),
            NodeKindMigrationIssue::MissingMigrator { .. }
            | NodeKindMigrationIssue::FutureVersion { .. } => 0,
        };
        let message_fits =
            message_bytes <= self.max_message_bytes.saturating_sub(self.message_bytes);
        if self.issues.len() >= self.max_issues || !message_fits {
            self.omitted_count = self.omitted_count.saturating_add(1);
            return;
        }
        self.message_bytes = self.message_bytes.saturating_add(message_bytes);
        self.issues.push(issue);
    }

    fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.omitted_count == 0
    }

    fn into_parts(self) -> (Vec<NodeKindMigrationIssue>, usize) {
        (self.issues, self.omitted_count)
    }
}

/// A graph that passed source validation, all known-kind migrations, and result validation.
#[derive(Debug, Clone)]
pub struct CanonicalGraph {
    graph: Graph,
    report: NodeCatalogMigrationReport,
    catalog: Arc<NodeCatalog>,
}

impl CanonicalGraph {
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn report(&self) -> &NodeCatalogMigrationReport {
        &self.report
    }

    /// Returns the semantic catalog snapshot that produced this canonical graph.
    pub fn catalog(&self) -> &NodeCatalog {
        &self.catalog
    }

    /// Clones the read-only semantic catalog provenance for a long-lived consumer.
    pub fn catalog_owned(&self) -> Arc<NodeCatalog> {
        Arc::clone(&self.catalog)
    }

    pub fn into_graph(self) -> Graph {
        self.graph
    }

    pub(crate) fn into_parts(self) -> (Graph, Arc<NodeCatalog>) {
        (self.graph, self.catalog)
    }

    pub(crate) fn shares_catalog_provenance_with(&self, catalog: &NodeCatalog) -> bool {
        self.catalog.shares_provenance_with(catalog)
    }
}

impl NodeCatalog {
    /// Canonicalizes an immutable source graph in scratch state.
    ///
    /// Unknown kinds remain opaque. Any problem with a registered kind rejects the entire result,
    /// so callers can never publish a partially migrated graph.
    pub fn canonicalize_graph(
        &self,
        source: &Graph,
    ) -> Result<CanonicalGraph, NodeCatalogMigrationError> {
        self.canonicalize_graph_with_limits(source, PersistenceLimits::default())
    }

    /// Canonicalizes an immutable source graph under explicit persistence and migration budgets.
    pub fn canonicalize_graph_with_limits(
        &self,
        source: &Graph,
        limits: PersistenceLimits,
    ) -> Result<CanonicalGraph, NodeCatalogMigrationError> {
        validate_graph_resources(source, limits)?;
        let validation = validate_graph_storage(source);
        if !validation.is_ok() {
            return Err(NodeCatalogMigrationError::InvalidSource {
                errors: validation.into_errors(),
            });
        }
        limits.ensure(
            PersistenceResource::MigrationOperations,
            self.required_migration_operations(source),
        )?;

        let mut transaction = GraphTransaction::new().with_label("Canonicalize node kinds");
        let mut report = NodeCatalogMigrationReport::default();
        let mut issues = MigrationIssueCollector::new(limits);
        let mut migrated_payloads = PayloadCounter::new(limits);

        for (node_id, node) in source.nodes() {
            let Some(spec) = self.get(&node.kind) else {
                report.opaque.push(OpaqueNodeKind {
                    node: *node_id,
                    kind: node.kind.clone(),
                    version: node.kind_version,
                });
                continue;
            };
            let canonical_kind = spec.kind().clone();
            let supported = spec.latest_kind_version();
            if node.kind_version > supported {
                issues.push(NodeKindMigrationIssue::FutureVersion {
                    node: *node_id,
                    kind: canonical_kind,
                    found: node.kind_version,
                    supported,
                });
                continue;
            }

            let migrated_data = if node.kind_version < supported {
                let Some(migrator) = self.migrator(&canonical_kind) else {
                    issues.push(NodeKindMigrationIssue::MissingMigrator {
                        node: *node_id,
                        kind: canonical_kind,
                        from: node.kind_version,
                        to: supported,
                    });
                    continue;
                };
                match migrator.migrate(node.kind_version, supported, &node.data) {
                    Ok(data) => {
                        migrated_payloads.visit_json(&data, 1)?;
                        Some(data)
                    }
                    Err(error) => {
                        issues.push(NodeKindMigrationIssue::MigrationFailed {
                            node: *node_id,
                            kind: canonical_kind,
                            from: node.kind_version,
                            to: supported,
                            message: error.into_message(),
                        });
                        continue;
                    }
                }
            } else {
                None
            };

            if node.kind != canonical_kind {
                transaction.push(GraphOp::SetNodeKind {
                    id: *node_id,
                    from: node.kind.clone(),
                    to: canonical_kind.clone(),
                });
                report.canonicalized.push(NodeKindCanonicalization {
                    node: *node_id,
                    from: node.kind.clone(),
                    to: canonical_kind.clone(),
                });
            }
            if let Some(data) = migrated_data {
                if data != node.data {
                    transaction.push(GraphOp::SetNodeData {
                        id: *node_id,
                        from: node.data.clone(),
                        to: data,
                    });
                }
                transaction.push(GraphOp::SetNodeKindVersion {
                    id: *node_id,
                    from: node.kind_version,
                    to: supported,
                });
                report.upgraded.push(NodeKindUpgrade {
                    node: *node_id,
                    kind: canonical_kind,
                    from: node.kind_version,
                    to: supported,
                });
            }
        }

        if !issues.is_empty() {
            let (issues, omitted_count) = issues.into_parts();
            return Err(NodeCatalogMigrationError::IncompatibleKinds {
                issues,
                omitted_count,
            });
        }

        let mut graph = source.clone();
        transaction.apply_to(&mut graph).map_err(|source| {
            NodeCatalogMigrationError::InternalMutation {
                message: source.to_string(),
            }
        })?;
        validate_graph_resources(&graph, limits)?;
        let validation = validate_graph_structural(&graph);
        if !validation.is_ok() {
            return Err(NodeCatalogMigrationError::InvalidResult {
                errors: validation.into_errors(),
            });
        }

        Ok(CanonicalGraph {
            graph,
            report,
            catalog: Arc::new(self.clone()),
        })
    }
}
