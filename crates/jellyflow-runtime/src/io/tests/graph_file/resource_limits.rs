use super::support::*;
use super::*;

#[test]
fn default_graph_save_prevalidates_string_resources_before_replace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let original_id = GraphId::new();
    GraphFile::from_graph(Graph::new(original_id))
        .save_json(&path)
        .unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let replacement_id = GraphId::new();
    let mut node = file_fixture_node("placeholder", 1, serde_json::Value::Null);
    node.kind = NodeKindKey::new("x".repeat(PersistenceLimits::default().max_string_bytes() + 1));
    let replacement = GraphFile::from_graph(
        GraphBuilder::new(replacement_id)
            .with_node(NodeId::from_u128(1), node)
            .build()
            .unwrap(),
    );

    let error = replacement.save_json(&path).unwrap_err();

    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected pre-serialization string limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::StringBytes);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(GraphFile::load_json(&path).unwrap().graph_id, original_id);
}

#[test]
fn graph_save_with_limits_prevalidates_entities_before_replace() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let original_id = GraphId::new();
    GraphFile::from_graph(Graph::new(original_id))
        .save_json(&path)
        .unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let replacement_id = GraphId::new();
    let replacement = GraphFile::from_graph(
        GraphBuilder::new(replacement_id)
            .with_node(
                NodeId::from_u128(1),
                file_fixture_node("entity.over-budget", 1, serde_json::Value::Null),
            )
            .build()
            .unwrap(),
    );
    let error = replacement
        .save_json_with_limits(&path, PersistenceLimits::default().with_max_entities(0))
        .unwrap_err();

    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected pre-serialization entity limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::Entities);
    assert_eq!(error.actual(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(GraphFile::load_json(&path).unwrap().graph_id, original_id);
}

#[test]
fn graph_save_rejects_pretty_json_before_staging_when_byte_limit_is_exceeded() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let original_id = GraphId::new();
    GraphFile::from_graph(Graph::new(original_id))
        .save_json(&path)
        .unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let replacement_id = GraphId::new();
    let replacement = GraphFile::from_graph(
        GraphBuilder::new(replacement_id)
            .with_node(
                NodeId::from_u128(1),
                file_fixture_node(
                    "pretty.byte-limit",
                    1,
                    serde_json::json!({ "escaped": "\n\t\r" }),
                ),
            )
            .build()
            .unwrap(),
    );
    let pretty_len = serde_json::to_vec_pretty(&replacement).unwrap().len();
    let error = replacement
        .save_json_with_limits(
            &path,
            PersistenceLimits::default().with_max_bytes(pretty_len - 1),
        )
        .unwrap_err();

    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected serialization byte limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), pretty_len - 1);
    assert!(error.actual() > error.limit());
    assert!(error.actual() <= pretty_len);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(GraphFile::load_json(&path).unwrap().graph_id, original_id);
}

#[test]
fn graph_save_rejects_deep_in_memory_types_with_the_json_depth_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let file = graph_with_nested_symbol_type(256);
    let limits = PersistenceLimits::default().with_max_json_depth(32);

    let error = file.save_json_with_limits(&path, limits).unwrap_err();

    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected type depth resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 32);
    assert_eq!(error.actual(), 33);
    assert!(!path.exists());
}

#[test]
fn graph_save_and_reload_share_the_exact_wire_depth_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let rejected_path = directory.path().join("rejected.json");
    let accepted_path = directory.path().join("accepted.json");
    let file = graph_with_nested_symbol_type(4);

    let error = file
        .save_json_with_limits(
            &rejected_path,
            PersistenceLimits::default().with_max_json_depth(7),
        )
        .unwrap_err();

    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected encode-time wire depth limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 7);
    assert_eq!(error.actual(), 8);
    assert!(!rejected_path.exists());

    let limits = PersistenceLimits::default().with_max_json_depth(9);
    file.save_json_with_limits(&accepted_path, limits).unwrap();
    let loaded = GraphFile::load_json_with_limits(&accepted_path, limits).unwrap();
    assert_eq!(loaded.graph_id, file.graph_id);
}

#[test]
fn graph_canonicalize_rejects_deep_in_memory_types_with_the_json_depth_limit() {
    let file = graph_with_nested_symbol_type(256);
    let limits = PersistenceLimits::default().with_max_json_depth(32);

    let error = file
        .canonicalize_with_limits(&NodeCatalog::new(), limits)
        .unwrap_err();

    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected type depth resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 32);
    assert_eq!(error.actual(), 33);
}

