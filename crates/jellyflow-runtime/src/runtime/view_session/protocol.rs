//! Owned frame materialization and one-shot feedback submission.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, NodeId, PortId};

use crate::runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, FeedbackOperation, FeedbackOperationReceipt, FeedbackReceipt,
    FeedbackSubmissionError, FrameBoundsSource, FrameFeedback, FrameMaterializationError,
    FrameMaterializationOverflow, FrameNode, FramePort, FrameRequest, FrameToken, NodeInstanceKey,
    NodeLayout, NodeLayoutTarget, PublishedPortOutcome, RendererStamp, StaleFrameReason,
};
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::graph_runtime::{
    DocumentEpoch, GeometryImpact, GraphRevision, GraphSnapshot, IncidentEdgeSnapshot,
    IncidentSnapshotWork, NodeIncarnation, RuntimePatch,
};
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume};
use crate::runtime::viewport::ViewportTransform;

use super::geometry::{admission_bounds, expanded_bounds, publish_layouts, recompute_routes};
use super::order::sort_render_order;
use super::{ViewSession, ViewSessionError, ViewSessionEvent, ensure_open};

#[derive(Debug, Clone)]
pub(super) struct ExpectedNodeLayout {
    pub(super) incarnation: NodeIncarnation,
    pub(super) renderer: RendererStamp,
    pub(super) layout_generation: u64,
    pub(super) ports: BTreeSet<PortId>,
    pub(super) incident_edges: IncidentEdgeSnapshot,
}

#[derive(Debug, Clone)]
pub(super) struct PendingFrame {
    pub(super) sequence: u64,
    pub(super) graph_revision: GraphRevision,
    pub(super) impact_through_revision: GraphRevision,
    pub(super) geometry_impact: GeometryImpact,
    pub(super) document_epoch: DocumentEpoch,
    pub(super) transform_revision: u64,
    pub(super) transform: ViewportTransform,
    pub(super) environment: AdapterEnvironment,
    pub(super) nodes: BTreeMap<NodeId, ExpectedNodeLayout>,
}

impl PendingFrame {
    /// Merges one authoritative patch into the facts changed since this frame was materialized.
    ///
    /// Returns `true` when revision continuity was lost and the frame had to fall back to a global
    /// geometry impact.
    pub(super) fn observe_patch(&mut self, patch: &RuntimePatch) -> bool {
        merge_patch_impact(
            &mut self.geometry_impact,
            &mut self.impact_through_revision,
            self.document_epoch,
            patch,
        )
    }
}

fn merge_patch_impact(
    impact: &mut GeometryImpact,
    through_revision: &mut GraphRevision,
    document_epoch: DocumentEpoch,
    patch: &RuntimePatch,
) -> bool {
    let fallback = patch.document_epoch() != document_epoch
        || patch.from_revision() != *through_revision
        || patch.kind() == crate::runtime::graph_runtime::RuntimePatchKind::DocumentReset;
    if fallback {
        impact.make_global();
    } else {
        impact.merge(patch.geometry_impact());
    }
    *through_revision = patch.to_revision();
    fallback
}

pub(super) struct PendingLayout {
    pub(super) operation_index: usize,
    pub(super) layout: NodeLayout,
}

struct FrameCandidate {
    node: NodeId,
    selected: bool,
    renderer: RendererStamp,
    layout_generation: u64,
    binding_revision: GraphRevision,
    bounds: CanvasRect,
    bounds_source: FrameBoundsSource,
}

fn take_candidate_budget(
    mut candidates: Vec<FrameCandidate>,
    limit: usize,
    explicit_ranks: &BTreeMap<NodeId, usize>,
) -> (Vec<FrameCandidate>, usize) {
    if candidates.len() <= limit {
        return (candidates, 0);
    }
    sort_render_order(
        &mut candidates,
        |candidate| candidate.node,
        |candidate| candidate.selected,
        explicit_ranks,
    );
    let overflow = candidates.len() - limit;
    candidates.truncate(limit);
    (candidates, overflow)
}

