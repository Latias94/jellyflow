use jellyflow_core::{GraphBuilder, GraphId, Node, NodeId, NodeKindKey};
use jellyflow_open_gpui::{
    OpenGpuiCommandTicketError, RetainedFieldAttemptState, RetainedFieldConflictChoice,
    RetainedFieldError, RetainedFieldPhase, RetainedFieldResolution, RetainedFieldSettlement,
    RetainedFieldSubmitMode, RetainedFieldUpdate, RevisionedField,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{GraphCommand, GraphCommandOutcome, GraphRuntime};

const KIND: &str = "test.revisioned-field";
const RENDERER: &str = "test.revisioned.field";

#[test]
fn authoritative_observations_distinguish_clean_dirty_and_composing_fields() {
    let mut clean = RevisionedField::new(4, String::from("before"));

    assert_eq!(clean.phase(), RetainedFieldPhase::Clean);
    assert_eq!(
        clean.observe(5, String::from("after")),
        RetainedFieldUpdate::RemoteApplied
    );
    assert_eq!(clean.local(), "after");
    assert_eq!(clean.remote(), "after");
    assert_eq!(clean.base_revision(), 5);
    assert_eq!(clean.remote_revision(), 5);

    clean.edit(String::from("local edit"));
    assert_eq!(clean.phase(), RetainedFieldPhase::Dirty);
    assert_eq!(
        clean.observe(6, String::from("other view")),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(clean.local(), "local edit");
    assert_eq!(clean.remote(), "other view");
    assert_eq!(clean.phase(), RetainedFieldPhase::Conflict);
    let conflict = clean.conflict().expect("revision conflict");
    assert_eq!(conflict.base_revision(), 5);
    assert_eq!(conflict.remote_revision(), 6);

    let mut composing = RevisionedField::new(20, String::from("base"));
    composing.set_composing(true);
    composing.edit(String::from("partial composition"));
    assert_eq!(composing.phase(), RetainedFieldPhase::Composing);
    assert_eq!(
        composing.observe(21, String::from("new remote value")),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(composing.local(), "partial composition");
    assert_eq!(composing.remote(), "new remote value");
    assert_eq!(composing.phase(), RetainedFieldPhase::ComposingConflict);
}

#[test]
fn unchanged_authoritative_value_advances_a_dirty_fields_base() {
    let mut field = RevisionedField::new(30, String::from("remote"));
    field.edit(String::from("local"));

    assert_eq!(
        field.observe(31, String::from("remote")),
        RetainedFieldUpdate::BaseAdvanced
    );
    assert_eq!(field.base_revision(), 31);
    assert_eq!(field.local(), "local");
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    field.edit(String::from("remote"));
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert!(matches!(field.plan(31), Err(RetainedFieldError::Clean)));
}

#[test]
fn conflicts_require_an_explicit_accept_keep_or_reapply_choice() {
    let mut accepted = conflicting_field();
    assert!(matches!(
        accepted
            .resolve(RetainedFieldConflictChoice::AcceptRemote, 71)
            .expect("accept authoritative value"),
        RetainedFieldResolution::AcceptedRemote { revision: 71 }
    ));
    assert_eq!(accepted.local(), "remote");
    assert_eq!(accepted.phase(), RetainedFieldPhase::Clean);

    let mut kept = conflicting_field();
    let conflict = kept.conflict().expect("conflicting field");
    assert!(matches!(
        kept.resolve(RetainedFieldConflictChoice::KeepLocal, 71)
            .expect("keep local value"),
        RetainedFieldResolution::KeptLocal { conflict: actual } if actual == conflict
    ));
    assert_eq!(kept.local(), "local");
    assert_eq!(kept.phase(), RetainedFieldPhase::Conflict);

    let mut reapplied = conflicting_field();
    let intent = match reapplied
        .resolve(RetainedFieldConflictChoice::ReapplyLocal, 71)
        .expect("explicit reapply intent")
    {
        RetainedFieldResolution::Reapply(intent) => intent,
        resolution => panic!("expected reapply intent, got {resolution:?}"),
    };
    assert_eq!(intent.mode(), RetainedFieldSubmitMode::Reapply);
    assert_eq!(intent.expected_revision(), 71);
    assert_eq!(intent.value(), "local");
    assert!(matches!(
        reapplied.plan(71),
        Err(RetainedFieldError::AttemptPending { .. })
    ));

    let mut composing = conflicting_field();
    composing.set_composing(true);
    assert!(matches!(
        composing.resolve(RetainedFieldConflictChoice::ReapplyLocal, 71),
        Err(RetainedFieldError::CompositionActive)
    ));
}

#[test]
fn planning_requires_the_latest_authoritative_revision() {
    let mut field = RevisionedField::new(35, String::from("remote"));
    field.edit(String::from("local"));
    assert_eq!(
        field.observe(36, String::from("remote")),
        RetainedFieldUpdate::BaseAdvanced
    );

    assert!(matches!(
        field.plan(35),
        Err(RetainedFieldError::RevisionMismatch {
            observed_revision: 36,
            requested_revision: 35,
        })
    ));
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);

    let intent = field.plan(36).expect("fresh submission intent");
    assert_eq!(intent.attempt().get(), 1);
    assert_eq!(intent.expected_revision(), 36);
    assert_eq!(intent.value(), "local");
    field
        .not_dispatched(intent.attempt())
        .expect("definitely undispatched intent can be cleared");
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
    assert!(field.can_submit());
}

#[test]
fn receipt_first_waits_for_a_matching_authoritative_snapshot() {
    let (mut field, attempt) = planned_field();

    assert_eq!(
        field.settle_ticket(attempt, Ok(applied_outcome())),
        RetainedFieldSettlement::AwaitingSnapshot {
            attempt,
            committed_revision: 1,
        }
    );
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::ReceiptFirst {
            attempt,
            committed_revision: 1,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    assert_eq!(
        field.observe(1, String::from("submitted")),
        RetainedFieldUpdate::Acknowledged {
            attempt,
            revision: 1,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
}

#[test]
fn snapshot_first_stays_dirty_until_its_matching_receipt_arrives() {
    let (mut field, attempt) = planned_field();

    assert_eq!(
        field.observe(1, String::from("submitted")),
        RetainedFieldUpdate::SnapshotFirst {
            attempt,
            revision: 1,
        }
    );
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::SnapshotFirst {
            attempt,
            observed_revision: 1,
        }
    );
    assert_eq!(field.conflict(), None);
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    assert_eq!(
        field.settle_ticket(attempt, Ok(applied_outcome())),
        RetainedFieldSettlement::Acknowledged {
            attempt,
            revision: 1,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
}

#[test]
fn noop_uses_the_same_receipt_and_snapshot_evidence_as_an_applied_command() {
    let (mut field, attempt) = planned_field();

    assert_eq!(
        field.settle_ticket(attempt, Ok(noop_outcome_at_revision_one())),
        RetainedFieldSettlement::AwaitingSnapshot {
            attempt,
            committed_revision: 1,
        }
    );
    assert_eq!(
        field.observe(1, String::from("submitted")),
        RetainedFieldUpdate::Acknowledged {
            attempt,
            revision: 1,
            local_changed_after_submission: false,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
}

#[test]
fn acknowledgement_preserves_an_edit_made_after_submission() {
    let (mut field, attempt) = planned_field();
    field.edit(String::from("newer local"));

    assert!(matches!(
        field.settle_ticket(attempt, Ok(applied_outcome())),
        RetainedFieldSettlement::AwaitingSnapshot { .. }
    ));
    assert_eq!(
        field.observe(1, String::from("submitted")),
        RetainedFieldUpdate::Acknowledged {
            attempt,
            revision: 1,
            local_changed_after_submission: true,
        }
    );
    assert_eq!(field.local(), "newer local");
    assert_eq!(field.remote(), "submitted");
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
}

#[test]
fn conflict_and_rejection_outcomes_require_an_authoritative_refresh() {
    let (mut conflicted, conflict_attempt) = planned_field();
    let settlement = conflicted.settle_ticket(conflict_attempt, Ok(conflict_outcome()));
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision: 1,
            ..
        } if attempt == conflict_attempt
    ));
    assert_eq!(
        conflicted.attempt_state(),
        RetainedFieldAttemptState::RefreshRequired {
            attempt: Some(conflict_attempt),
            minimum_revision: 1,
        }
    );
    assert!(matches!(
        conflicted.plan(0),
        Err(RetainedFieldError::AttemptPending { .. })
    ));
    assert_eq!(
        conflicted.observe(1, String::from("other writer")),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(conflicted.attempt_state(), RetainedFieldAttemptState::Idle);
    assert_eq!(conflicted.phase(), RetainedFieldPhase::Conflict);

    let (mut rejected, rejected_attempt) = planned_field();
    let settlement = rejected.settle_ticket(rejected_attempt, Ok(rejected_outcome()));
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision: 0,
            ..
        } if attempt == rejected_attempt
    ));
    assert!(matches!(
        rejected.plan(0),
        Err(RetainedFieldError::AttemptPending { .. })
    ));
    assert_eq!(
        rejected.observe(0, String::from("base")),
        RetainedFieldUpdate::Unchanged
    );
    assert_eq!(rejected.attempt_state(), RetainedFieldAttemptState::Idle);
    assert_eq!(rejected.phase(), RetainedFieldPhase::Dirty);
}

#[test]
fn transport_failure_is_uncertain_and_never_implicitly_retried() {
    let (mut field, attempt) = planned_field();

    let settlement = field.settle_ticket(
        attempt,
        Err(OpenGpuiCommandTicketError::MissingReceipt { operation_index: 0 }),
    );
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::Uncertain {
            attempt: uncertain_attempt,
            ..
        } if uncertain_attempt == attempt
    ));
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::Uncertain {
            attempt,
            expected_revision: 0,
        }
    );
    assert!(field.notice().is_some());
    assert!(matches!(
        field.plan(0),
        Err(RetainedFieldError::AttemptPending { .. })
    ));

    assert_eq!(
        field.observe(0, String::from("base")),
        RetainedFieldUpdate::Unchanged
    );
    assert!(matches!(
        field.attempt_state(),
        RetainedFieldAttemptState::Uncertain { .. }
    ));
    assert_eq!(
        field.observe(1, String::from("submitted")),
        RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision: 1,
            submitted_value_is_authoritative: true,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
}

#[test]
fn explicit_fresh_resolution_preserves_a_continued_local_edit() {
    let (mut field, attempt) = planned_field();
    field
        .mark_uncertain(attempt, "command may have committed")
        .expect("matching attempt becomes uncertain");
    field.edit(String::from("continued local edit"));

    assert_eq!(
        field
            .resolve_uncertain_from_fresh(1, String::from("other writer"))
            .expect("fresh authoritative evidence"),
        RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision: 1,
            submitted_value_is_authoritative: false,
        }
    );
    assert_eq!(field.local(), "continued local edit");
    assert_eq!(field.remote(), "other writer");
    assert_eq!(field.phase(), RetainedFieldPhase::Conflict);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
    assert!(field.notice().is_none());
}

