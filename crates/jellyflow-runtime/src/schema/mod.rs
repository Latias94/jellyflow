//! Node and port schema registry.

pub mod kit;
mod migration;
mod registry;
mod types;

pub use kit::{
    NodeKitAdapterKey, NodeKitContentDensity, NodeKitFixture, NodeKitFixtureEdge,
    NodeKitFixtureError, NodeKitFixtureNode, NodeKitKey, NodeKitLayoutHints, NodeKitManifest,
    NodeKitRegistry,
};
pub use migration::{
    CanonicalizeKindsPlan, MigrateNodesPlan, MigrateNodesReport, NodeKindMigrateError,
    NodeKindMigrator, NodeKindRewrite, NodeMigrationErrorEntry, NodeMigrationMissingMigrator,
    NodeMigrationMissingSchema, NodeMigrationNewerThanSchema, NodeMigrationUpgraded,
};
pub use registry::NodeRegistry;
pub use types::{
    NodeInstantiation, NodeInstantiationError, NodeKindViewDescriptor, NodeSchema,
    NodeSchemaBuilder, NodeSurfaceProjection, NodeSurfaceSlotDescriptor, NodeSurfaceSlotKind,
    NodeSurfaceSlotProjection, NodeSurfaceSlotVisibility, PortDecl, PortHandleVisibility,
    PortViewDescriptor, PortViewSide,
};

#[cfg(test)]
mod tests;
