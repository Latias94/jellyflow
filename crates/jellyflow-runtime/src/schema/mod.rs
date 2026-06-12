//! Node and port schema registry.

mod migration;
mod registry;
mod types;

pub use migration::{
    CanonicalizeKindsPlan, MigrateNodesPlan, MigrateNodesReport, NodeKindMigrateError,
    NodeKindMigrator, NodeKindRewrite, NodeMigrationErrorEntry, NodeMigrationMissingMigrator,
    NodeMigrationMissingSchema, NodeMigrationNewerThanSchema, NodeMigrationUpgraded,
};
pub use registry::NodeRegistry;
pub use types::{NodeKindViewDescriptor, NodeSchema, PortDecl};

#[cfg(test)]
mod tests;