fn graph_with_nested_symbol_type(depth: usize) -> GraphFile {
    let ty = (1..depth).fold(TypeDesc::Float, |of, _| TypeDesc::Option {
        of: Box::new(of),
    });
    let graph_id = GraphId::new();
    let graph = GraphBuilder::new(graph_id)
        .with_symbol(
            SymbolId::from_u128(1),
            Symbol {
                name: "deep-type".to_owned(),
                ty: Some(ty),
                default_value: None,
                meta: serde_json::Value::Null,
            },
        )
        .build()
        .unwrap();
    GraphFile::from_graph(graph)
}

#[test]
fn matching_custom_graph_save_and_load_limits_roundtrip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("graph.json");
    let graph_id = GraphId::new();
    let node_id = NodeId::from_u128(1);
    let file = GraphFile::from_graph(
        GraphBuilder::new(graph_id)
            .with_node(
                node_id,
                file_fixture_node("custom.limit", 1, serde_json::json!({ "value": 1 })),
            )
            .build()
            .unwrap(),
    );
    let pretty_len = serde_json::to_vec_pretty(&file).unwrap().len();
    let limits = PersistenceLimits::default()
        .with_max_bytes(pretty_len)
        .with_max_entities(1);

    file.save_json_with_limits(&path, limits).unwrap();
    let loaded =
        GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), limits).unwrap();

    assert_eq!(loaded.graph().graph_id(), graph_id);
    assert_eq!(
        loaded.graph().node(&node_id).unwrap().kind,
        NodeKindKey::new("custom.limit")
    );
}

#[test]
fn canonical_graph_load_enforces_exact_file_byte_boundary() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_byte_limit", graph_id);
    let bytes = serde_json::to_vec(&GraphFile::from_graph(Graph::new(graph_id))).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    let exact = PersistenceLimits::default().with_max_bytes(bytes.len());
    let loaded =
        GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), exact).unwrap();
    assert_eq!(loaded.graph().graph_id(), graph_id);

    let error = GraphFile::load_canonical_json_with_limits(
        &path,
        &NodeCatalog::new(),
        exact.with_max_bytes(bytes.len() - 1),
    )
    .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected byte resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), bytes.len() - 1);
    assert_eq!(error.actual(), bytes.len());

    let _ = std::fs::remove_file(path);
}

#[test]
fn canonical_graph_load_enforces_entity_boundary_before_publication() {
    let graph_id = GraphId::new();
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            first,
            file_fixture_node("unknown.entity", 7, serde_json::Value::Null),
        )
        .with_node(
            second,
            file_fixture_node("unknown.entity", 7, serde_json::Value::Null),
        )
        .build()
        .unwrap();
    let path = temp_path("graph_file_entity_limit", graph_id);
    std::fs::write(
        &path,
        serde_json::to_vec(&GraphFile::from_graph(graph)).unwrap(),
    )
    .unwrap();

    let exact = PersistenceLimits::default().with_max_entities(2);
    let loaded =
        GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), exact).unwrap();
    assert_eq!(loaded.graph().nodes().len(), 2);
    assert_eq!(loaded.report().opaque().len(), 2);

    let error = GraphFile::load_canonical_json_with_limits(
        &path,
        &NodeCatalog::new(),
        exact.with_max_entities(1),
    )
    .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected entity resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::Entities);
    assert_eq!(error.limit(), 1);
    assert_eq!(error.actual(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn in_memory_graph_budget_wins_before_allocating_storage_diagnostics() {
    let graph_id = GraphId::new();
    let mut first = file_fixture_node("invalid.first", 1, serde_json::Value::Null);
    first.ports = vec![PortId::from_u128(10)];
    let mut second = file_fixture_node("invalid.second", 1, serde_json::Value::Null);
    second.ports = vec![PortId::from_u128(20)];
    let graph = GraphBuilder::new(graph_id)
        .with_node(NodeId::from_u128(1), first)
        .with_node(NodeId::from_u128(2), second)
        .build_unchecked();
    let file = GraphFile::from_graph(graph);

    let error = file
        .canonicalize_with_limits(
            &NodeCatalog::new(),
            PersistenceLimits::default().with_max_entities(1),
        )
        .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected the entity limit before storage diagnostics, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::Entities);
    assert_eq!(error.limit(), 1);
    assert_eq!(error.actual(), 4);
}

#[test]
fn default_in_memory_canonicalize_applies_budgets_before_storage_diagnostics() {
    let graph_id = GraphId::new();
    let oversized_kind = "x".repeat(PersistenceLimits::default().max_string_bytes() + 1);
    let mut invalid = file_fixture_node(&oversized_kind, 1, serde_json::Value::Null);
    invalid.ports = vec![PortId::from_u128(10)];
    let graph = GraphBuilder::new(graph_id)
        .with_node(NodeId::from_u128(1), invalid)
        .build_unchecked();
    let file = GraphFile::from_graph(graph);

    let error = file.canonicalize(&NodeCatalog::new()).unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected the default string budget before storage diagnostics, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::StringBytes);
    assert_eq!(
        error.limit(),
        PersistenceLimits::default().max_string_bytes()
    );
    assert!(error.actual() > error.limit());
}

