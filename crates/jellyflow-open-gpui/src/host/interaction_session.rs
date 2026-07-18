use std::collections::BTreeMap;

use jellyflow::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::runtime::viewport::{ViewportPanRequest, pan_viewport};
use jellyflow::runtime::{
    CanvasInteractionEffect, CanvasInteractionError, CanvasInteractionPhase,
    CanvasInteractionQuery, CanvasInteractionUpdate, CommandSettlement, ConnectionProbeQueryError,
    EdgePickQueryError, FreshGraphMutation, InteractionOwner, InteractionPosition,
    InteractionTerminalReason, InteractionTickDemand, InteractionTicketId, SelectionBaseline,
    SelectionMode, SelectionMutation, ViewPoint, ViewRect, ViewportMutation,
};
use open_gpui::{Context, Pixels, Point, Task, Window};

use crate::command_journal::{
    OpenGpuiCommandOrigin, OpenGpuiCommandPrefixBarrierCancellation,
    OpenGpuiCommandPrefixBarrierError, OpenGpuiCommandPrefixPermit,
};
use crate::port_handle::projected_view_rect;

use super::OpenGpuiSceneHost;

const INTERACTION_TICK_SECONDS: f32 = 1.0 / 60.0;

pub(super) struct PendingInteractionSubmission {
    ticket: InteractionTicketId,
    stage: PendingInteractionSubmissionStage,
    prefix_task: Option<Task<()>>,
    prefix_cancellation: Option<OpenGpuiCommandPrefixBarrierCancellation>,
}

enum PendingInteractionSubmissionStage {
    WaitingForPrefix(PendingInteractionMutation),
    Revalidating { permit: OpenGpuiCommandPrefixPermit },
    Enqueued,
}

enum PendingInteractionMutation {
    NodeDrag(FreshGraphMutation),
    Connection(PendingConnectionMutation),
}

#[derive(Clone, Copy)]
enum PendingConnectionMutation {
    Ordinary,
    CreateFromWire,
}

impl OpenGpuiSceneHost {
    pub(super) fn interaction_position(
        &self,
        position: Point<Pixels>,
    ) -> Option<InteractionPosition> {
        let screen = self.scene_screen_position(position)?;
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        let canvas = transform.canvas_point_at_screen(screen);
        InteractionPosition::new(ViewPoint::new(screen.x, screen.y), canvas)
    }

    pub(super) fn apply_canvas_interaction_update(
        &mut self,
        update: CanvasInteractionUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (queries, effects, damage, tick_demand) = update.into_parts();
        let has_queries = !queries.is_empty();
        let has_effects = !effects.is_empty();
        let has_damage = damage != Default::default();
        self.queue_interaction_queries(queries);
        for effect in effects {
            if let Err(error) = self.apply_canvas_interaction_effect(effect, cx) {
                self.record_command_diagnostic(error);
                if matches!(
                    self.canvas_interaction.presentation().phase(),
                    CanvasInteractionPhase::Armed
                        | CanvasInteractionPhase::NodeDrag
                        | CanvasInteractionPhase::Pan
                        | CanvasInteractionPhase::Marquee
                        | CanvasInteractionPhase::ConnectionDrag
                        | CanvasInteractionPhase::EdgeActive
                ) {
                    let cancelled = self.canvas_interaction.cancel();
                    self.discard_pending_interaction_submission();
                    self.queue_interaction_queries(cancelled.into_parts().0);
                }
            }
        }
        self.discard_orphaned_interaction_revalidation();
        self.sync_native_connection_state(window, cx);
        if tick_demand == InteractionTickDemand::WhileActive
            && (has_queries || has_effects || has_damage)
        {
            self.schedule_interaction_tick(window, cx);
        }
        if has_damage || !self.pending_interaction_queries.is_empty() {
            cx.notify();
        }
    }

