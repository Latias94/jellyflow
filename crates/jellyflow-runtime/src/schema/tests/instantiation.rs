use serde_json::json;

use crate::schema::{NodeInstantiationError, NodeRegistry, NodeSchema, PortDecl};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, GraphBuilder, NodeId, NodeKindKey, PortCapacity, PortDirection,
    PortId, PortKey, PortKind,
};
use jellyflow_core::ops::GraphOp;
use jellyflow_core::types::TypeDesc;

fn note_schema() -> NodeSchema {
    NodeSchema {
        kind: NodeKindKey::new("demo.note"),
        latest_kind_version: 3,
        kind_aliases: vec![NodeKindKey::new("demo.sticky")],
        title: "Note".into(),
        category: vec!["Knowledge".into()],
        keywords: vec!["memo".into()],
        renderer_key: Some("note-card".into()),
        default_size: Some(CanvasSize {
            width: 180.0,
            height: 120.0,
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
                label: None,
            },
        ],
        default_data: json!({ "body": "" }),
    }
}

#[test]
fn node_schema_instantiates_node_ports_and_transaction_in_schema_order() {
    let schema = note_schema();
    let node_id = NodeId::from_u128(1);
    let source_id = PortId::from_u128(2);
    let result_id = PortId::from_u128(3);
    let pos = CanvasPoint { x: 10.0, y: 20.0 };

    let instantiation = schema
        .instantiate_with_ids(node_id, pos, [source_id, result_id])
        .expect("node instantiation");

    assert_eq!(instantiation.node_id, node_id);
    assert_eq!(instantiation.node.kind, NodeKindKey::new("demo.note"));
    assert_eq!(instantiation.node.kind_version, 3);
    assert_eq!(instantiation.node.pos, pos);
    assert_eq!(
        instantiation.node.size,
        Some(CanvasSize {
            width: 180.0,
            height: 120.0,
        })
    );
    assert_eq!(instantiation.node.ports, vec![source_id, result_id]);
    assert_eq!(instantiation.node.data, json!({ "body": "" }));
    assert!(!instantiation.node.hidden);
    assert!(!instantiation.node.collapsed);

    assert_eq!(instantiation.ports.len(), 2);
    assert_eq!(instantiation.ports[0].0, source_id);
    assert_eq!(instantiation.ports[0].1.node, node_id);
    assert_eq!(instantiation.ports[0].1.key, PortKey::new("source"));
    assert_eq!(instantiation.ports[0].1.dir, PortDirection::In);
    assert_eq!(instantiation.ports[0].1.kind, PortKind::Data);
    assert_eq!(instantiation.ports[0].1.capacity, PortCapacity::Single);
    assert_eq!(instantiation.ports[0].1.data, serde_json::Value::Null);
    assert_eq!(instantiation.ports[1].0, result_id);
    assert_eq!(instantiation.ports[1].1.key, PortKey::new("result"));

    let tx = instantiation.into_labeled_transaction("Create Node");
    assert_eq!(tx.label(), Some("Create Node"));
    assert_eq!(tx.ops().len(), 4);
    assert!(
        matches!(&tx.ops()[0], GraphOp::AddNode { id, node } if *id == node_id && node.ports.is_empty())
    );
    assert!(matches!(tx.ops()[1], GraphOp::AddPort { id, .. } if id == source_id));
    assert!(matches!(tx.ops()[2], GraphOp::AddPort { id, .. } if id == result_id));
    assert!(
        matches!(&tx.ops()[3], GraphOp::SetNodePorts { id, from, to } if *id == node_id && from.is_empty() && *to == vec![source_id, result_id])
    );

    let mut graph = GraphBuilder::default();
    tx.apply_to(&mut graph).expect("apply instantiation");
    assert_eq!(graph.nodes()[&node_id].ports, vec![source_id, result_id]);
    assert_eq!(graph.ports()[&source_id].node, node_id);
    assert_eq!(graph.ports()[&result_id].node, node_id);
}

#[test]
fn registry_instantiates_alias_as_canonical_kind() {
    let mut registry = NodeRegistry::new();
    registry.register(note_schema());

    let instantiation = registry
        .instantiate_node(&NodeKindKey::new("demo.sticky"), CanvasPoint::default())
        .expect("node instantiation by alias");

    assert_eq!(instantiation.node.kind, NodeKindKey::new("demo.note"));
    assert_eq!(instantiation.node.kind_version, 3);
    assert_eq!(instantiation.node.ports.len(), 2);
    assert_eq!(instantiation.ports.len(), 2);
    assert_eq!(instantiation.ports[0].1.node, instantiation.node_id);
    assert_eq!(instantiation.ports[1].1.node, instantiation.node_id);
}

#[test]
fn registry_reports_missing_schema_for_unknown_kind() {
    let registry = NodeRegistry::new();

    let error = registry
        .instantiate_node(&NodeKindKey::new("demo.unknown"), CanvasPoint::default())
        .expect_err("missing schema");

    assert_eq!(
        error,
        NodeInstantiationError::MissingSchema(NodeKindKey::new("demo.unknown"))
    );
}

#[test]
fn schema_reports_port_id_count_mismatch() {
    let schema = note_schema();

    let error = schema
        .instantiate_with_ids(
            NodeId::from_u128(1),
            CanvasPoint::default(),
            [PortId::from_u128(2)],
        )
        .expect_err("port id mismatch");

    assert_eq!(
        error,
        NodeInstantiationError::PortIdCountMismatch {
            expected: 2,
            actual: 1
        }
    );
}
