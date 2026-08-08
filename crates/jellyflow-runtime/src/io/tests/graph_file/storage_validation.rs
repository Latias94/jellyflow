use super::support::*;
use super::*;

#[test]
fn graph_file_rejects_dangling_edge_endpoint() {
    let errors = load_corrupt_graph_fixture("graph_file_dangling_edge", |source| {
        source["graph"]["edges"]["00000000-0000-0000-0000-00000000e001"]["to"] =
            serde_json::json!("00000000-0000-0000-0000-00000000ffff");
    });

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, GraphValidationError::EdgeMissingPort { .. }))
    );
}

#[test]
fn graph_file_rejects_port_listed_by_the_wrong_owner() {
    let errors = load_corrupt_graph_fixture("graph_file_wrong_port_owner", |source| {
        source["graph"]["ports"]["00000000-0000-0000-0000-000000001001"]["node"] =
            serde_json::json!("00000000-0000-0000-0000-000000000020");
    });

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, GraphValidationError::NodePortsWrongOwner { .. }))
    );
}

#[test]
fn graph_file_rejects_duplicate_node_port_order_entries() {
    let errors = load_corrupt_graph_fixture("graph_file_duplicate_port_order", |source| {
        source["graph"]["nodes"]["00000000-0000-0000-0000-000000000020"]["ports"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!("00000000-0000-0000-0000-000000002003"));
    });

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, GraphValidationError::NodePortsDuplicate { .. }))
    );
}

#[test]
fn graph_file_rejects_dangling_graph_local_binding_target() {
    let errors = load_corrupt_graph_fixture("graph_file_dangling_binding", |source| {
        source["graph"]["bindings"]["00000000-0000-0000-0000-000000000c00"]["subject"]["target"]
            ["id"] = serde_json::json!("00000000-0000-0000-0000-00000000ffff");
    });

    assert!(
        errors
            .iter()
            .any(|error| matches!(error, GraphValidationError::BindingTargetMissing { .. }))
    );
}

#[test]
fn graph_file_refuses_to_replace_a_valid_target_with_an_invalid_wrapper() {
    let persisted_graph_id = GraphId::new();
    let path = temp_path("graph_file_invalid_save", persisted_graph_id);
    let persisted = GraphFile::from_graph(Graph::new(persisted_graph_id));
    persisted.save_json(&path).unwrap();
    let persisted_bytes = std::fs::read(&path).unwrap();

    let replacement = GraphFile {
        graph_id: GraphId::new(),
        graph_version: GRAPH_FILE_VERSION,
        graph: Graph::new(GraphId::new()),
    };

    let err = replacement.save_json(&path).unwrap_err();

    assert!(matches!(err, GraphFileError::InconsistentGraphId));
    assert_eq!(std::fs::read(&path).unwrap(), persisted_bytes);
    assert_eq!(
        GraphFile::load_json(&path).unwrap().graph_id,
        persisted_graph_id
    );

    let _ = std::fs::remove_file(&path);
}
