use super::support::*;
use super::*;

#[test]
fn public_canonicalize_rejects_invalid_wrapper_fields_before_catalog_migration() {
    let graph_id = GraphId::new();
    let node = NodeId::from_u128(1);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            node,
            file_fixture_node(
                "test.wrapper-order",
                1,
                serde_json::json!({ "requires": "migration" }),
            ),
        )
        .build()
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("test.wrapper-order")
                .latest_kind_version(2)
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(
            NodeKindKey::new("test.wrapper-order"),
            Arc::new(CountingFileFixtureMigrator {
                calls: Arc::clone(&calls),
            }),
        )
        .unwrap();
    let unsupported = GraphFile {
        graph_id,
        graph_version: GRAPH_FILE_VERSION + 1,
        graph: graph.clone(),
    };

    assert!(matches!(
        unsupported.canonicalize(&catalog),
        Err(CanonicalGraphFileError::File(
            GraphFileError::UnsupportedEnvelopeVersion {
                found,
                supported: GRAPH_FILE_VERSION,
            }
        )) if found == GRAPH_FILE_VERSION + 1
    ));

    let inconsistent = GraphFile {
        graph_id: GraphId::new(),
        graph_version: GRAPH_FILE_VERSION,
        graph,
    };
    assert!(matches!(
        inconsistent.canonicalize(&catalog),
        Err(CanonicalGraphFileError::File(
            GraphFileError::InconsistentGraphId
        ))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonical_graph_file_load_migrates_known_kinds_and_preserves_unknown_kinds() {
    let graph_id = GraphId::new();
    let known = NodeId::from_u128(1);
    let unknown = NodeId::from_u128(2);
    let path = temp_path("canonical_graph_file_load", graph_id);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            known,
            file_fixture_node("legacy.note", 1, serde_json::json!({ "text": "hello" })),
        )
        .with_node(
            unknown,
            file_fixture_node("plugin.future", 77, serde_json::json!({ "sentinel": true })),
        )
        .build()
        .unwrap();
    GraphFile::from_graph(graph).save_json(&path).unwrap();
    let source_bytes = std::fs::read(&path).unwrap();
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("core.note")
                .latest_kind_version(2)
                .alias("legacy.note")
                .build(),
        )
        .unwrap();
    catalog
        .register_migrator(NodeKindKey::new("core.note"), Arc::new(FileFixtureMigrator))
        .unwrap();

    let canonical = GraphFile::load_canonical_json_if_exists(&path, &catalog)
        .unwrap()
        .expect("saved graph exists");

    assert_eq!(
        canonical.graph().node(&known).unwrap().kind,
        NodeKindKey::new("core.note")
    );
    assert_eq!(canonical.graph().node(&known).unwrap().kind_version, 2);
    assert_eq!(
        canonical.graph().node(&known).unwrap().data["canonical"],
        true
    );
    assert_eq!(
        canonical.graph().node(&unknown).unwrap().data,
        serde_json::json!({ "sentinel": true })
    );
    assert_eq!(canonical.report().opaque().len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), source_bytes);

    let runtime = GraphRuntime::from_canonical(canonical).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    assert_eq!(
        view.graph_snapshot()
            .unwrap()
            .graph()
            .node(&known)
            .unwrap()
            .kind_version,
        2
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn canonical_graph_file_load_rejects_a_partial_known_kind_migration() {
    let graph_id = GraphId::new();
    let path = temp_path("canonical_graph_file_failure", graph_id);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            NodeId::from_u128(1),
            file_fixture_node("core.note", 1, serde_json::json!({ "text": "hello" })),
        )
        .build()
        .unwrap();
    GraphFile::from_graph(graph).save_json(&path).unwrap();
    let source_bytes = std::fs::read(&path).unwrap();
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("core.note")
                .latest_kind_version(2)
                .build(),
        )
        .unwrap();

    let error = GraphFile::load_canonical_json(&path, &catalog).unwrap_err();

    assert!(matches!(
        error,
        CanonicalGraphFileError::Catalog(NodeCatalogMigrationError::IncompatibleKinds { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), source_bytes);

    let _ = std::fs::remove_file(path);
}
