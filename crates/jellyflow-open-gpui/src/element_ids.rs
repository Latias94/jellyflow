use jellyflow::core::NodeId;

/// Stable element-id scope used by host GPUI components.
///
/// Node-owned controls/actions include the node id. Graph-level actions use the
/// literal graph scope so they do not accidentally collide with node internals.
pub fn open_gpui_node_element_scope(node_id: Option<NodeId>) -> String {
    node_id
        .map(|node_id| node_id.0.to_string())
        .unwrap_or_else(|| "graph".to_owned())
}

pub fn open_gpui_action_button_element_id(
    node_id: Option<NodeId>,
    menu_key: impl AsRef<str>,
    action_key: impl AsRef<str>,
    index: usize,
) -> String {
    format!(
        "jellyflow-action-button:{}:{}:{}:{index}",
        open_gpui_node_element_scope(node_id),
        id_segment(menu_key),
        id_segment(action_key)
    )
}

pub fn open_gpui_action_menu_element_id(
    node_id: Option<NodeId>,
    menu_key: impl AsRef<str>,
    id_suffix: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-action-menu:{}:{}:{}",
        open_gpui_node_element_scope(node_id),
        id_segment(menu_key),
        id_segment(id_suffix)
    )
}

pub fn open_gpui_action_summary_element_id(node_id: NodeId, action_key: impl AsRef<str>) -> String {
    format!(
        "jellyflow-action-summary:{}:{}",
        node_id.0,
        id_segment(action_key)
    )
}

pub fn open_gpui_slot_action_button_element_id(
    node_id: NodeId,
    slot_key: impl AsRef<str>,
    index: usize,
) -> String {
    format!(
        "jellyflow-action:{}:{}:{index}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_control_element_id(
    node_id: NodeId,
    control_scope: impl AsRef<str>,
    control_key: impl AsRef<str>,
    index: usize,
) -> String {
    format!(
        "jellyflow-control:{}:{}:{}:{index}",
        node_id.0,
        id_segment(control_scope),
        id_segment(control_key)
    )
}

pub fn open_gpui_chrome_fallback_button_element_id(
    node_id: NodeId,
    node_kind: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-chrome-run-fallback:{}:{}",
        node_id.0,
        id_segment(node_kind)
    )
}

pub fn open_gpui_slot_badge_element_id(node_id: NodeId, slot_key: impl AsRef<str>) -> String {
    format!(
        "jellyflow-slot-badge:{}:{}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_slot_status_label_element_id(
    node_id: NodeId,
    slot_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-status-label:{}:{}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_slot_action_label_element_id(
    node_id: NodeId,
    slot_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-action-label:{}:{}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_slot_preview_progress_element_id(
    node_id: NodeId,
    slot_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-preview-progress:{}:{}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_slot_value_element_id(node_id: NodeId, slot_key: impl AsRef<str>) -> String {
    format!(
        "jellyflow-slot-value:{}:{}",
        node_id.0,
        id_segment(slot_key)
    )
}

pub fn open_gpui_blackboard_item_element_id(
    node_id: NodeId,
    blackboard_key: impl AsRef<str>,
    item_id: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-blackboard-item:{}:{}:{}",
        node_id.0,
        id_segment(blackboard_key),
        id_segment(item_id)
    )
}

pub fn open_gpui_blackboard_status_element_id(
    node_id: NodeId,
    blackboard_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-blackboard-status:{}:{}",
        node_id.0,
        id_segment(blackboard_key)
    )
}

pub fn open_gpui_repeatable_collection_element_id(
    node_id: NodeId,
    collection_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-repeatable:{}:{}",
        node_id.0,
        id_segment(collection_key)
    )
}

pub fn open_gpui_repeatable_add_action_element_id(
    node_id: NodeId,
    collection_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-repeatable-add:{}:{}",
        node_id.0,
        id_segment(collection_key)
    )
}

pub fn open_gpui_repeatable_item_element_id(
    node_id: NodeId,
    collection_key: impl AsRef<str>,
    item_id: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-repeatable-item:{}:{}:{}",
        node_id.0,
        id_segment(collection_key),
        id_segment(item_id)
    )
}

pub fn open_gpui_repeatable_reorder_action_element_id(
    node_id: NodeId,
    collection_key: impl AsRef<str>,
    item_id: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-repeatable-up:{}:{}:{}",
        node_id.0,
        id_segment(collection_key),
        id_segment(item_id)
    )
}

