use serde_json::json;

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::create_node::{
    CREATE_NODE_TRANSACTION_LABEL, CreateNodeError, CreateNodeRequest,
};
use crate::runtime::store::NodeGraphStore;
use crate::schema::{NodeInstantiationError, NodeRegistry, NodeSchema, PortDecl};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Graph, GraphId, NodeKindKey, PortCapacity, PortDirection, PortKey,
    PortKind,
};
use jellyflow_core::ops::GraphOp;
use jellyflow_core::types::TypeDesc;

#[test]
fn store_applies_create_node_from_schema_through_dispatch() {
    let mut registry = NodeRegistry::new();
    registry.register(note_schema());
    let mut store = empty_store();

    let outcome = store
        .apply_create_node_from_schema(
            &registry,
            CreateNodeRequest::new(
                NodeKindKey::new("demo.sticky"),
                CanvasPoint { x: 40.0, y: 80.0 },
            ),
        )
        .expect("create node");

    let node_id = outcome.node_id();
    let port_ids: Vec<_> = outcome.port_ids().collect();
    assert_eq!(port_ids.len(), 2);

    let node = store.graph().nodes().get(&node_id).expect("created node");
    assert_eq!(node.kind, NodeKindKey::new("demo.note"));
    assert_eq!(node.kind_version, 2);
    assert_eq!(node.pos, CanvasPoint { x: 40.0, y: 80.0 });
    assert_eq!(
        node.size,
        Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        })
    );
    assert_eq!(node.ports, port_ids);
    assert_eq!(node.data, json!({ "body": "" }));

    assert_eq!(store.graph().ports()[&port_ids[0]].node, node_id);
    assert_eq!(
        store.graph().ports()[&port_ids[0]].key,
        PortKey::new("source")
    );
    assert_eq!(store.graph().ports()[&port_ids[1]].node, node_id);
    assert_eq!(
        store.graph().ports()[&port_ids[1]].key,
        PortKey::new("result")
    );

    let committed = outcome.dispatch.committed();
    assert_eq!(committed.label(), Some(CREATE_NODE_TRANSACTION_LABEL));
    assert_eq!(committed.ops().len(), 4);
    assert!(
        matches!(&committed.ops()[0], GraphOp::AddNode { id, node } if *id == node_id && node.ports.is_empty())
    );
    assert!(matches!(&committed.ops()[1], GraphOp::AddPort { id, .. } if *id == port_ids[0]));
    assert!(matches!(&committed.ops()[2], GraphOp::AddPort { id, .. } if *id == port_ids[1]));
    assert!(
        matches!(&committed.ops()[3], GraphOp::SetNodePorts { id, from, to } if *id == node_id && from.is_empty() && *to == port_ids)
    );

    let undone = store.undo().expect("undo succeeds").expect("undo commit");
    assert!(
        undone
            .committed()
            .ops()
            .iter()
            .any(|op| { matches!(op, GraphOp::RemoveNode { id, .. } if *id == node_id) })
    );
    assert!(!store.graph().nodes().contains_key(&node_id));
}

#[test]
fn store_create_node_reports_missing_schema_without_mutating_graph() {
    let registry = NodeRegistry::new();
    let mut store = empty_store();

    let error = store
        .apply_create_node_from_schema(
            &registry,
            CreateNodeRequest::new(NodeKindKey::new("demo.missing"), CanvasPoint::default()),
        )
        .expect_err("missing schema");

    assert!(matches!(
        error,
        CreateNodeError::Schema(NodeInstantiationError::MissingSchema(kind))
            if kind == NodeKindKey::new("demo.missing")
    ));
    assert!(store.graph().nodes().is_empty());
    assert!(store.graph().ports().is_empty());
}

fn empty_store() -> NodeGraphStore {
    NodeGraphStore::new(
        Graph::new(GraphId::from_u128(1)),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    )
}

fn note_schema() -> NodeSchema {
    NodeSchema {
        kind: NodeKindKey::new("demo.note"),
        latest_kind_version: 2,
        kind_aliases: vec![NodeKindKey::new("demo.sticky")],
        title: "Note".into(),
        category: vec!["Knowledge".into()],
        keywords: vec!["memo".into()],
        renderer_key: Some("note-card".into()),
        default_size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        ports: vec![
            PortDecl {
                key: PortKey::new("source"),
                dir: PortDirection::In,
                kind: PortKind::Data,
                capacity: PortCapacity::Single,
                ty: Some(TypeDesc::Opaque {
                    key: "markdown".into(),
                    params: Vec::new(),
                }),
                label: Some("Source".into()),
            },
            PortDecl {
                key: PortKey::new("result"),
                dir: PortDirection::Out,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                ty: None,
                label: Some("Result".into()),
            },
        ],
        default_data: json!({ "body": "" }),
    }
}
