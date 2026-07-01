use jellyflow::{
    core::ops::GraphMutationPlanner,
    core::{
        CanvasRect, Graph, GraphOp, GraphTransaction, Node, NodeId, PortDirection, PortId, PortKey,
    },
    runtime::{
        runtime::{
            geometry::HandlePosition,
            measurement::{
                MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeInternalsInvalidation,
                NodeInternalsInvalidationReason,
            },
        },
        schema::{
            NodeControlBindingSource, NodeControlDescriptor, NodeKindViewDescriptor,
            NodeRepeatableCollectionDescriptor, NodeRepeatableItemProjection,
        },
    },
};
use serde_json::Value;

use crate::json_binding::{
    json_scalar_to_stable_string, repeatable_item_id as semantic_repeatable_item_id,
    semantic_json_lookup, set_bound_value, set_dot_path_value,
};

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
    pub remove_disabled_reason: Option<String>,
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

/// Adapter-local repeatable authoring command.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenGpuiRepeatableActionPlan {
    Add {
        collection_key: String,
        item: Value,
    },
    Remove {
        collection_key: String,
        item_id: String,
    },
    Reorder {
        collection_key: String,
        item_id: String,
        to_index: usize,
    },
    Edit {
        collection_key: String,
        item_id: String,
        control_key: String,
        value: Value,
    },
}

/// Mutation plan emitted by a GPUI repeatable edit.
#[derive(Debug, Clone)]
pub struct OpenGpuiRepeatableEditPlan {
    pub collection_key: String,
    pub item_id: Option<String>,
    pub transaction: GraphTransaction,
    pub invalidation: NodeInternalsInvalidation,
    pub diagnostics: Vec<OpenGpuiRepeatablePortDiagnostic>,
}

