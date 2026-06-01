use super::*;
use jellyflow_core::core::{Graph, GraphId};

#[test]
fn graph_file_roundtrips_wrapped_graph() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_roundtrip", graph_id);
    let graph = Graph::new(graph_id);
    let file = GraphFileV1::from_graph(graph);

    file.save_json(&path).unwrap();

    let root: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        root.get("graph_version").and_then(|v| v.as_u64()),
        Some(u64::from(GRAPH_FILE_VERSION))
    );
    assert!(root.get("graph").is_some());

    let loaded = GraphFileV1::load_json(&path).unwrap();
    assert_eq!(loaded.graph_id, graph_id);
    assert_eq!(loaded.graph_version, GRAPH_FILE_VERSION);
    assert_eq!(loaded.graph.graph_id, graph_id);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn graph_file_rejects_plain_graph_root() {
    let graph_id = GraphId::new();
    let path = temp_path("graph_file_plain_graph", graph_id);
    let graph = Graph::new(graph_id);
    std::fs::write(&path, serde_json::to_vec_pretty(&graph).unwrap()).unwrap();

    let err = GraphFileV1::load_json(&path).unwrap_err();
    assert!(matches!(err, GraphFileError::Parse { .. }));

    let _ = std::fs::remove_file(&path);
}
