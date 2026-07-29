use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind, Symbol, SymbolId, TypeDesc,
};
use jellyflow_runtime::commands::{CommandExecutionKind, GraphCommand, IncidentEdgePolicy};
use jellyflow_runtime::graph::{GraphRuntime, GraphSnapshot};
use jellyflow_runtime::reconciliation::{MutationCommandResult, MutationPhase, MutationReconciler};
use jellyflow_runtime::view::ViewSessionOptions;
use open_gpui_canvas_jellyflow::product_domain::blackboard::{
    BlackboardApplyAction, BlackboardAuthoring, BlackboardDispatchOutcome, BlackboardDraftPhase,
    BlackboardError, BlackboardIntent, BlackboardObservation, BlackboardRetryReason,
    BlackboardSubmissionPhase, BlackboardSymbol,
};
use open_gpui_canvas_jellyflow::product_domain::blueprint::{
    BLUEPRINT_CALL_KIND, BlueprintError, BlueprintIntent, BlueprintObservation, BlueprintPin,
    blueprint_default_data,
};
use open_gpui_canvas_jellyflow::product_domain::condition::{
    CONDITION_ELSE_PORT_KEY, CONDITION_INPUT_PORT_KEY, CONDITION_KIND, ConditionCase,
    ConditionError, ConditionIntent, ConditionObservation, ConditionOperator, ConditionPredicate,
    condition_default_data,
};

const GRAPH_ID: u128 = 0xc011_d170;
const CONDITION_NODE: u128 = 0xc011_d171;
const INPUT_PORT: u128 = 0xc011_d172;
const FIRST_CASE_PORT: u128 = 0xc011_d173;
const SECOND_CASE_PORT: u128 = 0xc011_d174;
const ELSE_PORT: u128 = 0xc011_d175;
const TARGET_NODE: u128 = 0xc011_d176;
const TARGET_INPUT: u128 = 0xc011_d177;
const CONNECTED_EDGE: u128 = 0xc011_d178;
const ADDED_CASE_PORT: u128 = 0xc011_d179;
const CONCURRENT_CASE_PORT: u128 = 0xc011_d17a;
const DATA_SOURCE_NODE: u128 = 0xc011_d17b;
const DATA_SOURCE_OUTPUT: u128 = 0xc011_d17c;
const CONNECTED_DATA_EDGE: u128 = 0xc011_d17d;
const CASE_VALUE_PORT_OFFSET: u128 = 0x100;
const BLACKBOARD_GRAPH: u128 = 0xb1ac_d001;
const BLACKBOARD_PRIMARY: u128 = 0xb1ac_d002;
const BLACKBOARD_OTHER: u128 = 0xb1ac_d003;
const BLACKBOARD_ADDED: u128 = 0xb1ac_d004;

#[test]
fn blackboard_intents_validate_and_plan_atomic_add_update_remove_commands() {
    let runtime = blackboard_runtime();
    let initial = runtime.snapshot().unwrap();
    let observation = BlackboardObservation::from_snapshot(&initial).unwrap();
    let added = BlackboardSymbol::new(
        "temperature",
        Some(TypeDesc::Float),
        Some(serde_json::json!(0.5)),
        serde_json::json!({ "group": "material" }),
    )
    .unwrap();
    let add = BlackboardIntent::add(blackboard_added(), added.clone());
    assert!(!observation.satisfies(&add));
    assert_eq!(
        runtime.execute(observation.plan(&add).unwrap()).kind(),
        CommandExecutionKind::Applied
    );

    let after_add = runtime.snapshot().unwrap();
    let added_observation = BlackboardObservation::from_snapshot(&after_add).unwrap();
    assert!(added_observation.satisfies(&add));
    let rename = BlackboardIntent::update_name(blackboard_added(), " temperature_c ").unwrap();
    assert_eq!(
        runtime
            .execute(added_observation.plan(&rename).unwrap())
            .kind(),
        CommandExecutionKind::Applied
    );
    let renamed = runtime.snapshot().unwrap();
    let renamed_observation = BlackboardObservation::from_snapshot(&renamed).unwrap();
    assert!(renamed_observation.satisfies(&rename));
    let renamed_symbol = renamed.graph().symbol(&blackboard_added()).unwrap();
    assert_eq!(renamed_symbol.name, "temperature_c");
    assert_eq!(renamed_symbol.ty, Some(TypeDesc::Float));
    assert_eq!(renamed_symbol.default_value, Some(serde_json::json!(0.5)));
    assert_eq!(
        renamed_symbol.meta,
        serde_json::json!({ "group": "material" })
    );

    let remove = BlackboardIntent::remove(blackboard_added());
    assert_eq!(
        runtime
            .execute(renamed_observation.plan(&remove).unwrap())
            .kind(),
        CommandExecutionKind::Applied
    );
    assert!(
        BlackboardObservation::from_snapshot(&runtime.snapshot().unwrap())
            .unwrap()
            .satisfies(&remove)
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 3);

    assert_eq!(
        BlackboardSymbol::new("  ", None, None, serde_json::Value::Null),
        Err(BlackboardError::EmptySymbolName)
    );
    assert_eq!(
        BlackboardIntent::update_name(blackboard_primary(), "\t"),
        Err(BlackboardError::EmptySymbolName)
    );
    assert!(matches!(
        observation.plan(&BlackboardIntent::remove(blackboard_added())),
        Err(BlackboardError::MissingSymbol { symbol }) if symbol == blackboard_added()
    ));
}

#[test]
fn stale_blackboard_draft_preserves_local_value_until_explicit_reapply() {
    let runtime = blackboard_runtime();
    let initial = runtime.snapshot().unwrap();
    let initial_observation = BlackboardObservation::from_snapshot(&initial).unwrap();
    let mut authoring = BlackboardAuthoring::new();
    authoring.observe(&initial_observation).unwrap();
    authoring
        .edit_name(
            &initial_observation,
            blackboard_primary(),
            "local winner".to_owned(),
        )
        .unwrap();

    let remote = Symbol {
        name: "remote winner".to_owned(),
        ..blackboard_symbol("exposure")
    };
    assert_eq!(
        runtime
            .execute(GraphCommand::symbols(
                initial.version(),
                jellyflow_runtime::commands::SymbolEdit::new().update(blackboard_primary(), remote),
            ))
            .kind(),
        CommandExecutionKind::Applied
    );
    let conflicted_snapshot = runtime.snapshot().unwrap();
    let conflicted = BlackboardObservation::from_snapshot(&conflicted_snapshot).unwrap();
    authoring.observe(&conflicted).unwrap();
    let projection = authoring
        .projection(&conflicted, blackboard_primary())
        .unwrap();
    assert_eq!(projection.value(), "local winner");
    assert_eq!(projection.phase(), BlackboardDraftPhase::Conflict);
    assert!(projection.can_accept_remote());
    assert_eq!(
        projection.apply_action(),
        Some(BlackboardApplyAction::Reapply)
    );

    let submission = authoring
        .plan_name(
            &conflicted,
            blackboard_primary(),
            BlackboardApplyAction::Reapply,
        )
        .unwrap();
    let completion = runtime.execute(submission.command().clone());
    assert_eq!(completion.kind(), CommandExecutionKind::Applied);
    authoring
        .complete(submission, BlackboardDispatchOutcome::completed(completion))
        .unwrap();
    let reapplied_snapshot = runtime.snapshot().unwrap();
    let reapplied = BlackboardObservation::from_snapshot(&reapplied_snapshot).unwrap();
    assert!(authoring.observe(&reapplied).unwrap().became_satisfied());
    let projection = authoring
        .projection(&reapplied, blackboard_primary())
        .unwrap();
    assert_eq!(projection.phase(), BlackboardDraftPhase::Clean);
    assert_eq!(projection.value(), "local winner");
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
}