fn conflicting_field() -> RevisionedField<String> {
    let mut field = RevisionedField::new(70, String::from("base"));
    field.edit(String::from("local"));
    assert_eq!(
        field.observe(71, String::from("remote")),
        RetainedFieldUpdate::Conflict
    );
    field
}

fn planned_field() -> (
    RevisionedField<String>,
    jellyflow_open_gpui::RetainedFieldAttempt,
) {
    let mut field = RevisionedField::new(0, String::from("base"));
    field.edit(String::from("submitted"));
    let intent = field.plan(0).expect("field submission intent");
    (field, intent.attempt())
}

fn applied_outcome() -> GraphCommandOutcome {
    let runtime = runtime_with_value("base");
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "submitted".into(),
        ))
        .expect("applied command outcome")
}

fn noop_outcome_at_revision_one() -> GraphCommandOutcome {
    let runtime = runtime_with_value("base");
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "submitted".into(),
        ))
        .expect("seed applied outcome");
    let outcome = runtime
        .execute(GraphCommand::set_node_data(
            1,
            node_id(),
            "submitted".into(),
        ))
        .expect("noop command outcome");
    assert!(matches!(outcome, GraphCommandOutcome::Noop(_)));
    outcome
}

fn conflict_outcome() -> GraphCommandOutcome {
    let runtime = runtime_with_value("base");
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "other writer".into(),
        ))
        .expect("seed applied outcome");
    let outcome = runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "submitted".into(),
        ))
        .expect("conflict command outcome");
    assert!(matches!(outcome, GraphCommandOutcome::Conflict(_)));
    outcome
}

