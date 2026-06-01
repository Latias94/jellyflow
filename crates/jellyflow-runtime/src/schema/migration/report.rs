use jellyflow_core::core::{NodeId, NodeKindKey};
use jellyflow_core::ops::GraphTransaction;

#[derive(Debug, Default, Clone)]
pub struct MigrateNodesReport {
    pub upgraded: Vec<NodeMigrationUpgraded>,
    pub missing_schema: Vec<NodeMigrationMissingSchema>,
    pub missing_migrator: Vec<NodeMigrationMissingMigrator>,
    pub newer_than_schema: Vec<NodeMigrationNewerThanSchema>,
    pub errors: Vec<NodeMigrationErrorEntry>,
}

impl MigrateNodesReport {
    pub fn is_empty(&self) -> bool {
        self.upgraded.is_empty()
            && self.missing_schema.is_empty()
            && self.missing_migrator.is_empty()
            && self.newer_than_schema.is_empty()
            && self.errors.is_empty()
    }

    pub fn upgraded(&self) -> &[NodeMigrationUpgraded] {
        &self.upgraded
    }

    pub fn missing_schema(&self) -> &[NodeMigrationMissingSchema] {
        &self.missing_schema
    }

    pub fn missing_migrator(&self) -> &[NodeMigrationMissingMigrator] {
        &self.missing_migrator
    }

    pub fn newer_than_schema(&self) -> &[NodeMigrationNewerThanSchema] {
        &self.newer_than_schema
    }

    pub fn errors(&self) -> &[NodeMigrationErrorEntry] {
        &self.errors
    }

    pub(in crate::schema) fn push_upgraded(
        &mut self,
        node: NodeId,
        kind: NodeKindKey,
        from: u32,
        to: u32,
    ) {
        self.upgraded.push(NodeMigrationUpgraded {
            node,
            kind,
            from,
            to,
        });
    }

    pub(in crate::schema) fn push_missing_schema(&mut self, node: NodeId, kind: NodeKindKey) {
        self.missing_schema
            .push(NodeMigrationMissingSchema { node, kind });
    }

    pub(in crate::schema) fn push_missing_migrator(
        &mut self,
        node: NodeId,
        kind: NodeKindKey,
        from: u32,
        to: u32,
    ) {
        self.missing_migrator.push(NodeMigrationMissingMigrator {
            node,
            kind,
            from,
            to,
        });
    }

    pub(in crate::schema) fn push_newer_than_schema(
        &mut self,
        node: NodeId,
        kind: NodeKindKey,
        node_kind_version: u32,
        schema_latest_kind_version: u32,
    ) {
        self.newer_than_schema.push(NodeMigrationNewerThanSchema {
            node,
            kind,
            node_kind_version,
            schema_latest_kind_version,
        });
    }

    pub(in crate::schema) fn push_error(
        &mut self,
        node: NodeId,
        kind: NodeKindKey,
        from: u32,
        to: u32,
        message: impl Into<String>,
    ) {
        self.errors.push(NodeMigrationErrorEntry {
            node,
            kind,
            from,
            to,
            message: message.into(),
        });
    }
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

impl MigrateNodesPlan {
    pub(in crate::schema) fn from_parts(tx: GraphTransaction, report: MigrateNodesReport) -> Self {
        Self { tx, report }
    }

    pub fn transaction(&self) -> &GraphTransaction {
        &self.tx
    }

    pub fn report(&self) -> &MigrateNodesReport {
        &self.report
    }

    pub fn into_parts(self) -> (GraphTransaction, MigrateNodesReport) {
        (self.tx, self.report)
    }
}
