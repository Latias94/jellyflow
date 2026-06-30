use jellyflow::{
    core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeId, PortDirection},
    runtime::{
        runtime::{
            geometry::HandlePosition,
            measurement::{MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeMeasurement},
        },
        schema::{
            NodeKindViewDescriptor, NodeRepeatableCollectionDescriptor, NodeSurfaceSlotDescriptor,
            NodeSurfaceSlotKind, NodeSurfaceSlotProjection, NodeSurfaceSlotVisibility,
        },
    },
};
use serde_json::Value;

use crate::OpenGpuiMeasurementMode;

const NODE_SURFACE_CHROME_HEIGHT: f32 = 78.0;
const NODE_SURFACE_SLOT_ROW_HEIGHT: f32 = 26.0;

/// Adapter-local projection layout for one semantic node surface.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiNodeSurfaceLayout {
    pub slots: Vec<OpenGpuiNodeSurfaceSlotLayout>,
    pub repeatables: Vec<OpenGpuiRepeatableSurfaceLayout>,
    pub measurement_mode: OpenGpuiMeasurementMode,
}

impl OpenGpuiNodeSurfaceLayout {
    pub fn new(
        slots: Vec<(NodeSurfaceSlotProjection, Option<NodeSurfaceSlotDescriptor>)>,
        repeatables: Vec<OpenGpuiRepeatableSurfaceProjection>,
        node_size: CanvasSize,
        slot_limit: usize,
    ) -> Self {
        let slots = ordered_visible_slot_projections(slots)
            .into_iter()
            .take(slot_limit)
            .enumerate()
            .map(
                |(index, (slot, descriptor))| OpenGpuiNodeSurfaceSlotLayout {
                    rect: slot_row_rect(index, node_size),
                    anchor_rect: slot_anchor_rect(index, node_size),
                    slot,
                    descriptor,
                },
            )
            .collect::<Vec<_>>();
        let slot_count = slots.len();
        let repeatables = repeatables
            .into_iter()
            .enumerate()
            .map(|(index, projection)| {
                let row_index = slot_count + index;
                OpenGpuiRepeatableSurfaceLayout {
                    rect: slot_row_rect(row_index, node_size),
                    anchor_rect: slot_anchor_rect(row_index, node_size),
                    projection,
                }
            })
            .collect();
        Self {
            slots,
            repeatables,
            measurement_mode: OpenGpuiMeasurementMode::ProjectionFallback,
        }
    }

    pub fn slot_rect(&self, key: &str) -> Option<CanvasRect> {
        self.slots
            .iter()
            .find(|slot| slot.slot.key == key)
            .map(|slot| slot.rect)
    }

    pub fn anchor_rect(&self, key: &str) -> Option<CanvasRect> {
        self.slots
            .iter()
            .find(|slot| slot.slot.key == key)
            .map(|slot| slot.anchor_rect)
    }
}

/// One slot row in the adapter-local projected surface layout.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiNodeSurfaceSlotLayout {
    pub slot: NodeSurfaceSlotProjection,
    pub descriptor: Option<NodeSurfaceSlotDescriptor>,
    pub rect: CanvasRect,
    pub anchor_rect: CanvasRect,
}

/// Summary for one repeatable collection rendered by the GPUI adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiRepeatableSurfaceProjection {
    pub key: String,
    pub label: String,
    pub item_count: usize,
    pub controls: usize,
}

/// Layout facts for one projected repeatable collection row.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiRepeatableSurfaceLayout {
    pub projection: OpenGpuiRepeatableSurfaceProjection,
    pub rect: CanvasRect,
    pub anchor_rect: CanvasRect,
}

/// Build the projection fallback layout for one node descriptor and data payload.
pub fn projected_node_surface_component_layout(
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
    node_size: CanvasSize,
) -> OpenGpuiNodeSurfaceLayout {
    let slots = descriptor
        .surface_slots_projection(&node.data, None, 1.0)
        .into_iter()
        .filter(|slot| {
            descriptor
                .surface_slot(&slot.key)
                .is_some_and(|descriptor_slot| descriptor_slot.is_visible())
        })
        .map(|slot| {
            let descriptor_slot = descriptor.surface_slot(&slot.key).cloned();
            (slot, descriptor_slot)
        })
        .collect();
    OpenGpuiNodeSurfaceLayout::new(
        slots,
        repeatable_surface_projection(descriptor, &node.data),
        node_size,
        usize::MAX,
    )
}

