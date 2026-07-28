use jellyflow_core::{GraphBuilder, GraphId, Node, NodeId, NodeKindKey};
use jellyflow_open_gpui::retained_field::{
    ApplicationRetainedField, ApplicationRetainedFieldDispatchReceipt,
    ApplicationRetainedFieldResolution, RetainedFieldAttemptState, RetainedFieldConflictChoice,
    RetainedFieldError, RetainedFieldPhase, RetainedFieldSettlement, RetainedFieldSubmitMode,
    RetainedFieldUpdate,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::CommandExecutionCompletion, commands::CommandExecutionKind, commands::GraphCommand,
    graph::GraphRuntime, graph::GraphVersion,
};

const KIND: &str = "test.revisioned-field";
const RENDERER: &str = "test.revisioned.field";

#[test]
fn authoritative_observations_distinguish_clean_dirty_and_composing_fields() {
    let clean_runtime = runtime_with_value("before");
    let clean_initial = version(&clean_runtime);
    let mut clean = ApplicationRetainedField::new(clean_initial, String::from("before"));

    assert_eq!(clean.phase(), RetainedFieldPhase::Clean);
    let after = advance_runtime(&clean_runtime, "after");
    assert_eq!(
        observe(&mut clean, after, "after"),
        RetainedFieldUpdate::RemoteApplied
    );
    assert_eq!(clean.local(), "after");
    assert_eq!(clean.remote(), "after");
    assert_eq!(clean.base_revision(), after.revision());
    assert_eq!(clean.remote_version(), after);

    clean.edit(String::from("local edit"));
    assert_eq!(clean.phase(), RetainedFieldPhase::Dirty);
    let other_view = advance_runtime(&clean_runtime, "other view");
    assert_eq!(
        observe(&mut clean, other_view, "other view"),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(clean.local(), "local edit");
    assert_eq!(clean.remote(), "other view");
    assert_eq!(clean.phase(), RetainedFieldPhase::Conflict);
    let conflict = clean.conflict().expect("revision conflict");
    assert_eq!(conflict.base_revision(), after.revision());
    assert_eq!(conflict.remote_revision(), other_view.revision());

    let composing_runtime = runtime_with_value("base");
    let mut composing =
        ApplicationRetainedField::new(version(&composing_runtime), String::from("base"));
    composing.set_composing(true);
    composing.edit(String::from("partial composition"));
    assert_eq!(composing.phase(), RetainedFieldPhase::Composing);
    let new_remote = advance_runtime(&composing_runtime, "new remote value");
    assert_eq!(
        observe(&mut composing, new_remote, "new remote value"),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(composing.local(), "partial composition");
    assert_eq!(composing.remote(), "new remote value");
    assert_eq!(composing.phase(), RetainedFieldPhase::ComposingConflict);
}

#[test]
fn foreign_same_revision_snapshot_is_rejected_without_mutating_the_field() {
    let owner = runtime_with_value("owner");
    let foreign = runtime_with_value("foreign");
    let mut field = ApplicationRetainedField::new(version(&owner), String::from("owner"));
    field.edit(String::from("local"));

    assert!(matches!(
        field.observe(version(&foreign), String::from("foreign")),
        Err(RetainedFieldError::ForeignRuntimeAuthority { .. })
    ));
    assert_eq!(field.local(), "local");
    assert_eq!(field.remote(), "owner");
    assert_eq!(field.remote_version(), version(&owner));
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
}

#[test]
fn unchanged_authoritative_value_advances_a_dirty_fields_base() {
    let runtime = runtime_with_value("remote");
    let initial = version(&runtime);
    let mut field = ApplicationRetainedField::new(initial, String::from("remote"));
    field.edit(String::from("local"));
    let advanced = advance_runtime(&runtime, "unrelated graph value");

    assert_eq!(
        observe(&mut field, advanced, "remote"),
        RetainedFieldUpdate::BaseAdvanced
    );
    assert_eq!(field.base_revision(), advanced.revision());
    assert_eq!(field.local(), "local");
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    field.edit(String::from("remote"));
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert!(matches!(
        field.plan(advanced),
        Err(RetainedFieldError::Clean)
    ));
}

#[test]
fn conflicts_require_an_explicit_accept_keep_or_reapply_choice() {
    let (accepted_runtime, mut accepted) = conflicting_field();
    let accepted_version = version(&accepted_runtime);
    assert!(matches!(
        accepted
            .resolve(RetainedFieldConflictChoice::AcceptRemote, accepted_version)
            .expect("accept authoritative value"),
        ApplicationRetainedFieldResolution::AcceptedRemote { version }
            if version == accepted_version
    ));
    assert_eq!(accepted.local(), "remote");
    assert_eq!(accepted.phase(), RetainedFieldPhase::Clean);

    let (kept_runtime, mut kept) = conflicting_field();
    let kept_version = version(&kept_runtime);
    let conflict = kept.conflict().expect("conflicting field");
    assert!(matches!(
        kept.resolve(RetainedFieldConflictChoice::KeepLocal, kept_version)
            .expect("keep local value"),
        ApplicationRetainedFieldResolution::KeptLocal { conflict: actual }
            if actual == conflict
    ));
    assert_eq!(kept.local(), "local");
    assert_eq!(kept.phase(), RetainedFieldPhase::Conflict);

    let (reapplied_runtime, mut reapplied) = conflicting_field();
    let reapplied_version = version(&reapplied_runtime);
    let intent = match reapplied
        .resolve(RetainedFieldConflictChoice::ReapplyLocal, reapplied_version)
        .expect("explicit reapply intent")
    {
        ApplicationRetainedFieldResolution::Reapply(intent) => intent,
        resolution => panic!("expected reapply intent, got {resolution:?}"),
    };
    assert_eq!(intent.mode(), RetainedFieldSubmitMode::Reapply);
    assert_eq!(intent.expected_version(), reapplied_version);
    assert_eq!(intent.value(), "local");
    assert!(matches!(
        reapplied.plan(reapplied_version),
        Err(RetainedFieldError::AttemptPending { .. })
    ));

    let (composing_runtime, mut composing) = conflicting_field();
    composing.set_composing(true);
    assert!(matches!(
        composing.resolve(
            RetainedFieldConflictChoice::ReapplyLocal,
            version(&composing_runtime),
        ),
        Err(RetainedFieldError::CompositionActive)
    ));
}

#[test]
fn planning_requires_the_latest_authoritative_version() {
    let runtime = runtime_with_value("remote");
    let initial = version(&runtime);
    let mut field = ApplicationRetainedField::new(initial, String::from("remote"));
    field.edit(String::from("local"));
    let advanced = advance_runtime(&runtime, "unrelated graph value");
    assert_eq!(
        observe(&mut field, advanced, "remote"),
        RetainedFieldUpdate::BaseAdvanced
    );

    assert!(matches!(
        field.plan(initial),
        Err(RetainedFieldError::DispatchVersionMismatch {
            expected,
            actual,
        }) if expected == advanced && actual == initial
    ));
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);

    let intent = field.plan(advanced).expect("fresh submission intent");
    assert_eq!(intent.attempt().get(), 1);
    assert_eq!(intent.expected_version(), advanced);
    assert_eq!(intent.value(), "local");
    field
        .not_dispatched(intent.attempt())
        .expect("definitely undispatched intent can be cleared");
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
    assert!(field.can_submit());
}

#[test]
fn receipt_first_waits_for_a_matching_authoritative_snapshot() {
    let (runtime, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let completion = runtime.execute(command);
    let committed_version = completion.version().expect("applied field command version");

    assert_eq!(
        settle(&mut field, receipt, completion),
        RetainedFieldSettlement::AwaitingSnapshot {
            attempt,
            committed_revision: committed_version.revision(),
        }
    );
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::ReceiptFirst {
            attempt,
            committed_revision: committed_version.revision(),
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    assert_eq!(
        observe(&mut field, committed_version, "submitted"),
        RetainedFieldUpdate::Acknowledged {
            attempt,
            revision: committed_version.revision(),
            local_changed_after_submission: false,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
}

#[test]
fn snapshot_first_stays_dirty_until_its_matching_receipt_arrives() {
    let (runtime, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let completion = runtime.execute(command);
    let committed_version = completion.version().expect("applied field command version");

    assert_eq!(
        observe(&mut field, committed_version, "submitted"),
        RetainedFieldUpdate::SnapshotFirst {
            attempt,
            revision: committed_version.revision(),
        }
    );
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::SnapshotFirst {
            attempt,
            observed_revision: committed_version.revision(),
        }
    );
    assert_eq!(field.conflict(), None);
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);

    assert_eq!(
        settle(&mut field, receipt, completion),
        RetainedFieldSettlement::Acknowledged {
            attempt,
            revision: committed_version.revision(),
            local_changed_after_submission: false,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
}

#[test]
fn noop_requires_refresh_and_preserves_the_unsaved_value() {
    let (runtime, mut field, receipt, command) =
        planned_field(|version| GraphCommand::set_node_data(version, node_id(), "base".into()));
    let attempt = receipt.attempt();
    let completion = runtime.execute(command);
    assert_eq!(completion.kind(), CommandExecutionKind::Noop);
    let committed_version = completion.version().expect("noop field command version");

    assert_eq!(
        settle(&mut field, receipt, completion),
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision: committed_version.revision(),
            message: format!(
                "field command made no authoritative change at revision {}; refresh before retry",
                committed_version.revision()
            ),
        }
    );
    assert_eq!(
        observe(&mut field, committed_version, "base"),
        RetainedFieldUpdate::Unchanged
    );
    assert_eq!(field.local(), "submitted");
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
}

#[test]
fn acknowledgement_preserves_an_edit_made_after_submission() {
    let (runtime, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    field.edit(String::from("newer local"));
    let completion = runtime.execute(command);
    let committed_version = completion.version().expect("applied field command version");

    assert!(matches!(
        field.settle_command(receipt, completion),
        Ok(RetainedFieldSettlement::AwaitingSnapshot { .. })
    ));
    assert_eq!(
        observe(&mut field, committed_version, "submitted"),
        RetainedFieldUpdate::Acknowledged {
            attempt,
            revision: committed_version.revision(),
            local_changed_after_submission: true,
        }
    );
    assert_eq!(field.local(), "newer local");
    assert_eq!(field.remote(), "submitted");
    assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
}

#[test]
fn conflict_and_rejection_completions_require_an_authoritative_refresh() {
    let (conflict_runtime, mut conflicted, conflict_receipt, conflict_command) =
        planned_field(|version| {
            GraphCommand::set_node_data(version, node_id(), "submitted".into())
        });
    let conflict_attempt = conflict_receipt.attempt();
    let current = advance_runtime(&conflict_runtime, "other writer");
    let conflict_completion = conflict_runtime.execute(conflict_command);
    assert_eq!(conflict_completion.kind(), CommandExecutionKind::Conflict);
    let settlement = conflicted
        .settle_command(conflict_receipt, conflict_completion)
        .expect("matching conflict outcome");
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision,
            ..
        } if attempt == conflict_attempt && minimum_revision == current.revision()
    ));
    assert!(matches!(
        conflicted.attempt_state(),
        RetainedFieldAttemptState::RefreshRequired {
            attempt: Some(attempt),
            minimum_revision,
        } if attempt == conflict_attempt && minimum_revision == current.revision()
    ));
    assert!(matches!(
        conflicted.plan(version(&conflict_runtime)),
        Err(RetainedFieldError::AttemptPending { .. })
    ));
    assert_eq!(
        observe(&mut conflicted, current, "other writer"),
        RetainedFieldUpdate::Conflict
    );
    assert_eq!(conflicted.attempt_state(), RetainedFieldAttemptState::Idle);
    assert_eq!(conflicted.phase(), RetainedFieldPhase::Conflict);

    let (rejected_runtime, mut rejected, rejected_receipt, rejected_command) =
        planned_field(|version| {
            GraphCommand::set_node_data(version, NodeId::from_u128(999), "submitted".into())
        });
    let rejected_attempt = rejected_receipt.attempt();
    let rejected_completion = rejected_runtime.execute(rejected_command);
    assert_eq!(rejected_completion.kind(), CommandExecutionKind::Rejected);
    let settlement = rejected
        .settle_command(rejected_receipt, rejected_completion)
        .expect("matching rejected outcome");
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::RefreshRequired {
            attempt,
            minimum_revision,
            ..
        } if attempt == rejected_attempt
            && minimum_revision == version(&rejected_runtime).revision()
    ));
    assert!(matches!(
        rejected.plan(version(&rejected_runtime)),
        Err(RetainedFieldError::AttemptPending { .. })
    ));
    assert_eq!(
        observe(&mut rejected, version(&rejected_runtime), "base"),
        RetainedFieldUpdate::Unchanged
    );
    assert_eq!(rejected.attempt_state(), RetainedFieldAttemptState::Idle);
    assert_eq!(rejected.phase(), RetainedFieldPhase::Dirty);
}

#[test]
fn transport_failure_is_uncertain_and_never_implicitly_retried() {
    let (runtime, mut field, receipt, _command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let initial = receipt.expected_version();

    let settlement = field
        .mark_uncertain(receipt, "transport did not return a command outcome")
        .expect("matching dispatch becomes uncertain");
    assert!(matches!(
        settlement,
        RetainedFieldSettlement::Uncertain {
            attempt: uncertain_attempt,
            ..
        } if uncertain_attempt == attempt
    ));
    assert!(matches!(
        field.attempt_state(),
        RetainedFieldAttemptState::Uncertain {
            attempt: uncertain_attempt,
            expected_revision,
        } if uncertain_attempt == attempt && expected_revision == initial.revision()
    ));
    assert!(field.notice().is_some());
    assert!(matches!(
        field.plan(initial),
        Err(RetainedFieldError::AttemptPending { .. })
    ));

    assert_eq!(
        observe(&mut field, initial, "base"),
        RetainedFieldUpdate::Unchanged
    );
    assert!(matches!(
        field.attempt_state(),
        RetainedFieldAttemptState::Uncertain { .. }
    ));
    let committed = advance_runtime(&runtime, "submitted");
    assert_eq!(
        observe(&mut field, committed, "submitted"),
        RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision: committed.revision(),
            submitted_value_is_authoritative: true,
        }
    );
    assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
}

#[test]
fn explicit_fresh_resolution_preserves_a_continued_local_edit() {
    let (runtime, mut field, receipt, _command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    field
        .mark_uncertain(receipt, "command may have committed")
        .expect("matching attempt becomes uncertain");
    field.edit(String::from("continued local edit"));
    let fresh = advance_runtime(&runtime, "other writer");

    assert_eq!(
        field
            .resolve_uncertain_from_fresh(fresh, String::from("other writer"))
            .expect("fresh authoritative evidence"),
        RetainedFieldUpdate::UncertainResolved {
            attempt,
            revision: fresh.revision(),
            submitted_value_is_authoritative: false,
        }
    );
    assert_eq!(field.local(), "continued local edit");
    assert_eq!(field.remote(), "other writer");
    assert_eq!(field.phase(), RetainedFieldPhase::Conflict);
    assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
    assert!(field.notice().is_none());
}

#[test]
fn foreign_same_revision_completion_cannot_settle_a_bound_command() {
    let (owner, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let foreign = runtime_with_value("base");
    let foreign_completion = foreign.execute(GraphCommand::set_node_data(
        version(&foreign),
        node_id(),
        "submitted".into(),
    ));

    assert_eq!(foreign_completion.observed_revision(), Some(1));
    assert!(matches!(
        field.settle_command(receipt.clone(), foreign_completion),
        Err(RetainedFieldError::ForeignRuntimeAuthority { .. })
    ));
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::Pending { attempt }
    );

    let owner_completion = owner.execute(command);
    assert!(matches!(
        field.settle_command(receipt, owner_completion),
        Ok(RetainedFieldSettlement::AwaitingSnapshot { .. })
    ));
}

#[test]
fn same_runtime_same_version_noop_and_rejection_cannot_cross_command_identity() {
    let (runtime, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let expected = receipt.expected_version();

    let wrong_noop = runtime.execute(GraphCommand::set_node_data(
        expected,
        node_id(),
        "base".into(),
    ));
    assert_eq!(wrong_noop.kind(), CommandExecutionKind::Noop);
    assert!(matches!(
        field.settle_command(receipt.clone(), wrong_noop),
        Err(RetainedFieldError::CompletionCommandMismatch)
    ));

    let wrong_rejection = runtime.execute(GraphCommand::set_node_data(
        expected,
        NodeId::from_u128(999),
        "submitted".into(),
    ));
    assert_eq!(wrong_rejection.kind(), CommandExecutionKind::Rejected);
    assert!(matches!(
        field.settle_command(receipt.clone(), wrong_rejection),
        Err(RetainedFieldError::CompletionCommandMismatch)
    ));
    assert_eq!(
        field.attempt_state(),
        RetainedFieldAttemptState::Pending { attempt }
    );

    let matching = runtime.execute(command);
    assert!(matches!(
        field.settle_command(receipt, matching),
        Ok(RetainedFieldSettlement::AwaitingSnapshot { .. })
    ));
}

#[test]
fn identity_matched_execution_failure_is_definitive_not_transport_uncertainty() {
    let (owner, mut field, receipt, command) = planned_field(|version| {
        GraphCommand::set_node_data(version, node_id(), "submitted".into())
    });
    let attempt = receipt.attempt();
    let expected = receipt.expected_version();
    let foreign = runtime_with_value("foreign");

    let completion = foreign.execute(command);
    assert_eq!(completion.kind(), CommandExecutionKind::Failed);
    assert_eq!(completion.version(), None);
    let settlement = field
        .settle_command(receipt, completion)
        .expect("identity-matched execution failure");

    assert!(matches!(
        settlement,
        RetainedFieldSettlement::RefreshRequired {
            attempt: actual_attempt,
            minimum_revision,
            ..
        } if actual_attempt == attempt && minimum_revision == expected.revision()
    ));
    assert!(matches!(
        field.attempt_state(),
        RetainedFieldAttemptState::RefreshRequired {
            attempt: Some(actual_attempt),
            minimum_revision,
        } if actual_attempt == attempt && minimum_revision == expected.revision()
    ));
    assert_eq!(version(&owner), expected);
}

fn conflicting_field() -> (GraphRuntime, ApplicationRetainedField<String>) {
    let runtime = runtime_with_value("base");
    let mut field = ApplicationRetainedField::new(version(&runtime), String::from("base"));
    field.edit(String::from("local"));
    let remote = advance_runtime(&runtime, "remote");
    assert_eq!(
        observe(&mut field, remote, "remote"),
        RetainedFieldUpdate::Conflict
    );
    (runtime, field)
}

fn planned_field(
    command: impl FnOnce(GraphVersion) -> GraphCommand,
) -> (
    GraphRuntime,
    ApplicationRetainedField<String>,
    ApplicationRetainedFieldDispatchReceipt,
    GraphCommand,
) {
    let runtime = runtime_with_value("base");
    let expected = version(&runtime);
    let mut field = ApplicationRetainedField::new(expected, String::from("base"));
    field.edit(String::from("submitted"));
    let intent = field.plan(expected).expect("field submission intent");
    let command = command(expected);
    let receipt = intent.bind_command(&command).expect("bind field command");
    (runtime, field, receipt, command)
}

fn observe(
    field: &mut ApplicationRetainedField<String>,
    version: GraphVersion,
    authoritative: &str,
) -> RetainedFieldUpdate {
    field
        .observe(version, authoritative.to_owned())
        .expect("same-runtime authoritative observation")
}

fn settle(
    field: &mut ApplicationRetainedField<String>,
    receipt: ApplicationRetainedFieldDispatchReceipt,
    completion: CommandExecutionCompletion,
) -> RetainedFieldSettlement {
    field
        .settle_command(receipt, completion)
        .expect("matching field command outcome")
}

fn version(runtime: &GraphRuntime) -> GraphVersion {
    runtime.snapshot().expect("graph version").version()
}

fn advance_runtime(runtime: &GraphRuntime, value: &str) -> GraphVersion {
    let completion = runtime.execute(GraphCommand::set_node_data(
        version(runtime),
        node_id(),
        value.into(),
    ));
    assert_eq!(completion.kind(), CommandExecutionKind::Applied);
    completion.version().expect("advanced graph version")
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
