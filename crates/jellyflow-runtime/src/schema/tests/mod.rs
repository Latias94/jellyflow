use serde_json::json;

use crate::schema::{NodeKindMigrateError, NodeKindMigrator};
use jellyflow_core::core::{CanvasPoint, Node, NodeKindKey};

mod catalog;

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

fn demo_add_node(kind: &str, kind_version: u32, data: serde_json::Value) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
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
