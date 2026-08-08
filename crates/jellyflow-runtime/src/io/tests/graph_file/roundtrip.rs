use super::support::*;
use super::*;

#[test]
fn graph_file_roundtrips_wrapped_graph() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_roundtrip", graph_id);
    let graph = Graph::new(graph_id);
    let file = GraphFile::from_graph(graph);

    file.save_json(&path).unwrap();

    let root: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        root.get("graph_version").and_then(|v| v.as_u64()),
        Some(u64::from(GRAPH_FILE_VERSION))
    );
    assert!(root.get("graph").is_some());

    let loaded = GraphFile::load_json(&path).unwrap();
    assert_eq!(loaded.graph_id, graph_id);
    assert_eq!(loaded.graph_version, GRAPH_FILE_VERSION);
    assert_eq!(loaded.graph.graph_id(), graph_id);

    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn canonical_graph_file_optional_load_propagates_non_not_found_read_errors() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("self-referencing-graph.json");
    symlink(&path, &path).unwrap();

    let error = GraphFile::load_canonical_json_if_exists(&path, &NodeCatalog::new()).unwrap_err();

    assert!(matches!(
        error,
        CanonicalGraphFileError::File(GraphFileError::Read { .. })
    ));
}

#[test]
fn canonical_graph_file_optional_load_returns_none_only_for_a_missing_file() {
    let path = temp_path("missing_canonical_graph_file", GraphId::new());
    let _ = std::fs::remove_file(&path);

    let loaded = GraphFile::load_canonical_json_if_exists(&path, &NodeCatalog::new()).unwrap();

    assert!(loaded.is_none());
}

#[test]
fn graph_file_v2_canonical_fixture_roundtrips_all_stable_facts_idempotently() {
    let fixture_path = graph_fixture_path("graph-file-v2-canonical.json");
    let source_bytes = std::fs::read(&fixture_path).expect("read committed V2 graph fixture");
    let source_value: serde_json::Value =
        serde_json::from_slice(&source_bytes).expect("parse committed V2 graph fixture");
    let decoded = GraphFile::load_json(&fixture_path).expect("decode committed V2 graph fixture");

    assert_eq!(
        serde_json::to_value(&decoded).expect("encode decoded graph fixture"),
        source_value,
        "the committed fixture must already be in canonical V2 form"
    );
    let catalog = NodeCatalog::new();
    let canonical = GraphFile::load_canonical_json(&fixture_path, &catalog)
        .expect("load and canonicalize committed V2 graph fixture");
    let runtime =
        GraphRuntime::from_canonical(canonical).expect("publish canonical V2 graph fixture");
    let runtime_snapshot = runtime.snapshot().expect("snapshot canonical V2 graph");
    let loaded = GraphFile::from_graph(runtime_snapshot.graph().clone());
    assert_eq!(runtime_snapshot.revision(), 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(std::fs::read(&fixture_path).unwrap(), source_bytes);

    assert_eq!(loaded.graph.nodes().len(), 5);
    assert_eq!(loaded.graph.ports().len(), 9);
    assert_eq!(loaded.graph.edges().len(), 3);
    assert_eq!(loaded.graph.groups().len(), 1);
    assert_eq!(loaded.graph.imports().len(), 1);
    assert_eq!(loaded.graph.symbols().len(), 1);
    assert_eq!(loaded.graph.sticky_notes().len(), 1);
    assert_eq!(loaded.graph.bindings().len(), 1);
    assert_eq!(
        loaded
            .graph
            .import(&GraphId::from_u128(0xd00))
            .and_then(|import| import.alias.as_deref()),
        Some("shared-workflow")
    );

    for kind in [
        "dify.workflow.llm",
        "unity.shader.multiply",
        "erd.table.users",
        "mind-map.topic",
        "plugin.acme.future_node",
    ] {
        assert!(
            loaded
                .graph
                .nodes()
                .values()
                .any(|node| node.kind.0 == kind),
            "missing fixture node kind {kind}"
        );
    }

    let shader = loaded
        .graph
        .node(&NodeId::from_u128(0x20))
        .expect("shader fixture node");
    assert_eq!(
        shader.ports,
        vec![
            PortId::from_u128(0x2003),
            PortId::from_u128(0x2001),
            PortId::from_u128(0x2002),
        ],
        "port order is a stable fact, not UUID map order"
    );
    let mut sorted_shader_ports = shader.ports.clone();
    sorted_shader_ports.sort();
    assert_ne!(shader.ports, sorted_shader_ports);
    assert!(
        loaded
            .graph
            .nodes()
            .values()
            .flat_map(|node| &node.ports)
            .all(|port| loaded.graph.port(port).is_some()),
        "all declared node ports must be real graph topology"
    );

    let unknown = loaded
        .graph
        .node(&NodeId::from_u128(0x50))
        .expect("unknown-kind fixture node");
    assert_eq!(unknown.kind_version, 77);
    assert_eq!(
        unknown.data["opaque"]["arrays"][1]["deep"][2]["sentinel"],
        "keep-me"
    );
    assert_eq!(
        unknown.data["plugin_state"]["custom_key"],
        "must-round-trip"
    );
    let mind_map = loaded
        .graph
        .node(&NodeId::from_u128(0x40))
        .expect("mind-map fixture node");
    assert_eq!(mind_map.kind, NodeKindKey::new("mind-map.topic"));
    assert_eq!(mind_map.data["title"], "Native adapters");
    assert_eq!(mind_map.data["branch"]["order"], 2);

    let first_path = temp_path("graph_file_v2_first", GraphId::new());
    let second_path = temp_path("graph_file_v2_second", GraphId::new());
    loaded
        .save_json(&first_path)
        .expect("save canonical V2 fixture");
    let reopened = GraphRuntime::from_canonical(
        GraphFile::load_canonical_json(&first_path, &catalog).expect("reopen canonical V2 fixture"),
    )
    .expect("publish reopened canonical V2 fixture");
    let reopened = GraphFile::from_graph(reopened.snapshot().unwrap().graph().clone());
    assert_eq!(
        serde_json::to_value(&reopened).expect("encode reopened graph fixture"),
        source_value,
        "load-save-reopen must preserve every persisted fact"
    );

    reopened
        .save_json(&second_path)
        .expect("save canonical fixture a second time");
    assert_eq!(
        std::fs::read(&first_path).expect("read first canonical save"),
        std::fs::read(&second_path).expect("read second canonical save"),
        "a second save must be byte-idempotent"
    );

    let _ = std::fs::remove_file(first_path);
    let _ = std::fs::remove_file(second_path);
}
