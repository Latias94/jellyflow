use super::*;

#[test]
fn graph_stores_graph_local_and_source_binding_endpoints() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.insert_node(node_id, make_node("core.note"));

    let binding_id = BindingId::new();
    let binding = Binding::node_to_source(
        node_id,
        "source.pdf",
        serde_json::json!({ "page": 3, "rect": [10, 20, 30, 40] }),
    )
    .with_kind("excerpt")
    .with_meta(serde_json::json!({ "color": "yellow" }));
    graph.insert_binding(binding_id, binding.clone());

    assert_eq!(graph.bindings().get(&binding_id), Some(&binding));
    assert_eq!(
        graph.nodes().get(&node_id).and_then(|node| node.origin),
        None
    );
}

#[test]
fn binding_helpers_build_common_node_to_source_relationships() {
    let node_id = NodeId::from_u128(5);
    let binding = Binding::node_to_source(
        node_id,
        "paper.pdf",
        serde_json::json!({ "page": 3, "quote": "headless" }),
    )
    .with_kind("excerpt")
    .with_meta(serde_json::json!({ "color": "yellow" }));

    assert_eq!(binding.subject, BindingEndpoint::node(node_id));
    assert_eq!(
        binding.subject.graph_local_target(),
        Some(GraphLocalBindingTarget::node(node_id))
    );
    assert_eq!(
        binding.target,
        BindingEndpoint::source_payload(
            "paper.pdf",
            serde_json::json!({ "page": 3, "quote": "headless" })
        )
    );
    assert_eq!(binding.kind.as_deref(), Some("excerpt"));
    assert_eq!(binding.meta, serde_json::json!({ "color": "yellow" }));
}

#[test]
fn validate_rejects_missing_graph_local_binding_targets() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    let binding_id = BindingId::new();
    graph.insert_binding(
        binding_id,
        Binding {
            subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: node_id }),
            target: BindingEndpoint::source(SourceAnchor::new(
                "source.pdf",
                serde_json::json!({ "page": 1 }),
            )),
            kind: None,
            meta: serde_json::Value::Null,
        },
    );

    let report = validate_graph(&graph);

    assert!(report.errors.iter().any(|error| matches!(
        error,
        GraphValidationError::BindingTargetMissing { binding, target }
            if *binding == binding_id
                && *target == GraphLocalBindingTarget::Node { id: node_id }
    )));

    graph.insert_node(node_id, make_node("core.note"));
    assert!(validate_graph(&graph).is_ok());
}

#[test]
fn validate_accepts_opaque_source_anchor_without_external_schema() {
    let mut graph = Graph::default();
    let binding_id = BindingId::new();
    graph.insert_binding(
        binding_id,
        Binding {
            subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Graph),
            target: BindingEndpoint::source(SourceAnchor::new(
                "host-owned-image",
                serde_json::json!({ "region": { "x": 1, "y": 2 } }),
            )),
            kind: None,
            meta: serde_json::Value::Null,
        },
    );

    assert!(validate_graph(&graph).is_ok());
}