/// Failure path for repeatable authoring planning.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum OpenGpuiRepeatableEditError {
    #[error("repeatable collection not found: {collection_key}")]
    MissingCollection { collection_key: String },
    #[error("repeatable collection `{collection_key}` is not array-backed")]
    UnsupportedCollectionShape { collection_key: String },
    #[error("repeatable collection `{collection_key}` reached its maximum of {max_items} items")]
    MaxItemsReached {
        collection_key: String,
        max_items: usize,
    },
    #[error("repeatable collection `{collection_key}` requires at least {min_items} items")]
    MinItemsReached {
        collection_key: String,
        min_items: usize,
    },
    #[error("repeatable item already exists in `{collection_key}`: {item_id}")]
    DuplicateItem {
        collection_key: String,
        item_id: String,
    },
    #[error("repeatable item not found in `{collection_key}`: {item_id}")]
    MissingItem {
        collection_key: String,
        item_id: String,
    },
    #[error("repeatable item id is missing or invalid for `{collection_key}`")]
    InvalidItemId { collection_key: String },
    #[error("repeatable collection `{collection_key}` is not reorderable")]
    NotReorderable { collection_key: String },
    #[error("repeatable target index {to_index} is out of bounds for `{collection_key}`")]
    ReorderIndexOutOfBounds {
        collection_key: String,
        to_index: usize,
    },
    #[error("repeatable control not found in `{collection_key}`: {control_key}")]
    MissingControl {
        collection_key: String,
        control_key: String,
    },
    #[error("repeatable control `{control_key}` is not writable")]
    NonWritableControl { control_key: String },
    #[error("repeatable edit failed: {0}")]
    InvalidEdit(String),
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
            let remove_disabled_reason = collection.remove_disabled_reason(&node.data);
            collection
                .item_projections(&node.data)
                .into_iter()
                .map(move |item| {
                    project_repeatable_item(
                        collection,
                        item,
                        graph,
                        node_id,
                        remove_disabled_reason.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Plan an adapter-local repeatable authoring mutation.
pub fn plan_repeatable_action(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    action: OpenGpuiRepeatableActionPlan,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    match action {
        OpenGpuiRepeatableActionPlan::Add {
            collection_key,
            item,
        } => plan_repeatable_add(descriptor, graph, node_id, node, &collection_key, item),
        OpenGpuiRepeatableActionPlan::Remove {
            collection_key,
            item_id,
        } => plan_repeatable_remove(descriptor, graph, node_id, node, &collection_key, &item_id),
        OpenGpuiRepeatableActionPlan::Reorder {
            collection_key,
            item_id,
            to_index,
        } => plan_repeatable_reorder(
            descriptor,
            graph,
            node_id,
            node,
            &collection_key,
            &item_id,
            to_index,
        ),
        OpenGpuiRepeatableActionPlan::Edit {
            collection_key,
            item_id,
            control_key,
            value,
        } => plan_repeatable_item_control_edit(
            descriptor,
            graph,
            node_id,
            node,
            &collection_key,
            &item_id,
            &control_key,
            value,
        ),
    }
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
    remove_disabled_reason: Option<String>,
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
        remove_disabled_reason,
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

fn plan_repeatable_add(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    collection_key: &str,
    mut item: Value,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    let collection = repeatable_collection(descriptor, collection_key)?;
    let mut items = repeatable_array(collection, &node.data)?.to_vec();
    if let Some(max_items) = collection.max_items
        && items.len() >= max_items
    {
        return Err(OpenGpuiRepeatableEditError::MaxItemsReached {
            collection_key: collection_key.to_owned(),
            max_items,
        });
    }
    ensure_item_id(
        collection,
        &mut item,
        next_repeatable_item_id(collection, &items),
    )?;
    let item_id = repeatable_item_id(collection, &item)?;
    if items
        .iter()
        .any(|candidate| repeatable_item_id(collection, candidate).is_ok_and(|id| id == item_id))
    {
        return Err(OpenGpuiRepeatableEditError::DuplicateItem {
            collection_key: collection_key.to_owned(),
            item_id,
        });
    }
    items.push(item);
    repeatable_data_plan(descriptor, graph, node_id, node, collection, None, items)
}

fn plan_repeatable_remove(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    collection_key: &str,
    item_id: &str,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    let collection = repeatable_collection(descriptor, collection_key)?;
    let mut items = repeatable_array(collection, &node.data)?.to_vec();
    if let Some(min_items) = collection.min_items
        && items.len() <= min_items
    {
        return Err(OpenGpuiRepeatableEditError::MinItemsReached {
            collection_key: collection_key.to_owned(),
            min_items,
        });
    }
    let index = repeatable_item_index(collection, &items, item_id)?;
    let removed = items.remove(index);
    let mut plan = repeatable_data_plan(
        descriptor,
        graph,
        node_id,
        node,
        collection,
        Some(item_id),
        items,
    )?;

    if let Some(port_key) = repeatable_item_port_key(collection, &removed)
        && let Some((port_id, _)) = graph_port_by_key(graph, &node_id, &port_key)
        && let Some(plan) = plan.as_mut()
    {
        let mut ops = plan.transaction.ops().to_vec();
        let remove_ops = GraphMutationPlanner::new(graph)
            .remove_port_ops(port_id)
            .map_err(|error| OpenGpuiRepeatableEditError::InvalidEdit(error.to_string()))?;
        ops.extend(remove_ops);
        plan.transaction = GraphTransaction::from_ops(ops).with_label("Edit repeatable item");
        plan.diagnostics.retain(|diagnostic| {
            diagnostic.collection_key != collection.key || diagnostic.item_id != item_id
        });
    }

    Ok(plan)
}

fn plan_repeatable_reorder(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    collection_key: &str,
    item_id: &str,
    to_index: usize,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    let collection = repeatable_collection(descriptor, collection_key)?;
    if !collection.reorderable {
        return Err(OpenGpuiRepeatableEditError::NotReorderable {
            collection_key: collection_key.to_owned(),
        });
    }
    let mut items = repeatable_array(collection, &node.data)?.to_vec();
    if to_index >= items.len() {
        return Err(OpenGpuiRepeatableEditError::ReorderIndexOutOfBounds {
            collection_key: collection_key.to_owned(),
            to_index,
        });
    }
    let from_index = repeatable_item_index(collection, &items, item_id)?;
    if from_index == to_index {
        return Ok(None);
    }
    let item = items.remove(from_index);
    items.insert(to_index, item);
    repeatable_data_plan(
        descriptor,
        graph,
        node_id,
        node,
        collection,
        Some(item_id),
        items,
    )
}

fn plan_repeatable_item_control_edit(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    collection_key: &str,
    item_id: &str,
    control_key: &str,
    value: Value,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    let collection = repeatable_collection(descriptor, collection_key)?;
    let mut items = repeatable_array(collection, &node.data)?.to_vec();
    let index = repeatable_item_index(collection, &items, item_id)?;
    let control = repeatable_control(collection, control_key)?;
    if control.editability.read_only || control.editability.disabled_reason.is_some() {
        return Err(OpenGpuiRepeatableEditError::NonWritableControl {
            control_key: control_key.to_owned(),
        });
    }
    let binding = control.binding.as_ref().ok_or_else(|| {
        OpenGpuiRepeatableEditError::NonWritableControl {
            control_key: control_key.to_owned(),
        }
    })?;
    if matches!(
        binding.source,
        NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor
    ) {
        return Err(OpenGpuiRepeatableEditError::NonWritableControl {
            control_key: control_key.to_owned(),
        });
    }
    set_bound_value(&mut items[index], binding, value)
        .map_err(OpenGpuiRepeatableEditError::InvalidEdit)?;
    repeatable_data_plan(
        descriptor,
        graph,
        node_id,
        node,
        collection,
        Some(item_id),
        items,
    )
}

fn repeatable_data_plan(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    collection: &NodeRepeatableCollectionDescriptor,
    item_id: Option<&str>,
    items: Vec<Value>,
) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
    let from = node.data.clone();
    let mut to = from.clone();
    set_dot_path_value(&mut to, &collection.item_source, Value::Array(items))
        .map_err(OpenGpuiRepeatableEditError::InvalidEdit)?;
    if from == to {
        return Ok(None);
    }

    let mut updated = node.clone();
    updated.data = to.clone();
    let projections = repeatable_item_projection(descriptor, &updated, graph, &node_id);
    Ok(Some(OpenGpuiRepeatableEditPlan {
        collection_key: collection.key.clone(),
        item_id: item_id.map(ToOwned::to_owned),
        transaction: GraphTransaction::from_ops([GraphOp::SetNodeData {
            id: node_id,
            from,
            to,
        }])
        .with_label("Edit repeatable item"),
        invalidation: NodeInternalsInvalidation::one(
            node_id,
            NodeInternalsInvalidationReason::DataChanged,
        ),
        diagnostics: repeatable_port_diagnostics(&projections),
    }))
}

fn repeatable_collection<'a>(
    descriptor: &'a NodeKindViewDescriptor,
    collection_key: &str,
) -> Result<&'a NodeRepeatableCollectionDescriptor, OpenGpuiRepeatableEditError> {
    descriptor
        .repeatable_collections
        .iter()
        .find(|collection| collection.key == collection_key)
        .ok_or_else(|| OpenGpuiRepeatableEditError::MissingCollection {
            collection_key: collection_key.to_owned(),
        })
}

fn repeatable_array<'a>(
    collection: &NodeRepeatableCollectionDescriptor,
    node_data: &'a Value,
) -> Result<&'a [Value], OpenGpuiRepeatableEditError> {
    match semantic_json_lookup(node_data, &collection.item_source) {
        Some(Value::Array(items)) => Ok(items.as_slice()),
        None => Ok(&[]),
        Some(_) => Err(OpenGpuiRepeatableEditError::UnsupportedCollectionShape {
            collection_key: collection.key.clone(),
        }),
    }
}

