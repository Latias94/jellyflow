use super::support::*;
use super::*;

#[test]
fn graph_file_v2_omits_removed_focusability_and_rejects_v1_before_typed_decode() {
    let current_path = graph_fixture_path("graph-file-v2-canonical.json");
    let fixture = GraphFile::load_json(&current_path).unwrap();
    let encoded = serde_json::to_value(fixture).unwrap();

    for collection in ["nodes", "edges"] {
        let entities = encoded["graph"][collection].as_object().unwrap();
        assert!(!entities.is_empty(), "fixture must contain {collection}");
        assert!(
            entities.values().all(|entity| entity
                .as_object()
                .is_some_and(|fields| !fields.contains_key(REMOVED_FOCUSABILITY_KEY))),
            "GraphFile serialized a removed V2 {collection} field"
        );
    }

    let legacy_path = graph_fixture_path("jellyflow-0.3-v1-focusability.json");
    let legacy_bytes = std::fs::read(&legacy_path).expect("read committed Jellyflow 0.3 fixture");
    let legacy: serde_json::Value =
        serde_json::from_slice(&legacy_bytes).expect("parse committed Jellyflow 0.3 fixture");
    for collection in ["nodes", "edges"] {
        assert!(
            legacy["graph"][collection]
                .as_object()
                .unwrap()
                .values()
                .any(|entity| entity.get(REMOVED_FOCUSABILITY_KEY).is_some()),
            "Jellyflow 0.3 fixture must exercise {collection} focusability"
        );
    }

    let error = GraphFile::load_canonical_json(&legacy_path, &NodeCatalog::new()).unwrap_err();
    assert!(matches!(
        error,
        CanonicalGraphFileError::File(GraphFileError::UnsupportedEnvelopeVersion {
            found: 1,
            supported: GRAPH_FILE_VERSION,
        })
    ));
    assert_eq!(std::fs::read(&legacy_path).unwrap(), legacy_bytes);
}

#[test]
fn graph_file_rejects_plain_graph_root() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_plain_graph", graph_id);
    let plain_graph = serde_json::json!({
        "graph_id": graph_id,
        "graph_version": GRAPH_FILE_VERSION,
        "nodes": {},
        "ports": {},
        "edges": {},
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&plain_graph).unwrap()).unwrap();

    let err = GraphFile::load_json(&path).unwrap_err();
    assert!(matches!(err, GraphFileError::Parse { .. }));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn graph_file_rejects_future_envelope_before_decoding_its_payload() {
    let path = graph_fixture_path("future-graph-envelope.json");
    let source_bytes = std::fs::read(&path).expect("read committed future-envelope fixture");

    let err = GraphFile::load_json(&path).unwrap_err();

    assert!(matches!(
        err,
        GraphFileError::UnsupportedEnvelopeVersion {
            found: 3,
            supported: GRAPH_FILE_VERSION
        }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), source_bytes);
}

