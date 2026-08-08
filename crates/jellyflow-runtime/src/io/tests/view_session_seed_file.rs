use super::*;
use jellyflow_core::core::{CanvasPoint, GraphBuilder, GraphId, Node, NodeId, NodeKindKey};

fn sample_seed() -> ViewSessionSeed {
    ViewSessionSeed::new(GraphId::from_u128(1))
        .with_viewport(ViewportSeed::new(CanvasPoint { x: 12.5, y: -3.0 }, 1.25))
        .with_selection(ViewSelectionSeed::new(
            vec![NodeId::from_u128(2)],
            vec![jellyflow_core::core::EdgeId::from_u128(3)],
            vec![jellyflow_core::core::GroupId::from_u128(4)],
        ))
        .with_stacking(ViewStackingSeed::new(
            vec![NodeId::from_u128(5), NodeId::from_u128(2)],
            vec![jellyflow_core::core::EdgeId::from_u128(3)],
            vec![jellyflow_core::core::GroupId::from_u128(4)],
        ))
}

#[test]
fn view_session_seed_v1_roundtrips_exact_supported_facts() {
    let seed = sample_seed();

    let bytes = ViewSessionSeedFile::encode_json(&seed).unwrap();
    let decoded = ViewSessionSeedFile::decode_json(&bytes).unwrap();

    assert_eq!(decoded, seed);
}

#[test]
fn view_session_seed_encode_enforces_explicit_byte_boundary() {
    let seed = sample_seed();
    let bytes = ViewSessionSeedFile::encode_json(&seed).unwrap();
    let exact = PersistenceLimits::default().with_max_bytes(bytes.len());

    assert_eq!(
        ViewSessionSeedFile::encode_json_with_limits(&seed, exact).unwrap(),
        bytes
    );

    let error =
        ViewSessionSeedFile::encode_json_with_limits(&seed, exact.with_max_bytes(bytes.len() - 1))
            .unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected encoding byte resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), bytes.len() - 1);
    assert!(error.actual() > error.limit());
    assert!(error.actual() <= bytes.len());
}

#[test]
fn view_session_seed_byte_slice_decode_enforces_explicit_byte_boundary() {
    let seed = sample_seed();
    let bytes = ViewSessionSeedFile::encode_json(&seed).unwrap();
    let exact = PersistenceLimits::default().with_max_bytes(bytes.len());

    assert_eq!(
        ViewSessionSeedFile::decode_json_with_limits(&bytes, exact).unwrap(),
        seed,
    );

    let error =
        ViewSessionSeedFile::decode_json_with_limits(&bytes, exact.with_max_bytes(bytes.len() - 1))
            .unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected byte resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), bytes.len() - 1);
    assert_eq!(error.actual(), bytes.len());
}

#[test]
fn view_session_seed_file_save_and_load_use_current_format() {
    let seed = sample_seed();
    let path = temp_path("view_session_seed_roundtrip", seed.graph_id());

    ViewSessionSeedFile::save_json(&seed, &path).unwrap();
    let loaded = ViewSessionSeedFile::load_json(&path).unwrap();

    assert_eq!(loaded, seed);

    let _ = std::fs::remove_file(path);
}

#[test]
fn view_session_seed_save_rejects_pretty_json_before_replacing_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("view-session-seed.json");
    let original = ViewSessionSeed::new(GraphId::from_u128(100));
    ViewSessionSeedFile::save_json(&original, &path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let replacement = sample_seed();
    let encoded = ViewSessionSeedFile::encode_json(&replacement).unwrap();
    let limits = PersistenceLimits::default().with_max_bytes(encoded.len() - 1);
    let error =
        ViewSessionSeedFile::save_json_with_limits(&replacement, &path, limits).unwrap_err();

    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected serialization byte limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), encoded.len() - 1);
    assert!(error.actual() > error.limit());
    assert!(error.actual() <= encoded.len());
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(ViewSessionSeedFile::load_json(&path).unwrap(), original);
}