fn repeatable_item_index(
    collection: &NodeRepeatableCollectionDescriptor,
    items: &[Value],
    item_id: &str,
) -> Result<usize, OpenGpuiRepeatableEditError> {
    items
        .iter()
        .position(|item| repeatable_item_id(collection, item).is_ok_and(|id| id == item_id))
        .ok_or_else(|| OpenGpuiRepeatableEditError::MissingItem {
            collection_key: collection.key.clone(),
            item_id: item_id.to_owned(),
        })
}

fn repeatable_item_id(
    collection: &NodeRepeatableCollectionDescriptor,
    item: &Value,
) -> Result<String, OpenGpuiRepeatableEditError> {
    semantic_repeatable_item_id(collection, item).ok_or_else(|| {
        OpenGpuiRepeatableEditError::InvalidItemId {
            collection_key: collection.key.clone(),
        }
    })
}

fn ensure_item_id(
    collection: &NodeRepeatableCollectionDescriptor,
    item: &mut Value,
    fallback: String,
) -> Result<(), OpenGpuiRepeatableEditError> {
    if repeatable_item_id(collection, item).is_ok() {
        return Ok(());
    }
    set_dot_path_value(item, &collection.item_id_path, Value::String(fallback))
        .map_err(OpenGpuiRepeatableEditError::InvalidEdit)
}