pub fn open_gpui_repeatable_remove_action_element_id(
    node_id: NodeId,
    collection_key: impl AsRef<str>,
    item_id: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-repeatable-remove:{}:{}:{}",
        node_id.0,
        id_segment(collection_key),
        id_segment(item_id)
    )
}

pub fn open_gpui_custom_renderer_badge_element_id(
    node_id: NodeId,
    renderer_key: impl AsRef<str>,
) -> String {
    format!(
        "jellyflow-custom-renderer:{}:{}",
        node_id.0,
        id_segment(renderer_key)
    )
}

pub fn open_gpui_custom_action_missing_element_id(node_id: NodeId) -> String {
    format!("jellyflow-custom-action-missing:{}", node_id.0)
}

pub fn open_gpui_custom_slots_badge_element_id(node_id: NodeId) -> String {
    format!("jellyflow-custom-slots:{}", node_id.0)
}

pub fn open_gpui_custom_repeatables_badge_element_id(node_id: NodeId) -> String {
    format!("jellyflow-custom-repeatables:{}", node_id.0)
}

fn id_segment(segment: impl AsRef<str>) -> String {
    segment.as_ref().replace('%', "%25").replace(':', "%3A")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_scoped_element_ids_do_not_collide_for_same_keys() {
        let first = NodeId::from_u128(1);
        let second = NodeId::from_u128(2);

        assert_ne!(
            open_gpui_control_element_id(first, "field.prompt", "control.prompt", 0),
            open_gpui_control_element_id(second, "field.prompt", "control.prompt", 0)
        );
        assert_ne!(
            open_gpui_action_button_element_id(Some(first), "synthetic.Node", "action.llm.run", 0),
            open_gpui_action_button_element_id(Some(second), "synthetic.Node", "action.llm.run", 0)
        );
        assert_ne!(
            open_gpui_action_menu_element_id(Some(first), "synthetic.Node", "toolbar"),
            open_gpui_action_menu_element_id(Some(second), "synthetic.Node", "toolbar")
        );
        assert_ne!(
            open_gpui_chrome_fallback_button_element_id(first, "demo.llm"),
            open_gpui_chrome_fallback_button_element_id(second, "demo.llm")
        );
    }

    #[test]
    fn graph_level_action_ids_use_graph_scope() {
        assert!(
            open_gpui_action_button_element_id(None, "synthetic.Graph", "action.graph", 0)
                .contains(":graph:")
        );
        assert!(
            open_gpui_action_menu_element_id(None, "synthetic.Graph", "canvas").contains(":graph:")
        );
    }

    #[test]
    fn repeatable_and_blackboard_ids_are_node_scoped() {
        let first = NodeId::from_u128(1);
        let second = NodeId::from_u128(2);

        assert_ne!(
            open_gpui_repeatable_item_element_id(first, "shader.inputs", "factor"),
            open_gpui_repeatable_item_element_id(second, "shader.inputs", "factor")
        );
        assert_ne!(
            open_gpui_repeatable_remove_action_element_id(first, "shader.inputs", "factor"),
            open_gpui_repeatable_remove_action_element_id(second, "shader.inputs", "factor")
        );
        assert_ne!(
            open_gpui_blackboard_item_element_id(first, "blackboard.shader.properties", "color"),
            open_gpui_blackboard_item_element_id(second, "blackboard.shader.properties", "color")
        );
        assert_ne!(
            open_gpui_slot_value_element_id(first, "field.prompt"),
            open_gpui_slot_value_element_id(second, "field.prompt")
        );
    }

    #[test]
    fn id_segments_escape_separator_collisions() {
        let node = NodeId::from_u128(1);

        assert_ne!(
            open_gpui_action_button_element_id(Some(node), "menu:a", "b", 0),
            open_gpui_action_button_element_id(Some(node), "menu", "a:b", 0)
        );
        assert_ne!(
            open_gpui_control_element_id(node, "scope:a", "b", 0),
            open_gpui_control_element_id(node, "scope", "a:b", 0)
        );
        assert_ne!(
            open_gpui_repeatable_item_element_id(node, "items:a", "b"),
            open_gpui_repeatable_item_element_id(node, "items", "a:b")
        );
    }

    #[test]
    fn custom_renderer_badges_are_node_scoped() {
        assert_ne!(
            open_gpui_custom_renderer_badge_element_id(NodeId::from_u128(1), "decision-card"),
            open_gpui_custom_renderer_badge_element_id(NodeId::from_u128(2), "decision-card")
        );
    }
}
