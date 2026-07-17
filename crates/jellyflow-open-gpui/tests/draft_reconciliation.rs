use jellyflow_open_gpui::draft::DraftSubmissionRejectionDecision;
use jellyflow_open_gpui::{
    DraftConflict, DraftConflictResolution, DraftIntentRejection, DraftPhase, DraftReceiptDecision,
    DraftReconcileDecision, DraftResolutionDecision, DraftStaleRejectionDecision,
    DraftSubmissionDecision, DraftSubmissionKind, RevisionedDraft,
};

#[test]
fn clean_draft_applies_a_new_remote_snapshot_directly() {
    let mut draft = RevisionedDraft::new(4, String::from("before"));

    assert_eq!(draft.phase(), DraftPhase::Clean);
    assert_eq!(
        draft.reconcile_remote(5, String::from("after")),
        DraftReconcileDecision::RemoteApplied {
            from_revision: 4,
            to_revision: 5,
        }
    );
    assert_eq!(draft.local(), "after");
    assert_eq!(draft.remote(), "after");
    assert_eq!(draft.base_revision(), 5);
    assert_eq!(draft.remote_revision(), 5);
    assert_eq!(draft.phase(), DraftPhase::Clean);
}

#[test]
fn dirty_and_composing_drafts_preserve_local_input_on_new_remote_revisions() {
    let mut dirty = RevisionedDraft::new(10, String::from("base"));
    dirty.edit_local(String::from("local edit"));

    assert_eq!(
        dirty.reconcile_remote(11, String::from("other view")),
        DraftReconcileDecision::Conflict {
            conflict: DraftConflict::new(10, 11),
        }
    );
    assert_eq!(dirty.local(), "local edit");
    assert_eq!(dirty.remote(), "other view");
    assert_eq!(dirty.phase(), DraftPhase::Conflict);

    let mut composing = RevisionedDraft::new(20, String::from("base"));
    composing.mark_composing();
    composing.edit_local(String::from("partial composition"));

    assert_eq!(
        composing.reconcile_remote(21, String::from("new remote value")),
        DraftReconcileDecision::Conflict {
            conflict: DraftConflict::new(20, 21),
        }
    );
    assert_eq!(composing.local(), "partial composition");
    assert_eq!(composing.remote(), "new remote value");
    assert_eq!(composing.phase(), DraftPhase::ComposingConflict);
}

#[test]
fn an_unchanged_field_advances_the_base_without_conflicting_with_a_dirty_draft() {
    let mut draft = RevisionedDraft::new(30, String::from("remote"));
    draft.edit_local(String::from("local"));

    assert_eq!(
        draft.reconcile_remote(31, String::from("remote")),
        DraftReconcileDecision::BaseAdvanced {
            from_revision: 30,
            to_revision: 31,
            phase: DraftPhase::Dirty,
        }
    );
    assert_eq!(draft.base_revision(), 31);
    assert_eq!(draft.local(), "local");
    assert_eq!(draft.phase(), DraftPhase::Dirty);
}

#[test]
fn editing_back_to_the_authoritative_value_restores_a_clean_draft() {
    let mut draft = RevisionedDraft::new(34, String::from("remote"));

    assert!(matches!(
        draft.edit_local(String::from("local")),
        jellyflow_open_gpui::DraftEditDecision::Updated {
            phase: DraftPhase::Dirty
        }
    ));
    assert_eq!(
        draft.edit_local(String::from("remote")),
        jellyflow_open_gpui::DraftEditDecision::Updated {
            phase: DraftPhase::Clean,
        }
    );
    assert_eq!(
        draft.plan_submission_at(34),
        DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean)
    );
}

#[test]
fn editing_a_conflicted_draft_back_to_the_authoritative_value_resolves_the_conflict() {
    let mut draft = conflicting_draft();

    assert_eq!(
        draft.edit_local(String::from("remote")),
        jellyflow_open_gpui::DraftEditDecision::Updated {
            phase: DraftPhase::Clean,
        }
    );
    assert_eq!(draft.base_revision(), 71);
    assert_eq!(draft.conflict(), None);
    assert_eq!(
        draft.plan_submission_at(71),
        DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean)
    );
}

