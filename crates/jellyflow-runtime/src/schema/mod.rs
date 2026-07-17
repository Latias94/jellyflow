//! Semantic node-kind catalog and migration contracts.

mod catalog;
mod migration;

pub use catalog::{
    CanonicalGraph, CreateFromWireCandidate, NodeCatalog, NodeCatalogInstantiationError,
    NodeCatalogMigrationError, NodeCatalogMigrationReport, NodeCatalogRegistrationError,
    NodeCreation, NodeKindCanonicalization, NodeKindMigrationIssue, NodeKindSpec,
    NodeKindSpecBuilder, NodeKindUpgrade, NodePortSpec, OpaqueNodeKind, PortPresentationHint,
    PortSide,
};
pub use migration::{NodeKindMigrateError, NodeKindMigrator};

#[cfg(test)]
mod tests;