#[test]
fn view_session_seed_load_enforces_exact_file_byte_boundary() {
    let seed = sample_seed();
    let path = temp_path("view_session_seed_byte_limit", seed.graph_id());
    let bytes = ViewSessionSeedFile::encode_json(&seed).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    let exact = PersistenceLimits::default().with_max_bytes(bytes.len());
    assert_eq!(
        ViewSessionSeedFile::load_json_with_limits(&path, exact).unwrap(),
        seed
    );

    let error =
        ViewSessionSeedFile::load_json_with_limits(&path, exact.with_max_bytes(bytes.len() - 1))
            .unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected byte resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::FileBytes);
    assert_eq!(error.limit(), bytes.len() - 1);
    assert_eq!(error.actual(), bytes.len());

    let _ = std::fs::remove_file(path);
}

#[test]
fn view_session_seed_load_enforces_persisted_id_boundary() {
    let seed = sample_seed();
    let path = temp_path("view_session_seed_entity_limit", seed.graph_id());
    ViewSessionSeedFile::save_json(&seed, &path).unwrap();

    let exact = PersistenceLimits::default().with_max_entities(7);
    assert_eq!(
        ViewSessionSeedFile::load_json_with_limits(&path, exact).unwrap(),
        seed
    );

    let error =
        ViewSessionSeedFile::load_json_with_limits(&path, exact.with_max_entities(6)).unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected entity resource limit, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::Entities);
    assert_eq!(error.limit(), 6);
    assert_eq!(error.actual(), 7);

    let _ = std::fs::remove_file(path);
}

#[test]
fn current_seed_collection_budget_stops_before_a_later_element_error() {
    let graph_id = GraphId::from_u128(1);
    let first = NodeId::from_u128(2);
    let second = NodeId::from_u128(3);
    let bytes = format!(
        r#"{{
            "graph_id": "{graph_id}",
            "view_session_seed_version": 1,
            "viewport": {{ "pan": {{ "x": 0.0, "y": 0.0 }}, "zoom": 1.0 }},
            "selection": {{
                "nodes": ["{first}", "{second}", 42],
                "edges": [],
                "groups": []
            }},
            "stacking": {{ "nodes": [], "edges": [], "groups": [] }}
        }}"#,
        first = first.0,
        second = second.0,
    );

    let error = ViewSessionSeedFile::decode_json_with_limits(
        bytes.as_bytes(),
        PersistenceLimits::default().with_max_collection_items(1),
    )
    .unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected the collection limit before the later element error, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::CollectionItems);
    assert_eq!(error.limit(), 1);
    assert_eq!(error.actual(), 2);
}

#[test]
fn seed_envelope_probe_enforces_depth_before_rejecting_an_unknown_field() {
    let bytes = br#"{"view_session_seed_version":1,"unknown_payload":[null]}"#;

    let error = ViewSessionSeedFile::decode_json_with_limits(
        bytes,
        PersistenceLimits::default().with_max_json_depth(2),
    )
    .unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!("expected the envelope depth limit before the unknown-field error, got {error}");
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 2);
    assert_eq!(error.actual(), 3);
}

#[test]
fn persistence_depth_is_hard_capped_and_owned_by_the_budget_tracker() {
    let nested = format!("{}null{}", "[".repeat(127), "]".repeat(127));
    let bytes = format!(r#"{{"view_session_seed_version":1,"unknown_payload":{nested}}}"#);
    let limits = PersistenceLimits::default().with_max_json_depth(usize::MAX);
    assert_eq!(limits.max_json_depth(), 128);

    let error = ViewSessionSeedFile::decode_json_with_limits(bytes.as_bytes(), limits).unwrap_err();
    let ViewSessionSeedFileError::ResourceLimit(error) = error else {
        panic!(
            "expected the owned depth budget instead of serde_json recursion failure, got {error}"
        );
    };
    assert_eq!(error.resource(), PersistenceResource::JsonDepth);
    assert_eq!(error.limit(), 128);
    assert_eq!(error.actual(), 129);
}

#[cfg(unix)]
#[test]
fn view_session_seed_optional_load_propagates_non_not_found_read_errors() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("self-referencing-view-session-seed.json");
    symlink(&path, &path).unwrap();

    let error = ViewSessionSeedFile::load_json_if_exists(&path).unwrap_err();

    assert!(matches!(error, ViewSessionSeedFileError::Read { .. }));
}

#[test]
fn seed_serialization_failure_preserves_the_last_valid_file() {
    assert_seed_save_fault_preserves_target(ViewSessionSeedFileSaveFault::Serialization);
}

