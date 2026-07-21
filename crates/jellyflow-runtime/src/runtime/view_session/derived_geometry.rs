//! Atomic ownership of all view-local derived geometry.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, EdgeId, Graph, Node, NodeId, PortId};

use crate::runtime::adapter_protocol::{
    FrameBoundsSource, FrameRequest, GeometrySnapshot, LayoutPublicationReceipt,
    PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry, PublishedPortOutcome,
    RendererStamp, SharedGeometryMap, ViewPoint,
};
use crate::runtime::geometry::{
    EdgeEndpointPosition, EdgeInteractionFacts, EdgePosition, resolve_edge_route_path,
};
use crate::runtime::graph_runtime::{
    GraphRevision, GraphRuntimePolicy, GraphSnapshot, IncidentEdgeSnapshot, IncidentSnapshotWork,
    RuntimePatch, RuntimePatchKind,
};
use crate::runtime::interaction::ConnectionGestureIntent;
use crate::runtime::policy::resolve_edge_policy;
use crate::runtime::viewport::ViewportTransform;

use super::edge_pick::{EdgePickCursor, InternalEdgePickOutcome, record_edge_pick_stats};
use super::geometry::{AdmissionIndex, SpatialQuery, ViewSpatialIndex};
use super::port_geometry::{
    ConnectionProbeCursor, InternalProbeOutcome, PortGeometryIndex, record_probe_query_stats,
};
use super::route_geometry::{
    InternalRouteQueryOutcome, RouteGeometryIndex, RouteGeometryRevision, VisibleRouteCursor,
    record_route_query_stats,
};
use super::{ViewGeneration, ViewId, ViewSessionStats};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RouteRecomputation {
    pub(super) attempts: u64,
    pub(super) changed: bool,
}

#[derive(Debug)]
pub(super) struct LayoutWithdrawal {
    node: NodeId,
    ports: BTreeSet<PortId>,
    incident_edges: IncidentEdgeSnapshot,
    current_incident_edges: Option<IncidentEdgeSnapshot>,
}

