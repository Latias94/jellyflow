use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, NodeId, NodeKindKey, PortId, PortKey};

use super::migration::NodeKindMigrator;
mod candidates;
mod canonical;
mod creation;
mod spec;

pub use candidates::CreateFromWireCandidate;
pub use canonical::{
    CanonicalGraph, NodeCatalogMigrationError, NodeCatalogMigrationReport,
    NodeKindCanonicalization, NodeKindMigrationIssue, NodeKindUpgrade, OpaqueNodeKind,
};
pub use creation::NodeCreation;
pub use spec::{NodeKindSpec, NodeKindSpecBuilder, NodePortSpec};

/// Invalid or conflicting semantic catalog registration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeCatalogRegistrationError {
    #[error("node kind key must not be empty")]
    EmptyKind,
    #[error("node kind `{kind:?}` must declare a positive latest version")]
    InvalidKindVersion { kind: NodeKindKey },
    #[error("node kind `{kind:?}` declares duplicate port key `{port:?}`")]
    DuplicatePortKey { kind: NodeKindKey, port: PortKey },
    #[error("node kind `{kind:?}` declares invalid alias `{alias:?}`")]
    InvalidAlias {
        kind: NodeKindKey,
        alias: NodeKindKey,
    },
    #[error("node kind `{kind:?}` is already registered")]
    DuplicateKind { kind: NodeKindKey },
    #[error("node kind `{kind:?}` conflicts with registered alias for `{canonical:?}`")]
    KindConflictsWithAlias {
        kind: NodeKindKey,
        canonical: NodeKindKey,
    },
    #[error("node alias `{alias:?}` conflicts with registered kind or alias")]
    AliasConflict { alias: NodeKindKey },
    #[error("cannot register migrator for unknown node kind `{kind:?}`")]
    MissingMigratorKind { kind: NodeKindKey },
    #[error("node kind migrator is already registered for `{kind:?}`")]
    DuplicateMigrator { kind: NodeKindKey },
}

/// Failure to instantiate a semantic node kind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeCatalogInstantiationError {
    #[error("node kind is not registered: {kind:?}")]
    MissingKind { kind: NodeKindKey },
    #[error("initial node bounds must have a finite origin and positive finite size")]
    InvalidInitialBounds,
    #[error("port id count mismatch: expected {expected}, got {actual}")]
    PortIdCountMismatch { expected: usize, actual: usize },
    #[error("port id {port:?} was supplied more than once")]
    DuplicatePortId { port: PortId },
}

#[derive(Debug)]
struct NodeCatalogProvenance;

/// Host/application-owned semantic definitions for node creation and kind migration.
#[derive(Clone)]
pub struct NodeCatalog {
    provenance: Arc<NodeCatalogProvenance>,
    by_kind: BTreeMap<NodeKindKey, NodeKindSpec>,
    by_alias: BTreeMap<NodeKindKey, NodeKindKey>,
    migrators: BTreeMap<NodeKindKey, Arc<dyn NodeKindMigrator>>,
}

impl Default for NodeCatalog {
    fn default() -> Self {
        Self {
            provenance: Arc::new(NodeCatalogProvenance),
            by_kind: BTreeMap::new(),
            by_alias: BTreeMap::new(),
            migrators: BTreeMap::new(),
        }
    }
}

impl std::fmt::Debug for NodeCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeCatalog")
            .field("kind_count", &self.by_kind.len())
            .field("alias_count", &self.by_alias.len())
            .field("migrator_count", &self.migrators.len())
            .finish()
    }
}

