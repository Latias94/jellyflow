use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::io::NodeGraphSpatialIndexTuning;
use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::measurement::LayoutFactsQueryResult;
use crate::runtime::rendering::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use crate::runtime::rendering::query::RenderingQueryResult;
use crate::runtime::rendering::visibility::{
    all_non_hidden_edge_ids, all_non_hidden_node_ids, resolve_visible_edge_order_from_ids,
    resolve_visible_node_order_from_ids,
};
use crate::runtime::utils::get_node_rect;
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, GroupId, NodeId};

use super::backend::{NodeGraphQuerySnapshot, QueryBackend, QueryBackendKind};
use super::bindings::resolve_binding_read_model;
use super::layout_facts::resolve_layout_facts_read_model;

/// Store-cached spatial query backend.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SpatialQueryBackend;

/// Cached spatial read-model data derived from the current store snapshot.
#[derive(Debug, Default)]
pub(crate) struct SpatialQueryCache {
    node_index: Option<CachedSpatialNodeIndex>,
    #[cfg(test)]
    node_index_build_count: u64,
}

impl SpatialQueryCache {
    fn node_index(
        &mut self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        transform: ViewportTransform,
        tuning: NodeGraphSpatialIndexTuning,
    ) -> &SpatialNodeIndex {
        let key = SpatialNodeIndexCacheKey::new(snapshot, transform, tuning);
        let needs_rebuild = self
            .node_index
            .as_ref()
            .is_none_or(|cached| cached.key != key);

        if needs_rebuild {
            self.node_index = Some(CachedSpatialNodeIndex {
                key,
                index: SpatialNodeIndex::build(snapshot, transform, tuning),
            });
            #[cfg(test)]
            {
                self.node_index_build_count = self.node_index_build_count.saturating_add(1);
            }
        }

        &self
            .node_index
            .as_ref()
            .expect("node index exists after cache lookup")
            .index
    }

    #[cfg(test)]
    pub(crate) fn node_index_build_count(&self) -> u64 {
        self.node_index_build_count
    }
}

#[derive(Debug)]
struct CachedSpatialNodeIndex {
    key: SpatialNodeIndexCacheKey,
    index: SpatialNodeIndex,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpatialNodeIndexCacheKey {
    graph_revision: u64,
    layout_facts_revision: u64,
    node_origin: (f32, f32),
    cell_size: f32,
    tuning: NodeGraphSpatialIndexTuning,
}

impl SpatialNodeIndexCacheKey {
    fn new(
        snapshot: &NodeGraphQuerySnapshot<'_>,
        transform: ViewportTransform,
        tuning: NodeGraphSpatialIndexTuning,
    ) -> Self {
        Self {
            graph_revision: snapshot.graph_revision,
            layout_facts_revision: snapshot.layout_facts_revision,
            node_origin: snapshot.node_origin(),
            cell_size: spatial_cell_size(transform, tuning),
            tuning,
        }
    }
}

impl QueryBackend for SpatialQueryBackend {
    fn kind(&self) -> QueryBackendKind {
        QueryBackendKind::Spatial
    }

    fn rendering_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> RenderingQueryResult {
        resolve_spatial_rendering_query(snapshot, viewport_size)
    }

    fn layout_facts_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> LayoutFactsQueryResult {
        resolve_layout_facts_read_model(snapshot, viewport_size)
    }

    fn binding_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        options: BindingQueryOptions,
    ) -> BindingQueryResult {
        resolve_binding_read_model(snapshot, options)
    }
}