fn rejected_outcome() -> GraphCommandOutcome {
    let runtime = runtime_with_value("base");
    let outcome = runtime
        .execute(GraphCommand::set_node_data(
            0,
            NodeId::from_u128(999),
            "submitted".into(),
        ))
        .expect("rejected command outcome");
    assert!(matches!(outcome, GraphCommandOutcome::Rejected(_)));
    outcome
}

fn runtime_with_value(value: &str) -> GraphRuntime {
    GraphRuntime::builder(catalog())
        .build(
            GraphBuilder::new(GraphId::from_u128(0xf1e1d))
                .with_node(
                    node_id(),
                    Node {
                        kind: NodeKindKey::new(KIND),
                        kind_version: 1,
                        pos: Default::default(),
                        origin: None,
                        selectable: None,
                        draggable: None,
                        connectable: None,
                        deletable: None,
                        parent: None,
                        extent: None,
                        expand_parent: None,
                        size: None,
                        hidden: false,
                        collapsed: false,
                        ports: Vec::new(),
                        data: serde_json::json!(value),
                    },
                )
                .build()
                .expect("revisioned field graph"),
        )
        .expect("revisioned field runtime")
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND, "Revisioned field", RENDERER).build())
        .expect("revisioned field kind");
    catalog
}

fn node_id() -> NodeId {
    NodeId::from_u128(1)
}