#[test]
fn submission_requires_the_latest_authoritative_field_observation() {
    let mut draft = RevisionedDraft::new(35, String::from("remote"));
    draft.edit_local(String::from("local"));
    assert!(matches!(
        draft.reconcile_remote(36, String::from("remote")),
        DraftReconcileDecision::BaseAdvanced { .. }
    ));

    assert_eq!(
        draft.plan_submission_at(35),
        DraftSubmissionDecision::Rejected(DraftIntentRejection::UnreconciledRevision {
            observed_revision: 36,
            requested_revision: 35,
        })
    );
    assert_eq!(draft.pending_attempt(), None);

    let intent = submission(draft.plan_submission_at(36));
    assert_eq!(intent.attempt().get(), 1);
    assert_eq!(intent.expected_revision(), 36);
    assert_eq!(intent.value(), "local");
}

#[test]
fn self_receipt_and_matching_snapshot_acknowledge_the_submitted_draft() {
    let mut draft = RevisionedDraft::new(40, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(40));

    assert_eq!(intent.kind(), DraftSubmissionKind::Commit);
    assert_eq!(intent.expected_revision(), 40);
    assert_eq!(intent.value(), "submitted");
    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 41),
        DraftReceiptDecision::Recorded {
            attempt: intent.attempt(),
            committed_revision: 41,
        }
    );
    assert_eq!(draft.base_revision(), 41);
    assert_eq!(draft.local(), "submitted");
    assert_eq!(draft.phase(), DraftPhase::Dirty);

    assert_eq!(
        draft.reconcile_remote(41, String::from("submitted")),
        DraftReconcileDecision::SelfAcknowledged {
            attempt: intent.attempt(),
            revision: 41,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(draft.local(), "submitted");
    assert_eq!(draft.remote(), "submitted");
    assert_eq!(draft.phase(), DraftPhase::Clean);
    assert_eq!(draft.pending_attempt(), None);
}

#[test]
fn receipt_is_acknowledged_by_a_later_matching_authoritative_observation() {
    let mut draft = RevisionedDraft::new(10, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(10));

    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 11),
        DraftReceiptDecision::Recorded {
            attempt: intent.attempt(),
            committed_revision: 11,
        }
    );
    assert_eq!(
        draft.reconcile_remote(12, String::from("submitted")),
        DraftReconcileDecision::SelfAcknowledged {
            attempt: intent.attempt(),
            revision: 12,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(draft.base_revision(), 12);
    assert_eq!(draft.remote_revision(), 12);
    assert_eq!(draft.phase(), DraftPhase::Clean);
}

#[test]
fn a_snapshot_observed_before_its_receipt_is_acknowledged_only_after_both_exist() {
    let mut draft = RevisionedDraft::new(50, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(50));

    assert_eq!(
        draft.reconcile_remote(51, String::from("submitted")),
        DraftReconcileDecision::Conflict {
            conflict: DraftConflict::new(50, 51),
        },
        "a matching value is not a self acknowledgement without its receipt"
    );
    assert_eq!(draft.phase(), DraftPhase::Conflict);

    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 51),
        DraftReceiptDecision::Acknowledged {
            attempt: intent.attempt(),
            revision: 51,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(draft.phase(), DraftPhase::Clean);
}

#[test]
fn later_matching_observation_is_acknowledged_when_its_receipt_arrives_late() {
    let mut draft = RevisionedDraft::new(10, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(10));

    assert_eq!(
        draft.reconcile_remote(12, String::from("submitted")),
        DraftReconcileDecision::Conflict {
            conflict: DraftConflict::new(10, 12),
        },
        "a matching value remains untrusted until its attempt receipt arrives"
    );
    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 11),
        DraftReceiptDecision::Acknowledged {
            attempt: intent.attempt(),
            revision: 12,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(draft.base_revision(), 12);
    assert_eq!(draft.remote_revision(), 12);
    assert_eq!(draft.phase(), DraftPhase::Clean);
}

#[test]
fn late_receipt_does_not_acknowledge_a_newer_mismatching_observation() {
    let mut draft = RevisionedDraft::new(10, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(10));

    assert_eq!(
        draft.reconcile_remote(12, String::from("other writer")),
        DraftReconcileDecision::Conflict {
            conflict: DraftConflict::new(10, 12),
        }
    );
    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 11),
        DraftReceiptDecision::Superseded {
            attempt: intent.attempt(),
            committed_revision: 11,
            current_revision: 12,
        }
    );
    assert_eq!(draft.pending_attempt(), None);
    assert_eq!(draft.local(), "submitted");
    assert_eq!(draft.remote(), "other writer");
    assert_eq!(draft.conflict(), Some(DraftConflict::new(10, 12)));
    assert_eq!(draft.phase(), DraftPhase::Conflict);
}