impl ViewSession {
    /// Materializes one owned native-adapter frame for this view.
    pub fn frame(&self, request: FrameRequest) -> Result<AdapterFrame, FrameMaterializationError> {
        if !request.viewport.is_positive_finite() {
            return Err(FrameMaterializationError::InvalidViewport);
        }
        let runtime = self.runtime()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        let viewport = expanded_bounds(request.viewport, request.overscan)
            .ok_or(FrameMaterializationError::InvalidViewport)?;

        let (view_id, mut candidates, candidate_ids, inactive_nodes, materialization_overflow) = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let (rebuild_visits, rebuilt) = state.admission.prepare(snapshot.graph(), &request);
            let viewport_rect = viewport.to_rect();
            let visible_scan_limit = request.materialization_budget.visible_node_scan_limit();
            let measured_query = state
                .spatial
                .nodes_intersecting_with_limit(viewport_rect, visible_scan_limit);
            let remaining_scan_limit =
                visible_scan_limit.saturating_sub(measured_query.entries_examined as usize);
            let admission_query = state
                .admission
                .nodes_intersecting_with_limit(viewport_rect, remaining_scan_limit);
            let visible_scan_truncated = measured_query.truncated || admission_query.truncated;
            let query_cells_examined = admission_query
                .cells_examined
                .saturating_add(measured_query.cells_examined);
            let query_entries_examined = admission_query
                .entries_examined
                .saturating_add(measured_query.entries_examined);
            let mut admission_candidates = admission_query.nodes;
            admission_candidates.extend(measured_query.nodes);
            let (candidates, candidate_ids, materialization_overflow, selection_membership_lookups) = {
                let candidate_for = |node_id, forced| {
                    let node = snapshot.graph().node(&node_id)?;
                    if node.hidden {
                        return None;
                    }
                    let renderer = request.renderer_for(node_id, &node.kind);
                    let layout_generation = state
                        .layout_generations
                        .get(&node_id)
                        .copied()
                        .unwrap_or_default();
                    let binding_revision = state
                        .binding_revisions
                        .get(&node_id)
                        .copied()
                        .unwrap_or_default();
                    let (bounds, bounds_source) = admission_bounds(
                        node_id,
                        node,
                        &renderer,
                        layout_generation,
                        &state.geometry,
                        &request,
                    )?;
                    let is_visible = CanvasBounds::from_rect(bounds)
                        .is_some_and(|bounds| bounds.intersects(viewport));
                    (forced || is_visible).then(|| {
                        let selected = state.selected_node_membership.contains(&node_id);
                        FrameCandidate {
                            node: node_id,
                            selected,
                            renderer,
                            layout_generation,
                            binding_revision,
                            bounds,
                            bounds_source,
                        }
                    })
                };
                let visible_candidates = admission_candidates
                    .into_iter()
                    .filter_map(|node| candidate_for(node, false))
                    .collect::<Vec<_>>();
                let mut selection_membership_lookups = visible_candidates.len() as u64;
                let (mut candidates, visible_overflow) = take_candidate_budget(
                    visible_candidates,
                    request.materialization_budget.visible_nodes(),
                    &state.node_draw_order_ranks,
                );
                let visible_ids = candidates
                    .iter()
                    .map(|candidate| candidate.node)
                    .collect::<BTreeSet<_>>();
                let mut candidate_ids = visible_ids.clone();

                let required_candidates = request
                    .required_nodes
                    .iter()
                    .filter(|node| !candidate_ids.contains(node))
                    .filter_map(|node| candidate_for(*node, true))
                    .collect::<Vec<_>>();
                selection_membership_lookups =
                    selection_membership_lookups.saturating_add(required_candidates.len() as u64);
                let (required_candidates, required_overflow) = take_candidate_budget(
                    required_candidates,
                    request.materialization_budget.required_nodes(),
                    &state.node_draw_order_ranks,
                );
                candidate_ids.extend(required_candidates.iter().map(|candidate| candidate.node));
                candidates.extend(required_candidates);

                let routing_budget = request.materialization_budget.routing_dependency_nodes();
                let explicit_routing_candidates = request
                    .routing_dependencies
                    .iter()
                    .filter(|node| !candidate_ids.contains(node))
                    .filter_map(|node| candidate_for(*node, true))
                    .collect::<Vec<_>>();
                selection_membership_lookups = selection_membership_lookups
                    .saturating_add(explicit_routing_candidates.len() as u64);
                let (explicit_routing_candidates, mut routing_overflow) = take_candidate_budget(
                    explicit_routing_candidates,
                    routing_budget,
                    &state.node_draw_order_ranks,
                );
                let explicit_routing_count = explicit_routing_candidates.len();
                candidate_ids.extend(
                    explicit_routing_candidates
                        .iter()
                        .map(|candidate| candidate.node),
                );
                candidates.extend(explicit_routing_candidates);

                let remaining_routing_budget =
                    routing_budget.saturating_sub(explicit_routing_count);
                let incident_query = if request.incident_routing_dependencies {
                    Some(
                        runtime
                            .adapter_frame_incident_routing_dependencies(
                                &visible_ids,
                                &candidate_ids,
                                remaining_routing_budget,
                                remaining_routing_budget,
                            )
                            .map_err(ViewSessionError::from)?,
                    )
                } else {
                    None
                };
                let incident_scan_truncated =
                    incident_query.as_ref().is_some_and(|query| query.truncated);
                let incident_candidates = incident_query
                    .into_iter()
                    .flat_map(|query| query.nodes)
                    .filter(|node| !candidate_ids.contains(node))
                    .filter_map(|node| candidate_for(node, true))
                    .collect::<Vec<_>>();
                selection_membership_lookups =
                    selection_membership_lookups.saturating_add(incident_candidates.len() as u64);
                let (incident_candidates, incident_overflow) = take_candidate_budget(
                    incident_candidates,
                    remaining_routing_budget,
                    &state.node_draw_order_ranks,
                );
                routing_overflow = routing_overflow.saturating_add(incident_overflow);
                candidate_ids.extend(incident_candidates.iter().map(|candidate| candidate.node));
                candidates.extend(incident_candidates);

                let materialization_overflow = FrameMaterializationOverflow::new(
                    visible_overflow,
                    required_overflow,
                    routing_overflow,
                    visible_scan_truncated,
                    incident_scan_truncated,
                );
                sort_render_order(
                    &mut candidates,
                    |candidate| candidate.node,
                    |candidate| candidate.selected,
                    &state.node_draw_order_ranks,
                );
                (
                    candidates,
                    candidate_ids,
                    materialization_overflow,
                    selection_membership_lookups,
                )
            };
            let inactive_nodes = state
                .active_frame_nodes
                .difference(&candidate_ids)
                .filter(|node| {
                    state.geometry.get(node).is_some_and(|geometry| {
                        geometry
                            .port_outcomes()
                            .values()
                            .any(|outcome| !matches!(outcome, PublishedPortOutcome::Culled))
                    })
                })
                .copied()
                .collect::<BTreeSet<_>>();
            state.active_frame_nodes = candidate_ids.clone();
            state.stats.admission_nodes_visited = state
                .stats
                .admission_nodes_visited
                .saturating_add(rebuild_visits);
            state.stats.admission_query_cells_examined = state
                .stats
                .admission_query_cells_examined
                .saturating_add(query_cells_examined);
            state.stats.admission_query_entries_examined = state
                .stats
                .admission_query_entries_examined
                .saturating_add(query_entries_examined);
            state.stats.admission_index_rebuilds = state
                .stats
                .admission_index_rebuilds
                .saturating_add(u64::from(rebuilt));
            state.stats.frame_order_candidates_ranked = state
                .stats
                .frame_order_candidates_ranked
                .saturating_add(candidates.len() as u64);
            state.stats.frame_selection_membership_lookups = state
                .stats
                .frame_selection_membership_lookups
                .saturating_add(selection_membership_lookups);
            (
                state.id,
                candidates,
                candidate_ids,
                inactive_nodes,
                materialization_overflow,
            )
        };
        let fact_nodes = candidate_ids
            .union(&inactive_nodes)
            .copied()
            .collect::<BTreeSet<_>>();
        let facts = runtime
            .adapter_frame_node_facts(&fact_nodes)
            .map_err(ViewSessionError::from)?;

