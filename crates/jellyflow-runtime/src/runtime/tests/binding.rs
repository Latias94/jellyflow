use crate::runtime::binding::{
    BindingEndpointResolution, BindingEndpointResolutionStatus, BindingQueryOptions,
};
use crate::runtime::measurement::NodeMeasurement;
use crate::runtime::tests::fixtures::make_store;
use jellyflow_core::core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasSize, Graph, GraphBuilder, GraphId,
    GraphLocalBindingTarget, Node, NodeId, NodeKindKey, SourceAnchor,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn binding_query_resolves_node_anchor_from_runtime_measurement() {
    let node = NodeId::from_u128(1);
    let binding = BindingId::from_u128(10);
    let mut store = make_store(graph_with_source_binding(node, binding));

    assert_eq!(
        store
            .binding_query()
            .binding(binding)
            .unwrap()
            .subject
            .status(),
        BindingEndpointResolutionStatus::Unresolved
    );

    store
        .report_node_measurement(NodeMeasurement::new(node).with_size(Some(CanvasSize {
            width: 100.0,
            height: 40.0,
        })))
        .expect("report measurement");

    let query = store.binding_query();
    let resolved = query.binding(binding).expect("binding");
    assert_eq!(query.revision, store.layout_facts_revision());
    assert!(matches!(
        resolved.subject.resolution,
        BindingEndpointResolution::NodeRect {
            node: resolved_node,
            rect,
            center,
        } if resolved_node == node
            && rect.origin == CanvasPoint { x: 10.0, y: 20.0 }
            && rect.size == CanvasSize { width: 100.0, height: 40.0 }
            && center == CanvasPoint { x: 60.0, y: 40.0 }
    ));
    assert!(matches!(
        resolved.target.resolution,
        BindingEndpointResolution::Source
    ));
}

#[test]
fn binding_query_marks_hidden_node_targets_unless_requested() {
    let node = NodeId::from_u128(2);
    let binding = BindingId::from_u128(20);
    let mut graph = graph_with_source_binding(node, binding);
    graph
        .update_node(&node, |node| node.hidden = true)
        .expect("node exists");
    graph
        .update_node(&node, |node| {
            node.size = Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            })
        })
        .expect("node exists");
    let store = make_store(graph);

    assert_eq!(
        store
            .binding_query()
            .binding(binding)
            .unwrap()
            .subject
            .status(),
        BindingEndpointResolutionStatus::Hidden
    );
    assert_eq!(
        store
            .binding_query_with_options(BindingQueryOptions::default().include_hidden(true))
            .binding(binding)
            .unwrap()
            .subject
            .status(),
        BindingEndpointResolutionStatus::Resolved
    );
}

#[test]
fn binding_query_updates_after_binding_dispatch() {
    let node = NodeId::from_u128(3);
    let binding = BindingId::from_u128(30);
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(node, node_fixture());
    graph
        .update_node(&node, |node| {
            node.size = Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            })
        })
        .expect("node exists");
    let mut store = make_store(graph.into());

    assert!(store.binding_query().binding(binding).is_none());

    store
        .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::AddBinding {
            id: binding,
            binding: source_binding(node),
        }]))
        .expect("dispatch binding");

    assert!(store.binding_query().binding(binding).is_some());
}

#[test]
fn layout_context_with_binding_pins_uses_resolved_node_binding_targets() {
    let node = NodeId::from_u128(4);
    let binding = BindingId::from_u128(40);
    let mut graph = graph_with_source_binding(node, binding);
    graph
        .update_node(&node, |node| {
            node.size = Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            })
        })
        .expect("node exists");
    let store = make_store(graph);

    let context = store.layout_context_with_binding_pins();

    assert!(context.pinned_nodes.contains(&node));
}

fn graph_with_source_binding(node: NodeId, binding: BindingId) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node,
        Node {
            pos: CanvasPoint { x: 10.0, y: 20.0 },
            ..node_fixture()
        },
    );
    graph.insert_binding(binding, source_binding(node));
    graph.into()
}

fn source_binding(node: NodeId) -> Binding {
    Binding {
        subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: node }),
        target: BindingEndpoint::source(SourceAnchor::new(
            "source.pdf",
            serde_json::json!({ "page": 1 }),
        )),
        kind: Some("excerpt".to_string()),
        meta: serde_json::Value::Null,
    }
}

fn node_fixture() -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos: CanvasPoint::default(),
        origin: None,
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
        data: serde_json::Value::Null,
    }
}