#[test]
fn graph_parse_budget_wins_before_a_later_wire_type_error() {
    let graph_id = GraphId::new();
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            first,
            file_fixture_node("first.string-is-over-budget", 1, serde_json::Value::Null),
        )
        .with_node(
            second,
            file_fixture_node("second.invalid", 1, serde_json::Value::Null),
        )
        .build()
        .unwrap();
    let mut source = serde_json::to_value(GraphFile::from_graph(graph)).unwrap();
    source["graph"]["nodes"]
        .as_object_mut()
        .unwrap()
        .get_mut(&second.0.to_string())
        .unwrap()["kind"] = serde_json::json!(42);
    let path = temp_path("graph_file_parse_budget_precedence", graph_id);
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();

    let error = GraphFile::load_canonical_json_with_limits(
        &path,
        &NodeCatalog::new(),
        PersistenceLimits::default().with_max_string_bytes(0),
    )
    .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected the parse-time string limit before the later type error, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::StringBytes);
    assert!(error.actual() > 0);

    let _ = std::fs::remove_file(path);
}

#[test]
fn graph_envelope_probe_enforces_depth_before_rejecting_an_unknown_field() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_envelope_depth_limit", graph_id);
    std::fs::write(&path, br#"{"graph_version":2,"unknown_payload":[null]}"#).unwrap();

    let error = GraphFile::load_json_with_limits(
        &path,
        PersistenceLimits::default().with_max_json_depth(2),
    )
    .unwrap_err();
    let GraphFileError::ResourceLimit(error) = error else {
        panic!("expected the envelope depth limit before the unknown-field error, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 2);
    assert_eq!(error.actual(), 3);

    let _ = std::fs::remove_file(path);
}

#[test]
fn graph_entity_budget_counts_ordered_port_references_before_validation() {
    let graph_id = GraphId::new();
    let node_id = NodeId::from_u128(1);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            node_id,
            file_fixture_node("unknown.references", 1, serde_json::Value::Null),
        )
        .build()
        .unwrap();
    let mut source = serde_json::to_value(GraphFile::from_graph(graph)).unwrap();
    source["graph"]["nodes"]
        .as_object_mut()
        .unwrap()
        .get_mut(&node_id.0.to_string())
        .unwrap()["ports"] = serde_json::json!([PortId::from_u128(10), PortId::from_u128(10),]);
    let path = temp_path("graph_file_reference_limit", graph_id);
    std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();

    let error = GraphFile::load_canonical_json_with_limits(
        &path,
        &NodeCatalog::new(),
        PersistenceLimits::default().with_max_entities(2),
    )
    .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected entity resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::Entities);
    assert_eq!(error.actual(), 3);

    let _ = std::fs::remove_file(path);
}

#[test]
fn default_persistence_limits_accept_ten_thousand_node_typed_chain() {
    const NODE_COUNT: usize = 10_000;

    let graph_id = GraphId::new();
    let mut builder = GraphBuilder::new(graph_id);
    let mut previous_output = None;
    for index in 0..NODE_COUNT {
        let node_id = NodeId::from_u128(index as u128 + 1);
        let input = PortId::from_u128(100_000 + index as u128 * 2);
        let output = PortId::from_u128(100_001 + index as u128 * 2);
        let mut node = file_fixture_node("unknown.large-graph", 1, serde_json::Value::Null);
        node.ports = vec![input, output];
        builder.insert_node(node_id, node);
        builder.insert_port(
            input,
            file_fixture_port(node_id, "in", PortDirection::In, PortCapacity::Single),
        );
        builder.insert_port(
            output,
            file_fixture_port(node_id, "out", PortDirection::Out, PortCapacity::Multi),
        );
        if let Some(previous_output) = previous_output {
            builder.insert_edge(
                EdgeId::from_u128(1_000_000 + index as u128),
                Edge::new(EdgeKind::Data, previous_output, input),
            );
        }
        previous_output = Some(output);
    }
    let path = temp_path("graph_file_default_ten_thousand", graph_id);
    std::fs::write(
        &path,
        serde_json::to_vec(&GraphFile::from_graph(builder.build().unwrap())).unwrap(),
    )
    .unwrap();

    let canonical = GraphFile::load_canonical_json(&path, &NodeCatalog::new()).unwrap();
    assert_eq!(canonical.graph().nodes().len(), NODE_COUNT);
    assert_eq!(canonical.graph().ports().len(), NODE_COUNT * 2);
    assert_eq!(canonical.graph().edges().len(), NODE_COUNT - 1);
    assert_eq!(canonical.report().opaque().len(), NODE_COUNT);

    let _ = std::fs::remove_file(path);
}

