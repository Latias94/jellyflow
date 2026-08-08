//! Pure reconciliation reducers shared by product-owned authoring surfaces.
//!
//! The reducers deliberately know nothing about tasks, windows, focus, rendering, or native
//! event types.  They consume runtime completion evidence and authoritative observations while
//! leaving dispatch and lifecycle ownership to the adapter that initiated the work.

mod field;
mod mutation;

pub use field::{
    FieldAttempt, FieldCompositionDecision, FieldConflict, FieldConflictResolution,
    FieldEditDecision, FieldExternalization, FieldIntentRejection, FieldPhase,
    FieldReceiptDecision, FieldReconcileDecision, FieldReconciler, FieldRecovery,
    FieldResolutionDecision, FieldRestore, FieldStaleRejectionDecision, FieldSubmissionDecision,
    FieldSubmissionIntent, FieldSubmissionKind, FieldSubmissionRejectionDecision,
};
pub use mutation::{
    MutationAbandonment, MutationAttemptId, MutationBeginError, MutationCommandResult,
    MutationCompletionError, MutationPhase, MutationReconciler, MutationRetry, MutationRetryKind,
    MutationRetryReason, MutationTransition,
};