#[test]
fn blackboard_source_rejection_runtime_conflict_and_uncertainty_are_distinct() {
    let runtime = blackboard_runtime();
    let initial = runtime.snapshot().unwrap();
    let initial_observation = BlackboardObservation::from_snapshot(&initial).unwrap();

    let mut rejected = BlackboardAuthoring::new();
    rejected.observe(&initial_observation).unwrap();
    rejected
        .edit_name(
            &initial_observation,
            blackboard_primary(),
            "rejected local".to_owned(),
        )
        .unwrap();
    let rejected_submission = rejected
        .plan_name(
            &initial_observation,
            blackboard_primary(),
            BlackboardApplyAction::Commit,
        )
        .unwrap();
    assert!(
        !rejected
            .complete(
                rejected_submission,
                BlackboardDispatchOutcome::not_dispatched("source refused dispatch"),
            )
            .unwrap()
            .became_satisfied()
    );
    rejected.observe(&initial_observation).unwrap();
    let rejected_projection = rejected
        .projection(&initial_observation, blackboard_primary())
        .unwrap();
    assert_eq!(rejected_projection.phase(), BlackboardDraftPhase::Edited);
    assert_eq!(
        rejected_projection.submission_phase(),
        BlackboardSubmissionPhase::Retryable
    );
    assert_eq!(
        rejected_projection.apply_action(),
        Some(BlackboardApplyAction::Retry)
    );
    assert!(!rejected_projection.can_accept_remote());
    assert_eq!(
        rejected_projection.notice(),
        Some("source refused dispatch")
    );
    rejected
        .edit_name(
            &initial_observation,
            blackboard_primary(),
            "exposure".to_owned(),
        )
        .unwrap();
    let reverted = rejected
        .projection(&initial_observation, blackboard_primary())
        .unwrap();
    assert_eq!(reverted.phase(), BlackboardDraftPhase::Clean);
    assert_eq!(reverted.apply_action(), None);
    assert!(reverted.can_discard_submission());
    rejected
        .discard_submission(&initial_observation, blackboard_primary())
        .unwrap();
    let discarded = rejected
        .projection(&initial_observation, blackboard_primary())
        .unwrap();
    assert_eq!(
        discarded.submission_phase(),
        BlackboardSubmissionPhase::Idle
    );
    assert!(!discarded.can_discard_submission());

    let mut conflicted = BlackboardAuthoring::new();
    conflicted.observe(&initial_observation).unwrap();
    conflicted
        .edit_name(
            &initial_observation,
            blackboard_primary(),
            "conflicted local".to_owned(),
        )
        .unwrap();
    let conflict_submission = conflicted
        .plan_name(
            &initial_observation,
            blackboard_primary(),
            BlackboardApplyAction::Commit,
        )
        .unwrap();
    let unrelated = Symbol {
        name: "other changed".to_owned(),
        ..blackboard_symbol("other")
    };
    assert_eq!(
        runtime
            .execute(GraphCommand::symbols(
                initial.version(),
                jellyflow_runtime::commands::SymbolEdit::new()
                    .update(blackboard_other(), unrelated),
            ))
            .kind(),
        CommandExecutionKind::Applied
    );
    let completion = runtime.execute(conflict_submission.command().clone());
    assert_eq!(completion.kind(), CommandExecutionKind::Conflict);
    conflicted
        .complete(
            conflict_submission,
            BlackboardDispatchOutcome::completed(completion),
        )
        .unwrap();
    let conflict_snapshot = runtime.snapshot().unwrap();
    let conflict_observation = BlackboardObservation::from_snapshot(&conflict_snapshot).unwrap();
    conflicted.observe(&conflict_observation).unwrap();
    let conflict_projection = conflicted
        .projection(&conflict_observation, blackboard_primary())
        .unwrap();
    assert_eq!(conflict_projection.phase(), BlackboardDraftPhase::Edited);
    assert_eq!(
        conflict_projection.submission_phase(),
        BlackboardSubmissionPhase::Retryable
    );
    assert_eq!(
        conflict_projection.retry_reason(),
        Some(BlackboardRetryReason::RuntimeConflict)
    );
    assert_eq!(
        conflict_projection.apply_action(),
        Some(BlackboardApplyAction::Retry)
    );
    assert!(!conflict_projection.can_accept_remote());

    let mut uncertain = BlackboardAuthoring::new();
    uncertain.observe(&conflict_observation).unwrap();
    uncertain
        .edit_name(
            &conflict_observation,
            blackboard_primary(),
            "authoritative after uncertainty".to_owned(),
        )
        .unwrap();
    let uncertain_submission = uncertain
        .plan_name(
            &conflict_observation,
            blackboard_primary(),
            BlackboardApplyAction::Commit,
        )
        .unwrap();
    let completion = runtime.execute(uncertain_submission.command().clone());
    assert_eq!(completion.kind(), CommandExecutionKind::Applied);
    assert!(
        !uncertain
            .complete(
                uncertain_submission,
                BlackboardDispatchOutcome::uncertain("transport lost completion"),
            )
            .unwrap()
            .became_satisfied()
    );
    let authoritative_snapshot = runtime.snapshot().unwrap();
    let authoritative = BlackboardObservation::from_snapshot(&authoritative_snapshot).unwrap();
    assert!(
        uncertain
            .observe(&authoritative)
            .unwrap()
            .became_satisfied()
    );
    let uncertain_projection = uncertain
        .projection(&authoritative, blackboard_primary())
        .unwrap();
    assert_eq!(uncertain_projection.phase(), BlackboardDraftPhase::Clean);
    assert_eq!(
        uncertain_projection.value(),
        "authoritative after uncertainty"
    );
}

#[test]
fn blackboard_snapshot_first_and_receipt_first_each_satisfy_exactly_once() {
    let runtime = blackboard_runtime();
    let initial = runtime.snapshot().unwrap();
    let initial_observation = BlackboardObservation::from_snapshot(&initial).unwrap();
    let mut authoring = BlackboardAuthoring::new();
    authoring.observe(&initial_observation).unwrap();
    authoring
        .edit_name(
            &initial_observation,
            blackboard_primary(),
            "snapshot first".to_owned(),
        )
        .unwrap();
    let submission = authoring
        .plan_name(
            &initial_observation,
            blackboard_primary(),
            BlackboardApplyAction::Commit,
        )
        .unwrap();
    let completion = runtime.execute(submission.command().clone());
    let applied = BlackboardObservation::from_snapshot(&runtime.snapshot().unwrap()).unwrap();

    assert!(!authoring.observe(&applied).unwrap().became_satisfied());
    assert!(
        authoring
            .complete(submission, BlackboardDispatchOutcome::completed(completion),)
            .unwrap()
            .became_satisfied()
    );
    assert!(!authoring.observe(&applied).unwrap().became_satisfied());
    let projection = authoring
        .projection(&applied, blackboard_primary())
        .unwrap();
    assert_eq!(projection.phase(), BlackboardDraftPhase::Clean);
    assert_eq!(
        projection.submission_phase(),
        BlackboardSubmissionPhase::Idle
    );
}

