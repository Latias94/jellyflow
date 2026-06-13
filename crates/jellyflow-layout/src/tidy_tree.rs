use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jellyflow_core::{CanvasPoint, CanvasSize, Graph, GraphTransaction, NodeId};

use crate::engine::{
    LayoutContext, LayoutDirection, LayoutEngine, LayoutEngineId, LayoutError, LayoutNodePosition,
    LayoutOptions, LayoutRequest, LayoutResult, TIDY_TREE_LAYOUT_ENGINE_ID, position_from_center,
    resolve_node_origin, resolve_node_size, validate_request,
};
use crate::projection::{VisibleLayoutEdge, build_visible_edge_projection, result_from_placements};

/// Built-in tidy tree layout engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct TidyTreeLayoutEngine;

impl LayoutEngine for TidyTreeLayoutEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new(TIDY_TREE_LAYOUT_ENGINE_ID)
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        layout_graph_with_tidy_tree_context(graph, request, context)
    }
}

/// Runs the native tidy tree engine.
pub fn layout_graph_with_tidy_tree(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_tidy_tree_context(graph, request, &LayoutContext::default())
}

/// Runs the native tidy tree engine and converts the result into a transaction.
pub fn layout_graph_to_transaction_with_tidy_tree(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_tidy_tree(graph, request)?.to_transaction(graph)
}

fn layout_graph_with_tidy_tree_context(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    validate_request(graph, request)?;

    let mut projection = TidyTreeProjection::new(graph, request, context)?;
    projection.layout(request.options);
    projection.into_result()
}

struct TidyTreeProjection<'a> {
    graph: &'a Graph,
    request: &'a LayoutRequest,
    node_infos: BTreeMap<NodeId, TidyTreeNodeInfo>,
    visible_edges: Vec<VisibleLayoutEdge>,
    components: Vec<TidyTreeComponent>,
    placements: BTreeMap<NodeId, LayoutNodePosition>,
}

#[derive(Debug, Clone, Copy)]
struct TidyTreeNodeInfo {
    size: CanvasSize,
    origin: (f32, f32),
}

#[derive(Debug, Clone)]
struct TidyTreeComponent {
    root: NodeId,
    children: BTreeMap<NodeId, Vec<NodeId>>,
    depth: BTreeMap<NodeId, usize>,
}

#[derive(Debug, Clone, Copy)]
struct TreeCoordinates {
    main: f32,
    depth: usize,
}

impl<'a> TidyTreeProjection<'a> {
    fn new(
        graph: &'a Graph,
        request: &'a LayoutRequest,
        context: &'a LayoutContext,
    ) -> Result<Self, LayoutError> {
        let mut node_infos = BTreeMap::new();
        let mut visible_nodes = BTreeSet::new();

        for (id, node) in &graph.nodes {
            if node.hidden || !request.scope.contains(*id) {
                continue;
            }

            let size = resolve_node_size(graph, request, context, *id)?;
            let origin = resolve_node_origin(node.origin, request.options.node_origin, context);

            node_infos.insert(*id, TidyTreeNodeInfo { size, origin });
            visible_nodes.insert(*id);
        }

        let (outgoing, visible_edges) = build_visible_edge_projection(graph, &visible_nodes)?;
        let components = build_components(&visible_nodes, &outgoing);

        Ok(Self {
            graph,
            request,
            node_infos,
            visible_edges,
            components,
            placements: BTreeMap::new(),
        })
    }

    fn layout(&mut self, options: LayoutOptions) {
        if self.node_infos.is_empty() {
            return;
        }

        let profile = TidyTreeProfile::new(options, &self.node_infos);
        let components = self.components.clone();
        let mut component_offset = 0.0;

        for component in &components {
            let coordinates = compute_component_coordinates(component, &profile);
            let component_size = component_main_span(&coordinates, &profile);

            for (node, coords) in coordinates {
                let Some(info) = self.node_infos.get(&node).copied() else {
                    continue;
                };
                let center = center_from_tree_coordinates(
                    coords,
                    component_offset,
                    &profile,
                    options.direction,
                );
                let pos = position_from_center(center, info.size, info.origin);

                self.placements.insert(
                    node,
                    LayoutNodePosition {
                        node,
                        pos,
                        center,
                        size: info.size,
                    },
                );
            }

            component_offset += component_size + profile.component_gap;
        }
    }

