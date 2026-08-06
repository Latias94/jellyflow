use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{CanvasPoint, CanvasSize, NodeId};

use crate::engine::{
    LayoutCoordinateFrame, LayoutDirection, LayoutEngine, LayoutError, LayoutOptions,
    LayoutPlacements, PreparedLayoutEdge, PreparedLayoutInput,
};
use crate::projection::{SpanningForestComponent, build_spanning_forest, placements_from_centers};

/// Built-in tidy tree layout engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct TidyTreeLayoutEngine;

impl LayoutEngine for TidyTreeLayoutEngine {
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        run_tidy_tree_layout(input)
    }
}

fn run_tidy_tree_layout(input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
    let mut projection = TidyTreeProjection::new(input);
    projection.layout(input.options())?;
    projection.into_result()
}

struct TidyTreeProjection {
    node_infos: BTreeMap<NodeId, TidyTreeNodeInfo>,
    visible_edges: Vec<PreparedLayoutEdge>,
    components: Vec<SpanningForestComponent>,
    placements: BTreeMap<NodeId, CanvasPoint>,
}

#[derive(Debug, Clone, Copy)]
struct TidyTreeNodeInfo {
    size: CanvasSize,
}

#[derive(Debug, Clone, Copy)]
struct TreeCoordinates {
    main: f32,
    depth: usize,
}

impl TidyTreeProjection {
    fn new(input: &PreparedLayoutInput<'_>) -> Self {
        let mut node_infos = BTreeMap::new();
        let mut visible_nodes = BTreeSet::new();
        let mut outgoing = BTreeMap::new();

        for node in input.nodes() {
            node_infos.insert(node.id(), TidyTreeNodeInfo { size: node.size() });
            visible_nodes.insert(node.id());
            if !input.outgoing(node.id()).is_empty() {
                outgoing.insert(node.id(), input.outgoing(node.id()).to_vec());
            }
        }

        let components = build_spanning_forest(&visible_nodes, &outgoing);

        Self {
            node_infos,
            visible_edges: input.edges().to_vec(),
            components,
            placements: BTreeMap::new(),
        }
    }

    fn layout(&mut self, options: LayoutOptions) -> Result<(), LayoutError> {
        if self.node_infos.is_empty() {
            return Ok(());
        }

        let profile = TidyTreeProfile::new(options, &self.node_infos);
        let components = self.components.clone();
        let mut component_offset = 0.0;

        for component in &components {
            let coordinates = compute_component_coordinates(component, &profile);
            let component_size = component_main_span(&coordinates, &profile);

            for (node, coords) in coordinates {
                if !self.node_infos.contains_key(&node) {
                    continue;
                }
                let center = center_from_tree_coordinates(
                    coords,
                    component_offset,
                    &profile,
                    options.direction,
                );
                self.placements.insert(node, center);
            }

            component_offset += component_size + profile.component_gap;
        }

        Ok(())
    }

    fn into_result(self) -> Result<LayoutPlacements, LayoutError> {
        placements_from_centers(
            self.placements,
            &self.visible_edges,
            LayoutCoordinateFrame::NormalizeToMargin,
        )
    }
}

struct TidyTreeProfile {
    max_width: f32,
    max_height: f32,
    sibling_gap: f32,
    layer_gap: f32,
    component_gap: f32,
}

impl TidyTreeProfile {
    fn new(options: LayoutOptions, node_infos: &BTreeMap<NodeId, TidyTreeNodeInfo>) -> Self {
        let max_width = node_infos
            .values()
            .map(|info| info.size.width)
            .fold(0.0, f32::max);
        let max_height = node_infos
            .values()
            .map(|info| info.size.height)
            .fold(0.0, f32::max);
        let sibling_gap = options.spacing.nodesep.max(0.0);
        let layer_gap = options.spacing.ranksep.max(0.0);
        let component_gap = (options.spacing.edgesep.max(sibling_gap) + max_width).max(1.0);

        Self {
            max_width,
            max_height,
            sibling_gap,
            layer_gap,
            component_gap,
        }
    }

    fn sibling_stride(&self) -> f32 {
        (self.max_width + self.sibling_gap).max(1.0)
    }

    fn layer_stride(&self) -> f32 {
        (self.max_height + self.layer_gap).max(1.0)
    }
}

fn compute_component_coordinates(
    component: &SpanningForestComponent,
    profile: &TidyTreeProfile,
) -> BTreeMap<NodeId, TreeCoordinates> {
    let mut coordinates: BTreeMap<NodeId, TreeCoordinates> = BTreeMap::new();
    let mut next_leaf = 0.0;

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

        let depth = component.depth.get(&node).copied().unwrap_or(0);
        let main = match component.children.get(&node) {
            Some(children) if !children.is_empty() => {
                let first = children
                    .first()
                    .and_then(|child| coordinates.get(child))
                    .map(|coords| coords.main)
                    .unwrap_or(next_leaf);
                let last = children
                    .last()
                    .and_then(|child| coordinates.get(child))
                    .map(|coords| coords.main)
                    .unwrap_or(first);
                (first + last) * 0.5
            }
            _ => {
                let main = next_leaf;
                next_leaf += profile.sibling_stride();
                main
            }
        };
        coordinates.insert(node, TreeCoordinates { main, depth });
    }

    let min_main = coordinates
        .values()
        .map(|coords| coords.main)
        .fold(f32::INFINITY, f32::min);
    if min_main.is_finite() && min_main != 0.0 {
        for coords in coordinates.values_mut() {
            coords.main -= min_main;
        }
    }

    coordinates
}

fn component_main_span(
    coordinates: &BTreeMap<NodeId, TreeCoordinates>,
    profile: &TidyTreeProfile,
) -> f32 {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    for coords in coordinates.values() {
        min = min.min(coords.main);
        max = max.max(coords.main);
    }

    if min.is_finite() && max.is_finite() {
        (max - min + profile.max_width).max(profile.max_width)
    } else {
        profile.max_width
    }
}

fn center_from_tree_coordinates(
    coords: TreeCoordinates,
    component_offset: f32,
    profile: &TidyTreeProfile,
    direction: LayoutDirection,
) -> CanvasPoint {
    let main = component_offset + coords.main + profile.max_width * 0.5;
    let cross = coords.depth as f32 * profile.layer_stride() + profile.max_height * 0.5;

    match direction {
        LayoutDirection::TopToBottom => CanvasPoint { x: main, y: cross },
        LayoutDirection::BottomToTop => CanvasPoint { x: main, y: -cross },
        LayoutDirection::LeftToRight => CanvasPoint { x: cross, y: main },
        LayoutDirection::RightToLeft => CanvasPoint { x: -cross, y: main },
    }
}