#[test]
fn seed_temporary_write_failure_preserves_the_last_valid_file() {
    assert_seed_save_fault_preserves_target(ViewSessionSeedFileSaveFault::TemporaryWrite);
}

#[test]
fn seed_temporary_sync_failure_preserves_the_last_valid_file() {
    assert_seed_save_fault_preserves_target(ViewSessionSeedFileSaveFault::TemporarySync);
}

#[test]
fn seed_reload_validation_failure_preserves_the_last_valid_file() {
    assert_seed_save_fault_preserves_target(ViewSessionSeedFileSaveFault::ReloadValidation);
}

#[test]
fn seed_atomic_replace_failure_preserves_the_last_valid_file() {
    assert_seed_save_fault_preserves_target(ViewSessionSeedFileSaveFault::AtomicReplace);
}

#[test]
fn seed_unknown_replace_outcome_exposes_the_staged_recovery_file() {
    let original = sample_seed();
    let replacement = ViewSessionSeed::new(GraphId::from_u128(101));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("view-session-seed.json");
    ViewSessionSeedFile::save_json(&original, &path).unwrap();

    let error = ViewSessionSeedFile::save_json_with_fault(
        &replacement,
        &path,
        ViewSessionSeedFileSaveFault::ReplaceOutcomeUnknown,
    )
    .unwrap_err();

    let recovery_path = match error {
        ViewSessionSeedFileError::CommitOutcomeUnknown {
            recovery_path: Some(recovery_path),
            ..
        } => recovery_path,
        other => panic!("expected outcome-unknown recovery error, got {other:?}"),
    };
    assert_eq!(ViewSessionSeedFile::load_json(&path).unwrap(), original);
    assert_eq!(
        ViewSessionSeedFile::load_json(&recovery_path).unwrap(),
        replacement
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn seed_parent_directory_sync_failure_reports_that_new_bytes_are_committed() {
    let original = sample_seed();
    let replacement = ViewSessionSeed::new(GraphId::from_u128(101));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("view-session-seed.json");

    ViewSessionSeedFile::save_json(&original, &path).unwrap();
    let error = ViewSessionSeedFile::save_json_with_fault(
        &replacement,
        &path,
        ViewSessionSeedFileSaveFault::ParentDirectorySync,
    )
    .unwrap_err();

    match error {
        ViewSessionSeedFileError::CommitDurabilityUnknown { recovery_path, .. } => {
            assert_eq!(recovery_path, None);
        }
        other => panic!("expected durability-unknown error, got {other:?}"),
    }
    assert_eq!(ViewSessionSeedFile::load_json(&path).unwrap(), replacement);
    assert_eq!(
        directory_entries(directory.path()),
        vec!["view-session-seed.json".to_owned()]
    );
}

#[test]
fn view_session_seed_v1_rejects_unknown_fields_at_every_owned_level() {
    let seed = sample_seed();
    let bytes = ViewSessionSeedFile::encode_json(&seed).unwrap();
    let root: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    for (case, path) in [
        ("root", vec![]),
        ("viewport", vec!["viewport"]),
        ("pan", vec!["viewport", "pan"]),
        ("selection", vec!["selection"]),
        ("stacking", vec!["stacking"]),
    ] {
        let mut corrupted = root.clone();
        let mut target = &mut corrupted;
        for key in path {
            target = target.get_mut(key).unwrap();
        }
        target.as_object_mut().unwrap().insert(
            "future_field".into(),
            serde_json::json!({ "sentinel": case }),
        );

        let corrupted = serde_json::to_vec(&corrupted).unwrap();
        assert!(
            matches!(
                ViewSessionSeedFile::decode_json(&corrupted),
                Err(ViewSessionSeedFileError::Parse { .. })
            ),
            "case {case} must reject unknown fields"
        );
    }
}

#[test]
fn future_view_session_seed_version_wins_before_payload_validation() {
    let bytes = br#"{
        "view_session_seed_version": 99,
        "future_payload": { "sentinel": true }
    }"#;

    let error = ViewSessionSeedFile::decode_json(bytes).unwrap_err();

    assert!(matches!(
        error,
        ViewSessionSeedFileError::UnsupportedVersion {
            version: 99,
            expected: VIEW_SESSION_SEED_FILE_VERSION
        }
    ));
}

#[test]
fn view_session_seed_requires_the_current_version_field() {
    let error = ViewSessionSeedFile::decode_json(br#"{"unrecognized_version":1}"#).unwrap_err();

    assert!(matches!(error, ViewSessionSeedFileError::MissingVersion));
}

#[test]
fn current_writer_excludes_transient_and_policy_state() {
    let bytes = ViewSessionSeedFile::encode_json(&sample_seed()).unwrap();
    let root: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let object = root.as_object().unwrap();

    assert_eq!(object.len(), 5);
    for required in [
        "graph_id",
        "selection",
        "stacking",
        "view_session_seed_version",
        "viewport",
    ] {
        assert!(object.contains_key(required), "missing {required}");
    }
    for forbidden in [
        "geometry", "routes", "gestures", "focus", "ime", "drafts", "theme", "culling", "view_id",
        "revision", "history",
    ] {
        assert!(root.get(forbidden).is_none(), "unexpected {forbidden}");
    }
}

#[test]
fn seed_sanitization_rejects_other_graph_and_keeps_first_visible_id() {
    let graph_id = GraphId::from_u128(10);
    let visible = NodeId::from_u128(11);
    let hidden = NodeId::from_u128(12);
    let stale = NodeId::from_u128(13);
    let mut graph = GraphBuilder::new(graph_id);
    graph.insert_node(visible, node(false));
    graph.insert_node(hidden, node(true));

    let mut seed = ViewSessionSeed::new(graph_id)
        .with_selection(ViewSelectionSeed::new(
            vec![visible, hidden, visible, stale],
            vec![],
            vec![],
        ))
        .with_stacking(ViewStackingSeed::new(
            vec![hidden, visible, visible, stale],
            vec![],
            vec![],
        ));
    seed.sanitize_for_graph(&graph).unwrap();

    assert_eq!(seed.selection().nodes(), [visible]);
    assert_eq!(seed.stacking().nodes(), [visible]);

    let other = GraphBuilder::new(GraphId::from_u128(99));
    let before_mismatch = seed.clone();
    assert!(matches!(
        seed.sanitize_for_graph(&other),
        Err(ViewSessionSeedGraphMismatch { .. })
    ));
    assert_eq!(seed, before_mismatch);
}

fn node(hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos: CanvasPoint::default(),
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn assert_seed_save_fault_preserves_target(fault: ViewSessionSeedFileSaveFault) {
    let original = sample_seed();
    let replacement = ViewSessionSeed::new(GraphId::from_u128(101));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("view-session-seed.json");

    ViewSessionSeedFile::save_json(&original, &path).unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let error = ViewSessionSeedFile::save_json_with_fault(&replacement, &path, fault).unwrap_err();

    assert_ne!(replacement, original, "fixture must change persisted facts");
    assert_seed_save_fault_error(fault, &error);
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    assert_eq!(ViewSessionSeedFile::load_json(&path).unwrap(), original);
    assert_eq!(
        directory_entries(directory.path()),
        vec!["view-session-seed.json".to_owned()]
    );
}

fn assert_seed_save_fault_error(
    fault: ViewSessionSeedFileSaveFault,
    error: &ViewSessionSeedFileError,
) {
    match fault {
        ViewSessionSeedFileSaveFault::Serialization => {
            assert!(matches!(error, ViewSessionSeedFileError::Serialize { .. }));
        }
        ViewSessionSeedFileSaveFault::TemporaryWrite => {
            assert!(matches!(error, ViewSessionSeedFileError::Write { .. }));
        }
        ViewSessionSeedFileSaveFault::TemporarySync => {
            assert!(matches!(error, ViewSessionSeedFileError::StageSync { .. }));
        }
        ViewSessionSeedFileSaveFault::ReloadValidation => {
            assert!(matches!(
                error,
                ViewSessionSeedFileError::ReloadValidation { .. }
            ));
        }
        ViewSessionSeedFileSaveFault::AtomicReplace => {
            assert!(matches!(
                error,
                ViewSessionSeedFileError::AtomicReplace { .. }
            ));
        }
        ViewSessionSeedFileSaveFault::None
        | ViewSessionSeedFileSaveFault::ReplaceOutcomeUnknown
        | ViewSessionSeedFileSaveFault::ParentDirectoryOpen
        | ViewSessionSeedFileSaveFault::ParentDirectorySync => {
            panic!("the test requires a pre-commit fault")
        }
    }
}