#[test]
fn input_after_submission_is_not_cleared_by_the_old_acknowledgement() {
    let mut draft = RevisionedDraft::new(60, String::from("base"));
    draft.edit_local(String::from("first submission"));
    let first = submission(draft.plan_submission_at(60));
    assert!(matches!(
        draft.record_self_receipt(first.attempt(), 61),
        DraftReceiptDecision::Recorded { .. }
    ));

    draft.edit_local(String::from("typed after submit"));
    assert_eq!(
        draft.reconcile_remote(61, String::from("first submission")),
        DraftReconcileDecision::SelfAcknowledged {
            attempt: first.attempt(),
            revision: 61,
            local_changed_after_submission: true,
        }
    );
    assert_eq!(draft.local(), "typed after submit");
    assert_eq!(draft.remote(), "first submission");
    assert_eq!(draft.base_revision(), 61);
    assert_eq!(draft.phase(), DraftPhase::Dirty);

    let second = submission(draft.plan_submission_at(61));
    assert_eq!(second.expected_revision(), 61);
    assert_eq!(second.value(), "typed after submit");
}

#[test]
fn input_after_submission_that_returns_to_the_committed_value_acknowledges_cleanly() {
    let mut draft = RevisionedDraft::new(62, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(62));
    draft.edit_local(String::from("temporary"));
    draft.edit_local(String::from("submitted"));

    assert!(matches!(
        draft.record_self_receipt(intent.attempt(), 63),
        DraftReceiptDecision::Recorded { .. }
    ));
    assert!(matches!(
        draft.reconcile_remote(63, String::from("submitted")),
        DraftReconcileDecision::SelfAcknowledged {
            local_changed_after_submission: true,
            ..
        }
    ));
    assert_eq!(draft.phase(), DraftPhase::Clean);
    assert_eq!(
        draft.plan_submission_at(63),
        DraftSubmissionDecision::Rejected(DraftIntentRejection::Clean)
    );
}

#[test]
fn accepted_submission_converges_when_a_newer_remote_matches_post_submission_input() {
    let mut draft = RevisionedDraft::new(64, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(64));
    assert!(matches!(
        draft.record_self_receipt(intent.attempt(), 65),
        DraftReceiptDecision::Recorded { .. }
    ));
    draft.edit_local(String::from("newer"));

    assert_eq!(
        draft.reconcile_remote(66, String::from("newer")),
        DraftReconcileDecision::RemoteApplied {
            from_revision: 64,
            to_revision: 66,
        }
    );
    assert_eq!(draft.phase(), DraftPhase::Clean);
    assert_eq!(draft.base_revision(), 66);
    assert_eq!(draft.pending_attempt(), None);
}

#[test]
fn explicit_submission_rejection_preserves_dirty_state_without_retrying() {
    let mut draft = RevisionedDraft::new(65, String::from("base"));
    draft.edit_local(String::from("local"));
    let first = submission(draft.plan_submission_at(65));

    assert_eq!(
        draft.reject_submission(first.attempt()),
        DraftSubmissionRejectionDecision::Rejected {
            attempt: first.attempt(),
            phase: DraftPhase::Dirty,
        }
    );
    assert_eq!(draft.pending_attempt(), None);
    assert_eq!(draft.local(), "local");
    assert_eq!(draft.remote(), "base");
    assert_eq!(draft.phase(), DraftPhase::Dirty);

    let second = submission(draft.plan_submission_at(65));
    assert_ne!(first.attempt(), second.attempt());
    assert_eq!(second.attempt().get(), 2);
}

