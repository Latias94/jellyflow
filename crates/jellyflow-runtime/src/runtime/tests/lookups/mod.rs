use super::fixtures::{GraphFixtureUpdateExt, make_graph};

use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeKind, EdgeReconnectable, GraphBuilder, GraphId, Group,
    GroupId, Node, NodeId, NodeKindKey, PortKind,
};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

fn node_with_parent(pos: CanvasPoint, parent: Option<GroupId>) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

mod edge_updates;
mod parents;
mod rebuild;