    fn apply_canvas_interaction_effect(
        &mut self,
        effect: CanvasInteractionEffect,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match effect {
            CanvasInteractionEffect::Selection(selection) => self
                .apply_selection_mutation(selection)
                .map_err(|error| error.to_string()),
            CanvasInteractionEffect::Viewport(viewport) => self
                .apply_viewport_mutation(viewport)
                .map_err(|error| error.to_string()),
            CanvasInteractionEffect::Search(_) => Ok(()),
            CanvasInteractionEffect::SubmitGraph { ticket, mutation } => {
                self.submit_interaction_mutation(ticket, mutation, cx)
            }
        }
    }

    pub(super) fn queue_interaction_queries(
        &mut self,
        queries: impl IntoIterator<Item = CanvasInteractionQuery>,
    ) {
        for query in queries {
            match &query {
                CanvasInteractionQuery::ConnectionProbe { .. } => {
                    self.pending_interaction_queries.retain(|queued| {
                        !matches!(queued, CanvasInteractionQuery::ConnectionProbe { .. })
                    })
                }
                CanvasInteractionQuery::EdgePick { .. } => self
                    .pending_interaction_queries
                    .retain(|queued| !matches!(queued, CanvasInteractionQuery::EdgePick { .. })),
            }
            self.pending_interaction_queries.push_back(query);
        }
    }