    fn into_result(mut self) -> Result<LayoutResult, LayoutError> {
        Ok(result_from_placements(
            self.graph,
            self.request.options,
            &mut self.placements,
            &self.visible_edges,
        ))
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

fn build_components(
    visible_nodes: &BTreeSet<NodeId>,
    outgoing: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Vec<TidyTreeComponent> {
    let mut roots = visible_nodes
        .iter()
        .copied()
        .filter(|node| incoming_count(*node, outgoing) == 0)
        .collect::<Vec<_>>();
    let root_set = roots.iter().copied().collect::<BTreeSet<_>>();
    roots.extend(
        visible_nodes
            .iter()
            .copied()
            .filter(|node| !root_set.contains(node)),
    );

    let mut visited = BTreeSet::new();
    let mut components = Vec::new();

    for root in roots {
        if !visited.insert(root) {
            continue;
        }

        let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut depth: BTreeMap<NodeId, usize> = BTreeMap::new();
        let mut queue = VecDeque::new();

        queue.push_back(root);
        depth.insert(root, 0);

        while let Some(node) = queue.pop_front() {
            let child_depth = depth.get(&node).copied().unwrap_or(0) + 1;
            let next_nodes = outgoing.get(&node).cloned().unwrap_or_default();

            for child in next_nodes {
                if !visible_nodes.contains(&child) || !visited.insert(child) {
                    continue;
                }
                children.entry(node).or_default().push(child);
                depth.insert(child, child_depth);
                queue.push_back(child);
            }
        }

        components.push(TidyTreeComponent {
            root,
            children,
            depth,
        });
    }

    components
}

fn compute_component_coordinates(
    component: &TidyTreeComponent,
    profile: &TidyTreeProfile,
) -> BTreeMap<NodeId, TreeCoordinates> {
    let mut coordinates = BTreeMap::new();
    let mut next_leaf = 0.0;

    assign_subtree_main(
        component.root,
        component,
        profile,
        &mut next_leaf,
        &mut coordinates,
    );

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

fn assign_subtree_main(
    node: NodeId,
    component: &TidyTreeComponent,
    profile: &TidyTreeProfile,
    next_leaf: &mut f32,
    coordinates: &mut BTreeMap<NodeId, TreeCoordinates>,
) -> f32 {
    let depth = component.depth.get(&node).copied().unwrap_or(0);
    let Some(children) = component.children.get(&node) else {
        let main = *next_leaf;
        *next_leaf += profile.sibling_stride();
        coordinates.insert(node, TreeCoordinates { main, depth });
        return main;
    };

    if children.is_empty() {
        let main = *next_leaf;
        *next_leaf += profile.sibling_stride();
        coordinates.insert(node, TreeCoordinates { main, depth });
        return main;
    }

    let mut child_mains = Vec::with_capacity(children.len());
    for child in children {
        child_mains.push(assign_subtree_main(
            *child,
            component,
            profile,
            next_leaf,
            coordinates,
        ));
    }

    let first = child_mains.first().copied().unwrap_or(*next_leaf);
    let last = child_mains.last().copied().unwrap_or(first);
    let main = (first + last) * 0.5;
    coordinates.insert(node, TreeCoordinates { main, depth });
    main
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

fn incoming_count(node: NodeId, outgoing: &BTreeMap<NodeId, Vec<NodeId>>) -> usize {
    outgoing
        .values()
        .flat_map(|children| children.iter())
        .filter(|child| **child == node)
        .count()
}
