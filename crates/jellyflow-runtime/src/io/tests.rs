use std::path::PathBuf;

use jellyflow_core::core::{Graph, GraphId, NodeId};

use super::*;

fn temp_path(name: &str, graph_id: GraphId) -> PathBuf {
    std::env::temp_dir().join(format!("jellyflow_runtime_{name}_{graph_id}.json"))
}

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
    assert!(
        !loaded
            .editor_config
            .runtime_tuning
            .only_render_visible_elements
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn interaction_state_split_roundtrips_runtime_tuning() {
    let mut interaction = NodeGraphInteractionState::default();
    interaction.selection_on_drag = true;
    interaction.only_render_visible_elements = false;
    interaction.spatial_index.edge_aabb_pad_screen_px = 123.0;
    interaction.paint_cache_prune.max_entries = 4_096;

    let (config, runtime_tuning) = interaction.split();
    assert!(config.selection_on_drag);
    assert!(!runtime_tuning.only_render_visible_elements);
    assert_eq!(runtime_tuning.spatial_index.edge_aabb_pad_screen_px, 123.0);
    assert_eq!(runtime_tuning.paint_cache_prune.max_entries, 4_096);

    let rebuilt = NodeGraphInteractionState::from_parts(&config, &runtime_tuning);
    assert_eq!(rebuilt, interaction);
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
fn view_state_sanitize_removes_stale_ids() {
    let graph_id = GraphId::new();
    let mut graph = Graph::new(graph_id);

    let keep_node = NodeId::new();
    graph.nodes.insert(
        keep_node,
        jellyflow_core::core::Node {
            kind: jellyflow_core::core::NodeKindKey::new("test"),
            kind_version: 1,
            pos: jellyflow_core::core::CanvasPoint { x: 0.0, y: 0.0 },
            selectable: None,
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
        },
    );

    let mut state = NodeGraphViewState {
        selected_nodes: vec![keep_node, NodeId::new()],
        draw_order: vec![NodeId::new(), keep_node],
        ..NodeGraphViewState::default()
    };

    state.sanitize_for_graph(&graph);
    assert_eq!(state.selected_nodes, vec![keep_node]);
    assert_eq!(state.draw_order, vec![keep_node]);
}
