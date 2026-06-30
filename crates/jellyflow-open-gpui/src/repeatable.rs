use jellyflow::{
    core::{CanvasRect, Graph, Node, NodeId, PortDirection, PortId, PortKey},
    runtime::{
        runtime::{
            geometry::HandlePosition,
            measurement::{MeasuredSurfaceAnchor, MeasuredSurfaceSlot},
        },
        schema::{
            NodeKindViewDescriptor, NodeRepeatableCollectionDescriptor,
            NodeRepeatableItemProjection,
        },
    },
};
use serde_json::Value;

/// Dynamic-port support policy for one repeatable item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiDynamicPortPolicy {
    /// The item is display-only and must not publish graph-port anchors.
    DisplayOnly,
    /// A graph port with the semantic item port key exists and can be measured.
    BoundToGraphPort,
    /// The item declares a port key but the graph has no matching port yet.
    MissingGraphPort,
}

/// Adapter-visible diagnostic for repeatables that need graph/profile work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiRepeatablePortDiagnostic {
    pub collection_key: String,
    pub item_id: String,
    pub port_key: PortKey,
    pub policy: OpenGpuiDynamicPortPolicy,
    pub message: String,
}

/// Adapter projection for one stable repeatable item row.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiRepeatableItemProjection {
    pub collection_key: String,
    pub item_id: String,
    pub item_index: usize,
    pub slot_key: String,
    pub anchor: String,
    pub label: String,
    pub port_key: Option<PortKey>,
    pub port_id: Option<PortId>,
    pub port_direction: Option<PortDirection>,
    pub dynamic_port_policy: OpenGpuiDynamicPortPolicy,
    pub controls: usize,
    pub item_data: Value,
}

impl OpenGpuiRepeatableItemProjection {
    pub fn has_graph_port(&self) -> bool {
        self.port_id.is_some()
    }
}

/// Layout facts for one stable repeatable item row.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiRepeatableItemLayout {
    pub projection: OpenGpuiRepeatableItemProjection,
    pub rect: CanvasRect,
    pub anchor_rect: CanvasRect,
}

