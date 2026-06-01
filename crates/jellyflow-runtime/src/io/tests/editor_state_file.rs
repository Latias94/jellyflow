use super::*;
use jellyflow_core::core::GraphId;

#[test]
fn editor_state_file_roundtrips_split_view_and_config() {
    let graph_id = GraphId::new();
    let path = temp_path("editor_state_roundtrip", graph_id);

    let view_state = NodeGraphViewState {
        pan: jellyflow_core::core::CanvasPoint { x: 12.5, y: -3.0 },
        zoom: 1.25,
        ..NodeGraphViewState::default()
    };
    let mut editor_config = NodeGraphEditorConfig::default();
    editor_config.interaction.selection_on_drag = true;
    editor_config.interaction.select_nodes_on_drag = false;
    editor_config.runtime_tuning.only_render_visible_elements = false;

    let file = NodeGraphEditorStateFile::new(graph_id, view_state.clone(), editor_config.clone());
    file.save_json(&path).unwrap();

    let root: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        root.get("editor_state_version").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(
        root.get("view_state")
            .and_then(|v| v.get("interaction"))
            .is_none()
    );
    assert!(
        root.get("view_state")
            .and_then(|v| v.get("runtime_tuning"))
            .is_none()
    );
    assert_eq!(
        root.get("editor_config")
            .and_then(|v| v.get("interaction"))
            .and_then(|v| v.get("selection_on_drag"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        root.get("editor_config")
            .and_then(|v| v.get("interaction"))
            .and_then(|v| v.get("select_nodes_on_drag"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        root.get("editor_config")
            .and_then(|v| v.get("runtime_tuning"))
            .and_then(|v| v.get("only_render_visible_elements"))
            .and_then(|v| v.as_bool()),
        Some(false)
    );

    let loaded = NodeGraphEditorStateFile::load_json(&path, graph_id).unwrap();
    assert_eq!(loaded.graph_id, graph_id);
    assert_eq!(loaded.editor_state_version, EDITOR_STATE_FILE_VERSION);
    assert_eq!(loaded.view_state.pan.x, view_state.pan.x);
    assert_eq!(loaded.view_state.pan.y, view_state.pan.y);
    assert_eq!(loaded.view_state.zoom, view_state.zoom);
    assert!(loaded.editor_config.interaction.selection_on_drag);
    assert!(!loaded.editor_config.interaction.select_nodes_on_drag);
    assert!(
        !loaded
            .editor_config
            .runtime_tuning
            .only_render_visible_elements
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn editor_state_file_rejects_unsupported_version() {
    let graph_id = GraphId::new();
    let path = temp_path("editor_state_unsupported_version", graph_id);

    let file = NodeGraphEditorStateFile::new(
        graph_id,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    file.save_json(&path).unwrap();

    let mut root: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    root["editor_state_version"] = serde_json::json!(99);
    std::fs::write(&path, serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    let err = NodeGraphEditorStateFile::load_json(&path, graph_id).unwrap_err();
    assert!(matches!(
        err,
        NodeGraphEditorStateFileError::UnsupportedVersion {
            version: 99,
            expected: EDITOR_STATE_FILE_VERSION
        }
    ));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn editor_state_file_rejects_wrong_graph_id() {
    let graph_id = GraphId::new();
    let other = GraphId::new();
    let path = temp_path("editor_state_wrong_graph_id", graph_id);

    let file = NodeGraphEditorStateFile::new(
        graph_id,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    file.save_json(&path).unwrap();

    let err = NodeGraphEditorStateFile::load_json(&path, other).unwrap_err();
    assert!(matches!(
        err,
        NodeGraphEditorStateFileError::InconsistentGraphId
    ));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn editor_state_file_load_sanitizes_invalid_viewport() {
    let graph_id = GraphId::new();
    let path = temp_path("editor_state_invalid_viewport", graph_id);

    let file = NodeGraphEditorStateFile::new(
        graph_id,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    file.save_json(&path).unwrap();

    let mut root: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    root["view_state"]["zoom"] = serde_json::json!(-1.0);
    std::fs::write(&path, serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    let loaded = NodeGraphEditorStateFile::load_json(&path, graph_id).unwrap();
    assert_eq!(loaded.view_state.zoom, NodeGraphViewState::default().zoom);

    let _ = std::fs::remove_file(&path);
}
