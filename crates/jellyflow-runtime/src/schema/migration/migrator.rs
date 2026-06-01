use serde_json::Value;

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