#[test]
fn canonical_graph_load_bounds_unknown_json_without_changing_it() {
    let graph_id = GraphId::new();
    let node_id = NodeId::from_u128(1);
    let kind = "unknown.payload";
    let payload = serde_json::json!({ "opaque": ["keep"] });
    let graph = GraphBuilder::new(graph_id)
        .with_node(node_id, file_fixture_node(kind, 77, payload.clone()))
        .build()
        .unwrap();
    let path = temp_path("graph_file_payload_limit", graph_id);
    std::fs::write(
        &path,
        serde_json::to_vec(&GraphFile::from_graph(graph)).unwrap(),
    )
    .unwrap();

    let string_bytes = node_id.0.to_string().len() + kind.len() + "opaque".len() + "keep".len();
    let exact = PersistenceLimits::default()
        .with_max_json_values(3)
        .with_max_string_bytes(string_bytes);
    let loaded =
        GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), exact).unwrap();
    assert_eq!(loaded.graph().node(&node_id).unwrap().data, payload);

    for (limits, expected) in [
        (
            exact.with_max_json_values(2),
            PersistenceResource::JsonValues,
        ),
        (
            exact.with_max_string_bytes(string_bytes - 1),
            PersistenceResource::StringBytes,
        ),
    ] {
        let error = GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), limits)
            .unwrap_err();
        let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
            panic!("expected payload resource limit, got {error}");
        };
        assert_eq!(error.resource(), expected);
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn canonical_graph_load_rejects_migration_work_before_invoking_migrators() {
    let graph_id = GraphId::new();
    let kind = "test.bounded-migration";
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            NodeId::from_u128(1),
            file_fixture_node(kind, 1, serde_json::Value::Null),
        )
        .with_node(
            NodeId::from_u128(2),
            file_fixture_node(kind, 1, serde_json::Value::Null),
        )
        .build()
        .unwrap();
    let path = temp_path("graph_file_migration_limit", graph_id);
    std::fs::write(
        &path,
        serde_json::to_vec(&GraphFile::from_graph(graph)).unwrap(),
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(kind, "Bounded migration", "test.native.bounded-migration")
                .latest_kind_version(2)
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(
            NodeKindKey::new(kind),
            Arc::new(CountingFileFixtureMigrator {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();

    let error = GraphFile::load_canonical_json_with_limits(
        &path,
        &catalog,
        PersistenceLimits::default().with_max_migration_operations(1),
    )
    .unwrap_err();
    let CanonicalGraphFileError::File(GraphFileError::ResourceLimit(error)) = error else {
        panic!("expected migration resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::MigrationOperations);
    assert_eq!(error.actual(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let canonical = GraphFile::load_canonical_json_with_limits(
        &path,
        &catalog,
        PersistenceLimits::default().with_max_migration_operations(2),
    )
    .unwrap();
    assert_eq!(canonical.report().upgraded().len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn migration_output_budget_stops_before_invoking_the_next_migrator() {
    let graph_id = GraphId::new();
    let kind = "test.incremental-output-budget";
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            NodeId::from_u128(1),
            file_fixture_node(kind, 1, serde_json::Value::Null),
        )
        .with_node(
            NodeId::from_u128(2),
            file_fixture_node(kind, 1, serde_json::Value::Null),
        )
        .with_node(
            NodeId::from_u128(3),
            file_fixture_node(kind, 1, serde_json::Value::Null),
        )
        .build()
        .unwrap();
    let file = GraphFile::from_graph(graph);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(kind, "Incremental output budget", "test.native.incremental")
                .latest_kind_version(2)
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(
            NodeKindKey::new(kind),
            Arc::new(CumulativeOutputExceedsBudgetMigrator {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();

    let error = file
        .canonicalize_with_limits(
            &catalog,
            PersistenceLimits::default().with_max_json_values(3),
        )
        .unwrap_err();

    let CanonicalGraphFileError::Catalog(NodeCatalogMigrationError::ResourceLimit(error)) = error
    else {
        panic!("unexpected migration output failure: {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonValues);
    assert_eq!(error.limit(), 3);
    assert_eq!(error.actual(), 4);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