impl NodeCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one kind without overwriting any existing kind or alias.
    pub fn register(
        &mut self,
        spec: NodeKindSpec,
    ) -> Result<&mut Self, NodeCatalogRegistrationError> {
        validate_spec(&spec)?;
        if self.by_kind.contains_key(spec.kind()) {
            return Err(NodeCatalogRegistrationError::DuplicateKind {
                kind: spec.kind().clone(),
            });
        }
        if let Some(canonical) = self.by_alias.get(spec.kind()) {
            return Err(NodeCatalogRegistrationError::KindConflictsWithAlias {
                kind: spec.kind().clone(),
                canonical: canonical.clone(),
            });
        }
        if let Some(alias) = spec
            .aliases()
            .iter()
            .find(|alias| self.by_kind.contains_key(*alias) || self.by_alias.contains_key(*alias))
        {
            return Err(NodeCatalogRegistrationError::AliasConflict {
                alias: alias.clone(),
            });
        }

        for alias in spec.aliases() {
            self.by_alias.insert(alias.clone(), spec.kind().clone());
        }
        self.by_kind.insert(spec.kind().clone(), spec);
        self.fork_provenance();
        Ok(self)
    }

    /// Registers an in-memory migrator for one canonical kind or alias.
    pub fn register_migrator(
        &mut self,
        kind: NodeKindKey,
        migrator: Arc<dyn NodeKindMigrator>,
    ) -> Result<&mut Self, NodeCatalogRegistrationError> {
        let Some(canonical) = self.resolve_kind(&kind).cloned() else {
            return Err(NodeCatalogRegistrationError::MissingMigratorKind { kind });
        };
        if self.migrators.contains_key(&canonical) {
            return Err(NodeCatalogRegistrationError::DuplicateMigrator { kind: canonical });
        }
        self.migrators.insert(canonical, migrator);
        self.fork_provenance();
        Ok(self)
    }

    /// Resolves a registered canonical kind or alias.
    pub fn resolve_kind(&self, kind: &NodeKindKey) -> Option<&NodeKindKey> {
        self.by_kind
            .get_key_value(kind)
            .map(|(canonical, _)| canonical)
            .or_else(|| self.by_alias.get(kind))
    }

    /// Returns a kind spec by canonical key or alias.
    pub fn get(&self, kind: &NodeKindKey) -> Option<&NodeKindSpec> {
        self.resolve_kind(kind)
            .and_then(|canonical| self.by_kind.get(canonical))
    }

    /// Iterates canonical specs in deterministic kind-key order.
    pub fn kinds(&self) -> impl Iterator<Item = &NodeKindSpec> {
        self.by_kind.values()
    }

    pub fn instantiate_node(
        &self,
        kind: &NodeKindKey,
        bounds: CanvasRect,
    ) -> Result<NodeCreation, NodeCatalogInstantiationError> {
        let spec = self
            .get(kind)
            .ok_or_else(|| NodeCatalogInstantiationError::MissingKind { kind: kind.clone() })?;
        if !bounds.is_positive_finite() {
            return Err(NodeCatalogInstantiationError::InvalidInitialBounds);
        }
        Ok(spec.instantiate(bounds))
    }

    pub fn instantiate_node_with_ids(
        &self,
        kind: &NodeKindKey,
        node: NodeId,
        bounds: CanvasRect,
        ports: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeCreation, NodeCatalogInstantiationError> {
        let spec = self
            .get(kind)
            .ok_or_else(|| NodeCatalogInstantiationError::MissingKind { kind: kind.clone() })?;
        if !bounds.is_positive_finite() {
            return Err(NodeCatalogInstantiationError::InvalidInitialBounds);
        }
        spec.instantiate_with_ids(node, bounds, ports)
    }

    pub(super) fn migrator(&self, kind: &NodeKindKey) -> Option<&Arc<dyn NodeKindMigrator>> {
        self.migrators.get(kind)
    }

    pub(crate) fn required_migration_operations(
        &self,
        graph: &jellyflow_core::core::Graph,
    ) -> usize {
        graph
            .nodes()
            .values()
            .filter(|node| {
                self.get(&node.kind).is_some_and(|spec| {
                    node.kind_version < spec.latest_kind_version()
                        && self.migrator(spec.kind()).is_some()
                })
            })
            .count()
    }

    pub(crate) fn shares_provenance_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.provenance, &other.provenance)
    }

    fn fork_provenance(&mut self) {
        self.provenance = Arc::new(NodeCatalogProvenance);
    }
}

fn validate_spec(spec: &NodeKindSpec) -> Result<(), NodeCatalogRegistrationError> {
    if spec.kind().0.trim().is_empty() {
        return Err(NodeCatalogRegistrationError::EmptyKind);
    }
    if spec.latest_kind_version() == 0 {
        return Err(NodeCatalogRegistrationError::InvalidKindVersion {
            kind: spec.kind().clone(),
        });
    }
    let mut ports = BTreeSet::new();
    for port in spec.ports() {
        if !ports.insert(port.key().clone()) {
            return Err(NodeCatalogRegistrationError::DuplicatePortKey {
                kind: spec.kind().clone(),
                port: port.key().clone(),
            });
        }
    }

    let mut aliases = BTreeSet::new();
    for alias in spec.aliases() {
        if alias.0.trim().is_empty() || alias == spec.kind() || !aliases.insert(alias.clone()) {
            return Err(NodeCatalogRegistrationError::InvalidAlias {
                kind: spec.kind().clone(),
                alias: alias.clone(),
            });
        }
    }
    Ok(())
}