        let (renderer_publication, frame_incident_work) = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let mut node_invalidations = 0_u64;
            let mut invalidated_nodes = Vec::new();
            let mut route_edges = BTreeSet::new();
            let mut incident_work = IncidentSnapshotWork::default();
            let mut geometry_changed = false;
            let mut spatial_updates = 0_u64;
            for FrameCandidate {
                node,
                renderer,
                layout_generation,
                ..
            } in &mut candidates
            {
                let renderer_changed = match state.renderer_bindings.entry(*node) {
                    Entry::Vacant(entry) => {
                        entry.insert(renderer.clone());
                        false
                    }
                    Entry::Occupied(mut entry) if entry.get() != &*renderer => {
                        entry.insert(renderer.clone());
                        true
                    }
                    Entry::Occupied(_) => false,
                };
                if renderer_changed {
                    let generation = state.layout_generations.entry(*node).or_default();
                    *generation = generation.saturating_add(1);
                    *layout_generation = *generation;
                    geometry_changed |= state.remove_node_geometry(*node);
                    spatial_updates =
                        spatial_updates.saturating_add(u64::from(state.spatial.remove(*node)));
                    if let Some((_, incident_edges)) = facts.get(node) {
                        incident_work.materialize(incident_edges);
                        route_edges.extend(incident_edges.iter().copied());
                    }
                    node_invalidations = node_invalidations.saturating_add(1);
                    invalidated_nodes.push(*node);
                }
            }
            for node_id in &inactive_nodes {
                let ports = snapshot
                    .graph()
                    .node(node_id)
                    .map(|node| node.ports.iter().copied())
                    .into_iter()
                    .flatten();
                let ports_culled = state.mark_node_ports_culled(*node_id, ports);
                if !ports_culled {
                    continue;
                }
                geometry_changed = true;
                node_invalidations = node_invalidations.saturating_add(1);
                invalidated_nodes.push(*node_id);
                if let Some((_, incident_edges)) = facts.get(node_id) {
                    incident_work.materialize(incident_edges);
                    route_edges.extend(incident_edges.iter().copied());
                }
            }
            let route_invalidations = route_edges.len() as u64;
            let route_recomputation = recompute_routes(&mut state, &snapshot, &route_edges);
            let publication_changed = geometry_changed || route_recomputation.changed;
            if publication_changed {
                state.advance_layout_revision();
                state.stats.layout_publications = state.stats.layout_publications.saturating_add(1);
            }
            state.stats.node_invalidations = state
                .stats
                .node_invalidations
                .saturating_add(node_invalidations);
            state.stats.spatial_updates =
                state.stats.spatial_updates.saturating_add(spatial_updates);
            state.stats.route_invalidations = state
                .stats
                .route_invalidations
                .saturating_add(route_invalidations);
            state.stats.route_recomputations = state
                .stats
                .route_recomputations
                .saturating_add(route_recomputation.attempts);
            (
                publication_changed.then_some((state.layout_revision, invalidated_nodes)),
                incident_work,
            )
        };
        runtime
            .record_frame_incident_snapshot_work(frame_incident_work)
            .map_err(ViewSessionError::from)?;

        let sequence = {
            let state = self
                .shared
                .state
                .try_borrow()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            state
                .last_issued_frame
                .checked_add(1)
                .ok_or(FrameMaterializationError::IdentityExhausted)?
        };
        let mut frame_nodes = Vec::with_capacity(candidates.len());
        let mut expected_nodes = BTreeMap::new();
        for FrameCandidate {
            node: node_id,
            selected,
            renderer,
            layout_generation,
            binding_revision,
            bounds,
            bounds_source,
        } in candidates
        {
            let Some((incarnation, incident_edges)) = facts.get(&node_id) else {
                continue;
            };
            let Some(node) = snapshot.graph().node(&node_id) else {
                continue;
            };
            let instance = NodeInstanceKey::new(view_id, node_id, renderer.clone(), *incarnation);
            let layout_target = Arc::new(NodeLayoutTarget {
                instance,
                layout_generation,
                frame_sequence: sequence,
            });
            let ports = node
                .ports
                .iter()
                .filter_map(|port_id| {
                    snapshot.graph().port(port_id).map(|_| {
                        FramePort::new(snapshot.clone(), Arc::clone(&layout_target), *port_id)
                    })
                })
                .collect::<Vec<_>>();
            expected_nodes.insert(
                node_id,
                ExpectedNodeLayout {
                    incarnation: *incarnation,
                    renderer: renderer.clone(),
                    layout_generation,
                    ports: node.ports.iter().copied().collect(),
                    incident_edges: incident_edges.clone(),
                },
            );
            frame_nodes.push(FrameNode::new(
                snapshot.clone(),
                layout_target,
                selected,
                binding_revision,
                ports,
                bounds,
                bounds_source,
            ));
        }

        let materialized_count = frame_nodes.len() as u64;
        let (token, transform) = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            if state.seen_revision != snapshot.revision() {
                return Err(FrameMaterializationError::StaleFrame {
                    sequence: state.last_issued_frame,
                    reason: StaleFrameReason::GraphRevision,
                });
            }
            let next_sequence = state
                .last_issued_frame
                .checked_add(1)
                .ok_or(FrameMaterializationError::IdentityExhausted)?;
            if next_sequence != sequence {
                return Err(FrameMaterializationError::StaleFrame {
                    sequence,
                    reason: StaleFrameReason::Superseded,
                });
            }
            if state.pending_frame.is_some() {
                state.last_superseded_frame = state.last_issued_frame;
            }
            state.last_issued_frame = sequence;
            state.stats.frame_nodes_materialized = state
                .stats
                .frame_nodes_materialized
                .saturating_add(materialized_count);
            let token = FrameToken {
                view: state.id,
                generation: state.generation,
                sequence,
            };
            let transform_revision = state.transform_revision;
            let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(FrameMaterializationError::InvalidViewport)?;
            let environment = state.environment;
            state.pending_frame = Some(PendingFrame {
                sequence,
                graph_revision: snapshot.revision(),
                impact_through_revision: snapshot.revision(),
                geometry_impact: GeometryImpact::default(),
                document_epoch: snapshot.document_epoch(),
                transform_revision,
                transform,
                environment,
                nodes: expected_nodes,
            });
            (token, transform)
        };

        let frame = AdapterFrame::new(
            token.clone(),
            snapshot.revision(),
            snapshot.document_epoch(),
            transform,
            frame_nodes,
            materialization_overflow,
        );
        if let Some((layout_revision, nodes)) = renderer_publication {
            self.publish_layout_event(layout_revision, nodes);
        }
        self.ensure_materialized_frame_is_current(&token, &snapshot)?;
        Ok(frame)
    }

    fn ensure_materialized_frame_is_current(
        &self,
        token: &FrameToken,
        snapshot: &GraphSnapshot,
    ) -> Result<(), FrameMaterializationError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let Some(pending) = state.pending_frame.as_ref() else {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Superseded,
            });
        };
        if token.view != state.id
            || token.generation != state.generation
            || state.last_issued_frame != token.sequence
            || pending.sequence != token.sequence
        {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Superseded,
            });
        }
        if state.seen_revision != snapshot.revision()
            || pending.graph_revision != snapshot.revision()
        {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::GraphRevision,
            });
        }
        if pending.document_epoch != snapshot.document_epoch() {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::DocumentEpoch,
            });
        }
        if pending.transform_revision != state.transform_revision {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Transform,
            });
        }
        if pending.environment != state.environment {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Environment,
            });
        }
        if pending.nodes.iter().any(|(node, expected)| {
            state
                .layout_generations
                .get(node)
                .copied()
                .unwrap_or_default()
                != expected.layout_generation
        }) {
            return Err(FrameMaterializationError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::NodeLayoutGeneration,
            });
        }
        Ok(())
    }

    /// Submits one host-ordered log and atomically publishes its valid layout subset.
    pub fn submit_feedback(
        &self,
        feedback: FrameFeedback,
    ) -> Result<FeedbackReceipt, FeedbackSubmissionError> {
        let runtime = self.runtime()?;
        let before = runtime.snapshot().map_err(ViewSessionError::from)?;
        let pending = self.consume_frame(&feedback.token, &before)?;
        let view_event_batch = self.shared.subscriptions.defer_events();
        let notification_batch = runtime.defer_notifications();
        let submission = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let token = feedback.token;
            let mut geometry_impact = pending.geometry_impact.clone();
            let mut impact_through_revision = pending.impact_through_revision;
            let mut impact_patches_merged = 0_u64;
            let mut impact_global_fallbacks = 0_u64;
            let mut receipts = (0..feedback.operations.len())
                .map(|_| None)
                .collect::<Vec<_>>();
            let mut layouts = Vec::new();

            for (operation_index, operation) in feedback.operations.into_iter().enumerate() {
                match operation {
                    FeedbackOperation::Selection {
                        nodes,
                        edges,
                        groups,
                    } => {
                        self.set_selection(nodes, edges, groups)?;
                        receipts[operation_index] = Some(FeedbackOperationReceipt::Selection {
                            selection: self.selection()?,
                        });
                    }
                    FeedbackOperation::Viewport { pan, zoom } => {
                        self.set_viewport(pan, zoom)?;
                        receipts[operation_index] = Some(FeedbackOperationReceipt::Viewport {
                            transform: self.viewport_transform()?,
                        });
                    }
                    FeedbackOperation::Command(command) => {
                        let outcome = self.execute(command);
                        if let Ok(outcome) = &outcome
                            && let Some(applied) = outcome.applied()
                        {
                            impact_global_fallbacks = impact_global_fallbacks.saturating_add(
                                u64::from(merge_patch_impact(
                                    &mut geometry_impact,
                                    &mut impact_through_revision,
                                    pending.document_epoch,
                                    applied.patch(),
                                )),
                            );
                            impact_patches_merged = impact_patches_merged.saturating_add(1);
                        }
                        receipts[operation_index] = Some(FeedbackOperationReceipt::Command {
                            outcome: Box::new(outcome),
                        });
                    }
                    FeedbackOperation::Layout(layout) => {
                        layouts.push(PendingLayout {
                            operation_index,
                            layout,
                        });
                    }
                }
            }

            let (publication, feedback_incident_work) = if layouts.is_empty() {
                (None, IncidentSnapshotWork::default())
            } else {
                let current = runtime.snapshot().map_err(ViewSessionError::from)?;
                let layout_nodes = layouts
                    .iter()
                    .map(|pending| pending.layout.node())
                    .collect::<BTreeSet<_>>();
                let current_node_facts = runtime
                    .adapter_frame_node_facts(&layout_nodes)
                    .map_err(ViewSessionError::from)?;
                let mut state = self
                    .shared
                    .state
                    .try_borrow_mut()
                    .map_err(|_| ViewSessionError::Busy)?;
                ensure_open(state.status)?;
                if impact_through_revision != current.revision() {
                    geometry_impact.make_global();
                    impact_global_fallbacks = impact_global_fallbacks.saturating_add(1);
                }
                state.stats.geometry_impact_patches_merged = state
                    .stats
                    .geometry_impact_patches_merged
                    .saturating_add(impact_patches_merged);
                state.stats.geometry_impact_global_fallbacks = state
                    .stats
                    .geometry_impact_global_fallbacks
                    .saturating_add(impact_global_fallbacks);
                let global_stale = current.document_epoch() != pending.document_epoch
                    || state.transform_revision != pending.transform_revision
                    || state.environment != pending.environment
                    || state.last_issued_frame > token.sequence;
                publish_layouts(
                    &mut state,
                    &current,
                    &pending,
                    &current_node_facts,
                    &geometry_impact,
                    layouts,
                    &mut receipts,
                    global_stale,
                )
            };
            runtime
                .record_feedback_incident_snapshot_work(feedback_incident_work)
                .map_err(ViewSessionError::from)?;

            let operations = receipts
                .into_iter()
                .map(|receipt| receipt.expect("every feedback operation has a receipt"))
                .collect::<Vec<_>>();
            Ok::<_, FeedbackSubmissionError>((operations, publication))
        }));
        let mut callback_panics = PanicCollector::default();
        let submission = match submission {
            Ok(submission) => Some(submission),
            Err(payload) => {
                callback_panics.record(Some(payload));
                None
            }
        };
        callback_panics.record(notification_batch.finish());
        let submission = submission.map(|submission| {
            submission.map(|(operations, publication)| {
                if let Some(publication) = &publication {
                    let mut nodes = publication.published_nodes().to_vec();
                    nodes.extend_from_slice(publication.withdrawn_nodes());
                    callback_panics
                        .record(self.publish_layout_event(publication.layout_revision(), nodes));
                }
                FeedbackReceipt::new(operations, publication)
            })
        });
        callback_panics.record(view_event_batch.finish());
        callback_panics.resume();
        submission.expect("panicking feedback submission must resume its unwind")
    }

    fn consume_frame(
        &self,
        token: &FrameToken,
        snapshot: &GraphSnapshot,
    ) -> Result<PendingFrame, FeedbackSubmissionError> {
        let mut state = self
            .shared
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        if token.view != state.id || token.generation != state.generation {
            return Err(FeedbackSubmissionError::ForeignFrame {
                expected: state.id,
                actual: token.view,
            });
        }
        if token.sequence > state.last_issued_frame {
            return Err(FeedbackSubmissionError::FutureFrame {
                sequence: token.sequence,
            });
        }
        if token.sequence <= state.last_superseded_frame {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Superseded,
            });
        }
        let Some(pending) = state.pending_frame.take() else {
            return Err(FeedbackSubmissionError::ReplayedFrame {
                sequence: token.sequence,
            });
        };
        if pending.sequence != token.sequence {
            state.pending_frame = Some(pending);
            return Err(FeedbackSubmissionError::ReplayedFrame {
                sequence: token.sequence,
            });
        }
        if pending.document_epoch != snapshot.document_epoch() {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::DocumentEpoch,
            });
        }
        if pending.impact_through_revision != snapshot.revision()
            || state.seen_revision != snapshot.revision()
        {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::GraphRevision,
            });
        }
        if pending.transform_revision != state.transform_revision {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Transform,
            });
        }
        if pending.environment != state.environment {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::Environment,
            });
        }
        if pending.nodes.iter().any(|(node, expected)| {
            let generation_changed = state
                .layout_generations
                .get(node)
                .copied()
                .unwrap_or_default()
                != expected.layout_generation;
            generation_changed
                && !pending.geometry_impact.affects_layout(
                    *node,
                    expected.ports.iter().copied(),
                    expected.incident_edges.iter().copied(),
                )
        }) {
            return Err(FeedbackSubmissionError::StaleFrame {
                sequence: token.sequence,
                reason: StaleFrameReason::NodeLayoutGeneration,
            });
        }
        Ok(pending)
    }
    pub fn set_adapter_environment(
        &self,
        environment: AdapterEnvironment,
    ) -> Result<bool, ViewSessionError> {
        let runtime = self.runtime()?;
        {
            let state = self
                .shared
                .state
                .try_borrow()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            if state.environment == environment {
                return Ok(false);
            }
        }
        let snapshot = runtime.snapshot()?;
        let nodes = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?
            .geometry
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let facts = runtime.adapter_frame_node_facts(&nodes)?;
        let publication = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            if state.environment == environment {
                return Ok(false);
            }
            state.environment = environment;
            for node in &nodes {
                let generation = state.layout_generations.entry(*node).or_default();
                *generation = generation.saturating_add(1);
            }
            let ports_by_node = nodes
                .iter()
                .filter_map(|node| {
                    snapshot
                        .graph()
                        .node(node)
                        .map(|record| (*node, record.ports.clone()))
                })
                .collect::<BTreeMap<_, _>>();
            let stale_nodes = state.mark_all_node_geometry_stale(&ports_by_node);
            let route_edges = facts
                .values()
                .flat_map(|(_, incident_edges)| incident_edges.iter().copied())
                .collect::<BTreeSet<_>>();
            let route_invalidations = route_edges.len() as u64;
            let route_recomputation = recompute_routes(&mut state, &snapshot, &route_edges);
            let publication_changed = stale_nodes > 0 || route_recomputation.changed;
            if publication_changed {
                state.advance_layout_revision();
                state.stats.layout_publications = state.stats.layout_publications.saturating_add(1);
            }
            state.stats.node_invalidations = state
                .stats
                .node_invalidations
                .saturating_add(nodes.len() as u64);
            state.stats.route_invalidations = state
                .stats
                .route_invalidations
                .saturating_add(route_invalidations);
            state.stats.route_recomputations = state
                .stats
                .route_recomputations
                .saturating_add(route_recomputation.attempts);
            publication_changed.then_some((
                state.layout_revision,
                nodes.iter().copied().collect::<Vec<_>>(),
            ))
        };
        if let Some((layout_revision, nodes)) = publication {
            self.publish_layout_event(layout_revision, nodes);
        }
        Ok(true)
    }

    pub fn invalidate_node_layout(
        &self,
        instance: &NodeInstanceKey,
    ) -> Result<bool, ViewSessionError> {
        let runtime = self.runtime()?;
        let snapshot = runtime.snapshot()?;
        let Some(incarnation) = runtime.node_incarnation(instance.node())? else {
            return Err(ViewSessionError::NodeUnavailable {
                node: instance.node(),
            });
        };
        let nodes = BTreeSet::from([instance.node()]);
        let facts = runtime.adapter_frame_node_facts(&nodes)?;
        let publication = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            if instance.view() != state.id
                || incarnation != instance.incarnation()
                || state.renderer_bindings.get(&instance.node()) != Some(instance.renderer())
            {
                return Ok(false);
            }
            let generation = state.layout_generations.entry(instance.node()).or_default();
            *generation = generation.saturating_add(1);
            let ports = snapshot
                .graph()
                .node(&instance.node())
                .map(|node| node.ports.iter().copied())
                .into_iter()
                .flatten();
            let geometry_changed = state.mark_node_geometry_stale(instance.node(), ports);
            let route_edges = facts
                .get(&instance.node())
                .map(|(_, incident_edges)| incident_edges.clone())
                .unwrap_or_default();
            let route_invalidations = route_edges.len() as u64;
            let route_recomputation = recompute_routes(&mut state, &snapshot, &route_edges);
            let publication_changed = geometry_changed || route_recomputation.changed;
            if publication_changed {
                state.advance_layout_revision();
                state.stats.layout_publications = state.stats.layout_publications.saturating_add(1);
            }
            state.stats.node_invalidations = state.stats.node_invalidations.saturating_add(1);
            state.stats.route_invalidations = state
                .stats
                .route_invalidations
                .saturating_add(route_invalidations);
            state.stats.route_recomputations = state
                .stats
                .route_recomputations
                .saturating_add(route_recomputation.attempts);
            publication_changed.then_some((state.layout_revision, vec![instance.node()]))
        };
        if let Some((layout_revision, nodes)) = publication {
            resume(self.publish_layout_event(layout_revision, nodes));
        }
        Ok(true)
    }

    fn publish_layout_event(
        &self,
        layout_revision: u64,
        mut nodes: Vec<NodeId>,
    ) -> Option<PanicPayload> {
        nodes.sort();
        nodes.dedup();
        self.shared
            .subscriptions
            .publish_event(ViewSessionEvent::LayoutPublished {
                layout_revision,
                nodes,
            })
    }
}