fn next_repeatable_item_id(
    collection: &NodeRepeatableCollectionDescriptor,
    items: &[Value],
) -> String {
    let prefix = collection
        .key
        .rsplit('.')
        .next()
        .unwrap_or("item")
        .trim_end_matches('s')
        .replace('-', "_");
    let mut index = items.len() + 1;
    loop {
        let candidate = format!("{prefix}_{index}");
        if !items
            .iter()
            .any(|item| repeatable_item_id(collection, item).is_ok_and(|id| id == candidate))
        {
            return candidate;
        }
        index += 1;
    }
}

fn repeatable_control<'a>(
    collection: &'a NodeRepeatableCollectionDescriptor,
    control_key: &str,
) -> Result<&'a NodeControlDescriptor, OpenGpuiRepeatableEditError> {
    collection
        .item_template_slots
        .iter()
        .flat_map(|slot| slot.controls.iter())
        .find(|control| control.key == control_key)
        .ok_or_else(|| OpenGpuiRepeatableEditError::MissingControl {
            collection_key: collection.key.clone(),
            control_key: control_key.to_owned(),
        })
}

fn repeatable_item_port_key(
    collection: &NodeRepeatableCollectionDescriptor,
    item: &Value,
) -> Option<PortKey> {
    collection
        .anchor_rule
        .port_key_path
        .as_deref()
        .and_then(|path| semantic_json_lookup(item, path))
        .and_then(json_scalar_to_stable_string)
        .map(PortKey::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        NodeGraphStore,
        core::{CanvasPoint, Edge, EdgeId, EdgeKind, GraphId, NodeKindKey},
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
        let input_items = items
            .iter()
            .filter(|item| item.collection_key == "shader.inputs")
            .collect::<Vec<_>>();
        let normal = input_items
            .iter()
            .find(|item| item.item_id == "normal")
            .expect("normal dynamic shader input");

        assert_eq!(input_items.len(), 1);
        assert_eq!(normal.port_key, Some(PortKey::new("normal")));
        assert_eq!(
            normal.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::MissingGraphPort
        );
        assert!(normal.port_id.is_none());
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

    #[test]
    fn repeatable_add_plans_node_data_and_reports_missing_dynamic_port() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(4)),
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

        let plan = plan_repeatable_action(
            &descriptor,
            store.graph(),
            node_id,
            node,
            OpenGpuiRepeatableActionPlan::Add {
                collection_key: "shader.inputs".to_owned(),
                item: serde_json::json!({
                    "name": "Normal",
                    "ty": "vec4",
                    "port": "normal"
                }),
            },
        )
        .expect("add plan")
        .expect("changed add plan");

        assert_eq!(plan.collection_key, "shader.inputs");
        assert_eq!(plan.item_id, None);
        assert_eq!(plan.invalidation.nodes, vec![node_id]);
        assert_eq!(
            plan.invalidation.reason,
            NodeInternalsInvalidationReason::DataChanged
        );
        let [GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected one SetNodeData op");
        };
        assert_eq!(to["dynamic_inputs"][3]["id"], serde_json::json!("input_4"));
        assert_eq!(to["dynamic_inputs"][3]["port"], serde_json::json!("normal"));
        assert!(plan.diagnostics.iter().any(|diagnostic| {
            diagnostic.collection_key == "shader.inputs"
                && diagnostic.item_id == "input_4"
                && diagnostic.port_key == PortKey::new("normal")
                && diagnostic.policy == OpenGpuiDynamicPortPolicy::MissingGraphPort
        }));

        let mut graph = store.graph().clone();
        plan.transaction.apply_to(&mut graph).expect("apply add");
        let updated = graph.nodes().get(&node_id).expect("updated node");
        let items = repeatable_item_projection(&descriptor, updated, &graph, &node_id);
        let normal = items
            .iter()
            .find(|item| item.item_id == "input_4")
            .expect("normal item");
        assert_eq!(
            normal.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::MissingGraphPort
        );
        assert!(normal.port_id.is_none());
    }

    #[test]
    fn repeatable_reorder_preserves_item_identity() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(5)),
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

        let plan = plan_repeatable_action(
            &descriptor,
            store.graph(),
            node_id,
            node,
            OpenGpuiRepeatableActionPlan::Reorder {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
                to_index: 0,
            },
        )
        .expect("reorder plan")
        .expect("changed reorder plan");
        let [GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected one SetNodeData op");
        };
        let ids = to["dynamic_inputs"]
            .as_array()
            .expect("dynamic input array")
            .iter()
            .map(|item| item["id"].as_str().expect("id"))
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["factor", "a", "b"]);

        let mut graph = store.graph().clone();
        plan.transaction
            .apply_to(&mut graph)
            .expect("apply reorder");
        let updated = graph.nodes().get(&node_id).expect("updated node");
        let items = repeatable_item_projection(&descriptor, updated, &graph, &node_id);
        let factor = items
            .iter()
            .find(|item| item.item_id == "factor")
            .expect("factor item");
        assert_eq!(factor.item_index, 0);
        assert_eq!(factor.port_key, Some(PortKey::new("factor")));
        assert_eq!(
            factor.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::BoundToGraphPort
        );
    }

    #[test]
    fn repeatable_remove_drops_bound_port_and_incident_edges() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader mix descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(6)),
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
        let factor_port = port_id_for_key(store.graph(), node_id, "factor");
        let result_port = port_id_for_key(store.graph(), node_id, "result");
        let edge_id = EdgeId::from_u128(0xfeed);
        let add_edge = GraphTransaction::from_ops([GraphOp::AddEdge {
            id: edge_id,
            edge: Edge::new(EdgeKind::Data, result_port, factor_port),
        }]);
        store
            .dispatch_transaction(&add_edge)
            .expect("seed incident edge");
        assert!(store.graph().edges().contains_key(&edge_id));
        let node = store.graph().nodes().get(&node_id).expect("node");

        let plan = plan_repeatable_action(
            &descriptor,
            store.graph(),
            node_id,
            node,
            OpenGpuiRepeatableActionPlan::Remove {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            },
        )
        .expect("remove plan")
        .expect("changed remove plan");

        assert!(plan.transaction.ops().iter().any(|op| {
            matches!(op, GraphOp::SetNodeData { to, .. }
                if to["dynamic_inputs"]
                    .as_array()
                    .is_some_and(|items| items.iter().all(|item| item["id"] != "factor")))
        }));
        assert!(plan.transaction.ops().iter().any(|op| {
            matches!(op, GraphOp::SetNodePorts { id, to, .. }
                if *id == node_id && !to.contains(&factor_port))
        }));
        assert!(plan.transaction.ops().iter().any(|op| {
            matches!(op, GraphOp::RemovePort { id, edges, .. }
                if *id == factor_port && edges.iter().any(|(id, _)| *id == edge_id))
        }));

        let mut graph = store.graph().clone();
        plan.transaction.apply_to(&mut graph).expect("apply remove");
        let updated = graph.nodes().get(&node_id).expect("updated node");
        assert!(!graph.ports().contains_key(&factor_port));
        assert!(!graph.edges().contains_key(&edge_id));
        assert!(
            !updated.ports.contains(&factor_port),
            "removed dynamic port must be removed from node port order"
        );
        let items = repeatable_item_projection(&descriptor, updated, &graph, &node_id);
        assert!(items.iter().all(|item| item.item_id != "factor"));
    }

    #[test]
    fn repeatable_item_control_edit_updates_erd_field_row() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.table"))
            .expect("table descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(7)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.table"), CanvasPoint::default()),
            )
            .expect("create table");
        let node_id = outcome.node_id();
        let node = store.graph().nodes().get(&node_id).expect("node");

        let plan = plan_repeatable_action(
            &descriptor,
            store.graph(),
            node_id,
            node,
            OpenGpuiRepeatableActionPlan::Edit {
                collection_key: "table.columns".to_owned(),
                item_id: "email".to_owned(),
                control_key: "control.column.name".to_owned(),
                value: serde_json::json!("email_address"),
            },
        )
        .expect("edit plan")
        .expect("changed edit plan");
        let [GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected one SetNodeData op");
        };
        assert_eq!(to["columns"][1]["id"], serde_json::json!("email"));
        assert_eq!(to["columns"][1]["name"], serde_json::json!("email_address"));

        let mut graph = store.graph().clone();
        plan.transaction.apply_to(&mut graph).expect("apply edit");
        let updated = graph.nodes().get(&node_id).expect("updated node");
        let items = repeatable_item_projection(&descriptor, updated, &graph, &node_id);
        let email = items
            .iter()
            .find(|item| item.item_id == "email")
            .expect("email item");
        assert_eq!(email.label, "email_address");
        assert_eq!(
            email.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::MissingGraphPort
        );
    }

    #[test]
    fn dify_repeatable_params_stay_display_only_without_fake_handles() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("llm descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(8)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.llm"), CanvasPoint::default()),
            )
            .expect("create llm");
        let node_id = outcome.node_id();
        let node = store.graph().nodes().get(&node_id).expect("node");

        let plan = plan_repeatable_action(
            &descriptor,
            store.graph(),
            node_id,
            node,
            OpenGpuiRepeatableActionPlan::Add {
                collection_key: "llm.params".to_owned(),
                item: serde_json::json!({
                    "name": "locale",
                    "value": "{{ customer.locale }}"
                }),
            },
        )
        .expect("add param plan")
        .expect("changed add param");
        let mut graph = store.graph().clone();
        plan.transaction
            .apply_to(&mut graph)
            .expect("apply param add");
        let updated = graph.nodes().get(&node_id).expect("updated node");
        let items = repeatable_item_projection(&descriptor, updated, &graph, &node_id);
        let locale = items
            .iter()
            .find(|item| item.item_id == "param_3")
            .expect("locale param");
        assert_eq!(
            locale.dynamic_port_policy,
            OpenGpuiDynamicPortPolicy::DisplayOnly
        );
        assert!(locale.port_key.is_none());
        assert!(locale.port_id.is_none());
        assert!(repeatable_port_diagnostics(&items).is_empty());
    }

    fn port_id_for_key(graph: &Graph, node_id: NodeId, key: &str) -> PortId {
        graph
            .ports()
            .iter()
            .find_map(|(port_id, port)| {
                (port.node == node_id && port.key == PortKey::new(key)).then_some(*port_id)
            })
            .expect("port exists")
    }
}
