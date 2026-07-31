//! Atomic ownership of all view-local derived geometry.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, EdgeId, Graph, Node, NodeId, PortId};

use crate::runtime::adapter_protocol::{
    FrameBoundsSource, FrameRequest, GeometrySnapshot, GeometryVisibleCounts,
    LayoutPublicationReceipt, PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome, RendererStamp, SharedFrameCullSet, SharedGeometryMap,
    SharedGeometryOwnerMap, ViewPoint,
};
use crate::runtime::geometry::{
    CanvasBounds, EdgeEndpointPosition, EdgeInteractionFacts, EdgePosition, resolve_edge_route_path,
};
use crate::runtime::graph_runtime::{
    GraphRevision, GraphRuntimePolicy, GraphSnapshot, IncidentEdgeSnapshot, IncidentSnapshotWork,
    PortInstanceKey, RuntimePatch, RuntimePatchKind,
};
use crate::runtime::interaction::ConnectionGestureBinding;
use crate::runtime::policy::resolve_edge_policy;
use crate::runtime::viewport::ViewportTransform;

use super::edge_pick::{EdgePickCursor, InternalEdgePickOutcome};
use super::geometry::{
    AdmissionIndex, InternalNodeIntersectionOutcome, NodeIntersectionCursor, SpatialQuery,
    ViewSpatialIndex,
};
use super::port_geometry::{
    ConnectionProbeCursor, ConnectionProbeTargeting, InternalProbeOutcome, PortGeometryIndex,
};
use super::route_geometry::{
    InternalRouteQueryOutcome, RouteGeometryIndex, RouteGeometryRevision, VisibleRouteCursor,
};
use super::{SelectedEdgeMembership, ViewProvenance, ViewQueryIdentity};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RouteRecomputation {
    pub(super) attempts: u64,
    pub(super) changed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct FrameGeometryReconciliationWork {
    pub(super) node_invalidations: u64,
    pub(super) spatial_updates: u64,
    pub(super) route_invalidations: u64,
    pub(super) route_recomputations: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct FeedbackLayoutPublicationWork {
    pub(super) incident_snapshot: IncidentSnapshotWork,
    pub(super) spatial_updates: u64,
    pub(super) route_invalidations: u64,
    pub(super) route_recomputations: u64,
    pub(super) refresh_requested: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RuntimePatchGeometryWork {
    pub(super) admission_nodes_visited: u64,
    pub(super) node_invalidations: u64,
    pub(super) spatial_updates: u64,
    pub(super) route_invalidations: u64,
    pub(super) route_recomputations: u64,
}

#[derive(Debug)]
pub(super) struct LayoutWithdrawal {
    node: NodeId,
    ports: BTreeSet<PortId>,
    incident_edges: IncidentEdgeSnapshot,
    current_incident_edges: Option<IncidentEdgeSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingLayoutReplacement {
    RetainLastComplete,
    WithdrawOnInvalid,
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

#[derive(Debug)]
pub(super) struct DerivedGeometry {
    provenance: ViewProvenance,
    layout_revision: u64,
    route_facts_revision: u128,
    layout_generations: BTreeMap<NodeId, u64>,
    binding_revisions: BTreeMap<NodeId, GraphRevision>,
    renderer_bindings: BTreeMap<NodeId, RendererStamp>,
    pending_layout_replacements: BTreeMap<NodeId, PendingLayoutReplacement>,
    nodes: SharedGeometryMap<NodeId, PublishedNodeGeometry>,
    node_owners: SharedGeometryOwnerMap<NodeId>,
    ports: SharedGeometryMap<PortId, PublishedPortGeometry>,
    port_outcomes: SharedGeometryMap<PortId, PublishedPortOutcome>,
    port_owners: SharedGeometryOwnerMap<PortId>,
    frame_culled_nodes: SharedFrameCullSet,
    frame_culled_port_count: usize,
    frame_culled_port_outcome_count: usize,
    port_index: PortGeometryIndex,
    node_spatial: ViewSpatialIndex,
    admission: AdmissionIndex,
    active_frame_nodes: BTreeSet<NodeId>,
    routes: SharedGeometryMap<EdgeId, PublishedEdgeRoute>,
    route_index: RouteGeometryIndex,
    unresolved_routes: BTreeSet<EdgeId>,
    snapshot_cache: Option<GeometrySnapshot>,
}

impl DerivedGeometry {
    pub(super) fn new(provenance: ViewProvenance) -> Self {
        Self {
            provenance,
            layout_revision: 0,
            route_facts_revision: 0,
            layout_generations: BTreeMap::new(),
            binding_revisions: BTreeMap::new(),
            renderer_bindings: BTreeMap::new(),
            pending_layout_replacements: BTreeMap::new(),
            nodes: SharedGeometryMap::default(),
            node_owners: SharedGeometryOwnerMap::default(),
            ports: SharedGeometryMap::default(),
            port_outcomes: SharedGeometryMap::default(),
            port_owners: SharedGeometryOwnerMap::default(),
            frame_culled_nodes: SharedFrameCullSet::default(),
            frame_culled_port_count: 0,
            frame_culled_port_outcome_count: 0,
            port_index: PortGeometryIndex::default(),
            node_spatial: ViewSpatialIndex::default(),
            admission: AdmissionIndex::default(),
            active_frame_nodes: BTreeSet::new(),
            routes: SharedGeometryMap::default(),
            route_index: RouteGeometryIndex::default(),
            unresolved_routes: BTreeSet::new(),
            snapshot_cache: None,
        }
    }

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
        identity: ViewQueryIdentity,
        graph_revision: GraphRevision,
        order_revision: u128,
        intent: ConnectionGestureBinding,
        pointer: ViewPoint,
        targeting: ConnectionProbeTargeting,
        transform: ViewportTransform,
    ) -> Option<Box<ConnectionProbeCursor>> {
        self.port_index.new_cursor(
            identity,
            graph_revision,
            order_revision,
            intent,
            pointer,
            targeting,
            transform,
        )
    }

    pub(super) fn connection_probe_index_epoch(&self) -> u128 {
        self.port_index.index_epoch()
    }

    pub(super) fn connection_endpoint_owner_is_frame_visible(
        &self,
        instance: PortInstanceKey,
    ) -> bool {
        !self.frame_culled_nodes.contains(&instance.node())
    }

    pub(super) fn query_connection_probe_page(
        &mut self,
        snapshot: &GraphSnapshot,
        node_draw_order_ranks: &BTreeMap<NodeId, usize>,
        selected_nodes: &HashSet<NodeId>,
        recent_interaction_node: Option<NodeId>,
        cursor: Box<ConnectionProbeCursor>,
        entry_budget: usize,
    ) -> InternalProbeOutcome {
        let frame_culled_nodes = &self.frame_culled_nodes;
        self.port_index.query_page(
            snapshot.graph(),
            |port| {
                let node = snapshot.graph().port(&port)?.node;
                (!frame_culled_nodes.contains(&node))
                    .then(|| snapshot.port_instance_key(port))
                    .flatten()
            },
            node_draw_order_ranks,
            selected_nodes,
            recent_interaction_node,
            cursor,
            entry_budget,
        )
    }

    pub(super) fn new_visible_route_cursor(
        &self,
        identity: ViewQueryIdentity,
        bounds: CanvasRect,
    ) -> Box<VisibleRouteCursor> {
        self.route_index.new_cursor(identity, bounds)
    }

    pub(super) fn visible_route_index_epoch(&self) -> u128 {
        self.route_index.index_epoch()
    }

    pub(super) fn query_visible_route_page(
        &mut self,
        cursor: Box<VisibleRouteCursor>,
        entry_budget: usize,
        zoom: f32,
    ) -> InternalRouteQueryOutcome {
        self.route_index.query_page(cursor, entry_budget, zoom)
    }

    pub(super) fn route_geometry_revision(&self, edge: EdgeId) -> Option<RouteGeometryRevision> {
        self.route_index.geometry_revision(edge)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_edge_pick_cursor(
        &self,
        identity: ViewQueryIdentity,
        order_revision: u128,
        pointer: ViewPoint,
        radius_in_view: f32,
        transform: ViewportTransform,
    ) -> Option<Box<EdgePickCursor>> {
        self.route_index.new_pick_cursor(
            identity,
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
        selected_edges: &SelectedEdgeMembership,
        cursor: Box<EdgePickCursor>,
        entry_budget: usize,
    ) -> InternalEdgePickOutcome {
        self.route_index.query_pick_page(
            edge_draw_order_ranks,
            selected_edges,
            cursor,
            entry_budget,
        )
    }

    pub(super) fn node_ids(&self) -> BTreeSet<NodeId> {
        self.nodes.keys().copied().collect()
    }

    pub(super) fn node_approximately_equals(
        &self,
        candidate: &PublishedNodeGeometry,
        epsilon: f32,
    ) -> bool {
        if self.frame_culled_nodes.contains(&candidate.node()) {
            return false;
        }
        self.nodes
            .get(&candidate.node())
            .is_some_and(|current| current.approximately_eq(candidate, epsilon))
    }

    pub(super) fn new_node_intersection_cursor(
        &self,
        identity: ViewQueryIdentity,
        bounds: CanvasRect,
        canvas_bounds: CanvasBounds,
    ) -> Box<NodeIntersectionCursor> {
        self.node_spatial
            .new_intersection_cursor(identity, bounds, canvas_bounds)
    }

    pub(super) fn query_node_intersection_page(
        &self,
        cursor: Box<NodeIntersectionCursor>,
        entry_budget: usize,
    ) -> InternalNodeIntersectionOutcome {
        self.node_spatial
            .query_intersection_page(cursor, entry_budget, |node| {
                !self.frame_culled_nodes.contains(&node)
            })
    }

    pub(super) fn node_spatial_index_epoch(&self) -> u128 {
        self.node_spatial.index_epoch()
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
        if let Some(published) = self.nodes.get(&node_id)
            && published.is_fresh()
            && published.renderer() == renderer
            && published.bounds().is_positive_finite()
        {
            // A newer generation rejects old feedback, but the last valid geometry remains the
            // best admission bound until that generation publishes its replacement.
            return Some((published.bounds(), FrameBoundsSource::Published));
        }
        let (size, source) = if let Some(size) = node.size.filter(|size| size.is_positive_finite())
        {
            (size, FrameBoundsSource::Persisted)
        } else if let Some(size) = request.kind_fallback_sizes.get(&node.kind).copied() {
            (size, FrameBoundsSource::AdapterKindFallback)
        } else {
            (request.fallback_size, FrameBoundsSource::ProtocolFallback)
        };
        node.bounds_for_size(size).map(|bounds| (bounds, source))
    }

    pub(super) fn inactive_frame_nodes(&self, nodes: &BTreeSet<NodeId>) -> BTreeSet<NodeId> {
        self.active_frame_nodes
            .difference(nodes)
            .filter(|node| self.nodes.contains_key(node) && !self.frame_culled_nodes.contains(node))
            .copied()
            .collect()
    }

    pub(super) fn snapshot(&mut self) -> GeometrySnapshot {
        if let Some(snapshot) = &self.snapshot_cache {
            self.assert_consistent();
            return snapshot.clone();
        }
        let snapshot = GeometrySnapshot::new(
            self.provenance,
            self.layout_revision,
            self.nodes.clone(),
            self.node_owners.clone(),
            self.ports.clone(),
            self.port_outcomes.clone(),
            self.port_owners.clone(),
            self.frame_culled_nodes.clone(),
            GeometryVisibleCounts {
                nodes: self
                    .nodes
                    .size()
                    .saturating_sub(self.frame_culled_nodes.size()),
                ports: self
                    .ports
                    .size()
                    .saturating_sub(self.frame_culled_port_count),
                port_outcomes: self
                    .port_outcomes
                    .size()
                    .saturating_sub(self.frame_culled_port_outcome_count),
            },
            self.routes.clone(),
        );
        self.snapshot_cache = Some(snapshot.clone());
        self.assert_consistent();
        snapshot
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
        selected_edges: &SelectedEdgeMembership,
    ) -> (
        Option<LayoutPublicationReceipt>,
        FeedbackLayoutPublicationWork,
    ) {
        let mut published_nodes = Vec::new();
        let mut withdrawn_nodes = Vec::new();
        let mut route_edges = BTreeSet::new();
        let mut spatial_updates = 0_u64;
        let mut incident_work = IncidentSnapshotWork::default();

        for (node, geometry, incident_edges) in staged {
            self.pending_layout_replacements.remove(&node);
            spatial_updates = spatial_updates
                .saturating_add(u64::from(self.node_spatial.upsert(node, geometry.bounds())));
            self.upsert_node(node, geometry, true);
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
        if published_nodes.is_empty() && withdrawn_nodes.is_empty() {
            self.assert_consistent();
            return (
                None,
                FeedbackLayoutPublicationWork {
                    incident_snapshot: incident_work,
                    refresh_requested: request_refresh,
                    ..FeedbackLayoutPublicationWork::default()
                },
            );
        }

        let route_recomputation =
            self.recompute_routes(snapshot, &route_edges, policy, selected_edges);
        let before_revision = self.layout_revision;
        self.advance_layout_revision();
        self.assert_consistent();
        (
            Some(LayoutPublicationReceipt::new(
                before_revision,
                self.layout_revision,
                published_nodes,
                withdrawn_nodes,
            )),
            FeedbackLayoutPublicationWork {
                incident_snapshot: incident_work,
                spatial_updates,
                route_invalidations: route_edges.len() as u64,
                route_recomputations: route_recomputation.attempts,
                refresh_requested: request_refresh,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reconcile_frame_geometry(
        &mut self,
        snapshot: &GraphSnapshot,
        renderers: &BTreeMap<NodeId, RendererStamp>,
        active_nodes: &BTreeSet<NodeId>,
        node_facts: &BTreeMap<NodeId, crate::runtime::graph_runtime::AdapterNodeRuntimeFacts>,
        policy: GraphRuntimePolicy,
        selected_edges: &SelectedEdgeMembership,
    ) -> (
        Option<(u64, Vec<NodeId>)>,
        IncidentSnapshotWork,
        BTreeMap<NodeId, u64>,
        FrameGeometryReconciliationWork,
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
            if let Some(facts) = node_facts.get(node) {
                incident_work.materialize(&facts.incident_edges);
                route_edges.extend(facts.incident_edges.iter().copied());
            }
            invalidated_nodes.push(*node);
        }

        for node in &inactive_nodes {
            if !self.mark_node_frame_culled(*node) {
                continue;
            }
            geometry_changed = true;
            invalidated_nodes.push(*node);
        }

        let route_recomputation =
            self.recompute_routes(snapshot, &route_edges, policy, selected_edges);
        let publication_changed = geometry_changed || route_recomputation.changed;
        if publication_changed {
            self.advance_layout_revision();
        }
        self.active_frame_nodes = active_nodes.clone();
        self.assert_consistent();
        let work = FrameGeometryReconciliationWork {
            node_invalidations: invalidated_nodes.len() as u64,
            spatial_updates,
            route_invalidations: route_edges.len() as u64,
            route_recomputations: route_recomputation.attempts,
        };
        (
            publication_changed.then_some((self.layout_revision, invalidated_nodes)),
            incident_work,
            generations,
            work,
        )
    }

    pub(super) fn apply_runtime_patch(
        &mut self,
        patch: &RuntimePatch,
        policy: GraphRuntimePolicy,
        selected_edges: &SelectedEdgeMembership,
    ) -> RuntimePatchGeometryWork {
        let document_reset = patch.kind() == RuntimePatchKind::DocumentReset;
        if document_reset {
            self.reset();
        }

        let admission_nodes_visited = self.admission.update_nodes(
            patch.snapshot().graph(),
            patch.affected_nodes().iter().copied(),
        );
        let impact = patch.impact();
        let mut spatially_updated = BTreeSet::new();
        for node in &impact.retired_nodes {
            self.pending_layout_replacements.remove(node);
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
            self.pending_layout_replacements.remove(node);
            if self.remove_node(*node) {
                spatially_updated.insert(*node);
            }
        }
        for node in patch.affected_nodes() {
            let Some(Some(lifetime)) = patch.node_lifetime_change(*node) else {
                continue;
            };
            if self
                .nodes
                .get(node)
                .is_some_and(|geometry| geometry.lifetime() != lifetime)
                && self.remove_node(*node)
            {
                self.pending_layout_replacements.remove(node);
                spatially_updated.insert(*node);
            }
        }
        for translation in &impact.translated_nodes {
            if self.translate_node(translation.node, translation.delta.x, translation.delta.y) {
                spatially_updated.insert(translation.node);
            }
        }
        for node in &impact.layout_dirty_nodes {
            if impact.retired_nodes.contains(node) || impact.geometry_withdrawals.contains(node) {
                self.pending_layout_replacements.remove(node);
            } else if patch.geometry_impact().retains_last_complete_layout(*node) {
                self.pending_layout_replacements
                    .entry(*node)
                    .or_insert(PendingLayoutReplacement::RetainLastComplete);
            } else {
                self.pending_layout_replacements
                    .insert(*node, PendingLayoutReplacement::WithdrawOnInvalid);
            }
            self.advance_node_generation(*node);
        }
        let route_edges = impact.route_edges.iter().copied().collect::<BTreeSet<_>>();
        let recomputation =
            self.recompute_routes(patch.snapshot(), &route_edges, policy, selected_edges);
        if document_reset
            || !spatially_updated.is_empty()
            || !impact.layout_dirty_nodes.is_empty()
            || recomputation.changed
        {
            self.advance_layout_revision();
        }
        self.assert_consistent();
        RuntimePatchGeometryWork {
            admission_nodes_visited,
            node_invalidations: impact.layout_dirty_nodes.len() as u64,
            spatial_updates: spatially_updated.len() as u64,
            route_invalidations: impact.route_edges.len() as u64,
            route_recomputations: recomputation.attempts,
        }
    }

    pub(super) fn update_route_selection(
        &mut self,
        edges: &BTreeSet<EdgeId>,
        selected_edges: &SelectedEdgeMembership,
    ) -> u64 {
        for edge in edges {
            let Some(route) = self.routes.get(edge).map(|route| route.as_ref().clone()) else {
                continue;
            };
            let selected = selected_edges.contains(route.instance());
            self.upsert_route(*edge, route.with_selected(selected));
        }
        self.assert_consistent();
        edges.len() as u64
    }

    pub(super) fn mark_all_stale(
        &mut self,
        snapshot: &GraphSnapshot,
        ports_by_node: &BTreeMap<NodeId, Vec<PortId>>,
        route_edges: &BTreeSet<EdgeId>,
        policy: GraphRuntimePolicy,
        selected_edges: &SelectedEdgeMembership,
    ) -> Option<u64> {
        for node in ports_by_node.keys() {
            self.pending_layout_replacements.remove(node);
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
        }
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
        selected_edges: &SelectedEdgeMembership,
    ) -> Option<u64> {
        self.pending_layout_replacements.remove(&node);
        self.advance_node_generation(node);
        let geometry_changed = self.mark_node_stale(node, ports);
        let recomputation = self.recompute_routes(snapshot, route_edges, policy, selected_edges);
        let changed = geometry_changed || recomputation.changed;
        if changed {
            self.advance_layout_revision();
        }
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
        self.pending_layout_replacements.clear();
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

    fn invalidate_snapshot(&mut self) {
        self.snapshot_cache = None;
    }

    fn upsert_node(
        &mut self,
        node: NodeId,
        geometry: PublishedNodeGeometry,
        restore_frame_visibility: bool,
    ) -> bool {
        debug_assert_eq!(node, geometry.node());
        let current = self.nodes.get(&node).cloned();
        let current_port_count = current.as_ref().map_or(0, |node| node.ports().len());
        let current_port_outcome_count = current
            .as_ref()
            .map_or(0, |node| node.port_outcomes().len());
        let was_frame_culled = self.frame_culled_nodes.contains(&node);
        let visibility_changed =
            restore_frame_visibility && self.frame_culled_nodes.remove_mut(&node);
        if visibility_changed {
            subtract_count(&mut self.frame_culled_port_count, current_port_count);
            subtract_count(
                &mut self.frame_culled_port_outcome_count,
                current_port_outcome_count,
            );
            self.port_index.invalidate_visibility();
            self.node_spatial.invalidate_visibility();
        }
        if current
            .as_ref()
            .is_some_and(|current| current.as_ref() == &geometry)
        {
            if visibility_changed {
                self.invalidate_snapshot();
            }
            return visibility_changed;
        }

        let old_ports = current
            .as_ref()
            .map(|current| current.ports().keys().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let new_ports = geometry.ports().keys().copied().collect::<BTreeSet<_>>();
        let old_outcomes = current
            .as_ref()
            .map(|current| {
                current
                    .port_outcomes()
                    .keys()
                    .copied()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();

        for port in &old_ports {
            self.ports.remove_mut(port);
        }
        for port in old_outcomes {
            self.port_outcomes.remove_mut(&port);
            self.port_owners.remove_mut(&port);
        }
        for (port, published) in geometry.ports() {
            self.port_index.upsert(published);
            self.ports.insert_mut(*port, Arc::new(published.clone()));
        }
        for port in old_ports.difference(&new_ports) {
            self.port_index.remove(*port);
        }
        for (port, outcome) in geometry.port_outcomes() {
            self.port_outcomes
                .insert_mut(*port, Arc::new(outcome.clone()));
            self.port_owners.insert_mut(*port, node);
        }
        if was_frame_culled && !visibility_changed {
            replace_count(
                &mut self.frame_culled_port_count,
                current_port_count,
                geometry.ports().len(),
            );
            replace_count(
                &mut self.frame_culled_port_outcome_count,
                current_port_outcome_count,
                geometry.port_outcomes().len(),
            );
        }
        self.node_owners.insert_mut(node, node);
        self.nodes.insert_mut(node, Arc::new(geometry));
        self.invalidate_snapshot();
        true
    }

    fn remove_node(&mut self, node: NodeId) -> bool {
        self.pending_layout_replacements.remove(&node);
        let Some(current) = self.nodes.get(&node).cloned() else {
            return false;
        };
        self.nodes.remove_mut(&node);
        self.node_owners.remove_mut(&node);
        if self.frame_culled_nodes.remove_mut(&node) {
            subtract_count(&mut self.frame_culled_port_count, current.ports().len());
            subtract_count(
                &mut self.frame_culled_port_outcome_count,
                current.port_outcomes().len(),
            );
            self.port_index.invalidate_visibility();
        }
        for port in current.ports().keys() {
            self.ports.remove_mut(port);
            self.port_index.remove(*port);
        }
        for port in current.port_outcomes().keys() {
            self.port_outcomes.remove_mut(port);
            self.port_owners.remove_mut(port);
        }
        self.node_spatial.remove(node);
        self.invalidate_snapshot();
        true
    }

    fn translate_node(&mut self, node: NodeId, x: f32, y: f32) -> bool {
        let Some(mut translated) = self.nodes.get(&node).map(|value| value.as_ref().clone()) else {
            return false;
        };
        translated.translate(x, y);
        let bounds = translated.bounds();
        let geometry_changed = self.upsert_node(node, translated, false);
        let spatial_changed = self.node_spatial.upsert(node, bounds);
        geometry_changed || spatial_changed
    }

    fn mark_node_frame_culled(&mut self, node: NodeId) -> bool {
        let Some(current) = self.nodes.get(&node) else {
            return false;
        };
        if self.frame_culled_nodes.contains(&node) {
            return false;
        }
        self.frame_culled_nodes.insert_mut(node);
        self.frame_culled_port_count = self
            .frame_culled_port_count
            .saturating_add(current.ports().len());
        self.frame_culled_port_outcome_count = self
            .frame_culled_port_outcome_count
            .saturating_add(current.port_outcomes().len());
        self.port_index.invalidate_visibility();
        self.node_spatial.invalidate_visibility();
        self.invalidate_snapshot();
        true
    }

    fn mark_node_stale(&mut self, node: NodeId, ports: impl IntoIterator<Item = PortId>) -> bool {
        self.pending_layout_replacements.remove(&node);
        let Some(current) = self.nodes.get(&node) else {
            return false;
        };
        let mut stale = current.as_ref().clone();
        if !stale.mark_stale(ports) {
            return false;
        }
        self.upsert_node(node, stale, false)
    }

    pub(super) fn retains_last_complete_layout(&self, node: NodeId) -> bool {
        self.pending_layout_replacements.get(&node)
            == Some(&PendingLayoutReplacement::RetainLastComplete)
    }

    fn clear_nodes(&mut self) {
        if self.nodes.is_empty()
            && self.node_owners.is_empty()
            && self.ports.is_empty()
            && self.port_outcomes.is_empty()
            && self.port_owners.is_empty()
            && self.frame_culled_nodes.is_empty()
            && self.frame_culled_port_count == 0
            && self.frame_culled_port_outcome_count == 0
        {
            return;
        }
        self.nodes = SharedGeometryMap::new_sync();
        self.node_owners = SharedGeometryOwnerMap::new_sync();
        self.ports = SharedGeometryMap::new_sync();
        self.port_outcomes = SharedGeometryMap::new_sync();
        self.port_owners = SharedGeometryOwnerMap::new_sync();
        self.frame_culled_nodes = SharedFrameCullSet::new_sync();
        self.frame_culled_port_count = 0;
        self.frame_culled_port_outcome_count = 0;
        self.port_index.clear();
        self.invalidate_snapshot();
    }

    fn upsert_route(&mut self, edge: EdgeId, route: PublishedEdgeRoute) -> bool {
        if self
            .routes
            .get(&edge)
            .is_some_and(|current| current.as_ref() == &route)
        {
            return false;
        }
        self.route_index.upsert(edge, &route);
        self.routes.insert_mut(edge, Arc::new(route));
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.invalidate_snapshot();
        true
    }

    fn remove_route(&mut self, edge: EdgeId) -> bool {
        let route_removed = self.routes.remove_mut(&edge);
        let index = self.route_index.remove(edge);
        if !route_removed && !index.index_changed {
            return false;
        }
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.invalidate_snapshot();
        true
    }

    fn clear_routes(&mut self) {
        let index = self.route_index.clear();
        if self.routes.is_empty() && !index.index_changed {
            return;
        }
        self.routes = SharedGeometryMap::new_sync();
        self.route_facts_revision = self.route_facts_revision.saturating_add(1);
        self.invalidate_snapshot();
    }

    pub(super) fn recompute_routes(
        &mut self,
        snapshot: &GraphSnapshot,
        edges: &BTreeSet<EdgeId>,
        policy: GraphRuntimePolicy,
        selected_edges: &SelectedEdgeMembership,
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
                let instance = snapshot.edge_instance_key(*edge_id)?;
                let source_instance = snapshot.port_instance_key(edge.from)?;
                let target_instance = snapshot.port_instance_key(edge.to)?;
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
                            selected: selected_edges.contains(instance),
                            deletable: edge_policy.can_delete(),
                            reconnect_source: edge_policy.can_reconnect_source(),
                            reconnect_target: edge_policy.can_reconnect_target(),
                        })
                    })
                    .and_then(|facts| {
                        PublishedEdgeRoute::new(
                            self.provenance,
                            instance,
                            source_instance,
                            target_instance,
                            facts,
                        )
                    })
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
        // Frame culling suppresses native materialization and hit testing, not retained route
        // geometry. A visible crossing route may still depend on an off-screen endpoint.
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
            && self.node_owners.is_empty()
            && self.ports.is_empty()
            && self.port_outcomes.is_empty()
            && self.port_owners.is_empty()
            && self.frame_culled_nodes.is_empty()
            && self.frame_culled_port_count == 0
            && self.frame_culled_port_outcome_count == 0
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
            assert_eq!(self.node_owners.get(node), Some(node));
        }
        assert_eq!(self.node_owners.size(), self.nodes.size());
        assert!(
            self.frame_culled_nodes
                .iter()
                .all(|node| self.nodes.contains_key(node)),
            "only retained published nodes may be frame-culled",
        );
        let expected_culled_port_count = self
            .frame_culled_nodes
            .iter()
            .map(|node| {
                self.nodes
                    .get(node)
                    .expect("frame-culled node remains retained")
                    .ports()
                    .len()
            })
            .sum::<usize>();
        let expected_culled_port_outcome_count = self
            .frame_culled_nodes
            .iter()
            .map(|node| {
                self.nodes
                    .get(node)
                    .expect("frame-culled node remains retained")
                    .port_outcomes()
                    .len()
            })
            .sum::<usize>();
        assert_eq!(self.frame_culled_port_count, expected_culled_port_count);
        assert_eq!(
            self.frame_culled_port_outcome_count,
            expected_culled_port_outcome_count
        );

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
                assert_eq!(
                    self.port_owners.get(port).copied(),
                    Some(geometry.node()),
                    "every retained port outcome keeps its node owner"
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
            expected_outcomes,
            self.port_owners.keys().copied().collect::<BTreeSet<_>>()
        );
        let expected_interactive_ports = self
            .ports
            .iter()
            .filter_map(|(port, geometry)| geometry.hit_bounds().map(|_| *port))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            expected_interactive_ports,
            self.port_index.indexed_ports_for_test(),
            "only rendered ports with effective interaction bounds belong to the hit index"
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
            let visible_node_count = self
                .nodes
                .keys()
                .filter(|node| !self.frame_culled_nodes.contains(node))
                .count();
            let visible_port_count = self
                .ports
                .values()
                .filter(|port| !self.frame_culled_nodes.contains(&port.node()))
                .count();
            let visible_outcome_count = self
                .port_owners
                .values()
                .filter(|node| !self.frame_culled_nodes.contains(node))
                .count();
            assert_eq!(snapshot.layout_revision(), self.layout_revision);
            assert_eq!(snapshot.nodes().len(), visible_node_count);
            assert_eq!(snapshot.ports().len(), visible_port_count);
            assert_eq!(snapshot.port_outcomes().len(), visible_outcome_count);
            assert_eq!(snapshot.routes().len(), self.routes.size());
            for node in self.nodes.keys() {
                let expected = (!self.frame_culled_nodes.contains(node))
                    .then(|| self.nodes.get(node).map(Arc::as_ref))
                    .flatten();
                assert_eq!(snapshot.node(*node), expected);
            }
            for port in self.ports.keys() {
                let owner = self
                    .port_owners
                    .get(port)
                    .expect("published port has owner");
                let expected = (!self.frame_culled_nodes.contains(owner))
                    .then(|| self.ports.get(port).map(Arc::as_ref))
                    .flatten();
                assert_eq!(snapshot.port(*port), expected);
            }
            for port in self.port_outcomes.keys() {
                let owner = self.port_owners.get(port).expect("port outcome has owner");
                let expected = (!self.frame_culled_nodes.contains(owner))
                    .then(|| self.port_outcomes.get(port).map(Arc::as_ref))
                    .flatten();
                assert_eq!(snapshot.port_outcome(*port), expected);
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

fn subtract_count(count: &mut usize, removed: usize) {
    debug_assert!(*count >= removed);
    *count = count.saturating_sub(removed);
}

fn replace_count(count: &mut usize, previous: usize, next: usize) {
    subtract_count(count, previous);
    *count = count.saturating_add(next);
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