    pub(super) fn process_one_interaction_query(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(query) = self.pending_interaction_queries.pop_front() else {
            return;
        };
        let update = match query {
            CanvasInteractionQuery::ConnectionProbe { id, query, .. } => {
                match self.view.connection_probe(query) {
                    Ok(outcome) => self
                        .canvas_interaction
                        .resolve_connection_probe(id, outcome)
                        .map_err(|error| error.to_string()),
                    Err(ConnectionProbeQueryError::StaleCursor(reason))
                        if reason.is_restartable() =>
                    {
                        self.canvas_interaction
                            .restart_stale_query(id)
                            .map_err(|error| error.to_string())
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
            CanvasInteractionQuery::EdgePick { id, query } => match self.view.edge_pick(query) {
                Ok(outcome) => self
                    .canvas_interaction
                    .resolve_edge_pick(id, outcome)
                    .map_err(|error| error.to_string()),
                Err(EdgePickQueryError::StaleCursor(reason)) if reason.is_restartable() => self
                    .canvas_interaction
                    .restart_stale_query(id)
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            },
        };
        match update {
            Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
            Err(error) => {
                self.record_command_diagnostic(format!(
                    "native canvas interaction query failed: {error}"
                ));
                if self.canvas_interaction.presentation().phase()
                    == CanvasInteractionPhase::Committing
                    && let Some(ticket) = self
                        .pending_interaction_submission
                        .as_ref()
                        .map(|pending| pending.ticket)
                {
                    self.discard_pending_interaction_submission();
                    self.settle_interaction_without_native(ticket, CommandSettlement::Conflict);
                    self.sync_native_connection_state(window, cx);
                } else {
                    let update = self.canvas_interaction.cancel();
                    self.discard_pending_interaction_submission();
                    self.apply_canvas_interaction_update(update, window, cx);
                }
            }
        }
    }

    fn submit_interaction_mutation(
        &mut self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.pending_interaction_submission.is_some() {
            return self.enqueue_revalidated_interaction_mutation(ticket, mutation, cx);
        }

        let pending = match mutation {
            mutation @ FreshGraphMutation::MoveNodes(_) => PendingInteractionSubmission {
                ticket,
                stage: PendingInteractionSubmissionStage::WaitingForPrefix(
                    PendingInteractionMutation::NodeDrag(mutation),
                ),
                prefix_task: None,
                prefix_cancellation: None,
            },
            FreshGraphMutation::Connection { topology, .. } => PendingInteractionSubmission {
                ticket,
                stage: PendingInteractionSubmissionStage::WaitingForPrefix(
                    PendingInteractionMutation::Connection(if topology.is_empty() {
                        PendingConnectionMutation::Ordinary
                    } else {
                        PendingConnectionMutation::CreateFromWire
                    }),
                ),
                prefix_task: None,
                prefix_cancellation: None,
            },
        };
        let barrier = self.commands.capture_prefix_barrier().map_err(|error| {
            self.settle_interaction_without_native(ticket, CommandSettlement::TransportFailure);
            format!("failed to capture native command prefix: {error}")
        })?;
        let prefix_cancellation = barrier.cancellation();
        let mut pending = pending;
        pending.prefix_cancellation = Some(prefix_cancellation);
        self.pending_interaction_submission = Some(pending);
        let host = cx.weak_entity();
        let prefix_task = cx.spawn(async move |_, cx| {
            let result = barrier.await;
            let _ = host.update(cx, |host, cx| {
                host.ready_interaction_prefixes.push_back((ticket, result));
                cx.notify();
            });
        });
        self.pending_interaction_submission
            .as_mut()
            .expect("the prefix task is installed before control returns")
            .prefix_task = Some(prefix_task);
        Ok(())
    }

    fn enqueue_revalidated_interaction_mutation(
        &mut self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(mut pending) = self.pending_interaction_submission.take() else {
            return Err("interaction command prefix state disappeared".to_owned());
        };
        if pending.ticket != ticket {
            self.pending_interaction_submission = Some(pending);
            return Err("interaction command ticket does not match its prefix permit".to_owned());
        }
        let permit = match std::mem::replace(
            &mut pending.stage,
            PendingInteractionSubmissionStage::Enqueued,
        ) {
            PendingInteractionSubmissionStage::Revalidating { permit } => permit,
            stage => {
                pending.stage = stage;
                self.pending_interaction_submission = Some(pending);
                return Err("interaction mutation arrived before its prefix settled".to_owned());
            }
        };
        match self.bind_and_enqueue_interaction(ticket, mutation, permit) {
            Ok(true) => {
                pending.stage = PendingInteractionSubmissionStage::Enqueued;
                self.pending_interaction_submission = Some(pending);
                cx.notify();
                Ok(())
            }
            Ok(false) => {
                self.settle_interaction_without_native(ticket, CommandSettlement::Noop);
                Ok(())
            }
            Err(error) => {
                self.settle_interaction_without_native(ticket, error.settlement());
                Err(error.message)
            }
        }
    }

    pub(super) fn process_one_ready_interaction_prefix(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((ticket, result)) = self.ready_interaction_prefixes.pop_front() else {
            return;
        };
        let Some(mut pending) = self.pending_interaction_submission.take() else {
            drop(result);
            return;
        };
        if pending.ticket != ticket {
            self.pending_interaction_submission = Some(pending);
            drop(result);
            return;
        }
        pending.prefix_task = None;
        pending.prefix_cancellation = None;
        let mutation = match std::mem::replace(
            &mut pending.stage,
            PendingInteractionSubmissionStage::Enqueued,
        ) {
            PendingInteractionSubmissionStage::WaitingForPrefix(mutation) => mutation,
            stage => {
                pending.stage = stage;
                self.pending_interaction_submission = Some(pending);
                drop(result);
                return;
            }
        };
        let permit = match result {
            Ok(permit) => permit,
            Err(error) => {
                self.settle_interaction_without_native(ticket, CommandSettlement::TransportFailure);
                self.record_prefix_barrier_failure(error);
                self.sync_native_connection_state(window, cx);
                return;
            }
        };

        match mutation {
            PendingInteractionMutation::NodeDrag(mutation) => {
                match self.bind_and_enqueue_interaction(ticket, mutation, permit) {
                    Ok(true) => {
                        pending.stage = PendingInteractionSubmissionStage::Enqueued;
                        self.pending_interaction_submission = Some(pending);
                        cx.notify();
                    }
                    Ok(false) => {
                        self.settle_interaction_without_native(ticket, CommandSettlement::Noop);
                    }
                    Err(error) => {
                        self.settle_interaction_without_native(ticket, error.settlement());
                        self.record_command_diagnostic(error.message);
                    }
                }
            }
            PendingInteractionMutation::Connection(kind) => {
                pending.stage = PendingInteractionSubmissionStage::Revalidating { permit };
                self.pending_interaction_submission = Some(pending);
                let update = match kind {
                    PendingConnectionMutation::Ordinary => self
                        .canvas_interaction
                        .request_connection_commit_revalidation(ticket),
                    PendingConnectionMutation::CreateFromWire => self
                        .view
                        .graph_snapshot()
                        .map_err(|_| CanvasInteractionError::ConnectionCommitRequiresFreshRevision)
                        .and_then(|snapshot| {
                            self.canvas_interaction
                                .refresh_create_from_wire_commit_revision(
                                    ticket,
                                    snapshot.revision(),
                                )
                        }),
                };
                match update {
                    Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
                    Err(error) => {
                        self.discard_pending_interaction_submission();
                        self.settle_interaction_without_native(
                            ticket,
                            CommandSettlement::TransportFailure,
                        );
                        self.record_command_diagnostic(format!(
                            "failed to refresh native interaction command: {error}"
                        ));
                        self.sync_native_connection_state(window, cx);
                    }
                }
            }
        }
    }

    fn bind_and_enqueue_interaction(
        &self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        permit: OpenGpuiCommandPrefixPermit,
    ) -> Result<bool, InteractionEnqueueFailure> {
        let snapshot = self
            .view
            .graph_snapshot()
            .map_err(|error| InteractionEnqueueFailure::transport(error.to_string()))?;
        let command = mutation
            .into_command(&snapshot)
            .map_err(|error| InteractionEnqueueFailure::conflict(error.to_string()))?;
        let Some(command) = command else {
            permit.abandon();
            return Ok(false);
        };
        permit
            .enqueue_detached(command, OpenGpuiCommandOrigin::Interaction { ticket })
            .map_err(|error| InteractionEnqueueFailure::transport(error.to_string()))?;
        Ok(true)
    }

    pub(super) fn settle_interaction_command(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) {
        if self
            .pending_interaction_submission
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket)
        {
            self.pending_interaction_submission = None;
        }
        self.settle_interaction_without_native(ticket, settlement);
    }

    fn settle_interaction_without_native(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) {
        match self.canvas_interaction.settle(ticket, settlement) {
            Ok(update) => self.queue_interaction_queries(update.into_parts().0),
            Err(error) => self.record_command_diagnostic(format!(
                "failed to settle native canvas interaction: {error}"
            )),
        }
    }

    pub(super) fn discard_pending_interaction_submission(&mut self) {
        self.pending_interaction_submission = None;
        self.ready_interaction_prefixes.clear();
    }

    #[cfg(test)]
    pub(super) fn has_pending_interaction_prefix_task(&self) -> bool {
        self.pending_interaction_submission
            .as_ref()
            .is_some_and(|pending| pending.prefix_task.is_some())
    }

    fn discard_orphaned_interaction_revalidation(&mut self) {
        let Some(pending) = self.pending_interaction_submission.as_ref() else {
            return;
        };
        if !matches!(
            pending.stage,
            PendingInteractionSubmissionStage::Revalidating { .. }
        ) {
            return;
        }
        let presentation = self.canvas_interaction.presentation();
        let active_ticket = presentation
            .connection()
            .and_then(|connection| connection.ticket());
        if presentation.phase() != CanvasInteractionPhase::Committing
            || active_ticket != Some(pending.ticket)
        {
            self.pending_interaction_submission = None;
        }
    }

    pub(super) fn retire_canvas_interaction_owner(&mut self, owner: InteractionOwner) {
        let update = self.canvas_interaction.owner_retired(owner);
        if self.canvas_interaction.presentation().terminal()
            == Some(InteractionTerminalReason::OwnerRetired)
        {
            self.discard_pending_interaction_submission();
        }
        self.queue_interaction_queries(update.into_parts().0);
    }

    fn record_prefix_barrier_failure(&mut self, error: OpenGpuiCommandPrefixBarrierError) {
        self.record_command_diagnostic(format!("native command prefix failed: {error}"));
    }

    fn apply_selection_mutation(
        &self,
        mutation: SelectionMutation,
    ) -> Result<(), jellyflow::runtime::ViewSessionError> {
        let snapshot = self.view.graph_snapshot()?;
        match mutation {
            SelectionMutation::Node { node, mode } => {
                if !snapshot
                    .graph()
                    .node(&node)
                    .is_some_and(|record| record.selectable.unwrap_or(true))
                {
                    return Ok(());
                }
                let (mut nodes, mut edges, mut groups) = self.view.selection()?.into_parts();
                if mode != SelectionMode::Replace || !nodes.contains(&node) {
                    apply_item_selection(&mut nodes, node, mode);
                }
                if mode == SelectionMode::Replace {
                    edges.clear();
                    groups.clear();
                }
                self.view.set_selection(nodes, edges, groups)
            }
            SelectionMutation::Edge { edge, mode } => {
                if snapshot.graph().edge(&edge).is_none() {
                    return Ok(());
                }
                let (mut nodes, mut edges, mut groups) = self.view.selection()?.into_parts();
                apply_item_selection(&mut edges, edge, mode);
                if mode == SelectionMode::Replace {
                    nodes.clear();
                    groups.clear();
                }
                self.view.set_selection(nodes, edges, groups)
            }
            SelectionMutation::Marquee {
                bounds,
                mode,
                baseline,
            } => {
                let mut intersecting = self.view.nodes_intersecting(bounds)?;
                intersecting.retain(|node| {
                    snapshot
                        .graph()
                        .node(node)
                        .is_some_and(|record| record.selectable.unwrap_or(true))
                });
                let (nodes, edges, groups) =
                    marquee_selection_from_baseline(&baseline, intersecting, mode);
                self.view.set_selection(nodes, edges, groups)
            }
        }
    }

    fn apply_viewport_mutation(
        &self,
        mutation: ViewportMutation,
    ) -> Result<(), jellyflow::runtime::ViewSessionError> {
        let transform = self.view.viewport_transform()?;
        match mutation {
            ViewportMutation::PanByView(delta) => {
                if let Some(next) = pan_viewport(transform, ViewportPanRequest::new(delta)) {
                    self.view.set_viewport(next.pan, next.zoom)?;
                }
            }
        }
        Ok(())
    }

    fn schedule_interaction_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.interaction_tick_scheduled {
            return;
        }
        self.interaction_tick_scheduled = true;
        let host = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.interaction_tick_scheduled = false;
                let Some(viewport) = host.surface_size else {
                    return;
                };
                if let Ok(update) = host
                    .canvas_interaction
                    .tick(INTERACTION_TICK_SECONDS, viewport)
                {
                    host.apply_canvas_interaction_update(update, window, cx);
                }
            });
        });
    }

    pub(super) fn interaction_node_drag_offsets(&self) -> BTreeMap<NodeId, CanvasPoint> {
        self.canvas_interaction
            .presentation()
            .node_drag()
            .map(|drag| {
                drag.nodes()
                    .iter()
                    .copied()
                    .map(|node| (node, drag.delta()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn interaction_node_drag_delta(&self, node: NodeId) -> Option<CanvasPoint> {
        self.canvas_interaction
            .presentation()
            .node_drag()
            .filter(|drag| drag.nodes().contains(&node))
            .map(|drag| drag.delta())
    }

    pub(super) fn interaction_marquee_view_bounds(
        &self,
        transform: ViewportTransform,
    ) -> Option<ViewRect> {
        let bounds = self.canvas_interaction.presentation().marquee()?;
        projected_view_rect(bounds, transform)
    }
}

struct InteractionEnqueueFailure {
    settlement: CommandSettlement,
    message: String,
}

impl InteractionEnqueueFailure {
    fn conflict(message: String) -> Self {
        Self {
            settlement: CommandSettlement::Conflict,
            message,
        }
    }

    fn transport(message: String) -> Self {
        Self {
            settlement: CommandSettlement::TransportFailure,
            message,
        }
    }

    fn settlement(&self) -> CommandSettlement {
        self.settlement
    }
}

fn apply_item_selection<T: Copy + Eq>(items: &mut Vec<T>, item: T, mode: SelectionMode) {
    match mode {
        SelectionMode::Replace => {
            items.clear();
            items.push(item);
        }
        SelectionMode::Add => {
            if !items.contains(&item) {
                items.push(item);
            }
        }
        SelectionMode::Toggle => {
            if let Some(index) = items.iter().position(|current| *current == item) {
                items.remove(index);
            } else {
                items.push(item);
            }
        }
    }
}

fn apply_many_selection<T: Copy + Ord>(
    items: &mut Vec<T>,
    incoming: impl IntoIterator<Item = T>,
    mode: SelectionMode,
) {
    if mode == SelectionMode::Replace {
        items.clear();
    }
    let mut positions = items
        .iter()
        .copied()
        .enumerate()
        .map(|(index, item)| (item, index))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = items.iter().copied().map(Some).collect::<Vec<_>>();
    for item in incoming {
        match mode {
            SelectionMode::Replace | SelectionMode::Add => {
                if let std::collections::btree_map::Entry::Vacant(entry) = positions.entry(item) {
                    entry.insert(ordered.len());
                    ordered.push(Some(item));
                }
            }
            SelectionMode::Toggle => {
                if let Some(index) = positions.remove(&item) {
                    ordered[index] = None;
                } else {
                    positions.insert(item, ordered.len());
                    ordered.push(Some(item));
                }
            }
        }
    }
    *items = ordered.into_iter().flatten().collect();
}

fn marquee_selection_from_baseline(
    baseline: &SelectionBaseline,
    intersecting: impl IntoIterator<Item = NodeId>,
    mode: SelectionMode,
) -> (Vec<NodeId>, Vec<EdgeId>, Vec<GroupId>) {
    let mut nodes = baseline.nodes().to_vec();
    let mut edges = baseline.edges().to_vec();
    let mut groups = baseline.groups().to_vec();
    apply_many_selection(&mut nodes, intersecting, mode);
    if mode == SelectionMode::Replace {
        edges.clear();
        groups.clear();
    }
    (nodes, edges, groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_selection_preserves_stable_order_and_duplicate_toggle_semantics() {
        let mut toggled = vec![1_u8, 2];
        apply_many_selection(&mut toggled, [2, 3, 3, 1, 1, 2], SelectionMode::Toggle);
        assert_eq!(toggled, vec![1, 2]);

        let mut replaced = vec![9_u8];
        apply_many_selection(&mut replaced, [3, 2, 3], SelectionMode::Replace);
        assert_eq!(replaced, vec![3, 2]);

        apply_many_selection(&mut replaced, [2, 4, 4], SelectionMode::Add);
        assert_eq!(replaced, vec![3, 2, 4]);
    }

    #[test]
    fn repeated_toggle_marquee_resolution_is_absolute_from_the_frozen_baseline() {
        let retained = NodeId::from_u128(1);
        let toggled_off = NodeId::from_u128(2);
        let toggled_on = NodeId::from_u128(3);
        let retained_edge = EdgeId::from_u128(4);
        let baseline = SelectionBaseline::new([retained, toggled_off], [retained_edge], []);

        let first = marquee_selection_from_baseline(
            &baseline,
            [toggled_off, toggled_on],
            SelectionMode::Toggle,
        );
        let second = marquee_selection_from_baseline(
            &baseline,
            [toggled_off, toggled_on],
            SelectionMode::Toggle,
        );

        assert_eq!(first, second);
        assert_eq!(first.0, vec![retained, toggled_on]);
        assert_eq!(first.1, vec![retained_edge]);
        assert!(first.2.is_empty());
    }
}