#[test]
fn rejected_submission_that_returned_to_the_remote_value_clears_dirty_state() {
    let mut draft = RevisionedDraft::new(68, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(68));
    draft.edit_local(String::from("base"));

    assert_eq!(
        draft.reject_submission(intent.attempt()),
        DraftSubmissionRejectionDecision::Rejected {
            attempt: intent.attempt(),
            phase: DraftPhase::Clean,
        }
    );
    assert_eq!(draft.base_revision(), 68);
    assert_eq!(draft.conflict(), None);
    assert_eq!(draft.pending_attempt(), None);
}

#[test]
fn submission_rejection_cannot_cancel_an_attempt_with_an_accepted_receipt() {
    let mut draft = RevisionedDraft::new(66, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(66));

    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 67),
        DraftReceiptDecision::Recorded {
            attempt: intent.attempt(),
            committed_revision: 67,
        }
    );
    assert_eq!(
        draft.reject_submission(intent.attempt()),
        DraftSubmissionRejectionDecision::ReceiptAlreadyAccepted {
            attempt: intent.attempt(),
            committed_revision: 67,
        }
    );
    assert_eq!(draft.pending_attempt(), Some(intent.attempt()));
    assert_eq!(
        draft.reconcile_remote(67, String::from("submitted")),
        DraftReconcileDecision::SelfAcknowledged {
            attempt: intent.attempt(),
            revision: 67,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(draft.phase(), DraftPhase::Clean);
}

#[test]
fn explicit_reapply_rejection_preserves_the_conflict_without_retrying() {
    let mut draft = conflicting_draft();
    let first = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 71) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a reapply intent, got {other:?}"),
    };

    assert_eq!(
        draft.reject_submission(first.attempt()),
        DraftSubmissionRejectionDecision::Rejected {
            attempt: first.attempt(),
            phase: DraftPhase::Conflict,
        }
    );
    assert_eq!(draft.pending_attempt(), None);
    assert_eq!(draft.local(), "local");
    assert_eq!(draft.remote(), "remote");
    assert_eq!(draft.conflict(), Some(DraftConflict::new(70, 71)));

    let second = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 71) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a second explicit reapply intent, got {other:?}"),
    };
    assert_eq!(second.attempt().get(), 2);
}

#[test]
fn conflict_resolutions_accept_remote_keep_local_or_plan_an_explicit_reapply() {
    let mut accept_remote = conflicting_draft();
    assert_eq!(
        accept_remote.resolve_at(DraftConflictResolution::AcceptRemote, 71),
        DraftResolutionDecision::AcceptedRemote { revision: 71 }
    );
    assert_eq!(accept_remote.local(), "remote");
    assert_eq!(accept_remote.phase(), DraftPhase::Clean);

    let mut keep_local = conflicting_draft();
    assert_eq!(
        keep_local.resolve_at(DraftConflictResolution::KeepLocal, 71),
        DraftResolutionDecision::KeptLocal {
            conflict: DraftConflict::new(70, 71),
        }
    );
    assert_eq!(keep_local.local(), "local");
    assert_eq!(keep_local.remote(), "remote");
    assert_eq!(keep_local.phase(), DraftPhase::Conflict);

    let mut reapply_local = conflicting_draft();
    let intent = match reapply_local.resolve_at(DraftConflictResolution::ReapplyLocal, 71) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a reapply intent, got {other:?}"),
    };
    assert_eq!(intent.kind(), DraftSubmissionKind::Reapply);
    assert_eq!(intent.expected_revision(), 71);
    assert_eq!(intent.value(), "local");
    assert_eq!(reapply_local.phase(), DraftPhase::Conflict);
}

#[test]
fn reapply_requires_the_latest_authoritative_field_observation() {
    let mut draft = conflicting_draft();

    assert_eq!(
        draft.resolve_at(DraftConflictResolution::ReapplyLocal, 70),
        DraftResolutionDecision::Rejected(DraftIntentRejection::UnreconciledRevision {
            observed_revision: 71,
            requested_revision: 70,
        })
    );
    assert_eq!(draft.pending_attempt(), None);

    let intent = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 71) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a revision-current reapply intent, got {other:?}"),
    };
    assert_eq!(intent.attempt().get(), 1);
    assert_eq!(intent.expected_revision(), 71);
}

