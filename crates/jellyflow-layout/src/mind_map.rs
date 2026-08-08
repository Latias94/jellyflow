use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::{PI, TAU};

use jellyflow_core::{CanvasPoint, CanvasSize, NodeId};

use crate::engine::{
    LayoutCoordinateFrame, LayoutDirection, LayoutEngine, LayoutError, LayoutOptions,
    LayoutPlacements, PreparedLayoutEdge, PreparedLayoutInput,
};
use crate::projection::{SpanningForestComponent, build_spanning_forest, placements_from_centers};

/// Built-in radial mind-map engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct MindMapRadialLayoutEngine;

impl LayoutEngine for MindMapRadialLayoutEngine {
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        run_mind_map_radial_layout(input)
    }
}

fn run_mind_map_radial_layout(
    input: &PreparedLayoutInput<'_>,
) -> Result<LayoutPlacements, LayoutError> {
    let mut projection = MindMapProjection::new(input);
    projection.layout(input.options())?;
    projection.into_result()
}

struct MindMapProjection {
    node_infos: BTreeMap<NodeId, MindMapNodeInfo>,
    visible_edges: Vec<PreparedLayoutEdge>,
    components: Vec<MindMapComponent>,
    placements: BTreeMap<NodeId, CanvasPoint>,
    preserve_canvas_coordinates: bool,
}

#[derive(Debug, Clone, Copy)]
struct MindMapNodeInfo {
    size: CanvasSize,
    current_center: CanvasPoint,
    arc_demand: f32,
    pinned: bool,
}

#[derive(Debug, Clone)]
struct MindMapComponent {
    root: NodeId,
    children: BTreeMap<NodeId, Vec<NodeId>>,
    demand: BTreeMap<NodeId, f32>,
    max_depth: usize,
}

struct MindMapLayoutNode {
    node: NodeId,
    center: CanvasPoint,
    start_angle: f32,
    end_angle: f32,
    depth: usize,
}

impl MindMapProjection {
    fn new(input: &PreparedLayoutInput<'_>) -> Self {
        let mut node_infos = BTreeMap::new();
        let mut visible_nodes = BTreeSet::new();
        let mut outgoing = BTreeMap::new();

        for node in input.nodes() {
            let arc_demand = node_arc_demand(node.size(), input.options());

            node_infos.insert(
                node.id(),
                MindMapNodeInfo {
                    size: node.size(),
                    current_center: node.current_center(),
                    arc_demand,
                    pinned: node.is_pinned(),
                },
            );
            visible_nodes.insert(node.id());
            if !input.outgoing(node.id()).is_empty() {
                outgoing.insert(node.id(), input.outgoing(node.id()).to_vec());
            }
        }

        let components = build_components(&visible_nodes, &outgoing, &node_infos);

        Self {
            node_infos,
            visible_edges: input.edges().to_vec(),
            components,
            placements: BTreeMap::new(),
            preserve_canvas_coordinates: input.has_pinned_nodes(),
        }
    }

    fn layout(&mut self, options: LayoutOptions) -> Result<(), LayoutError> {
        if self.node_infos.is_empty() {
            return Ok(());
        }

        let ring_gap = compute_ring_gap(options, &self.node_infos);
        let component_radius =
            compute_component_radius(ring_gap, &self.components, &self.node_infos);
        let angle_offset = direction_angle_offset(options.direction);
        let components = self.components.clone();
        let component_count = components.len().max(1) as f32;

        for (index, component) in components.iter().enumerate() {
            let root_center = if self
                .node_infos
                .get(&component.root)
                .is_some_and(|info| info.pinned)
            {
                self.node_infos
                    .get(&component.root)
                    .map(|info| info.current_center)
                    .unwrap_or_default()
            } else if self.components.len() == 1 {
                CanvasPoint { x: 0.0, y: 0.0 }
            } else {
                polar_point(
                    CanvasPoint { x: 0.0, y: 0.0 },
                    component_radius,
                    angle_offset + TAU * (index as f32 / component_count),
                )
            };

            self.layout_component(
                component,
                ring_gap,
                MindMapLayoutNode {
                    node: component.root,
                    center: root_center,
                    start_angle: angle_offset,
                    end_angle: angle_offset + TAU,
                    depth: 0,
                },
            )?;
        }

        Ok(())
    }