#[test]
fn blackboard_authoring_rejects_foreign_authority_and_ignores_stale_observations() {
    let runtime = blackboard_runtime();
    let initial = runtime.snapshot().unwrap();
    let initial_observation = BlackboardObservation::from_snapshot(&initial).unwrap();
    let mut authoring = BlackboardAuthoring::new();
    authoring.observe(&initial_observation).unwrap();

    assert_eq!(
        runtime
            .execute(GraphCommand::symbols(
                initial.version(),
                jellyflow_runtime::commands::SymbolEdit::new().update(
                    blackboard_other(),
                    Symbol {
                        name: "advanced".to_owned(),
                        ..blackboard_symbol("other")
                    },
                ),
            ))
            .kind(),
        CommandExecutionKind::Applied
    );
    let latest = BlackboardObservation::from_snapshot(&runtime.snapshot().unwrap()).unwrap();
    authoring.observe(&latest).unwrap();
    assert!(
        authoring
            .observe(&initial_observation)
            .unwrap()
            .ignored_stale_observation()
    );

    let foreign_runtime = blackboard_runtime();
    let foreign =
        BlackboardObservation::from_snapshot(&foreign_runtime.snapshot().unwrap()).unwrap();
    assert!(matches!(
        authoring.observe(&foreign),
        Err(BlackboardError::ForeignAuthority { .. })
    ));
}

#[test]
fn observation_parses_complete_typed_condition_and_checks_intent_postconditions() {
    let runtime = runtime(false);
    let snapshot = runtime.snapshot().expect("condition snapshot");
    let observation = observe(&snapshot);
    let cases = observation.cases();

    assert_eq!(observation.node_id(), condition_node());
    assert_eq!(observation.version(), snapshot.version());
    assert_eq!(observation.title(), "Route by intent");
    assert_eq!(observation.input_port(), input_port());
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].id(), "match");
    assert_eq!(cases[0].label(), "Intent matches");
    assert_eq!(cases[0].value_type(), &TypeDesc::String);
    assert_eq!(cases[0].predicate().operand(), "customer_intent");
    assert_eq!(cases[0].predicate().operator(), ConditionOperator::Equals);
    assert_eq!(cases[0].predicate().literal(), "Intent matches");
    assert_eq!(
        snapshot
            .graph()
            .port(&cases[0].value_port_id())
            .and_then(|port| port.ty.as_ref()),
        Some(&TypeDesc::String),
    );
    assert_eq!(observation.else_branch().label(), "Else");
    assert_eq!(observation.else_branch().port_id(), else_port());

    assert!(observation.satisfies(&ConditionIntent::add(cases[0].clone())));
    assert!(observation.satisfies(&ConditionIntent::reorder("match", 0)));
    assert!(!observation.satisfies(&ConditionIntent::reorder("match", 1)));
    assert!(!observation.satisfies(&ConditionIntent::remove(
        "review",
        IncidentEdgePolicy::Cascade,
    )));
    assert!(observation.satisfies(&ConditionIntent::remove(
        "already-absent",
        IncidentEdgePolicy::Reject,
    )));
}

#[test]
fn schema_rejects_legacy_malformed_and_invalid_case_values_with_typed_errors() {
    assert_eq!(
        ConditionPredicate::equals(" ", serde_json::json!("value"), TypeDesc::String),
        Err(ConditionError::EmptyPredicateOperand),
    );
    assert_eq!(
        ConditionPredicate::equals(
            "risk_score",
            serde_json::json!("not-a-number"),
            TypeDesc::Float,
        ),
        Err(ConditionError::InvalidPredicateLiteral {
            operand: "risk_score".to_owned(),
        }),
    );
    assert_eq!(
        ConditionCase::new(
            " ",
            "Label",
            PortId::from_u128(1),
            PortId::from_u128(2),
            string_predicate("Label"),
        ),
        Err(ConditionError::EmptyCaseId),
    );
    assert_eq!(
        ConditionCase::new(
            "else",
            "Else",
            PortId::from_u128(1),
            PortId::from_u128(2),
            string_predicate("Else"),
        ),
        Err(ConditionError::ReservedCaseId {
            case_id: "else".to_owned(),
        }),
    );
    assert_eq!(
        ConditionCase::new(
            "same-port",
            "Same port",
            PortId::from_u128(1),
            PortId::from_u128(1),
            string_predicate("value"),
        ),
        Err(ConditionError::DuplicateCasePort {
            port: PortId::from_u128(1),
        }),
    );
    assert_eq!(
        ConditionCase::new(
            "valid",
            " ",
            PortId::from_u128(1),
            PortId::from_u128(2),
            string_predicate("value"),
        ),
        Err(ConditionError::EmptyCaseLabel {
            case_id: "valid".to_owned(),
        }),
    );

    let runtime = runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let (mut node, ports) = condition_parts(&snapshot);
    node.data["branches"] = serde_json::json!([]);
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::LegacyBranchesUnsupported { node }) if node == condition_node()
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    node.data["cases"][0]["port_id"] = serde_json::json!("not-a-port-id");
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidData {
            field: "cases[].port_id",
            ..
        })
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    node.data["cases"][0]["predicate"]["value_type"] =
        serde_json::to_value(TypeDesc::Float).unwrap();
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidPredicateLiteral { operand })
            if operand == "customer_intent"
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    node.data["cases"][0]["predicate"]["operator"] = serde_json::json!("contains");
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidData {
            field: "cases[].predicate.operator",
            ..
        })
    ));

    assert!(matches!(
        ConditionObservation::from_snapshot(&snapshot, NodeId::from_u128(0xdead)),
        Err(ConditionError::MissingNode { .. })
    ));
}

#[test]
fn default_condition_schema_is_owned_by_the_product_domain() {
    assert_eq!(
        condition_default_data(),
        serde_json::json!({
            "title": "Condition",
            "cases": [],
            "else_branch": { "label": "Else" },
        })
    );
}

#[test]
fn schema_rejects_duplicate_case_identity_before_topology_can_be_planned() {
    let runtime = runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let (mut node, ports) = condition_parts(&snapshot);
    let duplicate_id = node.data["cases"][0]["id"].clone();
    node.data["cases"][1]["id"] = duplicate_id;
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::DuplicateCaseId { case_id }) if case_id == "match"
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    let duplicate_port = node.data["cases"][0]["port_id"].clone();
    node.data["cases"][1]["port_id"] = duplicate_port;
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::DuplicateCasePort { port }) if port == first_case_port()
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    let duplicate_port = node.data["cases"][0]["value_port_id"].clone();
    node.data["cases"][1]["value_port_id"] = duplicate_port;
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::DuplicateCasePort { port }) if port == case_value_port(FIRST_CASE_PORT)
    ));
}

