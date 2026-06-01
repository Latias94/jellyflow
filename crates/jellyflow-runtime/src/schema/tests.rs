use serde_json::json;

use crate::schema::{NodeKindMigrateError, NodeKindMigrator, NodeSchema};
use jellyflow_core::core::{CanvasPoint, Node, NodeKindKey};

mod canonicalize;
mod facades;
mod migration;

struct DummyMigrator;

impl NodeKindMigrator for DummyMigrator {
    fn migrate(
        &self,
        from_version: u32,
        to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        Ok(json!({
            "from_version": from_version,
            "to_version": to_version,
            "prev": data,
            "migrated": true,
        }))
    }
}

struct IdentityMigrator;

impl NodeKindMigrator for IdentityMigrator {
    fn migrate(
        &self,
        _from_version: u32,
        _to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        Ok(data.clone())
    }
}

fn demo_add_schema(latest_kind_version: u32, kind_aliases: Vec<&str>) -> NodeSchema {
    NodeSchema {
        kind: NodeKindKey::new("demo.add"),
        latest_kind_version,
        kind_aliases: kind_aliases.into_iter().map(NodeKindKey::new).collect(),
        title: "Add".into(),
        category: Vec::new(),
        keywords: Vec::new(),
        ports: Vec::new(),
        default_data: serde_json::Value::Null,
    }
}

fn demo_add_node(kind: &str, kind_version: u32, data: serde_json::Value) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data,
    }
}
