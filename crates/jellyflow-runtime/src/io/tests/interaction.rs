use super::*;

#[test]
fn interaction_state_split_roundtrips_runtime_tuning() {
    let mut interaction = NodeGraphInteractionState::default();
    interaction.selection_on_drag = true;
    interaction.select_nodes_on_drag = false;
    interaction.only_render_visible_elements = false;
    interaction.spatial_index.edge_aabb_pad_screen_px = 123.0;
    interaction.paint_cache_prune.max_entries = 4_096;

    let (config, runtime_tuning) = interaction.split();
    assert!(config.selection_on_drag);
    assert!(!config.select_nodes_on_drag);
    assert!(!runtime_tuning.only_render_visible_elements);
    assert_eq!(runtime_tuning.spatial_index.edge_aabb_pad_screen_px, 123.0);
    assert_eq!(runtime_tuning.paint_cache_prune.max_entries, 4_096);

    let rebuilt = NodeGraphInteractionState::from_parts(&config, &runtime_tuning);
    assert_eq!(rebuilt, interaction);
}

#[test]
fn editor_config_parts_roundtrip() {
    let mut interaction = NodeGraphInteractionConfig::default();
    interaction.selection_on_drag = true;
    interaction.select_nodes_on_drag = false;
    let runtime_tuning = NodeGraphRuntimeTuning {
        only_render_visible_elements: false,
        ..NodeGraphRuntimeTuning::default()
    };

    let editor_config = NodeGraphEditorConfig::from_parts(interaction.clone(), runtime_tuning);
    assert_eq!(editor_config.interaction, interaction);
    assert_eq!(editor_config.runtime_tuning, runtime_tuning);
    assert_eq!(
        editor_config.clone().into_parts(),
        (interaction, runtime_tuning)
    );
}

#[test]
fn interaction_config_defaults_match_xyflow_connection_and_drag_feel() {
    let config: NodeGraphInteractionConfig = serde_json::from_value(serde_json::json!({})).unwrap();

    assert!(config.select_nodes_on_drag);
    assert!(config.connect_on_click);
    assert!(!config.pan_on_scroll);
    assert_eq!(config.pan_on_scroll_speed, 0.5);
    assert_eq!(config.connection_radius, 20.0);
    assert_eq!(config.connection_drag_threshold, 1.0);
    assert_eq!(config.node_click_distance, 0.0);
    assert_eq!(config.snap_grid.width, 15.0);
    assert_eq!(config.snap_grid.height, 15.0);
    assert!(config.nodes_focusable);
    assert!(config.edges_focusable);
    assert!(config.elevate_nodes_on_select);
    assert!(config.elevate_edges_on_select);
    assert!(config.auto_pan.on_node_drag);
    assert!(config.auto_pan.on_connect);
    assert!(config.auto_pan.on_node_focus);
    assert_eq!(config.auto_pan.margin, 40.0);
    assert!(NodeGraphInteractionConfig::default().select_nodes_on_drag);
    assert!(NodeGraphInteractionConfig::default().connect_on_click);
    assert!(NodeGraphInteractionState::default().select_nodes_on_drag);
    assert!(NodeGraphInteractionState::default().connect_on_click);
    assert!(!NodeGraphInteractionState::default().pan_on_scroll);
    assert!(
        NodeGraphInteractionState::default()
            .selection_interaction()
            .select_nodes_on_drag
    );
    assert!(
        NodeGraphInteractionState::default()
            .connection_interaction()
            .connect_on_click
    );
    assert!(
        NodeGraphInteractionState::default()
            .connection_interaction()
            .auto_pan
            .on_node_focus
    );
    assert_eq!(
        NodeGraphInteractionState::default()
            .connection_interaction()
            .connection_radius,
        20.0
    );
}