#[test]
fn role_first_case_keys_remain_unique_for_dotted_case_ids() {
    let runtime = runtime(false);
    let first = ConditionCase::new(
        "foo",
        "Foo",
        PortId::from_u128(0xc011_d1b0),
        PortId::from_u128(0xc011_d1b1),
        string_predicate("foo"),
    )
    .unwrap();
    let first_intent = ConditionIntent::add(first.clone());
    runtime
        .execute(
            observe(&runtime.snapshot().unwrap())
                .plan(&first_intent)
                .unwrap(),
        )
        .applied_revision()
        .expect("first dotted-key fixture applies");

    let second = ConditionCase::new(
        "foo.value",
        "Foo value",
        PortId::from_u128(0xc011_d1b2),
        PortId::from_u128(0xc011_d1b3),
        string_predicate("foo value"),
    )
    .unwrap();
    let second_intent = ConditionIntent::add(second.clone());
    runtime
        .execute(
            observe(&runtime.snapshot().unwrap())
                .plan(&second_intent)
                .unwrap(),
        )
        .applied_revision()
        .expect("second dotted-key fixture applies");

    let snapshot = runtime.snapshot().unwrap();
    let keys = [
        first.port_id(),
        first.value_port_id(),
        second.port_id(),
        second.value_port_id(),
    ]
    .map(|port| snapshot.graph().port(&port).unwrap().key.0.clone());
    assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), keys.len());
    assert_eq!(keys[0], "case.route.foo");
    assert_eq!(keys[1], "case.value.foo");
    assert_eq!(keys[2], "case.route.foo.value");
    assert_eq!(keys[3], "case.value.foo.value");
}

#[test]
fn complete_topology_validation_rejects_missing_invalid_extra_and_misordered_ports() {
    let runtime = runtime(false);
    let snapshot = runtime.snapshot().unwrap();

    let (node, mut ports) = condition_parts(&snapshot);
    ports.remove(&first_case_port());
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::MissingCasePort { port, .. }) if port == first_case_port()
    ));

    let (node, mut ports) = condition_parts(&snapshot);
    ports.get_mut(&first_case_port()).unwrap().node = target_node();
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidCasePort { port, .. }) if port == first_case_port()
    ));

    let (node, mut ports) = condition_parts(&snapshot);
    let value_port = case_value_port(FIRST_CASE_PORT);
    ports.get_mut(&value_port).unwrap().ty = Some(TypeDesc::Float);
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidCasePort { port, .. }) if port == value_port
    ));

    let (node, mut ports) = condition_parts(&snapshot);
    ports.get_mut(&input_port()).unwrap().capacity = PortCapacity::Multi;
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidInputPort { port, .. }) if port == input_port()
    ));

    let (node, mut ports) = condition_parts(&snapshot);
    ports.get_mut(&else_port()).unwrap().dir = PortDirection::In;
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidElsePort { port, .. }) if port == else_port()
    ));

    let (mut node, ports) = condition_parts(&snapshot);
    node.ports.swap(1, 2);
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidPortOrder { .. })
    ));

    let (node, mut ports) = condition_parts(&snapshot);
    ports.insert(
        target_input(),
        snapshot.graph().port(&target_input()).unwrap().clone(),
    );
    assert!(matches!(
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports),
        Err(ConditionError::InvalidPortOrder { .. })
    ));
}

#[test]
fn add_rejects_duplicate_ids_and_every_occupied_port_before_dispatch() {
    let runtime = runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let observation = observe(&snapshot);
    let initial_revision = snapshot.revision();

    let duplicate_id = case("match", "Another match", ADDED_CASE_PORT);
    assert!(matches!(
        observation.plan(&ConditionIntent::add(duplicate_id)),
        Err(ConditionError::DuplicateCaseId { case_id }) if case_id == "match"
    ));

    for occupied in [
        input_port(),
        case_value_port(FIRST_CASE_PORT),
        first_case_port(),
        case_value_port(SECOND_CASE_PORT),
        second_case_port(),
        else_port(),
        target_input(),
        data_source_output(),
    ] {
        let intent = ConditionIntent::add(
            ConditionCase::new(
                "new",
                "New",
                occupied,
                PortId::from_u128(0xc011_d1ff),
                string_predicate("New"),
            )
            .expect("valid colliding test case"),
        );
        assert!(matches!(
            observation.plan(&intent),
            Err(ConditionError::CasePortAlreadyExists { port }) if port == occupied
        ));
    }
    let value_collision = ConditionIntent::add(
        ConditionCase::new(
            "new-value",
            "New value",
            PortId::from_u128(0xc011_d1fe),
            target_input(),
            string_predicate("New value"),
        )
        .unwrap(),
    );
    assert!(matches!(
        observation.plan(&value_collision),
        Err(ConditionError::CasePortAlreadyExists { port }) if port == target_input()
    ));

    let (node, ports) = condition_parts(&snapshot);
    let local_observation =
        ConditionObservation::from_parts(snapshot.version(), condition_node(), &node, ports)
            .unwrap();
    let foreign_collision = ConditionIntent::add(
        ConditionCase::new(
            "foreign-collision",
            "Foreign collision",
            target_input(),
            PortId::from_u128(0xc011_d1fd),
            string_predicate("Foreign collision"),
        )
        .unwrap(),
    );
    assert!(local_observation.plan(&foreign_collision).is_ok());
    assert!(matches!(
        local_observation.plan_with_port_occupancy(&foreign_collision, |port| {
            snapshot.graph().port(&port).is_some()
        }),
        Err(ConditionError::CasePortAlreadyExists { port }) if port == target_input()
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), initial_revision);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn valid_add_reorder_remove_are_atomic_and_preserve_stable_identities() {
    let runtime = runtime(false);
    let initial = runtime.snapshot().unwrap();
    let initial_observation = observe(&initial);
    let stable_ports = initial_observation
        .cases()
        .iter()
        .flat_map(|case| [case.value_port_id(), case.port_id()])
        .collect::<Vec<_>>();
    let else_port = initial_observation.else_branch().port_id();
    let added = case("fallback", "Fallback", ADDED_CASE_PORT);

    let add = initial_observation
        .plan(&ConditionIntent::add(added.clone()))
        .unwrap();
    assert_eq!(runtime.execute(add).kind(), CommandExecutionKind::Applied);
    let after_add = runtime.snapshot().unwrap();
    assert_eq!(after_add.revision(), initial.revision() + 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let after_add_observation = observe(&after_add);
    assert_eq!(
        after_add_observation
            .cases()
            .iter()
            .take(2)
            .flat_map(|case| [case.value_port_id(), case.port_id()])
            .collect::<Vec<_>>(),
        stable_ports,
    );
    assert_eq!(after_add_observation.cases().last(), Some(&added));
    assert_eq!(after_add_observation.else_branch().port_id(), else_port);

    let reorder_intent = ConditionIntent::reorder("fallback", 0);
    let reorder = after_add_observation.plan(&reorder_intent).unwrap();
    assert_eq!(
        runtime.execute(reorder).kind(),
        CommandExecutionKind::Applied
    );
    let after_reorder = runtime.snapshot().unwrap();
    let after_reorder_observation = observe(&after_reorder);
    assert_eq!(after_reorder_observation.cases()[0], added);
    assert!(after_reorder_observation.satisfies(&reorder_intent));

    let remove_intent = ConditionIntent::remove("fallback", IncidentEdgePolicy::Reject);
    let remove = after_reorder_observation.plan(&remove_intent).unwrap();
    assert_eq!(
        runtime.execute(remove).kind(),
        CommandExecutionKind::Applied
    );
    let after_remove = runtime.snapshot().unwrap();
    let after_remove_observation = observe(&after_remove);
    assert_eq!(after_remove.revision(), initial.revision() + 3);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 3);
    assert!(after_remove_observation.satisfies(&remove_intent));
    assert_eq!(after_remove_observation.else_branch().port_id(), else_port);

    runtime
        .execute(GraphCommand::undo(after_remove.version()))
        .applied_revision()
        .expect("one undo restores the complete removed case");
    let restored = observe(&runtime.snapshot().unwrap());
    assert_eq!(restored.cases()[0], added);
    assert_eq!(restored.else_branch().port_id(), else_port);
}

#[test]
fn reject_rolls_back_connected_removal_and_cascade_removes_port_and_edge_atomically() {
    let runtime = runtime(true);
    let initial = runtime.snapshot().unwrap();
    let observation = observe(&initial);
    let else_port = observation.else_branch().port_id();

    let reject_intent = ConditionIntent::remove("review", IncidentEdgePolicy::Reject);
    let rejected = runtime.execute(observation.plan(&reject_intent).unwrap());
    assert_eq!(rejected.kind(), CommandExecutionKind::Rejected);
    let after_reject = runtime.snapshot().unwrap();
    assert_eq!(after_reject.revision(), initial.revision());
    assert!(after_reject.graph().port(&second_case_port()).is_some());
    assert!(
        after_reject
            .graph()
            .port(&case_value_port(SECOND_CASE_PORT))
            .is_some()
    );
    assert!(after_reject.graph().edge(&connected_edge()).is_some());
    assert!(after_reject.graph().edge(&connected_data_edge()).is_some());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);

    let cascade_intent = ConditionIntent::remove("review", IncidentEdgePolicy::Cascade);
    let cascade = observe(&after_reject).plan(&cascade_intent).unwrap();
    assert_eq!(
        runtime.execute(cascade).kind(),
        CommandExecutionKind::Applied
    );
    let after_cascade = runtime.snapshot().unwrap();
    assert_eq!(after_cascade.revision(), initial.revision() + 1);
    assert!(after_cascade.graph().port(&second_case_port()).is_none());
    assert!(
        after_cascade
            .graph()
            .port(&case_value_port(SECOND_CASE_PORT))
            .is_none()
    );
    assert!(after_cascade.graph().edge(&connected_edge()).is_none());
    assert!(after_cascade.graph().edge(&connected_data_edge()).is_none());
    assert_eq!(observe(&after_cascade).else_branch().port_id(), else_port);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
}

