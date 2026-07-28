//! Owned frame materialization and one-shot feedback submission.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, NodeId, PortId};

use crate::runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, FeedbackOperation, FeedbackReceipt, FeedbackSubmissionError,
    FrameAdmissionReceipt, FrameBoundsSource, FrameFeedback, FrameGeometryReceipt,
    FrameIncidentRoutingReceipt, FrameIncidentSnapshotReceipt, FrameMaterializationError,
    FrameMaterializationOverflow, FrameMaterializationReceipt, FrameNode, FramePort, FrameRequest,
    FrameToken, LayoutOperationReceipt, NodeInstanceKey, NodeLayout, NodeLayoutTarget,
    RendererStamp, StaleFrameReason,
};
use crate::runtime::commands::CommandExecutionCompletion;
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::graph_runtime::{
    DocumentEpoch, GeometryImpact, GraphRevision, GraphSnapshot, IncidentEdgeSnapshot,
    IncidentSnapshotWork, NodeLifetimeKey, RuntimePatch,
};
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume};
use crate::runtime::viewport::{ViewportRevision, ViewportSnapshot, ViewportTransform};

use super::geometry::{expanded_bounds, publish_layouts};
use super::order::sort_render_order;
use super::{ViewSession, ViewSessionError, ViewSessionEvent, ViewState, ensure_open};

#[derive(Debug, Clone)]
pub(super) struct ExpectedNodeLayout {
    pub(super) lifetime: NodeLifetimeKey,
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
    pub(super) viewport_revision: ViewportRevision,
    pub(super) transform: ViewportTransform,
    pub(super) environment: AdapterEnvironment,
    pub(super) nodes: BTreeMap<NodeId, ExpectedNodeLayout>,
}