#[test]
fn graph_file_rejects_unknown_fields_inside_the_v2_wire_schema() {
    let fixture_path = graph_fixture_path("graph-file-v2-canonical.json");
    let fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture_path).unwrap()).unwrap();

    for (case, mutate) in [
        (
            "root",
            (|source: &mut serde_json::Value| {
                source["titel"] = serde_json::json!("misspelled document title");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "graph",
            (|source: &mut serde_json::Value| {
                source["graph"]["titel"] = serde_json::json!("misspelled graph title");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "import",
            (|source: &mut serde_json::Value| {
                source["graph"]["imports"]["00000000-0000-0000-0000-000000000d00"]["titel"] =
                    serde_json::json!("misspelled import title");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "node",
            (|source: &mut serde_json::Value| {
                source["graph"]["nodes"]["00000000-0000-0000-0000-000000000010"]["titel"] =
                    serde_json::json!("misspelled node title");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "node_geometry",
            (|source: &mut serde_json::Value| {
                source["graph"]["nodes"]["00000000-0000-0000-0000-000000000010"]["pos"]["z"] =
                    serde_json::json!(1.0);
            }) as fn(&mut serde_json::Value),
        ),
        (
            "node_extent_unit_variant",
            (|source: &mut serde_json::Value| {
                source["graph"]["nodes"]["00000000-0000-0000-0000-000000000010"]["extent"] =
                    serde_json::json!({ "kind": "parent", "future": true });
            }) as fn(&mut serde_json::Value),
        ),
        (
            "port_type",
            (|source: &mut serde_json::Value| {
                source["graph"]["ports"]["00000000-0000-0000-0000-000000001001"]["ty"]["format"] =
                    serde_json::json!("future-format");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "edge_view",
            (|source: &mut serde_json::Value| {
                source["graph"]["edges"]["00000000-0000-0000-0000-00000000e001"]["view"]["animated"] =
                    serde_json::json!(true);
            }) as fn(&mut serde_json::Value),
        ),
        (
            "removed_edge_hit_width",
            (|source: &mut serde_json::Value| {
                source["graph"]["edges"]["00000000-0000-0000-0000-00000000e001"]["view"]["hit_target_width"] =
                    serde_json::json!(24.0);
            }) as fn(&mut serde_json::Value),
        ),
        (
            "binding_target",
            (|source: &mut serde_json::Value| {
                source["graph"]["bindings"]["00000000-0000-0000-0000-000000000c00"]["subject"]["target"]
                    ["name"] = serde_json::json!("future-name");
            }) as fn(&mut serde_json::Value),
        ),
        (
            "binding_graph_target_unit_variant",
            (|source: &mut serde_json::Value| {
                source["graph"]["bindings"]["00000000-0000-0000-0000-000000000c00"]["subject"]["target"] =
                    serde_json::json!({ "kind": "graph", "future": true });
            }) as fn(&mut serde_json::Value),
        ),
    ] {
        let mut source = fixture.clone();
        mutate(&mut source);
        let path = temp_path(&format!("graph_file_unknown_{case}"), GraphId::new());
        std::fs::write(&path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();

        let error = match GraphFile::load_json(&path) {
            Ok(_) => panic!("unknown {case}-level V2 field was accepted"),
            Err(error) => error,
        };

        assert!(
            matches!(error, GraphFileError::Parse { .. }),
            "unknown {case}-level V2 field must be rejected, got {error}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn graph_file_rejects_duplicate_ids_in_stable_entity_maps() {
    let fixture_path = graph_fixture_path("graph-file-v2-canonical.json");
    let fixture = std::fs::read_to_string(fixture_path).expect("read committed graph fixture");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&fixture).expect("parse committed graph fixture");

    for field in [
        "imports",
        "symbols",
        "nodes",
        "ports",
        "edges",
        "groups",
        "sticky_notes",
        "bindings",
    ] {
        let entities = fixture_value["graph"][field]
            .as_object()
            .expect("fixture entity map");
        let (id, entity) = entities.iter().next().expect("fixture entity");
        let map_start = format!("\"{field}\": {{");
        let duplicate_entry = format!(
            "{map_start}\n      {}: {},",
            serde_json::to_string(id).expect("encode fixture entity id"),
            serde_json::to_string(entity).expect("encode fixture entity")
        );
        let duplicate_source = fixture.replacen(&map_start, &duplicate_entry, 1);
        assert_ne!(duplicate_source, fixture, "missing fixture map {field}");

        let path = temp_path(&format!("graph_file_duplicate_{field}"), GraphId::new());
        std::fs::write(&path, duplicate_source).unwrap();

        let error = match GraphFile::load_json(&path) {
            Ok(_) => panic!("duplicate {field} id was accepted"),
            Err(error) => error,
        };
        let GraphFileError::Parse { source, .. } = error else {
            panic!("duplicate {field} id returned the wrong error: {error}");
        };
        assert!(
            source.to_string().contains("duplicate entity identifier"),
            "duplicate {field} id returned an unrelated parse error: {source}"
        );

        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn graph_file_rejects_unsupported_inner_graph_version() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_future_graph", graph_id);
    let mut source = serde_json::to_value(GraphFile::from_graph(Graph::new(graph_id))).unwrap();
    let graph_version = source["graph"]["graph_version"].as_u64().unwrap();
    source["graph"]["graph_version"] = serde_json::json!(graph_version + 1);
    source["graph"]["future_model_sentinel"] = serde_json::json!({
        "must_not_be_decoded_as_v2": [1, 2, 3]
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();

    let err = GraphFile::load_json(&path).unwrap_err();

    assert!(matches!(
        err,
        GraphFileError::UnsupportedGraphModelVersion {
            found,
            supported
        } if found == graph_version as u32 + 1 && supported == graph_version as u32
    ));

    let _ = std::fs::remove_file(&path);
}