#[test]
fn stale_intent_replans_from_fresh_observation_without_overwriting_remote_addition() {
    let runtime = runtime(false);
    let first_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let first_base = first_view.graph_snapshot().unwrap();
    let second_base = second_view.graph_snapshot().unwrap();
    let reorder_intent = ConditionIntent::reorder("review", 0);
    let stale = observe(&first_base).plan(&reorder_intent).unwrap();
    let concurrent = case("concurrent", "Needs review", CONCURRENT_CASE_PORT);
    let remote = observe(&second_base)
        .plan(&ConditionIntent::add(concurrent.clone()))
        .unwrap();
    second_view
        .execute(remote)
        .unwrap()
        .applied_revision()
        .expect("remote add applies");

    let conflict = first_view.execute(stale).unwrap();
    assert_eq!(conflict.kind(), CommandExecutionKind::Conflict);
    let refreshed = first_view.graph_snapshot().unwrap();
    let refreshed_observation = observe(&refreshed);
    assert_eq!(refreshed_observation.cases().last(), Some(&concurrent));
    first_view
        .execute(refreshed_observation.plan(&reorder_intent).unwrap())
        .unwrap()
        .applied_revision()
        .expect("explicitly replanned intent applies");

    let final_observation = observe(&first_view.graph_snapshot().unwrap());
    assert_eq!(final_observation.cases()[0].id(), "review");
    assert_eq!(final_observation.cases().last(), Some(&concurrent));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
}

#[test]
fn receipt_first_and_snapshot_first_satisfy_the_same_condition_intent_exactly_once() {
    let runtime = runtime(false);
    let before = runtime.snapshot().unwrap();
    let added = case("added", "Added", ADDED_CASE_PORT);
    let intent = ConditionIntent::add(added);
    let command = observe(&before).plan(&intent).unwrap();
    runtime
        .execute(command)
        .applied_revision()
        .expect("condition add applies");
    let after = runtime.snapshot().unwrap();
    let after_observation = observe(&after);
    assert!(after_observation.satisfies(&intent));

    let mut receipt_first = MutationReconciler::default();
    let receipt_attempt = receipt_first
        .begin(intent.clone(), before.revision())
        .unwrap();
    assert!(
        !receipt_first
            .complete(
                receipt_attempt,
                MutationCommandResult::Applied {
                    revision: after.revision(),
                },
            )
            .unwrap()
            .became_satisfied()
    );
    assert_eq!(receipt_first.phase(), MutationPhase::AwaitingSnapshot);
    assert!(
        receipt_first
            .observe(after.revision(), |active| after_observation
                .satisfies(active))
            .became_satisfied()
    );
    assert_eq!(receipt_first.phase(), MutationPhase::Idle);
    assert!(
        !receipt_first
            .observe(after.revision(), |active| after_observation
                .satisfies(active))
            .became_satisfied()
    );

    let mut snapshot_first = MutationReconciler::default();
    let snapshot_attempt = snapshot_first
        .begin(intent.clone(), before.revision())
        .unwrap();
    assert!(
        !snapshot_first
            .observe(after.revision(), |active| after_observation
                .satisfies(active))
            .became_satisfied()
    );
    assert!(
        snapshot_first
            .complete(
                snapshot_attempt,
                MutationCommandResult::Applied {
                    revision: after.revision(),
                },
            )
            .unwrap()
            .became_satisfied()
    );
    assert_eq!(snapshot_first.phase(), MutationPhase::Idle);
}