/// Project all repeatable items for one descriptor using graph ports as the source of truth for
/// item-level handle support.
pub fn repeatable_item_projection(
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
    graph: &Graph,
    node_id: &NodeId,
) -> Vec<OpenGpuiRepeatableItemProjection> {
    descriptor
        .repeatable_collections
        .iter()
        .flat_map(|collection| {
            collection
                .item_projections(&node.data)
                .into_iter()
                .map(|item| project_repeatable_item(collection, item, graph, node_id))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Build diagnostics for repeatable items whose dynamic-port contract is not satisfied by graph
/// ports yet.
pub fn repeatable_port_diagnostics(
    items: &[OpenGpuiRepeatableItemProjection],
) -> Vec<OpenGpuiRepeatablePortDiagnostic> {
    items
        .iter()
        .filter_map(|item| {
            let port_key = item.port_key.clone()?;
            (item.dynamic_port_policy == OpenGpuiDynamicPortPolicy::MissingGraphPort).then(|| {
                OpenGpuiRepeatablePortDiagnostic {
                    collection_key: item.collection_key.clone(),
                    item_id: item.item_id.clone(),
                    port_key: port_key.clone(),
                    policy: item.dynamic_port_policy,
                    message: format!(
                        "repeatable item `{}` declares port `{}` but the graph has no matching port",
                        item.item_id, port_key.0
                    ),
                }
            })
        })
        .collect()
}

/// Convert repeatable item layouts into measured semantic slots.
pub fn measured_repeatable_item_slots(
    repeatables: &[OpenGpuiRepeatableItemLayout],
) -> Vec<MeasuredSurfaceSlot> {
    repeatables
        .iter()
        .map(|repeatable| {
            MeasuredSurfaceSlot::new(repeatable.projection.slot_key.clone(), repeatable.rect)
        })
        .collect()
}

/// Convert repeatable item layouts with graph-bound ports into measured semantic anchors.
pub fn measured_repeatable_item_anchors(
    repeatables: &[OpenGpuiRepeatableItemLayout],
) -> Vec<MeasuredSurfaceAnchor> {
    repeatables
        .iter()
        .filter_map(|repeatable| {
            let port_id = repeatable.projection.port_id?;
            let port_key = repeatable.projection.port_key.clone()?;
            let position = HandlePosition::fallback_for_direction(
                repeatable
                    .projection
                    .port_direction
                    .unwrap_or(PortDirection::In),
            );
            Some(
                MeasuredSurfaceAnchor::new(
                    repeatable.projection.anchor.clone(),
                    repeatable.anchor_rect,
                    position,
                )
                .with_port(port_id)
                .with_port_key(port_key),
            )
        })
        .collect()
}

pub fn repeatable_item_label(item: &NodeRepeatableItemProjection) -> String {
    item.item_data
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| item.item_data.get("label").and_then(Value::as_str))
        .unwrap_or(item.item_id.as_str())
        .to_owned()
}

pub fn repeatable_item_control_count(item: &NodeRepeatableItemProjection) -> usize {
    item.slots.iter().map(|slot| slot.controls.len()).sum()
}

fn project_repeatable_item(
    collection: &NodeRepeatableCollectionDescriptor,
    item: NodeRepeatableItemProjection,
    graph: &Graph,
    node_id: &NodeId,
) -> OpenGpuiRepeatableItemProjection {
    let port_key = item.port_key.as_ref().map(|key| PortKey::new(key.clone()));
    let port = port_key
        .as_ref()
        .and_then(|port_key| graph_port_by_key(graph, node_id, port_key));
    let dynamic_port_policy = match (&port_key, port) {
        (None, _) => OpenGpuiDynamicPortPolicy::DisplayOnly,
        (Some(_), Some(_)) => OpenGpuiDynamicPortPolicy::BoundToGraphPort,
        (Some(_), None) => OpenGpuiDynamicPortPolicy::MissingGraphPort,
    };

    OpenGpuiRepeatableItemProjection {
        collection_key: collection.key.clone(),
        item_id: item.item_id.clone(),
        item_index: item.item_index,
        slot_key: item.slot_key.clone(),
        anchor: item.anchor.clone(),
        label: repeatable_item_label(&item),
        port_key,
        port_id: port.map(|(port_id, _)| port_id),
        port_direction: port.map(|(_, direction)| direction),
        dynamic_port_policy,
        controls: repeatable_item_control_count(&item),
        item_data: item.item_data,
    }
}

fn graph_port_by_key(
    graph: &Graph,
    node_id: &NodeId,
    key: &PortKey,
) -> Option<(PortId, PortDirection)> {
    graph.ports().iter().find_map(|(port_id, port)| {
        (port.node == *node_id && port.key == *key).then_some((*port_id, port.dir))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        NodeGraphStore,
        core::{CanvasPoint, GraphId, NodeKindKey},
        runtime::{
            io::{NodeGraphEditorConfig, NodeGraphViewState},
            runtime::create_node::CreateNodeRequest,
            schema::NodeKitRegistry,
        },
    };

    #[test]
    fn repeatable_items_preserve_identity_and_bind_existing_graph_ports() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(1)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();
        let node = store.graph().nodes().get(&node_id).expect("node");

        let items = repeatable_item_projection(&descriptor, node, store.graph(), &node_id);

        let a = items.iter().find(|item| item.item_id == "a").expect("a");
        assert_eq!(a.slot_key, "rail.inputs.a");
        assert_eq!(a.anchor, "rail.inputs.a");
        assert_eq!(a.port_key, Some(PortKey::new("a")));
        assert_eq!(
            a.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::BoundToGraphPort
        );
        assert!(a.has_graph_port());
    }

    #[test]
    fn repeatable_items_report_missing_graph_ports_without_pretending_to_bind_handles() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(2)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();
        let mut node = store.graph().nodes().get(&node_id).expect("node").clone();
        node.data["dynamic_inputs"] = serde_json::json!([
            { "id": "normal", "name": "Normal", "ty": "vec4", "port": "normal" }
        ]);

        let items = repeatable_item_projection(&descriptor, &node, store.graph(), &node_id);
        let diagnostics = repeatable_port_diagnostics(&items);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].port_key, Some(PortKey::new("normal")));
        assert_eq!(
            items[0].dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::MissingGraphPort
        );
        assert!(items[0].port_id.is_none());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].port_key, PortKey::new("normal"));
    }

    #[test]
    fn repeatable_item_identity_survives_reorder_add_and_remove() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(3)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();
        let base_node = store.graph().nodes().get(&node_id).expect("node");

        let mut reordered = base_node.clone();
        reordered.data["dynamic_inputs"] = serde_json::json!([
            { "id": "factor", "name": "Factor", "ty": "float", "port": "factor" },
            { "id": "a", "name": "A", "ty": "vec4", "port": "a" },
            { "id": "b", "name": "B", "ty": "vec4", "port": "b" }
        ]);
        let mut removed = base_node.clone();
        removed.data["dynamic_inputs"] = serde_json::json!([
            { "id": "a", "name": "A", "ty": "vec4", "port": "a" },
            { "id": "b", "name": "B", "ty": "vec4", "port": "b" }
        ]);
        let mut added = base_node.clone();
        added.data["dynamic_inputs"] = serde_json::json!([
            { "id": "a", "name": "A", "ty": "vec4", "port": "a" },
            { "id": "b", "name": "B", "ty": "vec4", "port": "b" },
            { "id": "factor", "name": "Factor", "ty": "float", "port": "factor" },
            { "id": "normal", "name": "Normal", "ty": "vec4", "port": "normal" }
        ]);

        let reordered_items =
            repeatable_item_projection(&descriptor, &reordered, store.graph(), &node_id);
        let removed_items =
            repeatable_item_projection(&descriptor, &removed, store.graph(), &node_id);
        let added_items = repeatable_item_projection(&descriptor, &added, store.graph(), &node_id);

        let factor = reordered_items
            .iter()
            .find(|item| item.item_id == "factor")
            .expect("factor item");
        assert_eq!(factor.item_index, 0);
        assert_eq!(factor.anchor, "rail.inputs.factor");
        assert_eq!(factor.port_key, Some(PortKey::new("factor")));
        assert_eq!(
            factor.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::BoundToGraphPort
        );
        assert!(
            removed_items.iter().all(|item| item.item_id != "factor"),
            "removed repeatable item must stop publishing item rows and anchors"
        );

        let normal = added_items
            .iter()
            .find(|item| item.item_id == "normal")
            .expect("added normal item");
        assert_eq!(
            normal.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::MissingGraphPort
        );
        assert_eq!(repeatable_port_diagnostics(&added_items).len(), 1);
    }
}