/// Build a projection fallback measurement for one node.
pub fn project_node_measurement(
    id: &NodeId,
    node: &Node,
    graph: &Graph,
    descriptor: &NodeKindViewDescriptor,
) -> NodeMeasurement {
    let node_size = node.size.unwrap_or(CanvasSize {
        width: 228.0,
        height: 168.0,
    });
    let layout = projected_node_surface_component_layout(descriptor, node, node_size);
    let slots = measured_surface_slots(&layout);
    let anchors = measured_surface_anchors(descriptor, graph, id, &layout);

    NodeMeasurement::new(*id)
        .with_revision(1)
        .with_size(Some(node_size))
        .with_slots(slots)
        .with_anchors(anchors)
}

/// Build repeatable collection summaries from semantic descriptors.
pub fn repeatable_surface_projection(
    descriptor: &NodeKindViewDescriptor,
    data: &Value,
) -> Vec<OpenGpuiRepeatableSurfaceProjection> {
    descriptor
        .repeatable_collections
        .iter()
        .map(|collection| {
            let item_count = collection.item_projections(data).len();
            OpenGpuiRepeatableSurfaceProjection {
                key: collection.key.clone(),
                label: collection
                    .label
                    .clone()
                    .unwrap_or_else(|| collection.key.clone()),
                item_count,
                controls: repeatable_control_count(collection),
            }
        })
        .collect()
}

pub fn measured_surface_slots(layout: &OpenGpuiNodeSurfaceLayout) -> Vec<MeasuredSurfaceSlot> {
    let mut slots = layout
        .slots
        .iter()
        .map(|slot| {
            MeasuredSurfaceSlot::new(slot.slot.key.clone(), slot.rect)
                .with_visibility(slot_projection_visibility(&slot.slot))
        })
        .collect::<Vec<_>>();
    slots.extend(layout.repeatables.iter().map(|repeatable| {
        MeasuredSurfaceSlot::new(repeatable.projection.key.clone(), repeatable.rect)
            .with_visibility(NodeSurfaceSlotVisibility::Visible)
    }));
    slots
}