#[test]
fn missing_and_stale_actions_fail_without_graph_mutation() {
    let runtime = runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let observation = observe(&snapshot);

    assert!(matches!(
        observation.plan(&ConditionIntent::reorder("missing", 0)),
        Err(ConditionError::CaseNotFound { case_id }) if case_id == "missing"
    ));
    assert!(matches!(
        observation.plan(&ConditionIntent::reorder("match", 99)),
        Err(ConditionError::CaseReorderIndexOutOfBounds {
            index: 99,
            case_count: 2,
        })
    ));
    assert!(matches!(
        observation.plan(&ConditionIntent::remove(
            "missing",
            IncidentEdgePolicy::Cascade,
        )),
        Err(ConditionError::CaseNotFound { case_id }) if case_id == "missing"
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), snapshot.revision());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn product_domain_source_has_no_native_framework_or_topology_supervisor_dependency() {
    let source = include_str!("../src/product_domain/condition.rs");
    assert!(!source.contains("open_gpui"));
    assert!(!source.contains("TopologyAttempt"));
}

fn runtime(connected: bool) -> GraphRuntime {
    GraphRuntime::builder(Default::default())
        .build(condition_graph(connected))
        .expect("valid pure Condition graph")
}

fn observe(snapshot: &GraphSnapshot) -> ConditionObservation {
    ConditionObservation::from_snapshot(snapshot, condition_node())
        .expect("valid Condition observation")
}

fn condition_parts(snapshot: &GraphSnapshot) -> (Node, BTreeMap<PortId, Port>) {
    let node = snapshot
        .graph()
        .node(&condition_node())
        .expect("condition node")
        .clone();
    let ports = node
        .ports
        .iter()
        .map(|port| (*port, snapshot.graph().port(port).unwrap().clone()))
        .collect();
    (node, ports)
}

fn condition_graph(connected: bool) -> Graph {
    let cases = [
        case("match", "Intent matches", FIRST_CASE_PORT),
        case("review", "Needs review", SECOND_CASE_PORT),
    ];
    let mut graph = GraphBuilder::new(GraphId::from_u128(GRAPH_ID));
    graph.insert_node(
        condition_node(),
        node(
            CONDITION_KIND,
            vec![
                input_port(),
                case_value_port(FIRST_CASE_PORT),
                first_case_port(),
                case_value_port(SECOND_CASE_PORT),
                second_case_port(),
                else_port(),
            ],
            condition_data("Route by intent", &cases),
        ),
    );
    graph.insert_port(
        input_port(),
        input(condition_node(), CONDITION_INPUT_PORT_KEY),
    );
    for case in &cases {
        graph.insert_port(
            case.value_port_id(),
            case_value_input(condition_node(), case),
        );
        graph.insert_port(case.port_id(), case_output(condition_node(), case));
    }
    graph.insert_port(
        else_port(),
        output(
            condition_node(),
            CONDITION_ELSE_PORT_KEY,
            serde_json::Value::Null,
        ),
    );
    graph.insert_node(
        target_node(),
        node(
            "workflow.target",
            vec![target_input()],
            serde_json::Value::Null,
        ),
    );
    graph.insert_port(target_input(), input(target_node(), "input"));
    graph.insert_node(
        data_source_node(),
        node(
            "workflow.data-source",
            vec![data_source_output()],
            serde_json::Value::Null,
        ),
    );
    graph.insert_port(
        data_source_output(),
        data_output(data_source_node(), "value"),
    );
    if connected {
        graph.insert_edge(
            connected_edge(),
            Edge::new(EdgeKind::Exec, second_case_port(), target_input()),
        );
        graph.insert_edge(
            connected_data_edge(),
            Edge::new(
                EdgeKind::Data,
                data_source_output(),
                case_value_port(SECOND_CASE_PORT),
            ),
        );
    }
    graph.build().expect("valid Condition fixture graph")
}

fn condition_data(title: &str, cases: &[ConditionCase]) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "cases": cases
            .iter()
            .map(|case| serde_json::json!({
                "id": case.id(),
                "label": case.label(),
                "port_id": case.port_id(),
                "value_port_id": case.value_port_id(),
                "predicate": {
                    "operand": case.predicate().operand(),
                    "operator": "equals",
                    "literal": case.predicate().literal(),
                    "value_type": case.value_type(),
                },
            }))
            .collect::<Vec<_>>(),
        "else_branch": { "label": "Else" },
    })
}

fn case(id: &str, label: &str, port: u128) -> ConditionCase {
    ConditionCase::new(
        id,
        label,
        PortId::from_u128(port),
        case_value_port(port),
        string_predicate(label),
    )
    .expect("valid test case")
}

fn string_predicate(literal: &str) -> ConditionPredicate {
    ConditionPredicate::equals(
        "customer_intent",
        serde_json::Value::String(literal.to_owned()),
        TypeDesc::String,
    )
    .expect("valid string predicate")
}

fn node(kind: &str, ports: Vec<PortId>, data: serde_json::Value) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint::default(),
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
        ports,
        data,
    }
}

fn input(node: NodeId, key: &str) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::In,
        kind: PortKind::Exec,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn output(node: NodeId, key: &str, data: serde_json::Value) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::Out,
        kind: PortKind::Exec,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data,
    }
}

fn data_output(node: NodeId, key: &str) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(TypeDesc::String),
        data: serde_json::Value::Null,
    }
}

fn case_output(node: NodeId, case: &ConditionCase) -> Port {
    output(
        node,
        &format!("case.route.{}", case.id()),
        serde_json::json!({
            "case_id": case.id(),
            "label": case.label(),
        }),
    )
}

fn case_value_input(node: NodeId, case: &ConditionCase) -> Port {
    Port {
        node,
        key: PortKey::new(format!("case.value.{}", case.id())),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(case.value_type().clone()),
        data: serde_json::json!({
            "case_id": case.id(),
            "label": case.label(),
        }),
    }
}

fn condition_node() -> NodeId {
    NodeId::from_u128(CONDITION_NODE)
}

fn input_port() -> PortId {
    PortId::from_u128(INPUT_PORT)
}

fn first_case_port() -> PortId {
    PortId::from_u128(FIRST_CASE_PORT)
}

fn second_case_port() -> PortId {
    PortId::from_u128(SECOND_CASE_PORT)
}

fn case_value_port(route_port: u128) -> PortId {
    PortId::from_u128(route_port + CASE_VALUE_PORT_OFFSET)
}

fn else_port() -> PortId {
    PortId::from_u128(ELSE_PORT)
}

fn target_node() -> NodeId {
    NodeId::from_u128(TARGET_NODE)
}

fn target_input() -> PortId {
    PortId::from_u128(TARGET_INPUT)
}

fn connected_edge() -> EdgeId {
    EdgeId::from_u128(CONNECTED_EDGE)
}

fn data_source_node() -> NodeId {
    NodeId::from_u128(DATA_SOURCE_NODE)
}

fn data_source_output() -> PortId {
    PortId::from_u128(DATA_SOURCE_OUTPUT)
}

fn connected_data_edge() -> EdgeId {
    EdgeId::from_u128(CONNECTED_DATA_EDGE)
}

const BLUEPRINT_GRAPH: u128 = 0xb10e_0001;
const BLUEPRINT_NODE: u128 = 0xb10e_0002;
const BLUEPRINT_EXEC_IN: u128 = 0xb10e_0003;
const BLUEPRINT_EXEC_OUT: u128 = 0xb10e_0004;
const BLUEPRINT_VALUE: u128 = 0xb10e_0005;
const BLUEPRINT_FIRST_PIN: u128 = 0xb10e_0006;
const BLUEPRINT_SECOND_PIN: u128 = 0xb10e_0007;
const BLUEPRINT_ADDED_PIN: u128 = 0xb10e_0008;
const BLUEPRINT_SOURCE_NODE: u128 = 0xb10e_0009;
const BLUEPRINT_SOURCE_PORT: u128 = 0xb10e_000a;
const BLUEPRINT_COLLISION_PORT: u128 = 0xb10e_000b;
const BLUEPRINT_EDGE: u128 = 0xb10e_000c;

#[test]
fn blueprint_observation_parses_complete_pins_and_checks_intent_postconditions() {
    let runtime = blueprint_runtime(false);
    let snapshot = runtime.snapshot().expect("Blueprint snapshot");
    let observation = observe_blueprint(&snapshot);

    assert_eq!(observation.version(), snapshot.version());
    assert_eq!(observation.node_id(), blueprint_node());
    assert_eq!(observation.title(), "Material function");
    assert_eq!(
        observation.static_ports(),
        &[blueprint_exec_in(), blueprint_exec_out(), blueprint_value()]
    );
    assert_eq!(observation.pins(), &[blueprint_first(), blueprint_second()]);
    assert_eq!(
        observation
            .port(blueprint_first_pin())
            .and_then(|port| port.data.get("custom")),
        Some(&serde_json::json!("preserve")),
    );

    assert!(observation.satisfies(&BlueprintIntent::add(blueprint_first())));
    assert!(observation.satisfies(&BlueprintIntent::reorder("roughness", 0)));
    assert!(!observation.satisfies(&BlueprintIntent::reorder("roughness", 1)));
    assert!(observation.satisfies(&BlueprintIntent::remove(
        "absent",
        IncidentEdgePolicy::Reject,
    )));
    assert!(!observation.satisfies(&BlueprintIntent::remove(
        "metallic",
        IncidentEdgePolicy::Cascade,
    )));
}