impl LayoutWithdrawal {
    pub(super) fn new(
        node: NodeId,
        ports: BTreeSet<PortId>,
        incident_edges: IncidentEdgeSnapshot,
        current_incident_edges: Option<IncidentEdgeSnapshot>,
    ) -> Self {
        Self {
            node,
            ports,
            incident_edges,
            current_incident_edges,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct DerivedGeometry {
    layout_revision: u64,
    route_facts_revision: u128,
    layout_generations: BTreeMap<NodeId, u64>,
    binding_revisions: BTreeMap<NodeId, GraphRevision>,
    renderer_bindings: BTreeMap<NodeId, RendererStamp>,
    nodes: SharedGeometryMap<NodeId, PublishedNodeGeometry>,
    ports: SharedGeometryMap<PortId, PublishedPortGeometry>,
    port_outcomes: SharedGeometryMap<PortId, PublishedPortOutcome>,
    port_index: PortGeometryIndex,
    node_spatial: ViewSpatialIndex,
    admission: AdmissionIndex,
    active_frame_nodes: BTreeSet<NodeId>,
    routes: SharedGeometryMap<EdgeId, PublishedEdgeRoute>,
    route_index: RouteGeometryIndex,
    unresolved_routes: BTreeSet<EdgeId>,
    snapshot_cache: Option<GeometrySnapshot>,
    stats: ViewSessionStats,
}

impl DerivedGeometry {
    #[cfg(test)]
    pub(super) fn layout_revision(&self) -> u64 {
        self.layout_revision
    }

    pub(super) fn route_facts_revision(&self) -> u128 {
        self.route_facts_revision
    }

    pub(super) fn layout_generation(&self, node: NodeId) -> u64 {
        self.layout_generations
            .get(&node)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn binding_revision(&self, node: NodeId) -> GraphRevision {
        self.binding_revisions
            .get(&node)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn renderer_matches(&self, node: NodeId, renderer: &RendererStamp) -> bool {
        self.renderer_bindings.get(&node) == Some(renderer)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_connection_probe_cursor(
        &self,
        view: ViewId,
        generation: ViewGeneration,
        transform_revision: u64,
        graph_revision: GraphRevision,
        order_revision: u128,
        intent: ConnectionGestureIntent,
        pointer: ViewPoint,
        radius_in_view: f32,
        transform: ViewportTransform,
    ) -> Option<ConnectionProbeCursor> {
        self.port_index.new_cursor(
            view,
            generation,
            transform_revision,
            graph_revision,
            order_revision,
            intent,
            pointer,
            radius_in_view,
            transform,
        )
    }

    pub(super) fn connection_probe_index_epoch(&self) -> u128 {
        self.port_index.index_epoch()
    }

    pub(super) fn query_connection_probe_page(
        &mut self,
        graph: &Graph,
        node_draw_order_ranks: &BTreeMap<NodeId, usize>,
        selected_nodes: &HashSet<NodeId>,
        cursor: ConnectionProbeCursor,
        entry_budget: usize,
    ) -> InternalProbeOutcome {
        let outcome = self.port_index.query_page(
            graph,
            node_draw_order_ranks,
            selected_nodes,
            cursor,
            entry_budget,
        );
        record_probe_query_stats(&mut self.stats, &outcome);
        outcome
    }

    pub(super) fn new_visible_route_cursor(
        &self,
        view: ViewId,
        generation: ViewGeneration,
        transform_revision: u64,
        bounds: CanvasRect,
    ) -> VisibleRouteCursor {
        self.route_index
            .new_cursor(view, generation, transform_revision, bounds)
    }

    pub(super) fn visible_route_index_epoch(&self) -> u128 {
        self.route_index.index_epoch()
    }

    pub(super) fn query_visible_route_page(
        &mut self,
        cursor: VisibleRouteCursor,
        entry_budget: usize,
        zoom: f32,
    ) -> InternalRouteQueryOutcome {
        let outcome = self.route_index.query_page(cursor, entry_budget, zoom);
        record_route_query_stats(&mut self.stats, &outcome);
        outcome
    }

    pub(super) fn route_geometry_revision(&self, edge: EdgeId) -> Option<RouteGeometryRevision> {
        self.route_index.geometry_revision(edge)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_edge_pick_cursor(
        &self,
        view: ViewId,
        generation: ViewGeneration,
        transform_revision: u64,
        order_revision: u128,
        pointer: ViewPoint,
        radius_in_view: f32,
        transform: ViewportTransform,
    ) -> Option<EdgePickCursor> {
        self.route_index.new_pick_cursor(
            view,
            generation,
            transform_revision,
            order_revision,
            pointer,
            radius_in_view,
            transform,
        )
    }

    pub(super) fn edge_pick_epochs(&self) -> (u128, u128) {
        (
            self.route_index.index_epoch(),
            self.route_index.pick_epoch(),
        )
    }

    pub(super) fn query_edge_pick_page(
        &mut self,
        edge_draw_order_ranks: &BTreeMap<EdgeId, usize>,
        selected_edges: &HashSet<EdgeId>,
        cursor: EdgePickCursor,
        entry_budget: usize,
    ) -> InternalEdgePickOutcome {
        let outcome = self.route_index.query_pick_page(
            edge_draw_order_ranks,
            selected_edges,
            cursor,
            entry_budget,
        );
        record_edge_pick_stats(&mut self.stats, &outcome);
        outcome
    }

    pub(super) fn node_ids(&self) -> BTreeSet<NodeId> {
        self.nodes.keys().copied().collect()
    }

    pub(super) fn node_approximately_equals(
        &self,
        candidate: &PublishedNodeGeometry,
        epsilon: f32,
    ) -> bool {
        self.nodes
            .get(&candidate.node())
            .is_some_and(|current| current.approximately_eq(candidate, epsilon))
    }

    pub(super) fn nodes_intersecting(&self, bounds: CanvasRect) -> SpatialQuery {
        self.node_spatial.nodes_intersecting(bounds)
    }

    pub(super) fn nodes_intersecting_with_limit(
        &self,
        bounds: CanvasRect,
        entry_limit: usize,
    ) -> SpatialQuery {
        self.node_spatial
            .nodes_intersecting_with_limit(bounds, entry_limit)
    }

    pub(super) fn nodes_near_alignment_with_limit(
        &self,
        moving_bounds: &[CanvasRect],
        tolerance: f32,
        entry_limit: usize,
    ) -> SpatialQuery {
        self.node_spatial
            .nodes_near_alignment_with_limit(moving_bounds, tolerance, entry_limit)
    }

    pub(super) fn prepare_admission(
        &mut self,
        graph: &Graph,
        request: &FrameRequest,
    ) -> (u64, bool) {
        self.admission.prepare(graph, request)
    }

    pub(super) fn admission_nodes_intersecting_with_limit(
        &self,
        bounds: CanvasRect,
        entry_limit: usize,
    ) -> SpatialQuery {
        self.admission
            .nodes_intersecting_with_limit(bounds, entry_limit)
    }

    pub(super) fn frame_bounds(
        &self,
        node_id: NodeId,
        node: &Node,
        renderer: &RendererStamp,
        request: &FrameRequest,
    ) -> Option<(CanvasRect, FrameBoundsSource)> {
        let generation = self.layout_generation(node_id);
        if let Some(published) = self.nodes.get(&node_id)
            && published.is_fresh()
            && published.renderer() == renderer
            && published.layout_generation() == generation
            && published.bounds().is_positive_finite()
        {
            return Some((published.bounds(), FrameBoundsSource::Published));
        }
        let (size, source) = if let Some(size) = node.size.filter(|size| size.is_positive_finite())
        {
            (size, FrameBoundsSource::Persisted)
        } else if let Some(size) = request.kind_default_sizes.get(&node.kind).copied() {
            (size, FrameBoundsSource::KindDefault)
        } else {
            (request.fallback_size, FrameBoundsSource::ProtocolFallback)
        };
        node.bounds_for_size(size).map(|bounds| (bounds, source))
    }

    pub(super) fn inactive_frame_nodes(&self, nodes: &BTreeSet<NodeId>) -> BTreeSet<NodeId> {
        self.active_frame_nodes
            .difference(nodes)
            .filter(|node| {
                self.nodes.get(node).is_some_and(|geometry| {
                    geometry
                        .port_outcomes()
                        .values()
                        .any(|outcome| !matches!(outcome, PublishedPortOutcome::Culled))
                })
            })
            .copied()
            .collect()
    }

    pub(super) fn snapshot(&mut self) -> GeometrySnapshot {
        if let Some(snapshot) = &self.snapshot_cache {
            self.assert_consistent();
            return snapshot.clone();
        }
        let snapshot = GeometrySnapshot::new(
            self.layout_revision,
            self.nodes.clone(),
            self.ports.clone(),
            self.port_outcomes.clone(),
            self.routes.clone(),
        );
        self.snapshot_cache = Some(snapshot.clone());
        self.stats.geometry_snapshot_builds = self.stats.geometry_snapshot_builds.saturating_add(1);
        self.assert_consistent();
        snapshot
    }

    pub(super) fn stats(&self) -> ViewSessionStats {
        self.stats
    }

    pub(super) fn record_geometry_impact_merge(
        &mut self,
        patches: u64,
        records: u64,
        fallbacks: u64,
    ) {
        self.stats.geometry_impact_patches_merged = self
            .stats
            .geometry_impact_patches_merged
            .saturating_add(patches);
        self.stats.geometry_impact_records_merged = self
            .stats
            .geometry_impact_records_merged
            .saturating_add(records);
        self.stats.geometry_impact_global_fallbacks = self
            .stats
            .geometry_impact_global_fallbacks
            .saturating_add(fallbacks);
    }

    pub(super) fn record_frame_admission(
        &mut self,
        nodes_visited: u64,
        cells_examined: u64,
        entries_examined: u64,
        rebuilt: bool,
        candidates_ranked: u64,
        selection_lookups: u64,
    ) {
        self.stats.admission_nodes_visited = self
            .stats
            .admission_nodes_visited
            .saturating_add(nodes_visited);
        self.stats.admission_query_cells_examined = self
            .stats
            .admission_query_cells_examined
            .saturating_add(cells_examined);
        self.stats.admission_query_entries_examined = self
            .stats
            .admission_query_entries_examined
            .saturating_add(entries_examined);
        self.stats.admission_index_rebuilds = self
            .stats
            .admission_index_rebuilds
            .saturating_add(u64::from(rebuilt));
        self.stats.frame_order_candidates_ranked = self
            .stats
            .frame_order_candidates_ranked
            .saturating_add(candidates_ranked);
        self.stats.frame_selection_membership_lookups = self
            .stats
            .frame_selection_membership_lookups
            .saturating_add(selection_lookups);
    }

    pub(super) fn record_frame_nodes_materialized(&mut self, count: u64) {
        self.stats.frame_nodes_materialized =
            self.stats.frame_nodes_materialized.saturating_add(count);
    }

    pub(super) fn record_layout_candidate_impact(&mut self, checked: u64, stale: u64) {
        self.stats.layout_impact_candidates_checked = self
            .stats
            .layout_impact_candidates_checked
            .saturating_add(checked);
        self.stats.layout_impact_candidates_stale = self
            .stats
            .layout_impact_candidates_stale
            .saturating_add(stale);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn publish_complete_layout(
        &mut self,
        snapshot: &GraphSnapshot,
        staged: Vec<(NodeId, PublishedNodeGeometry, IncidentEdgeSnapshot)>,
        withdrawals: Vec<LayoutWithdrawal>,
        impact_incident_edges: &BTreeSet<EdgeId>,
        request_refresh: bool,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) -> (Option<LayoutPublicationReceipt>, IncidentSnapshotWork) {
        let mut published_nodes = Vec::new();
        let mut withdrawn_nodes = Vec::new();
        let mut route_edges = BTreeSet::new();
        let mut spatial_updates = 0_u64;
        let mut incident_work = IncidentSnapshotWork::default();

        for (node, geometry, incident_edges) in staged {
            spatial_updates = spatial_updates
                .saturating_add(u64::from(self.node_spatial.upsert(node, geometry.bounds())));
            self.upsert_node(node, geometry);
            incident_work.materialize(&incident_edges);
            route_edges.extend(incident_edges.iter().copied());
            published_nodes.push(node);
        }

        let mut withdrew_geometry = false;
        for withdrawal in withdrawals {
            if !self.mark_node_stale(withdrawal.node, withdrawal.ports) {
                continue;
            }
            let mut affected_edges = withdrawal.incident_edges.as_ref().clone();
            if let Some(current) = withdrawal.current_incident_edges {
                affected_edges.extend(current.iter().copied());
            }
            incident_work.materialize(&affected_edges);
            route_edges.extend(affected_edges);
            withdrawn_nodes.push(withdrawal.node);
            withdrew_geometry = true;
        }
        if withdrew_geometry && !impact_incident_edges.is_empty() {
            incident_work.materialize(impact_incident_edges);
            route_edges.extend(impact_incident_edges.iter().copied());
        }
        if request_refresh {
            self.stats.refresh_requests = self.stats.refresh_requests.saturating_add(1);
        }
        if published_nodes.is_empty() && withdrawn_nodes.is_empty() {
            self.assert_consistent();
            return (None, incident_work);
        }

        let route_recomputation =
            self.recompute_routes(snapshot, &route_edges, policy, selected_edges);
        let before_revision = self.layout_revision;
        self.advance_layout_revision();
        self.stats.layout_publications = self.stats.layout_publications.saturating_add(1);
        self.stats.spatial_updates = self.stats.spatial_updates.saturating_add(spatial_updates);
        self.stats.route_invalidations = self
            .stats
            .route_invalidations
            .saturating_add(route_edges.len() as u64);
        self.stats.route_recomputations = self
            .stats
            .route_recomputations
            .saturating_add(route_recomputation.attempts);
        self.assert_consistent();
        (
            Some(LayoutPublicationReceipt::new(
                before_revision,
                self.layout_revision,
                published_nodes,
                withdrawn_nodes,
                spatial_updates,
                route_recomputation.attempts,
            )),
            incident_work,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reconcile_frame_geometry(
        &mut self,
        snapshot: &GraphSnapshot,
        renderers: &BTreeMap<NodeId, RendererStamp>,
        active_nodes: &BTreeSet<NodeId>,
        node_facts: &BTreeMap<
            NodeId,
            (
                crate::runtime::graph_runtime::NodeIncarnation,
                IncidentEdgeSnapshot,
            ),
        >,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) -> (
        Option<(u64, Vec<NodeId>)>,
        IncidentSnapshotWork,
        BTreeMap<NodeId, u64>,
    ) {
        let inactive_nodes = self.inactive_frame_nodes(active_nodes);
        let mut invalidated_nodes = Vec::new();
        let mut generations = BTreeMap::new();
        let mut route_edges = BTreeSet::new();
        let mut incident_work = IncidentSnapshotWork::default();
        let mut geometry_changed = false;
        let mut spatial_updates = 0_u64;

        for (node, renderer) in renderers {
            let renderer_changed = match self.renderer_bindings.get(node) {
                None => {
                    self.renderer_bindings.insert(*node, renderer.clone());
                    false
                }
                Some(current) if current != renderer => {
                    self.renderer_bindings.insert(*node, renderer.clone());
                    true
                }
                Some(_) => false,
            };
            if !renderer_changed {
                continue;
            }
            let generation = self.advance_node_generation(*node);
            generations.insert(*node, generation);
            let spatial_changed = self.node_spatial.remove(*node);
            geometry_changed |= self.remove_node(*node);
            spatial_updates = spatial_updates.saturating_add(u64::from(spatial_changed));
            if let Some((_, incident_edges)) = node_facts.get(node) {
                incident_work.materialize(incident_edges);
                route_edges.extend(incident_edges.iter().copied());
            }
            invalidated_nodes.push(*node);
        }

        for node in &inactive_nodes {
            let ports = snapshot
                .graph()
                .node(node)
                .map(|node| node.ports.iter().copied())
                .into_iter()
                .flatten();
            if !self.mark_node_culled(*node, ports) {
                continue;
            }
            geometry_changed = true;
            invalidated_nodes.push(*node);
            if let Some((_, incident_edges)) = node_facts.get(node) {
                incident_work.materialize(incident_edges);
                route_edges.extend(incident_edges.iter().copied());
            }
        }

        let route_recomputation =
            self.recompute_routes(snapshot, &route_edges, policy, selected_edges);
        let publication_changed = geometry_changed || route_recomputation.changed;
        if publication_changed {
            self.advance_layout_revision();
            self.stats.layout_publications = self.stats.layout_publications.saturating_add(1);
        }
        self.stats.node_invalidations = self
            .stats
            .node_invalidations
            .saturating_add(invalidated_nodes.len() as u64);
        self.stats.spatial_updates = self.stats.spatial_updates.saturating_add(spatial_updates);
        self.stats.route_invalidations = self
            .stats
            .route_invalidations
            .saturating_add(route_edges.len() as u64);
        self.stats.route_recomputations = self
            .stats
            .route_recomputations
            .saturating_add(route_recomputation.attempts);
        self.active_frame_nodes = active_nodes.clone();
        self.assert_consistent();
        (
            publication_changed.then_some((self.layout_revision, invalidated_nodes)),
            incident_work,
            generations,
        )
    }

    pub(super) fn apply_runtime_patch(
        &mut self,
        patch: &RuntimePatch,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) {
        let document_reset = patch.kind() == RuntimePatchKind::DocumentReset;
        if document_reset {
            self.reset();
        }

        let admission_nodes_visited = self.admission.update_nodes(
            patch.snapshot().graph(),
            patch.affected_nodes().iter().copied(),
        );
        self.stats.admission_nodes_visited = self
            .stats
            .admission_nodes_visited
            .saturating_add(admission_nodes_visited);
        let impact = patch.impact();
        let mut spatially_updated = BTreeSet::new();
        for node in &impact.retired_nodes {
            if self.remove_node(*node) {
                spatially_updated.insert(*node);
            }
            self.layout_generations.remove(node);
            self.binding_revisions.remove(node);
            self.renderer_bindings.remove(node);
        }
        for node in patch.affected_nodes() {
            if patch.snapshot().graph().node(node).is_some() {
                self.binding_revisions.insert(*node, patch.to_revision());
            } else {
                self.binding_revisions.remove(node);
            }
        }
        for node in &impact.geometry_withdrawals {
            if self.remove_node(*node) {
                spatially_updated.insert(*node);
            }
        }
        for node in patch.affected_nodes() {
            let Some(Some(incarnation)) = patch.node_incarnation_change(*node) else {
                continue;
            };
            if self
                .nodes
                .get(node)
                .is_some_and(|geometry| geometry.incarnation() != incarnation)
                && self.remove_node(*node)
            {
                spatially_updated.insert(*node);
            }
        }
        for translation in &impact.translated_nodes {
            if self.translate_node(translation.node, translation.delta.x, translation.delta.y) {
                spatially_updated.insert(translation.node);
            }
        }
        for node in &impact.layout_dirty_nodes {
            self.advance_node_generation(*node);
            let ports = patch
                .snapshot()
                .graph()
                .node(node)
                .map(|node| node.ports.iter().copied())
                .into_iter()
                .flatten();
            self.mark_node_stale(*node, ports);
        }
        let route_edges = impact.route_edges.iter().copied().collect::<BTreeSet<_>>();
        let recomputation =
            self.recompute_routes(patch.snapshot(), &route_edges, policy, selected_edges);
        self.stats.node_invalidations = self
            .stats
            .node_invalidations
            .saturating_add(impact.layout_dirty_nodes.len() as u64);
        self.stats.spatial_updates = self
            .stats
            .spatial_updates
            .saturating_add(spatially_updated.len() as u64);
        self.stats.route_invalidations = self
            .stats
            .route_invalidations
            .saturating_add(impact.route_edges.len() as u64);
        self.stats.route_recomputations = self
            .stats
            .route_recomputations
            .saturating_add(recomputation.attempts);
        if document_reset
            || !spatially_updated.is_empty()
            || !impact.layout_dirty_nodes.is_empty()
            || recomputation.changed
        {
            self.advance_layout_revision();
        }
        self.assert_consistent();
    }

    pub(super) fn update_route_selection(
        &mut self,
        edges: &BTreeSet<EdgeId>,
        selected_edges: &HashSet<EdgeId>,
    ) {
        for edge in edges {
            let Some(route) = self.routes.get(edge).map(|route| route.as_ref().clone()) else {
                continue;
            };
            self.upsert_route(*edge, route.with_selected(selected_edges.contains(edge)));
        }
        self.assert_consistent();
    }

    pub(super) fn mark_all_stale(
        &mut self,
        snapshot: &GraphSnapshot,
        ports_by_node: &BTreeMap<NodeId, Vec<PortId>>,
        route_edges: &BTreeSet<EdgeId>,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) -> Option<u64> {
        for node in ports_by_node.keys() {
            self.advance_node_generation(*node);
        }
        let stale_nodes = ports_by_node
            .iter()
            .filter(|(node, ports)| self.mark_node_stale(**node, ports.iter().copied()))
            .count();
        let recomputation = self.recompute_routes(snapshot, route_edges, policy, selected_edges);
        let changed = stale_nodes > 0 || recomputation.changed;
        if changed {
            self.advance_layout_revision();
            self.stats.layout_publications = self.stats.layout_publications.saturating_add(1);
        }
        self.stats.node_invalidations = self
            .stats
            .node_invalidations
            .saturating_add(ports_by_node.len() as u64);
        self.stats.route_invalidations = self
            .stats
            .route_invalidations
            .saturating_add(route_edges.len() as u64);
        self.stats.route_recomputations = self
            .stats
            .route_recomputations
            .saturating_add(recomputation.attempts);
        self.assert_consistent();
        changed.then_some(self.layout_revision)
    }

    pub(super) fn mark_one_stale(
        &mut self,
        snapshot: &GraphSnapshot,
        node: NodeId,
        ports: impl IntoIterator<Item = PortId>,
        route_edges: &BTreeSet<EdgeId>,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) -> Option<u64> {
        self.advance_node_generation(node);
        let geometry_changed = self.mark_node_stale(node, ports);
        let recomputation = self.recompute_routes(snapshot, route_edges, policy, selected_edges);
        let changed = geometry_changed || recomputation.changed;
        if changed {
            self.advance_layout_revision();
            self.stats.layout_publications = self.stats.layout_publications.saturating_add(1);
        }
        self.stats.node_invalidations = self.stats.node_invalidations.saturating_add(1);
        self.stats.route_invalidations = self
            .stats
            .route_invalidations
            .saturating_add(route_edges.len() as u64);
        self.stats.route_recomputations = self
            .stats
            .route_recomputations
            .saturating_add(recomputation.attempts);
        self.assert_consistent();
        changed.then_some(self.layout_revision)
    }

    pub(super) fn reset(&mut self) {
        self.clear_nodes();
        self.node_spatial.clear();
        self.admission.invalidate();
        self.active_frame_nodes.clear();
        self.layout_generations.clear();
        self.binding_revisions.clear();
        self.renderer_bindings.clear();
        self.clear_routes();
        self.unresolved_routes.clear();
        self.snapshot_cache = None;
        self.assert_consistent();
    }

    fn advance_node_generation(&mut self, node: NodeId) -> u64 {
        let generation = self.layout_generations.entry(node).or_default();
        *generation = generation.saturating_add(1);
        *generation
    }

    fn advance_layout_revision(&mut self) {
        self.snapshot_cache = None;
        self.layout_revision = self.layout_revision.saturating_add(1);
    }

    fn record_snapshot_updates(&mut self, count: usize) {
        self.snapshot_cache = None;
        self.stats.geometry_snapshot_fact_updates = self
            .stats
            .geometry_snapshot_fact_updates
            .saturating_add(count as u64);
    }

    fn upsert_node(&mut self, node: NodeId, geometry: PublishedNodeGeometry) -> bool {
        debug_assert_eq!(node, geometry.node());
        if self
            .nodes
            .get(&node)
            .is_some_and(|current| current.as_ref() == &geometry)
        {
            return false;
        }

        let old_ports = self
            .nodes
            .get(&node)
            .map(|current| current.ports().keys().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let new_ports = geometry.ports().keys().copied().collect::<BTreeSet<_>>();
        let touched_ports = old_ports.union(&new_ports).count();
        let old_outcomes = self
            .nodes
            .get(&node)
            .map(|current| {
                current
                    .port_outcomes()
                    .keys()
                    .copied()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let new_outcomes = geometry
            .port_outcomes()
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let touched_outcomes = old_outcomes.union(&new_outcomes).count();

        for port in &old_ports {
            self.ports.remove_mut(port);
        }
        for port in old_outcomes {
            self.port_outcomes.remove_mut(&port);
        }
        for (port, published) in geometry.ports() {
            if self.port_index.upsert(published) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
            self.ports.insert_mut(*port, Arc::new(published.clone()));
        }
        for port in old_ports.difference(&new_ports) {
            if self.port_index.remove(*port) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
        }
        for (port, outcome) in geometry.port_outcomes() {
            self.port_outcomes
                .insert_mut(*port, Arc::new(outcome.clone()));
        }
        self.nodes.insert_mut(node, Arc::new(geometry));
        self.record_snapshot_updates(1 + touched_ports + touched_outcomes);
        true
    }

    fn remove_node(&mut self, node: NodeId) -> bool {
        let Some(current) = self.nodes.get(&node).cloned() else {
            return false;
        };
        self.nodes.remove_mut(&node);
        for port in current.ports().keys() {
            self.ports.remove_mut(port);
            if self.port_index.remove(*port) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
        }
        for port in current.port_outcomes().keys() {
            self.port_outcomes.remove_mut(port);
        }
        self.node_spatial.remove(node);
        self.record_snapshot_updates(1 + current.ports().len() + current.port_outcomes().len());
        true
    }

    fn translate_node(&mut self, node: NodeId, x: f32, y: f32) -> bool {
        let Some(mut translated) = self.nodes.get(&node).map(|value| value.as_ref().clone()) else {
            return false;
        };
        translated.translate(x, y);
        let bounds = translated.bounds();
        let geometry_changed = self.upsert_node(node, translated);
        let spatial_changed = self.node_spatial.upsert(node, bounds);
        geometry_changed || spatial_changed
    }

    fn mark_node_culled(&mut self, node: NodeId, ports: impl IntoIterator<Item = PortId>) -> bool {
        let Some(current) = self.nodes.get(&node) else {
            return false;
        };
        let mut culled = current.as_ref().clone();
        if !culled.mark_ports_culled(ports) {
            return false;
        }
        self.upsert_node(node, culled)
    }

    fn mark_node_stale(&mut self, node: NodeId, ports: impl IntoIterator<Item = PortId>) -> bool {
        let Some(current) = self.nodes.get(&node) else {
            return false;
        };
        let mut stale = current.as_ref().clone();
        if !stale.mark_stale(ports) {
            return false;
        }
        self.upsert_node(node, stale)
    }

    fn clear_nodes(&mut self) {
        if self.nodes.is_empty() && self.ports.is_empty() && self.port_outcomes.is_empty() {
            return;
        }
        let removed = self
            .nodes
            .size()
            .saturating_add(self.ports.size())
            .saturating_add(self.port_outcomes.size());
        self.nodes = SharedGeometryMap::new_sync();
        self.ports = SharedGeometryMap::new_sync();
        self.port_outcomes = SharedGeometryMap::new_sync();
        if self.port_index.clear() {
            self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
        }
        self.record_snapshot_updates(removed);
    }

    fn upsert_route(&mut self, edge: EdgeId, route: PublishedEdgeRoute) -> bool {
        if self
            .routes
            .get(&edge)
            .is_some_and(|current| current.as_ref() == &route)
        {
            return false;
        }
        let index = self.route_index.upsert(edge, &route);
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.routes.insert_mut(edge, Arc::new(route));
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.record_snapshot_updates(1);
        true
    }

    fn remove_route(&mut self, edge: EdgeId) -> bool {
        let route_removed = self.routes.remove_mut(&edge);
        let index = self.route_index.remove(edge);
        if !route_removed && !index.index_changed {
            return false;
        }
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.record_snapshot_updates(1);
        true
    }

    fn clear_routes(&mut self) {
        let index = self.route_index.clear();
        if self.routes.is_empty() && !index.index_changed {
            return;
        }
        let removed = self.routes.size();
        self.routes = SharedGeometryMap::new_sync();
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.record_snapshot_updates(removed);
    }

    pub(super) fn recompute_routes(
        &mut self,
        snapshot: &GraphSnapshot,
        edges: &BTreeSet<EdgeId>,
        policy: GraphRuntimePolicy,
        selected_edges: &HashSet<EdgeId>,
    ) -> RouteRecomputation {
        let mut changed = false;
        for edge_id in edges {
            let Some(edge) = snapshot.graph().edge(edge_id) else {
                changed |= self.remove_route(*edge_id);
                changed |= self.unresolved_routes.remove(edge_id);
                continue;
            };
            if edge.hidden {
                changed |= self.remove_route(*edge_id);
                changed |= self.unresolved_routes.remove(edge_id);
                continue;
            }
            let route = (|| {
                let source = self.published_port(snapshot, edge.from)?;
                let target = self.published_port(snapshot, edge.to)?;
                let endpoints = EdgePosition {
                    source: absolute_port_endpoint(source)?,
                    target: absolute_port_endpoint(target)?,
                };
                let edge_policy = resolve_edge_policy(edge, &policy);
                resolve_edge_route_path(edge, endpoints)
                    .map(|facts| {
                        facts.with_interaction(EdgeInteractionFacts {
                            selectable: edge_policy.can_select(),
                            selected: selected_edges.contains(edge_id),
                            deletable: edge_policy.can_delete(),
                            reconnect_source: edge_policy.can_reconnect_source(),
                            reconnect_target: edge_policy.can_reconnect_target(),
                        })
                    })
                    .map(|facts| PublishedEdgeRoute::new(*edge_id, facts))
            })();
            if let Some(route) = route {
                changed |= self.upsert_route(*edge_id, route);
                changed |= self.unresolved_routes.remove(edge_id);
            } else {
                changed |= self.remove_route(*edge_id);
                changed |= self.unresolved_routes.insert(*edge_id);
            }
        }
        RouteRecomputation {
            attempts: edges.len() as u64,
            changed,
        }
    }

    fn published_port(
        &self,
        snapshot: &GraphSnapshot,
        port: PortId,
    ) -> Option<&PublishedPortGeometry> {
        let owner = snapshot.graph().port(&port)?.node;
        let node = self.nodes.get(&owner)?;
        node.is_fresh().then(|| node.ports().get(&port)).flatten()
    }

    #[cfg(test)]
    pub(super) fn is_empty_for_test(&self) -> bool {
        self.assert_consistent();
        self.layout_generations.is_empty()
            && self.binding_revisions.is_empty()
            && self.renderer_bindings.is_empty()
            && self.nodes.is_empty()
            && self.ports.is_empty()
            && self.port_outcomes.is_empty()
            && self.port_index.indexed_ports_for_test().is_empty()
            && self.node_spatial.indexed_bounds_for_test().is_empty()
            && self.active_frame_nodes.is_empty()
            && self.routes.is_empty()
            && self.route_index.indexed_edges_for_test().is_empty()
            && self.unresolved_routes.is_empty()
            && self.snapshot_cache.is_none()
    }

    #[cfg(test)]
    pub(super) fn assert_consistent(&self) {
        let spatial = self.node_spatial.indexed_bounds_for_test();
        assert_eq!(self.nodes.size(), spatial.len());
        for node in self.nodes.keys() {
            let geometry = self.nodes.get(node).expect("node key remains present");
            assert_eq!(spatial.get(node), Some(&geometry.bounds()));
        }

        let mut expected_ports = BTreeSet::new();
        let mut expected_outcomes = BTreeSet::new();
        for geometry in self.nodes.values() {
            for (port, published) in geometry.ports() {
                assert_eq!(published.node(), geometry.node());
                assert_eq!(published.port(), *port);
                assert!(expected_ports.insert(*port), "port belongs to one node");
                assert_eq!(
                    self.ports.get(port).map(Arc::as_ref),
                    Some(published),
                    "node and global port geometry must match"
                );
            }
            for (port, outcome) in geometry.port_outcomes() {
                assert!(
                    expected_outcomes.insert(*port),
                    "port outcome belongs to one node"
                );
                assert_eq!(
                    self.port_outcomes.get(port).map(Arc::as_ref),
                    Some(outcome),
                    "node and global port outcome must match"
                );
                assert_eq!(
                    outcome.rendered(),
                    geometry.ports().get(port),
                    "rendered outcomes and node port geometry must agree"
                );
            }
        }
        assert_eq!(
            expected_ports,
            self.ports.keys().copied().collect::<BTreeSet<_>>()
        );
        assert_eq!(
            expected_outcomes,
            self.port_outcomes.keys().copied().collect::<BTreeSet<_>>()
        );
        assert_eq!(
            expected_ports,
            self.port_index.indexed_ports_for_test(),
            "global rendered ports and the hit index must have identical keys"
        );
        for port in self.ports.values() {
            assert!(self.port_index.matches_geometry_for_test(port));
        }

        let published_routes = self.routes.keys().copied().collect::<BTreeSet<_>>();
        assert_eq!(
            published_routes,
            self.route_index.indexed_edges_for_test(),
            "published routes and route index must have identical keys"
        );
        for route in self.routes.values() {
            assert!(self.route_index.matches_route_for_test(route));
        }
        assert!(
            self.unresolved_routes
                .iter()
                .all(|edge| !published_routes.contains(edge)),
            "resolved and unresolved route sets must be disjoint"
        );

        if let Some(snapshot) = &self.snapshot_cache {
            assert_eq!(snapshot.layout_revision(), self.layout_revision);
            assert_eq!(snapshot.nodes().len(), self.nodes.size());
            assert_eq!(snapshot.ports().len(), self.ports.size());
            assert_eq!(snapshot.port_outcomes().len(), self.port_outcomes.size());
            assert_eq!(snapshot.routes().len(), self.routes.size());
            for node in self.nodes.keys() {
                assert_eq!(snapshot.node(*node), self.nodes.get(node).map(Arc::as_ref));
            }
            for port in self.ports.keys() {
                assert_eq!(snapshot.port(*port), self.ports.get(port).map(Arc::as_ref));
            }
            for port in self.port_outcomes.keys() {
                assert_eq!(
                    snapshot.port_outcome(*port),
                    self.port_outcomes.get(port).map(Arc::as_ref)
                );
            }
            for edge in self.routes.keys() {
                assert_eq!(
                    snapshot.route(*edge),
                    self.routes.get(edge).map(Arc::as_ref)
                );
            }
        }
    }

    #[cfg(not(test))]
    fn assert_consistent(&self) {}
}

fn absolute_port_endpoint(port: &PublishedPortGeometry) -> Option<EdgeEndpointPosition> {
    let point = port.route_anchor();
    if !point.is_finite() {
        return None;
    }
    Some(EdgeEndpointPosition {
        point,
        position: port.outward(),
    })
}
