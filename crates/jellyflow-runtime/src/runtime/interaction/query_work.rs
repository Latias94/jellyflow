use crate::runtime::view_session::{ConnectionProbeQuery, EdgePickQuery, NodeIntersectionQuery};

use super::navigation::AlignmentSnapQuery;
use super::session::{InteractionGeneration, InteractionQueryId};

#[derive(Debug, Default)]
pub(super) struct QueryWork {
    pending: Option<PendingInteractionQuery>,
    cleanup_pending: bool,
}

impl QueryWork {
    pub(super) fn replace(&mut self, query: PendingInteractionQuery) {
        self.pending = Some(query);
        self.cleanup_pending = false;
    }

    pub(super) fn take(&mut self) -> Option<PendingInteractionQuery> {
        self.pending.take()
    }

    pub(super) fn clear(&mut self) {
        self.pending = None;
        self.cleanup_pending = false;
    }

    pub(super) fn clear_if_id(&mut self, id: InteractionQueryId) {
        if self.pending.as_ref().is_some_and(|query| query.id() == id) {
            self.pending = None;
        }
    }

    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some() || self.cleanup_pending
    }

    pub(super) fn cleanup_pending(&self) -> bool {
        self.cleanup_pending
    }

    pub(super) fn require_cleanup(&mut self) {
        self.pending = None;
        self.cleanup_pending = true;
    }

    #[cfg(test)]
    pub(super) fn pending_for_test(&self) -> Option<&PendingInteractionQuery> {
        self.pending.as_ref()
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