#[test]
fn blueprint_schema_and_topology_fail_with_typed_errors() {
    assert_eq!(
        BlueprintPin::new(" ", PortId::from_u128(1), "Input", TypeDesc::Float),
        Err(BlueprintError::EmptyPinId),
    );
    assert_eq!(
        BlueprintPin::new("input", PortId::from_u128(1), " ", TypeDesc::Float),
        Err(BlueprintError::EmptyPinName),
    );

    let runtime = blueprint_runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let (mut blueprint, ports) = blueprint_parts(&snapshot);
    blueprint.data.as_object_mut().unwrap().remove("pins");
    assert!(matches!(
        BlueprintObservation::from_parts(snapshot.version(), blueprint_node(), &blueprint, ports),
        Err(BlueprintError::InvalidData { field: "pins", .. })
    ));

    let (mut blueprint, ports) = blueprint_parts(&snapshot);
    blueprint.data["pins"][0]["order"] = serde_json::json!(1);
    assert!(matches!(
        BlueprintObservation::from_parts(snapshot.version(), blueprint_node(), &blueprint, ports),
        Err(BlueprintError::InvalidData {
            field: "pins[].order",
            ..
        })
    ));

    let (blueprint, mut ports) = blueprint_parts(&snapshot);
    ports.get_mut(&blueprint_first_pin()).unwrap().ty = Some(TypeDesc::String);
    assert_eq!(
        BlueprintObservation::from_parts(snapshot.version(), blueprint_node(), &blueprint, ports,)
            .unwrap_err(),
        BlueprintError::InvalidPinPort {
            node: blueprint_node(),
            port: blueprint_first_pin(),
        }
    );

    let (mut blueprint, ports) = blueprint_parts(&snapshot);
    blueprint.ports.swap(0, 3);
    assert_eq!(
        BlueprintObservation::from_parts(snapshot.version(), blueprint_node(), &blueprint, ports,)
            .unwrap_err(),
        BlueprintError::InvalidPortOrder {
            node: blueprint_node(),
        }
    );

    let (mut blueprint, ports) = blueprint_parts(&snapshot);
    blueprint.kind = NodeKindKey::new("shader.not-blueprint");
    assert!(matches!(
        BlueprintObservation::from_parts(snapshot.version(), blueprint_node(), &blueprint, ports),
        Err(BlueprintError::WrongNodeKind { .. })
    ));
}

#[test]
fn blueprint_add_reorder_remove_are_atomic_and_preserve_existing_port_facts() {
    let runtime = blueprint_runtime(false);
    let initial = runtime.snapshot().unwrap();
    let added_pin =
        BlueprintPin::new("normal", blueprint_added_pin(), "Normal", TypeDesc::Float).unwrap();
    let command = observe_blueprint(&initial)
        .plan(&BlueprintIntent::add(added_pin.clone()))
        .unwrap();
    assert_eq!(
        runtime.execute(command).kind(),
        CommandExecutionKind::Applied
    );

    let added = runtime.snapshot().unwrap();
    assert_eq!(
        observe_blueprint(&added).pins(),
        &[blueprint_first(), blueprint_second(), added_pin.clone()]
    );
    let preserved = added.graph().port(&blueprint_first_pin()).unwrap();
    assert_eq!(preserved.connectable, Some(false));
    assert_eq!(preserved.data["custom"], "preserve");

    let command = observe_blueprint(&added)
        .plan(&BlueprintIntent::reorder(added_pin.id(), 0))
        .unwrap();
    assert_eq!(
        runtime.execute(command).kind(),
        CommandExecutionKind::Applied
    );
    let reordered = runtime.snapshot().unwrap();
    assert_eq!(
        observe_blueprint(&reordered).pins(),
        &[added_pin.clone(), blueprint_first(), blueprint_second()]
    );
    assert_eq!(
        &reordered.graph().node(&blueprint_node()).unwrap().ports[3..],
        &[
            blueprint_added_pin(),
            blueprint_first_pin(),
            blueprint_second_pin()
        ]
    );

    let command = observe_blueprint(&reordered)
        .plan(&BlueprintIntent::remove(
            "metallic",
            IncidentEdgePolicy::Cascade,
        ))
        .unwrap();
    assert_eq!(
        runtime.execute(command).kind(),
        CommandExecutionKind::Applied
    );
    let removed = runtime.snapshot().unwrap();
    assert_eq!(
        observe_blueprint(&removed).pins(),
        &[added_pin, blueprint_first()]
    );
    assert!(removed.graph().port(&blueprint_second_pin()).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 3);
}

