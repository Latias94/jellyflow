use crate::runtime::view_session::{ConnectionProbeQuery, EdgePickQuery, NodeIntersectionQuery};

use super::navigation::AlignmentSnapQuery;
use super::session::{InteractionGeneration, InteractionQueryId};

#[derive(Debug, Default)]
pub(super) struct QueryWork {
    state: QueryWorkState,
}

#[derive(Debug, Default)]
// Keeping the query inline avoids a heap allocation on every pointer-driven replacement.
#[allow(clippy::large_enum_variant)]
enum QueryWorkState {
    #[default]
    Empty,
    Pending(PendingInteractionQuery),
    CleanupPending,
}

impl QueryWork {
    pub(super) fn replace(&mut self, query: PendingInteractionQuery) {
        let can_replace = !matches!(self.state, QueryWorkState::CleanupPending);
        debug_assert!(
            can_replace,
            "terminal query cleanup must complete before replacement work is queued"
        );
        if !can_replace {
            return;
        }
        self.state = QueryWorkState::Pending(query);
    }

    pub(super) fn take(&mut self) -> Option<PendingInteractionQuery> {
        match std::mem::take(&mut self.state) {
            QueryWorkState::Pending(query) => Some(query),
            QueryWorkState::CleanupPending => {
                self.state = QueryWorkState::CleanupPending;
                None
            }
            QueryWorkState::Empty => None,
        }
    }

    pub(super) fn clear(&mut self) {
        self.state = QueryWorkState::Empty;
    }

    pub(super) fn clear_if_id(&mut self, id: InteractionQueryId) {
        if matches!(
            &self.state,
            QueryWorkState::Pending(query) if query.id() == id
        ) {
            self.state = QueryWorkState::Empty;
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        !matches!(self.state, QueryWorkState::Empty)
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        matches!(self.state, QueryWorkState::CleanupPending)
    }

    pub(super) fn require_cleanup(&mut self) {
        self.state = QueryWorkState::CleanupPending;
    }

    #[cfg(test)]
    pub(super) fn pending_for_test(&self) -> Option<&PendingInteractionQuery> {
        match &self.state {
            QueryWorkState::Pending(query) => Some(query),
            QueryWorkState::Empty | QueryWorkState::CleanupPending => None,
        }
    }
}

/// One runtime-private query continuation.
///
/// Replacing this value atomically supersedes the previous cursor. Native adapters only grant
/// advancement opportunities and never transport this payload.
#[derive(Debug)]
pub(super) struct PendingInteractionQuery {
    pub(super) kind: PendingInteractionQueryKind,
}

#[derive(Debug)]
pub(super) enum PendingInteractionQueryKind {
    AlignmentSnap {
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: AlignmentSnapQuery,
    },
    ConnectionProbe {
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: ConnectionProbeQuery,
    },
    EdgePick {
        id: InteractionQueryId,
        query: EdgePickQuery,
    },
    NodeIntersection {
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: NodeIntersectionQuery,
    },
}

impl PendingInteractionQuery {
    pub(super) fn alignment_snap(
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: AlignmentSnapQuery,
    ) -> Self {
        Self {
            kind: PendingInteractionQueryKind::AlignmentSnap {
                id,
                generation,
                query,
            },
        }
    }

    pub(super) fn connection_probe(
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: ConnectionProbeQuery,
    ) -> Self {
        Self {
            kind: PendingInteractionQueryKind::ConnectionProbe {
                id,
                generation,
                query,
            },
        }
    }

    pub(super) fn edge_pick(id: InteractionQueryId, query: EdgePickQuery) -> Self {
        Self {
            kind: PendingInteractionQueryKind::EdgePick { id, query },
        }
    }

    pub(super) fn node_intersection(
        id: InteractionQueryId,
        generation: InteractionGeneration,
        query: NodeIntersectionQuery,
    ) -> Self {
        Self {
            kind: PendingInteractionQueryKind::NodeIntersection {
                id,
                generation,
                query,
            },
        }
    }

    pub(super) fn id(&self) -> InteractionQueryId {
        match &self.kind {
            PendingInteractionQueryKind::AlignmentSnap { id, .. }
            | PendingInteractionQueryKind::ConnectionProbe { id, .. }
            | PendingInteractionQueryKind::EdgePick { id, .. }
            | PendingInteractionQueryKind::NodeIntersection { id, .. } => *id,
        }
    }
}