impl PendingFrame {
    /// Merges one authoritative patch into the facts changed since this frame was materialized.
    ///
    /// Returns `true` when revision continuity was lost and the frame had to fall back to a global
    /// geometry impact.
    pub(super) fn observe_patch(&mut self, patch: &RuntimePatch) -> (bool, u64) {
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
) -> (bool, u64) {
    let fallback = patch.document_epoch() != document_epoch
        || patch.from_revision() != *through_revision
        || patch.kind() == crate::runtime::graph_runtime::RuntimePatchKind::DocumentReset;
    if fallback {
        impact.make_global();
        *through_revision = patch.to_revision();
        (true, 0)
    } else {
        let records_merged = impact.merge(patch.geometry_impact());
        *through_revision = patch.to_revision();
        (false, records_merged)
    }
}

pub(super) struct PendingLayout {
    pub(super) receipt_index: usize,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FrameAdmissionWork {
    nodes_visited: u64,
    query_cells_examined: u64,
    query_entries_examined: u64,
    index_rebuilt: bool,
    candidates_ranked: u64,
    selection_lookups: u64,
    incident_routing_index_bucket_reads: u64,
    incident_routing_index_entries_read: u64,
}

fn take_candidate_budget(
    mut candidates: Vec<FrameCandidate>,
    limit: usize,
    explicit_ranks: &BTreeMap<NodeId, usize>,
    recent_interaction: Option<NodeId>,
) -> (Vec<FrameCandidate>, usize, u64) {
    if candidates.len() <= limit {
        return (candidates, 0, 0);
    }
    let candidates_ranked = candidates.len() as u64;
    sort_render_order(
        &mut candidates,
        |candidate| candidate.node,
        |candidate| candidate.selected,
        explicit_ranks,
        recent_interaction,
    );
    let overflow = candidates.len() - limit;
    candidates.drain(..overflow);
    (candidates, overflow, candidates_ranked)
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

        let (
            view_id,
            mut candidates,
            candidate_ids,
            inactive_nodes,
            materialization_overflow,
            admission_work,
        ) = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let (rebuild_visits, rebuilt) =
                state.derived.prepare_admission(snapshot.graph(), &request);
            let viewport_rect = viewport.to_rect();
            let visible_scan_limit = request.materialization_budget.visible_node_scan_limit();
            let measured_query = state
                .derived
                .nodes_intersecting_with_limit(viewport_rect, visible_scan_limit);
            let remaining_scan_limit =
                visible_scan_limit.saturating_sub(measured_query.entries_examined as usize);
            let admission_query = state
                .derived
                .admission_nodes_intersecting_with_limit(viewport_rect, remaining_scan_limit);
            let visible_scan_truncated = measured_query.truncated || admission_query.truncated;
            let query_cells_examined = admission_query
                .cells_examined
                .saturating_add(measured_query.cells_examined);
            let query_entries_examined = admission_query
                .entries_examined
                .saturating_add(measured_query.entries_examined);
            let mut admission_candidates = admission_query.nodes;
            admission_candidates.extend(measured_query.nodes);
            let (
                candidates,
                candidate_ids,
                materialization_overflow,
                selection_membership_lookups,
                order_candidates_ranked,
                incident_routing_index_bucket_reads,
                incident_routing_index_entries_read,
            ) = {
                let recent_interaction = state.recent_interaction_node_id();
                let candidate_for = |node_id, forced| {
                    let node = snapshot.graph().node(&node_id)?;
                    if node.hidden {
                        return None;
                    }
                    let renderer = request.renderer_for(node_id, &node.kind);
                    let layout_generation = state.derived.layout_generation(node_id);
                    let binding_revision = state.derived.binding_revision(node_id);
                    let (bounds, bounds_source) = state
                        .derived
                        .frame_bounds(node_id, node, &renderer, &request)?;
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
                let (mut candidates, visible_overflow, mut order_candidates_ranked) =
                    take_candidate_budget(
                        visible_candidates,
                        request.materialization_budget.visible_nodes(),
                        &state.node_draw_order_ranks,
                        recent_interaction,
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
                let (required_candidates, required_overflow, required_candidates_ranked) =
                    take_candidate_budget(
                        required_candidates,
                        request.materialization_budget.required_nodes(),
                        &state.node_draw_order_ranks,
                        recent_interaction,
                    );
                order_candidates_ranked =
                    order_candidates_ranked.saturating_add(required_candidates_ranked);
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
                let (
                    explicit_routing_candidates,
                    mut routing_overflow,
                    explicit_routing_candidates_ranked,
                ) = take_candidate_budget(
                    explicit_routing_candidates,
                    routing_budget,
                    &state.node_draw_order_ranks,
                    recent_interaction,
                );
                order_candidates_ranked =
                    order_candidates_ranked.saturating_add(explicit_routing_candidates_ranked);
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
                let incident_routing_index_bucket_reads = incident_query
                    .as_ref()
                    .map_or(0, |query| query.index_bucket_reads);
                let incident_routing_index_entries_read = incident_query
                    .as_ref()
                    .map_or(0, |query| query.index_entries_read);
                let incident_candidates = incident_query
                    .into_iter()
                    .flat_map(|query| query.nodes)
                    .filter(|node| !candidate_ids.contains(node))
                    .filter_map(|node| candidate_for(node, true))
                    .collect::<Vec<_>>();
                selection_membership_lookups =
                    selection_membership_lookups.saturating_add(incident_candidates.len() as u64);
                let (incident_candidates, incident_overflow, incident_candidates_ranked) =
                    take_candidate_budget(
                        incident_candidates,
                        remaining_routing_budget,
                        &state.node_draw_order_ranks,
                        recent_interaction,
                    );
                order_candidates_ranked =
                    order_candidates_ranked.saturating_add(incident_candidates_ranked);
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
                order_candidates_ranked =
                    order_candidates_ranked.saturating_add(candidates.len() as u64);
                sort_render_order(
                    &mut candidates,
                    |candidate| candidate.node,
                    |candidate| candidate.selected,
                    &state.node_draw_order_ranks,
                    recent_interaction,
                );
                (
                    candidates,
                    candidate_ids,
                    materialization_overflow,
                    selection_membership_lookups,
                    order_candidates_ranked,
                    incident_routing_index_bucket_reads,
                    incident_routing_index_entries_read,
                )
            };
            let inactive_nodes = state.derived.inactive_frame_nodes(&candidate_ids);
            let admission_work = FrameAdmissionWork {
                nodes_visited: rebuild_visits,
                query_cells_examined,
                query_entries_examined,
                index_rebuilt: rebuilt,
                candidates_ranked: order_candidates_ranked,
                selection_lookups: selection_membership_lookups,
                incident_routing_index_bucket_reads,
                incident_routing_index_entries_read,
            };
            (
                state.id,
                candidates,
                candidate_ids,
                inactive_nodes,
                materialization_overflow,
                admission_work,
            )
        };
        let fact_nodes = candidate_ids
            .union(&inactive_nodes)
            .copied()
            .collect::<BTreeSet<_>>();
        let facts = runtime
            .adapter_frame_node_facts(&fact_nodes)
            .map_err(ViewSessionError::from)?;

        let (renderer_publication, frame_incident_work, frame_geometry_work) = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let renderers = candidates
                .iter()
                .map(|candidate| (candidate.node, candidate.renderer.clone()))
                .collect::<BTreeMap<_, _>>();
            let policy = state.policy;
            let ViewState {
                derived,
                selected_edge_membership,
                ..
            } = &mut *state;
            let (publication, incident_work, generations, geometry_work) = derived
                .reconcile_frame_geometry(
                    &snapshot,
                    &renderers,
                    &candidate_ids,
                    &facts,
                    policy,
                    selected_edge_membership,
                );
            for candidate in &mut candidates {
                if let Some(generation) = generations.get(&candidate.node) {
                    candidate.layout_generation = *generation;
                }
            }
            (publication, incident_work, geometry_work)
        };
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
            let Some(runtime_facts) = facts.get(&node_id) else {
                continue;
            };
            let Some(node) = snapshot.graph().node(&node_id) else {
                continue;
            };
            let Some(instance) =
                NodeInstanceKey::new(view_id, renderer.clone(), runtime_facts.lifetime)
            else {
                continue;
            };
            let layout_target = Arc::new(NodeLayoutTarget {
                instance,
                layout_generation,
                frame_sequence: sequence,
            });
            let ports = node
                .ports
                .iter()
                .filter_map(|port_id| {
                    snapshot.graph().port(port_id)?;
                    let port_instance = runtime_facts.port_instances.get(port_id).copied()?;
                    Some(FramePort::new(
                        snapshot.clone(),
                        Arc::clone(&layout_target),
                        port_instance,
                    ))
                })
                .collect::<Vec<_>>();
            expected_nodes.insert(
                node_id,
                ExpectedNodeLayout {
                    lifetime: runtime_facts.lifetime,
                    renderer: renderer.clone(),
                    layout_generation,
                    ports: node.ports.iter().copied().collect(),
                    incident_edges: runtime_facts.incident_edges.clone(),
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
        let materialization_receipt = FrameMaterializationReceipt::new(
            materialization_overflow,
            FrameAdmissionReceipt::new(
                materialized_count,
                admission_work.nodes_visited,
                admission_work.query_cells_examined,
                admission_work.query_entries_examined,
                admission_work.index_rebuilt,
                admission_work.candidates_ranked,
                admission_work.selection_lookups,
            ),
            FrameIncidentRoutingReceipt::new(
                admission_work.incident_routing_index_bucket_reads,
                admission_work.incident_routing_index_entries_read,
            ),
            FrameIncidentSnapshotReceipt::new(
                frame_incident_work.entries_read(),
                frame_incident_work.materializations(),
            ),
            FrameGeometryReceipt::new(
                frame_geometry_work.node_invalidations,
                frame_geometry_work.spatial_updates,
                frame_geometry_work.route_invalidations,
                frame_geometry_work.route_recomputations,
            ),
        );
        let (token, viewport) = {
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
            let token = FrameToken {
                view: state.id,
                generation: state.generation,
                sequence,
            };
            let viewport_revision = state.viewport_revision;
            let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(FrameMaterializationError::InvalidViewport)?;
            let viewport = ViewportSnapshot::new(
                super::ViewProvenance::new(state.id, state.generation),
                viewport_revision,
                transform,
            );
            let environment = state.environment;
            state.pending_frame = Some(PendingFrame {
                sequence,
                graph_revision: snapshot.revision(),
                impact_through_revision: snapshot.revision(),
                geometry_impact: GeometryImpact::default(),
                document_epoch: snapshot.document_epoch(),
                viewport_revision,
                transform,
                environment,
                nodes: expected_nodes,
            });
            (token, viewport)
        };

        let frame = AdapterFrame::new(
            token.clone(),
            snapshot.version(),
            snapshot.document_epoch(),
            viewport,
            frame_nodes,
            materialization_receipt,
        );
        let callback_panic = renderer_publication
            .and_then(|(layout_revision, nodes)| self.publish_layout_event(layout_revision, nodes));
        let materialization = self.ensure_materialized_frame_is_current(&token, &snapshot);
        resume(callback_panic);
        materialization?;
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
        if pending.viewport_revision != state.viewport_revision {
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
            state.derived.layout_generation(*node) != expected.layout_generation
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
        let mut command_identities = HashSet::new();
        for operation in &feedback.operations {
            let FeedbackOperation::Command(command) = operation else {
                continue;
            };
            let identity = command.identity().clone();
            if !command_identities.insert(identity.clone()) {
                return Err(FeedbackSubmissionError::DuplicateCommandIdentity { identity });
            }
        }
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
            let mut impact_records_merged = 0_u64;
            let mut impact_global_fallbacks = 0_u64;
            let mut command_completions = Vec::<CommandExecutionCompletion>::new();
            let mut layout_receipts = Vec::<LayoutOperationReceipt>::new();
            let mut layouts = Vec::new();

            for operation in feedback.operations {
                match operation {
                    FeedbackOperation::Command(command) => {
                        let completion = runtime.execute_command(command);
                        if let Ok(outcome) = completion.result()
                            && let Some(applied) = outcome.applied()
                        {
                            let (fallback, records_merged) = merge_patch_impact(
                                &mut geometry_impact,
                                &mut impact_through_revision,
                                pending.document_epoch,
                                applied.patch(),
                            );
                            impact_global_fallbacks =
                                impact_global_fallbacks.saturating_add(u64::from(fallback));
                            impact_records_merged =
                                impact_records_merged.saturating_add(records_merged);
                            impact_patches_merged = impact_patches_merged.saturating_add(1);
                        }
                        command_completions.push(completion);
                    }
                    FeedbackOperation::Layout(layout) => {
                        let receipt_index = layout_receipts.len();
                        layout_receipts.push(LayoutOperationReceipt::pending(layout.node()));
                        layouts.push(PendingLayout {
                            receipt_index,
                            layout,
                        });
                    }
                }
            }

            let layout_result = (|| -> Result<_, ViewSessionError> {
                if layouts.is_empty() {
                    return Ok((None, IncidentSnapshotWork::default()));
                }
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
                state.derived.record_geometry_impact_merge(
                    impact_patches_merged,
                    impact_records_merged,
                    impact_global_fallbacks,
                );
                let global_stale = current.document_epoch() != pending.document_epoch
                    || state.viewport_revision != pending.viewport_revision
                    || state.environment != pending.environment
                    || state.last_issued_frame > token.sequence;
                Ok(publish_layouts(
                    &mut state,
                    &current,
                    &pending,
                    &current_node_facts,
                    &geometry_impact,
                    layouts,
                    &mut layout_receipts,
                    global_stale,
                ))
            })();
            let (publication, incident_work, mut post_admission_error) = match layout_result {
                Ok((publication, work)) => (publication, work, None),
                Err(error) => (None, IncidentSnapshotWork::default(), Some(error)),
            };
            if post_admission_error.is_none()
                && let Err(error) = runtime.record_feedback_incident_snapshot_work(incident_work)
            {
                post_admission_error = Some(error.into());
            }
            let receipt = FeedbackReceipt::from_prevalidated_parts(
                command_completions,
                layout_receipts,
                publication,
            );
            Ok::<_, FeedbackSubmissionError>((receipt, post_admission_error))
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
            submission.and_then(|(receipt, post_admission_error)| {
                if let Some(publication) = receipt.publication() {
                    let mut nodes = publication.published_nodes().to_vec();
                    nodes.extend_from_slice(publication.withdrawn_nodes());
                    callback_panics
                        .record(self.publish_layout_event(publication.layout_revision(), nodes));
                }
                match post_admission_error {
                    Some(source) if !command_identities.is_empty() => {
                        Err(FeedbackSubmissionError::AfterCommandAdmission {
                            source,
                            receipt: Some(Box::new(receipt)),
                        })
                    }
                    Some(source) => Err(FeedbackSubmissionError::Session(source)),
                    None => Ok(receipt),
                }
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
        if pending.viewport_revision != state.viewport_revision {
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
            let generation_changed =
                state.derived.layout_generation(*node) != expected.layout_generation;
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
            .derived
            .node_ids();
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
            let ports_by_node = nodes
                .iter()
                .filter_map(|node| {
                    snapshot
                        .graph()
                        .node(node)
                        .map(|record| (*node, record.ports.clone()))
                })
                .collect::<BTreeMap<_, _>>();
            let route_edges = facts
                .values()
                .flat_map(|facts| facts.incident_edges.iter().copied())
                .collect::<BTreeSet<_>>();
            let policy = state.policy;
            let ViewState {
                derived,
                selected_edge_membership,
                ..
            } = &mut *state;
            derived
                .mark_all_stale(
                    &snapshot,
                    &ports_by_node,
                    &route_edges,
                    policy,
                    selected_edge_membership,
                )
                .map(|layout_revision| (layout_revision, nodes.iter().copied().collect::<Vec<_>>()))
        };
        let callback_panic = publication
            .and_then(|(layout_revision, nodes)| self.publish_layout_event(layout_revision, nodes));
        resume(callback_panic);
        Ok(true)
    }

    pub fn invalidate_node_layout(
        &self,
        instance: &NodeInstanceKey,
    ) -> Result<bool, ViewSessionError> {
        let runtime = self.runtime()?;
        let snapshot = runtime.snapshot()?;
        let Some(lifetime) = runtime.node_lifetime_key(instance.node())? else {
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
                || lifetime != instance.lifetime()
                || !state
                    .derived
                    .renderer_matches(instance.node(), instance.renderer())
            {
                return Ok(false);
            }
            let ports = snapshot
                .graph()
                .node(&instance.node())
                .map(|node| node.ports.iter().copied())
                .into_iter()
                .flatten();
            let route_edges = facts
                .get(&instance.node())
                .map(|facts| facts.incident_edges.clone())
                .unwrap_or_default();
            let policy = state.policy;
            let ViewState {
                derived,
                selected_edge_membership,
                ..
            } = &mut *state;
            derived
                .mark_one_stale(
                    &snapshot,
                    instance.node(),
                    ports,
                    &route_edges,
                    policy,
                    selected_edge_membership,
                )
                .map(|layout_revision| (layout_revision, vec![instance.node()]))
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

#[cfg(test)]
mod candidate_budget_tests {
    use jellyflow_core::core::{CanvasPoint, CanvasSize};

    use super::*;

    #[test]
    fn overflow_preserves_frontmost_and_selected_candidates() {
        let back = NodeId::from_u128(1);
        let front = NodeId::from_u128(2);
        let selected = NodeId::from_u128(3);
        let candidates = [
            candidate(back, false),
            candidate(front, false),
            candidate(selected, true),
        ];
        let ranks = BTreeMap::from([(back, 0), (front, 1), (selected, 2)]);

        let (retained, overflow, candidates_ranked) =
            take_candidate_budget(candidates.into(), 2, &ranks, None);

        assert_eq!(overflow, 1);
        assert_eq!(candidates_ranked, 3);
        assert_eq!(
            retained
                .into_iter()
                .map(|candidate| candidate.node)
                .collect::<Vec<_>>(),
            vec![front, selected],
        );
    }

    fn candidate(node: NodeId, selected: bool) -> FrameCandidate {
        FrameCandidate {
            node,
            selected,
            renderer: RendererStamp::new("test.renderer", 1),
            layout_generation: 0,
            binding_revision: 0,
            bounds: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
            bounds_source: FrameBoundsSource::Persisted,
        }
    }
}