#[test]
fn composition_blocks_reapply_until_unmarked_and_uses_the_latest_revision() {
    let mut draft = RevisionedDraft::new(80, String::from("base"));
    draft.mark_composing();
    draft.edit_local(String::from("composed local"));
    draft.reconcile_remote(81, String::from("first remote"));
    draft.reconcile_remote(82, String::from("latest remote"));

    assert_eq!(
        draft.resolve_at(DraftConflictResolution::ReapplyLocal, 82),
        DraftResolutionDecision::Rejected(DraftIntentRejection::CompositionActive)
    );
    assert_eq!(draft.pending_attempt(), None);
    assert_eq!(draft.local(), "composed local");

    draft.unmark_composing();
    let intent = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 82) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a reapply intent, got {other:?}"),
    };
    assert_eq!(intent.expected_revision(), 82);
    assert_eq!(intent.value(), "composed local");
}

#[test]
fn stale_reapply_rejection_preserves_conflict_and_never_retries_implicitly() {
    let mut draft = conflicting_draft();
    let first = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 71) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("expected a reapply intent, got {other:?}"),
    };

    assert_eq!(
        draft.reject_stale(first.attempt(), 72, String::from("newer remote")),
        DraftStaleRejectionDecision::ConflictPreserved {
            conflict: DraftConflict::new(70, 72),
        }
    );
    assert_eq!(draft.local(), "local");
    assert_eq!(draft.remote(), "newer remote");
    assert_eq!(draft.phase(), DraftPhase::Conflict);
    assert_eq!(draft.pending_attempt(), None);

    let second = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 72) {
        DraftResolutionDecision::Reapply(intent) => intent,
        other => panic!("an explicit second reapply should plan a new intent: {other:?}"),
    };
    assert_ne!(first.attempt(), second.attempt());
    assert_eq!(second.expected_revision(), 72);
    assert_eq!(second.value(), "local");
}

#[test]
fn stale_rejection_converges_when_the_current_value_matches_local_input() {
    let mut draft = RevisionedDraft::new(73, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(73));
    draft.edit_local(String::from("current"));

    assert_eq!(
        draft.reject_stale(intent.attempt(), 74, String::from("current")),
        DraftStaleRejectionDecision::LocalMatchesRemote {
            revision: 74,
            phase: DraftPhase::Clean,
        }
    );
    assert_eq!(draft.local(), "current");
    assert_eq!(draft.remote(), "current");
    assert_eq!(draft.base_revision(), 74);
    assert_eq!(draft.conflict(), None);
}

#[test]
fn superseded_receipt_converges_when_the_newer_remote_matches_post_submission_input() {
    let mut draft = RevisionedDraft::new(75, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let intent = submission(draft.plan_submission_at(75));
    draft.edit_local(String::from("newer"));

    assert!(matches!(
        draft.reconcile_remote(77, String::from("newer")),
        DraftReconcileDecision::Conflict { .. }
    ));
    assert_eq!(
        draft.record_self_receipt(intent.attempt(), 76),
        DraftReceiptDecision::Superseded {
            attempt: intent.attempt(),
            committed_revision: 76,
            current_revision: 77,
        }
    );
    assert_eq!(draft.phase(), DraftPhase::Clean);
    assert_eq!(draft.base_revision(), 77);
    assert_eq!(draft.conflict(), None);
}

fn conflicting_draft() -> RevisionedDraft<String> {
    let mut draft = RevisionedDraft::new(70, String::from("base"));
    draft.edit_local(String::from("local"));
    assert!(matches!(
        draft.reconcile_remote(71, String::from("remote")),
        DraftReconcileDecision::Conflict { .. }
    ));
    draft
}

fn submission(
    decision: DraftSubmissionDecision<String>,
) -> jellyflow_open_gpui::DraftSubmissionIntent<String> {
    match decision {
        DraftSubmissionDecision::Submit(intent) => intent,
        other => panic!("expected a submission intent, got {other:?}"),
    }
}