    fn layout_component(
        &mut self,
        component: &MindMapComponent,
        ring_gap: f32,
        root: MindMapLayoutNode,
    ) -> Result<(), LayoutError> {
        let mut stack = vec![root];

        while let Some(frame) = stack.pop() {
            let MindMapLayoutNode {
                node,
                center,
                start_angle,
                end_angle,
                depth,
            } = frame;
            let Some(info) = self.node_infos.get(&node).copied() else {
                continue;
            };
            let final_center = if info.pinned {
                info.current_center
            } else {
                center
            };
            self.placements.insert(node, final_center);

            let Some(children) = component.children.get(&node) else {
                continue;
            };
            if children.is_empty() {
                continue;
            }

            let total_demand = children
                .iter()
                .map(|child| component.demand.get(child).copied().unwrap_or(1.0))
                .sum::<f32>()
                .max(1.0);
            let sector_span = end_angle - start_angle;
            let mut cursor = start_angle;
            let mut child_frames = Vec::with_capacity(children.len());

            for child in children {
                let child_demand = component.demand.get(child).copied().unwrap_or(1.0);
                let child_span = sector_span * (child_demand / total_demand);
                let child_angle = cursor + child_span * 0.5;
                let child_center =
                    polar_point(final_center, ring_gap * (depth as f32 + 1.0), child_angle);
                child_frames.push(MindMapLayoutNode {
                    node: *child,
                    center: child_center,
                    start_angle: cursor,
                    end_angle: cursor + child_span,
                    depth: depth + 1,
                });
                cursor += child_span;
            }

            stack.extend(child_frames.into_iter().rev());
        }

        Ok(())
    }

    fn into_result(self) -> Result<LayoutPlacements, LayoutError> {
        let coordinate_frame = if self.preserve_canvas_coordinates {
            LayoutCoordinateFrame::PreserveCanvasCoordinates
        } else {
            LayoutCoordinateFrame::NormalizeToMargin
        };
        placements_from_centers(self.placements, &self.visible_edges, coordinate_frame)
    }
}

fn build_components(
    visible_nodes: &BTreeSet<NodeId>,
    outgoing: &BTreeMap<NodeId, Vec<NodeId>>,
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> Vec<MindMapComponent> {
    build_spanning_forest(visible_nodes, outgoing)
        .into_iter()
        .map(|component| {
            let max_depth = component.depth.values().copied().max().unwrap_or(0);
            let demand = compute_subtree_demand(&component, node_infos);

            MindMapComponent {
                root: component.root,
                children: component.children,
                demand,
                max_depth,
            }
        })
        .collect()
}

fn compute_subtree_demand(
    component: &SpanningForestComponent,
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> BTreeMap<NodeId, f32> {
    let mut demand = BTreeMap::new();
    let mut stack = vec![(component.root, false)];

    while let Some((node, expanded)) = stack.pop() {
        if !expanded {
            stack.push((node, true));
            if let Some(children) = component.children.get(&node) {
                for child in children.iter().rev() {
                    stack.push((*child, false));
                }
            }
            continue;
        }

        let mut total = node_infos
            .get(&node)
            .map(|info| info.arc_demand)
            .unwrap_or(1.0);
        if let Some(children) = component.children.get(&node) {
            for child in children {
                total += demand.get(child).copied().unwrap_or(1.0);
            }
        }
        demand.insert(node, total);
    }

    demand
}

fn compute_ring_gap(options: LayoutOptions, node_infos: &BTreeMap<NodeId, MindMapNodeInfo>) -> f32 {
    let widest = node_infos
        .values()
        .map(|info| info.size.width.max(info.size.height))
        .fold(0.0, f32::max);
    let spacing = options.spacing;
    (widest * 1.25 + spacing.ranksep.max(spacing.nodesep.max(24.0))).max(80.0)
}

fn compute_component_radius(
    ring_gap: f32,
    components: &[MindMapComponent],
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> f32 {
    if components.len() <= 1 {
        return 0.0;
    }

    let max_depth = components
        .iter()
        .map(|component| component.max_depth)
        .max()
        .unwrap_or(0);
    let widest = node_infos
        .values()
        .map(|info| info.size.width.max(info.size.height))
        .fold(0.0, f32::max);

    ring_gap * (max_depth as f32 + 2.0) * 2.0 + widest.max(ring_gap)
}

fn node_arc_demand(size: CanvasSize, options: LayoutOptions) -> f32 {
    let baseline = options.spacing.nodesep.max(24.0);
    let arc = size.width.max(size.height) + baseline;
    (arc / options.spacing.ranksep.max(1.0)).max(1.0)
}

fn direction_angle_offset(direction: LayoutDirection) -> f32 {
    match direction {
        LayoutDirection::TopToBottom => -PI / 2.0,
        LayoutDirection::BottomToTop => PI / 2.0,
        LayoutDirection::LeftToRight => 0.0,
        LayoutDirection::RightToLeft => PI,
    }
}

fn polar_point(center: CanvasPoint, radius: f32, angle: f32) -> CanvasPoint {
    CanvasPoint {
        x: center.x + radius * angle.cos(),
        y: center.y + radius * angle.sin(),
    }
}