pub fn measured_surface_anchors(
    descriptor: &NodeKindViewDescriptor,
    graph: &Graph,
    node_id: &NodeId,
    layout: &OpenGpuiNodeSurfaceLayout,
) -> Vec<MeasuredSurfaceAnchor> {
    layout
        .slots
        .iter()
        .flat_map(|slot| {
            let Some(anchor) = descriptor
                .surface_slot(&slot.slot.key)
                .and_then(|descriptor_slot| descriptor_slot.anchor.as_ref())
            else {
                return Vec::new();
            };
            descriptor
                .ports_by_anchor(anchor)
                .into_iter()
                .filter_map(|decl| {
                    let port_id = graph
                        .ports()
                        .iter()
                        .find(|(_, port)| port.node == *node_id && port.key == decl.key)
                        .map(|(port_id, _)| *port_id)?;
                    let position = handle_position_for_direction(decl.dir);
                    Some(
                        MeasuredSurfaceAnchor::new(anchor.clone(), slot.anchor_rect, position)
                            .with_port(port_id)
                            .with_port_key(decl.key.clone())
                            .with_visibility(slot_projection_visibility(&slot.slot)),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn slot_projection_visibility(slot: &NodeSurfaceSlotProjection) -> NodeSurfaceSlotVisibility {
    if slot.visible {
        NodeSurfaceSlotVisibility::Visible
    } else {
        NodeSurfaceSlotVisibility::Hidden
    }
}

pub fn semantic_component_priority(kind: NodeSurfaceSlotKind) -> usize {
    match kind {
        NodeSurfaceSlotKind::FieldRow => 0,
        NodeSurfaceSlotKind::PortRail => 1,
        NodeSurfaceSlotKind::Badge => 2,
        NodeSurfaceSlotKind::MetricBadge => 3,
        NodeSurfaceSlotKind::StatusBanner => 4,
        NodeSurfaceSlotKind::ConfigGroup => 5,
        NodeSurfaceSlotKind::Preview => 6,
        NodeSurfaceSlotKind::NestedRegion => 7,
        NodeSurfaceSlotKind::ActionRow => 8,
        NodeSurfaceSlotKind::Header => 9,
        NodeSurfaceSlotKind::Body => 10,
        NodeSurfaceSlotKind::Footer => 11,
        NodeSurfaceSlotKind::Icon => 12,
    }
}

pub fn slot_row_rect(index: usize, node_size: CanvasSize) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: 12.0,
            y: slot_row_y(index, node_size.height),
        },
        size: CanvasSize {
            width: (node_size.width - 24.0).max(1.0),
            height: NODE_SURFACE_SLOT_ROW_HEIGHT.max(1.0),
        },
    }
}

pub fn slot_anchor_rect(index: usize, node_size: CanvasSize) -> CanvasRect {
    let row = slot_row_rect(index, node_size);
    CanvasRect {
        origin: CanvasPoint {
            x: 0.0,
            y: row.origin.y + 3.0,
        },
        size: CanvasSize {
            width: node_size.width.max(1.0),
            height: (row.size.height - 6.0).max(1.0),
        },
    }
}

pub fn slot_row_y(index: usize, node_height: f32) -> f32 {
    let y = NODE_SURFACE_CHROME_HEIGHT + NODE_SURFACE_SLOT_ROW_HEIGHT * index as f32;
    y.clamp(
        8.0,
        (node_height - NODE_SURFACE_SLOT_ROW_HEIGHT - 8.0).max(8.0),
    )
}

fn repeatable_control_count(collection: &NodeRepeatableCollectionDescriptor) -> usize {
    collection
        .item_template_slots
        .iter()
        .map(|slot| slot.controls.len())
        .sum()
}

fn ordered_visible_slot_projections(
    mut slots: Vec<(NodeSurfaceSlotProjection, Option<NodeSurfaceSlotDescriptor>)>,
) -> Vec<(NodeSurfaceSlotProjection, Option<NodeSurfaceSlotDescriptor>)> {
    slots.retain(|(slot, _)| slot.visible);
    slots.sort_by_key(|(slot, _)| (semantic_component_priority(slot.kind), slot.key.clone()));
    slots
}

fn handle_position_for_direction(direction: PortDirection) -> HandlePosition {
    match direction {
        PortDirection::In => HandlePosition::Left,
        PortDirection::Out => HandlePosition::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        NodeGraphStore,
        core::{Graph, GraphId, NodeKindKey, PortKey},
        runtime::{
            io::{NodeGraphEditorConfig, NodeGraphViewState},
            runtime::create_node::CreateNodeRequest,
            schema::NodeKitRegistry,
        },
    };

    #[test]
    fn projection_layout_orders_visible_slots_and_repeatables() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("builtin llm descriptor");
        let node = Node {
            kind: descriptor.kind.clone(),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 240.0,
                height: 168.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: descriptor.default_data.clone(),
        };

        let layout =
            projected_node_surface_component_layout(&descriptor, &node, node.size.unwrap());

        assert_eq!(
            layout.measurement_mode,
            OpenGpuiMeasurementMode::ProjectionFallback
        );
        assert!(layout.slot_rect("field.prompt").is_some());
        assert!(layout.anchor_rect("field.completion").is_some());
    }

    #[test]
    fn projection_measurement_builds_slots_and_port_anchors() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("builtin llm descriptor");
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(1)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.llm"), CanvasPoint::default()),
            )
            .expect("create builtin llm node");
        let node_id = outcome.node_id();
        let node = store.graph().nodes().get(&node_id).expect("created node");

        let measurement = project_node_measurement(&node_id, node, store.graph(), &descriptor);

        assert!(
            measurement
                .slots
                .iter()
                .any(|slot| slot.key == "field.prompt")
        );
        assert!(measurement.anchors.iter().any(|anchor| {
            anchor.anchor == "field.completion"
                && anchor.port_key == Some(PortKey::new("completion"))
        }));
    }
}