fn resolve_spatial_rendering_query(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> RenderingQueryResult {
    let group_order = resolve_group_render_order(
        snapshot.graph,
        snapshot.view_state,
        GroupRenderOrderOptions::from_interaction(&snapshot.interaction),
    );
    let node_order = resolve_node_render_order(
        snapshot.graph,
        snapshot.view_state,
        NodeRenderOrderOptions::from_interaction(&snapshot.interaction),
    );
    let edge_order = resolve_edge_render_order(
        snapshot.graph,
        snapshot.view_state,
        EdgeRenderOrderOptions::from_interaction(&snapshot.interaction),
    );
    let rendering = snapshot.interaction.rendering_interaction();
    let Some(transform) = ViewportTransform::from_view_state(snapshot.view_state) else {
        return empty_visibility_rendering_query(group_order, node_order, edge_order);
    };
    let Some(viewport) = spatial_viewport(transform, viewport_size) else {
        return empty_visibility_rendering_query(group_order, node_order, edge_order);
    };
    let (
        (visible_node_ids, visible_node_render_order),
        (visible_edge_ids, visible_edge_render_order),
    ) = if !rendering.only_render_visible_elements {
        (
            resolve_all_non_hidden_visible_nodes(snapshot, &node_order),
            resolve_all_non_hidden_visible_edges(snapshot, &edge_order),
        )
    } else {
        let mut cache = snapshot.spatial_cache.borrow_mut();
        let index = cache.node_index(snapshot, transform, rendering.spatial_index);
        (
            resolve_spatial_visible_nodes(index, viewport, &node_order),
            resolve_spatial_visible_edges(snapshot, index, viewport, &edge_order),
        )
    };

    RenderingQueryResult {
        group_order,
        node_order,
        edge_order,
        visible_node_ids,
        visible_node_render_order,
        visible_edge_ids,
        visible_edge_render_order,
    }
}

fn empty_visibility_rendering_query(
    group_order: Vec<GroupId>,
    node_order: Vec<NodeId>,
    edge_order: Vec<EdgeId>,
) -> RenderingQueryResult {
    RenderingQueryResult {
        group_order,
        node_order,
        edge_order,
        visible_node_ids: Vec::new(),
        visible_node_render_order: Vec::new(),
        visible_edge_ids: Vec::new(),
        visible_edge_render_order: Vec::new(),
    }
}

fn resolve_spatial_visible_nodes(
    index: &SpatialNodeIndex,
    viewport: SpatialViewport,
    node_order: &[NodeId],
) -> (Vec<NodeId>, Vec<NodeId>) {
    resolve_visible_node_order_from_ids(index.nodes_intersecting(viewport), node_order)
}

fn resolve_spatial_visible_edges(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    index: &SpatialNodeIndex,
    viewport: SpatialViewport,
    edge_order: &[EdgeId],
) -> (Vec<EdgeId>, Vec<EdgeId>) {
    let viewport_bounds = viewport.bounds;
    let mut visible_edge_ids = snapshot
        .graph
        .edges
        .iter()
        .filter_map(|(id, edge)| {
            if edge.hidden {
                return None;
            }
            let lookup = snapshot.lookups.edge_lookup.get(id)?;
            let source = index.node_bounds(lookup.from_node)?;
            let target = index.node_bounds(lookup.to_node)?;
            viewport_bounds
                .intersects(source.union(target))
                .then_some(*id)
        })
        .collect::<Vec<_>>();
    visible_edge_ids.sort();
    resolve_visible_edge_order_from_ids(visible_edge_ids, edge_order)
}

fn resolve_all_non_hidden_visible_nodes(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    node_order: &[NodeId],
) -> (Vec<NodeId>, Vec<NodeId>) {
    resolve_visible_node_order_from_ids(all_non_hidden_node_ids(snapshot.lookups), node_order)
}

fn resolve_all_non_hidden_visible_edges(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    edge_order: &[EdgeId],
) -> (Vec<EdgeId>, Vec<EdgeId>) {
    resolve_visible_edge_order_from_ids(all_non_hidden_edge_ids(snapshot.graph), edge_order)
}

fn spatial_viewport(
    transform: ViewportTransform,
    viewport_size: CanvasSize,
) -> Option<SpatialViewport> {
    if !transform.is_valid() || !viewport_size.is_positive_finite() {
        return None;
    }
    let origin = transform.canvas_point_at_screen(CanvasPoint::default());
    let far_corner = transform.canvas_point_at_screen(CanvasPoint {
        x: viewport_size.width,
        y: viewport_size.height,
    });
    let rect = CanvasRect {
        origin,
        size: CanvasSize {
            width: far_corner.x - origin.x,
            height: far_corner.y - origin.y,
        },
    };
    let bounds = CanvasBounds::from_rect(rect)?;
    Some(SpatialViewport { bounds })
}

#[derive(Debug, Clone, Copy)]
struct SpatialViewport {
    bounds: CanvasBounds,
}

#[derive(Debug)]
struct SpatialNodeIndex {
    cell_size: f32,
    cells: HashMap<GridCell, Vec<NodeId>>,
    nodes: BTreeMap<NodeId, CanvasBounds>,
}

impl SpatialNodeIndex {
    fn build(
        snapshot: &NodeGraphQuerySnapshot<'_>,
        transform: ViewportTransform,
        tuning: NodeGraphSpatialIndexTuning,
    ) -> Self {
        let cell_size = spatial_cell_size(transform, tuning);
        let mut index = Self {
            cell_size,
            cells: HashMap::new(),
            nodes: BTreeMap::new(),
        };

        for (node, entry) in &snapshot.lookups.node_lookup {
            if !entry.is_visible_with_hidden_policy(false) {
                continue;
            }
            let Some(rect) = get_node_rect(snapshot.lookups, *node, snapshot.node_origin(), None)
            else {
                continue;
            };
            let Some(bounds) = CanvasBounds::from_rect(rect) else {
                continue;
            };
            if !bounds.is_valid() {
                continue;
            }
            index.nodes.insert(*node, bounds);
            for cell in covered_cells(bounds, cell_size) {
                index.cells.entry(cell).or_default().push(*node);
            }
        }

        index
    }

    fn node_bounds(&self, node: NodeId) -> Option<CanvasBounds> {
        self.nodes.get(&node).copied()
    }

    fn nodes_intersecting(&self, viewport: SpatialViewport) -> Vec<NodeId> {
        let mut out = BTreeSet::new();
        for cell in covered_cells(viewport.bounds, self.cell_size) {
            let Some(nodes) = self.cells.get(&cell) else {
                continue;
            };
            for node in nodes {
                let Some(bounds) = self.node_bounds(*node) else {
                    continue;
                };
                if viewport.bounds.intersects(bounds) {
                    out.insert(*node);
                }
            }
        }

        out.into_iter().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GridCell {
    x: i32,
    y: i32,
}

fn covered_cells(bounds: CanvasBounds, cell_size: f32) -> impl Iterator<Item = GridCell> {
    let rect = bounds.to_rect();
    let min_x = cell_index(rect.origin.x, cell_size);
    let min_y = cell_index(rect.origin.y, cell_size);
    let max_x = cell_index(rect.origin.x + rect.size.width, cell_size);
    let max_y = cell_index(rect.origin.y + rect.size.height, cell_size);
    (min_x..=max_x).flat_map(move |x| (min_y..=max_y).map(move |y| GridCell { x, y }))
}

fn cell_index(value: f32, cell_size: f32) -> i32 {
    (value / cell_size).floor() as i32
}

fn spatial_cell_size(transform: ViewportTransform, tuning: NodeGraphSpatialIndexTuning) -> f32 {
    let preferred = tuning.cell_size_screen_px / transform.zoom;
    let min = tuning.min_cell_size_screen_px / transform.zoom;
    quantize_spatial_cell_size(preferred.max(min).max(1.0))
}

fn quantize_spatial_cell_size(cell_size: f32) -> f32 {
    if !cell_size.is_finite() {
        return 1.0;
    }
    // Quantizing only changes candidate buckets; exact bounds checks still decide visibility.
    cell_size.max(1.0).log2().ceil().exp2()
}
