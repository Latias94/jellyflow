use serde_json::Value;

use jellyflow_core::core::{NodeId, NodeKindKey};
use jellyflow_core::ops::GraphTransaction;

/// A per-kind node payload migrator.
pub trait NodeKindMigrator: Send + Sync {
    /// Migrates a node payload from `from_version` to `to_version`.
    fn migrate(
        &self,
        from_version: u32,
        to_version: u32,
        data: &Value,
    ) -> Result<Value, NodeKindMigrateError>;
}

#[derive(Debug, thiserror::Error)]
pub enum NodeKindMigrateError {
    #[error("{0}")]
    Message(String),
}

impl NodeKindMigrateError {
    pub fn message(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

#[derive(Debug, Clone)]
pub struct NodeKindRewrite {
    pub node: NodeId,
    pub from: NodeKindKey,
    pub to: NodeKindKey,
}

#[derive(Debug, Clone)]
pub struct CanonicalizeKindsPlan {
    pub tx: GraphTransaction,
    pub rewrites: Vec<NodeKindRewrite>,
}

#[derive(Debug, Default, Clone)]
pub struct MigrateNodesReport {
    pub upgraded: Vec<NodeMigrationUpgraded>,
    pub missing_schema: Vec<NodeMigrationMissingSchema>,
    pub missing_migrator: Vec<NodeMigrationMissingMigrator>,
    pub newer_than_schema: Vec<NodeMigrationNewerThanSchema>,
    pub errors: Vec<NodeMigrationErrorEntry>,
}

#[derive(Debug, Clone)]
pub struct NodeMigrationUpgraded {
    pub node: NodeId,
    pub kind: NodeKindKey,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone)]
pub struct NodeMigrationMissingSchema {
    pub node: NodeId,
    pub kind: NodeKindKey,
}

#[derive(Debug, Clone)]
pub struct NodeMigrationMissingMigrator {
    pub node: NodeId,
    pub kind: NodeKindKey,
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone)]
pub struct NodeMigrationNewerThanSchema {
    pub node: NodeId,
    pub kind: NodeKindKey,
    pub node_kind_version: u32,
    pub schema_latest_kind_version: u32,
}

#[derive(Debug, Clone)]
pub struct NodeMigrationErrorEntry {
    pub node: NodeId,
    pub kind: NodeKindKey,
    pub from: u32,
    pub to: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct MigrateNodesPlan {
    pub tx: GraphTransaction,
    pub report: MigrateNodesReport,
}