#[test]
fn blueprint_planning_rejects_graph_wide_collisions_duplicates_and_stale_intents() {
    let runtime = blueprint_runtime(false);
    let snapshot = runtime.snapshot().unwrap();
    let observation = observe_blueprint(&snapshot);

    let colliding = BlueprintPin::new(
        "collision",
        blueprint_collision_port(),
        "Collision",
        TypeDesc::Float,
    )
    .unwrap();
    assert_eq!(
        observation
            .plan(&BlueprintIntent::add(colliding))
            .unwrap_err(),
        BlueprintError::PinPortAlreadyExists {
            port: blueprint_collision_port(),
        }
    );
    let duplicate_name = BlueprintPin::new(
        "different-id",
        blueprint_added_pin(),
        "Roughness",
        TypeDesc::Float,
    )
    .unwrap();
    assert_eq!(
        observation
            .plan(&BlueprintIntent::add(duplicate_name))
            .unwrap_err(),
        BlueprintError::DuplicatePinName {
            name: "Roughness".to_owned(),
        }
    );
    assert!(matches!(
        observation.plan(&BlueprintIntent::reorder("missing", 0)),
        Err(BlueprintError::PinNotFound { pin_id }) if pin_id == "missing"
    ));
    assert!(matches!(
        observation.plan(&BlueprintIntent::reorder("roughness", 99)),
        Err(BlueprintError::PinIndexOutOfBounds {
            index: 99,
            pin_count: 2,
            ..
        })
    ));
    assert!(matches!(
        observation.plan(&BlueprintIntent::remove(
            "missing",
            IncidentEdgePolicy::Cascade,
        )),
        Err(BlueprintError::PinNotFound { pin_id }) if pin_id == "missing"
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), snapshot.revision());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn blueprint_connected_remove_rejects_or_cascades_as_one_history_step() {
    let runtime = blueprint_runtime(true);
    let initial = runtime.snapshot().unwrap();
    let reject = observe_blueprint(&initial)
        .plan(&BlueprintIntent::remove(
            "metallic",
            IncidentEdgePolicy::Reject,
        ))
        .unwrap();
    assert_eq!(
        runtime.execute(reject).kind(),
        CommandExecutionKind::Rejected
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), initial.revision());

    let cascade = observe_blueprint(&initial)
        .plan(&BlueprintIntent::remove(
            "metallic",
            IncidentEdgePolicy::Cascade,
        ))
        .unwrap();
    assert_eq!(
        runtime.execute(cascade).kind(),
        CommandExecutionKind::Applied
    );
    let removed = runtime.snapshot().unwrap();
    assert!(removed.graph().edge(&blueprint_edge()).is_none());
    assert!(removed.graph().port(&blueprint_second_pin()).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    assert_eq!(
        runtime
            .execute(GraphCommand::undo(removed.version()))
            .kind(),
        CommandExecutionKind::Applied,
    );
    let restored = runtime.snapshot().unwrap();
    assert!(restored.graph().edge(&blueprint_edge()).is_some());
    assert_eq!(observe_blueprint(&restored).pins().len(), 2);
}

#[test]
fn blueprint_default_schema_and_source_are_renderer_independent() {
    assert_eq!(
        blueprint_default_data(),
        serde_json::json!({ "title": "Function Call", "pins": [] })
    );
    let source = include_str!("../src/product_domain/blueprint.rs");
    assert!(!source.contains("open_gpui"));
    assert!(!source.contains("TopologyAttempt"));
    let renderer = include_str!("../src/nodes/shader/blueprint_call.rs");
    assert!(!renderer.contains("TopologyAttempt"));
}

fn blueprint_runtime(connected: bool) -> GraphRuntime {
    GraphRuntime::builder(Default::default())
        .build(blueprint_graph(connected))
        .expect("valid pure Blueprint graph")
}

fn observe_blueprint(snapshot: &GraphSnapshot) -> BlueprintObservation {
    BlueprintObservation::from_snapshot(snapshot, blueprint_node())
        .expect("valid Blueprint observation")
}

fn blueprint_parts(snapshot: &GraphSnapshot) -> (Node, BTreeMap<PortId, Port>) {
    let node = snapshot
        .graph()
        .node(&blueprint_node())
        .expect("Blueprint node")
        .clone();
    let ports = node
        .ports
        .iter()
        .map(|port| (*port, snapshot.graph().port(port).unwrap().clone()))
        .collect();
    (node, ports)
}

fn blueprint_graph(connected: bool) -> Graph {
    let pins = [blueprint_first(), blueprint_second()];
    let mut graph = GraphBuilder::new(GraphId::from_u128(BLUEPRINT_GRAPH));
    graph.insert_node(
        blueprint_node(),
        node(
            BLUEPRINT_CALL_KIND,
            vec![
                blueprint_exec_in(),
                blueprint_exec_out(),
                blueprint_value(),
                blueprint_first_pin(),
                blueprint_second_pin(),
            ],
            blueprint_data("Material function", &pins),
        ),
    );
    graph.insert_port(blueprint_exec_in(), input(blueprint_node(), "exec_in"));
    graph.insert_port(
        blueprint_exec_out(),
        output(blueprint_node(), "exec_out", serde_json::Value::Null),
    );
    graph.insert_port(
        blueprint_value(),
        blueprint_data_input(blueprint_node(), "value", serde_json::Value::Null),
    );
    graph.insert_port(
        blueprint_first_pin(),
        blueprint_dynamic_input(blueprint_node(), &pins[0], true),
    );
    graph.insert_port(
        blueprint_second_pin(),
        blueprint_dynamic_input(blueprint_node(), &pins[1], false),
    );
    graph.insert_node(
        blueprint_source_node(),
        node(
            "shader.source",
            vec![blueprint_source_port(), blueprint_collision_port()],
            serde_json::Value::Null,
        ),
    );
    graph.insert_port(
        blueprint_source_port(),
        blueprint_data_output(blueprint_source_node(), "source"),
    );
    graph.insert_port(
        blueprint_collision_port(),
        blueprint_data_output(blueprint_source_node(), "occupied"),
    );
    if connected {
        graph.insert_edge(
            blueprint_edge(),
            Edge::new(
                EdgeKind::Data,
                blueprint_source_port(),
                blueprint_second_pin(),
            ),
        );
    }
    graph.build().expect("valid Blueprint fixture graph")
}

fn blueprint_data(title: &str, pins: &[BlueprintPin]) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "pins": pins
            .iter()
            .enumerate()
            .map(|(order, pin)| serde_json::json!({
                "id": pin.id(),
                "port_id": pin.port_id(),
                "name": pin.name(),
                "type": pin.ty(),
                "order": order,
            }))
            .collect::<Vec<_>>(),
    })
}

fn blueprint_dynamic_input(node: NodeId, pin: &BlueprintPin, custom: bool) -> Port {
    Port {
        node,
        key: PortKey::new(format!("arg.{}", pin.id())),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: custom.then_some(false),
        connectable_start: None,
        connectable_end: None,
        ty: Some(pin.ty().clone()),
        data: serde_json::json!({
            "pin_id": pin.id(),
            "label": pin.name(),
            "custom": custom.then_some("preserve"),
        }),
    }
}

fn blueprint_data_input(node: NodeId, key: &str, data: serde_json::Value) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(TypeDesc::Float),
        data,
    }
}

fn blueprint_data_output(node: NodeId, key: &str) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: PortDirection::Out,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(TypeDesc::Float),
        data: serde_json::Value::Null,
    }
}

fn blueprint_first() -> BlueprintPin {
    BlueprintPin::new(
        "roughness",
        blueprint_first_pin(),
        "Roughness",
        TypeDesc::Float,
    )
    .unwrap()
}

fn blueprint_second() -> BlueprintPin {
    BlueprintPin::new(
        "metallic",
        blueprint_second_pin(),
        "Metallic",
        TypeDesc::Float,
    )
    .unwrap()
}

fn blueprint_node() -> NodeId {
    NodeId::from_u128(BLUEPRINT_NODE)
}

fn blueprint_exec_in() -> PortId {
    PortId::from_u128(BLUEPRINT_EXEC_IN)
}

fn blueprint_exec_out() -> PortId {
    PortId::from_u128(BLUEPRINT_EXEC_OUT)
}

fn blueprint_value() -> PortId {
    PortId::from_u128(BLUEPRINT_VALUE)
}

fn blueprint_first_pin() -> PortId {
    PortId::from_u128(BLUEPRINT_FIRST_PIN)
}

fn blueprint_second_pin() -> PortId {
    PortId::from_u128(BLUEPRINT_SECOND_PIN)
}

fn blueprint_added_pin() -> PortId {
    PortId::from_u128(BLUEPRINT_ADDED_PIN)
}

fn blueprint_source_node() -> NodeId {
    NodeId::from_u128(BLUEPRINT_SOURCE_NODE)
}

fn blueprint_source_port() -> PortId {
    PortId::from_u128(BLUEPRINT_SOURCE_PORT)
}

fn blueprint_collision_port() -> PortId {
    PortId::from_u128(BLUEPRINT_COLLISION_PORT)
}

fn blueprint_edge() -> EdgeId {
    EdgeId::from_u128(BLUEPRINT_EDGE)
}

fn blackboard_runtime() -> GraphRuntime {
    GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(BLACKBOARD_GRAPH))
                .with_symbol(blackboard_primary(), blackboard_symbol("exposure"))
                .with_symbol(blackboard_other(), blackboard_symbol("other"))
                .build()
                .expect("valid pure Blackboard graph"),
        )
        .expect("pure Blackboard runtime")
}

fn blackboard_symbol(name: &str) -> Symbol {
    Symbol {
        name: name.to_owned(),
        ty: Some(TypeDesc::Float),
        default_value: Some(serde_json::json!(1.0)),
        meta: serde_json::json!({ "group": "material" }),
    }
}

fn blackboard_primary() -> SymbolId {
    SymbolId::from_u128(BLACKBOARD_PRIMARY)
}

fn blackboard_other() -> SymbolId {
    SymbolId::from_u128(BLACKBOARD_OTHER)
}

fn blackboard_added() -> SymbolId {
    SymbolId::from_u128(BLACKBOARD_ADDED)
}
