use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow::runtime::{
    FrameRequest, GraphCommand, GraphRuntime, IncidentEdgePolicy, NodeInstanceKey, RendererStamp,
    TopologyEdit, ViewSession, ViewSessionOptions,
};
use jellyflow_open_gpui::{
    DraftConflict, DraftPhase, DraftSubmissionDecision, DraftSubmissionIntent,
    ExternalizedDraftStore, RevisionedDraft,
};

const NODE: u128 = 0xd7af_7001;
const SIBLING_NODE: u128 = 0xd7af_7002;

#[test]
fn restore_reconciles_with_current_authority_and_clears_a_converged_interrupted_attempt() {
    let (_runtime, view) = runtime_and_view();
    let instance = instance(&view, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let mut draft = RevisionedDraft::new(0, String::from("base"));
    draft.edit_local(String::from("submitted"));
    let attempt = submission(draft.plan_submission_at(0)).attempt();

    assert!(store.externalize_overflow(&instance, "prompt", &draft));
    let restored = store.restore(&instance, "prompt", 1, String::from("submitted"));

    assert_eq!(restored.draft().phase(), DraftPhase::Clean);
    assert_eq!(restored.draft().pending_attempt(), None);
    assert_eq!(restored.recovery().interrupted_attempt(), None);
    assert!(!restored.recovery().composition_interrupted());
    assert!(store.is_empty());

    let mut draft = restored.into_draft();
    draft.edit_local(String::from("next"));
    let next = submission(draft.plan_submission_at(1));
    assert_ne!(next.attempt(), attempt);
}

#[test]
fn restore_marks_interrupted_composition_without_recreating_framework_ime_state() {
    let (_runtime, view) = runtime_and_view();
    let instance = instance(&view, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let mut draft = RevisionedDraft::new(0, String::from("base"));
    draft.mark_composing();
    draft.edit_local(String::from("partial composition"));

    assert!(store.externalize_overflow(&instance, "prompt", &draft));
    let restored = store.restore(&instance, "prompt", 0, String::from("base"));

    assert_eq!(restored.draft().local(), "partial composition");
    assert_eq!(restored.draft().phase(), DraftPhase::Dirty);
    assert!(restored.recovery().composition_interrupted());
}

#[test]
fn restore_retires_snapshots_from_an_old_renderer_identity() {
    let (_runtime, view) = runtime_and_view();
    let old = instance(&view, "test.renderer.old", 1);
    let current = instance(&view, "test.renderer.current", 2);
    let store = ExternalizedDraftStore::<String>::new();
    let mut draft = RevisionedDraft::new(0, String::from("base"));
    draft.edit_local(String::from("old renderer draft"));
    assert!(store.externalize_overflow(&old, "prompt", &draft));

    let restored = store.restore(&current, "prompt", 0, String::from("current"));

    assert_eq!(restored.draft().local(), "current");
    assert_eq!(restored.draft().phase(), DraftPhase::Clean);
    assert!(restored.recovery().is_empty());
    assert!(store.is_empty());
}

#[test]
fn bound_store_clears_externalized_state_when_the_view_closes() {
    let (_runtime, view) = runtime_and_view();
    let instance = instance(&view, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let _subscription = store.bind(&view).expect("bind draft store");
    let mut draft = RevisionedDraft::new(0, String::from("base"));
    draft.edit_local(String::from("dirty"));
    assert!(store.externalize_overflow(&instance, "prompt", &draft));
    assert_eq!(store.len(), 1);

    view.close().expect("close view");

    assert!(store.is_empty());
}

#[test]
fn runtime_incarnation_patch_retires_only_the_replaced_node_draft() {
    let runtime = runtime_with_nodes(&[NODE, SIBLING_NODE]);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("draft view");
    let retired = instance_for(&view, NODE, "test.renderer", 1);
    let sibling = instance_for(&view, SIBLING_NODE, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let _subscription = store.bind(&view).expect("bind draft store");
    let mut retired_draft = RevisionedDraft::new(0, String::from("retired remote"));
    retired_draft.edit_local(String::from("retired local"));
    let mut sibling_draft = RevisionedDraft::new(0, String::from("sibling remote"));
    sibling_draft.edit_local(String::from("sibling local"));

    assert!(store.externalize_overflow(&retired, "prompt", &retired_draft));
    assert!(store.externalize_overflow(&sibling, "prompt", &sibling_draft));
    assert_eq!(store.len(), 2);

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(NodeId::from_u128(NODE), IncidentEdgePolicy::Reject),
        ))
        .expect("retire draft node");

    assert_eq!(store.len(), 1);
    assert!(!store.discard_externalized(&retired, "prompt"));

    runtime
        .execute(GraphCommand::undo(1))
        .expect("restore draft node");
    let replacement = instance_for(&view, NODE, "test.renderer", 1);
    assert_ne!(retired.incarnation(), replacement.incarnation());

    let replacement_draft = store.restore(
        &replacement,
        "prompt",
        2,
        String::from("replacement remote"),
    );
    assert_eq!(replacement_draft.draft().local(), "replacement remote");
    assert_eq!(replacement_draft.draft().phase(), DraftPhase::Clean);
    assert_eq!(store.len(), 1);

    let sibling_draft = store.restore(&sibling, "prompt", 2, String::from("sibling remote"));
    assert_eq!(sibling_draft.draft().local(), "sibling local");
    assert_eq!(sibling_draft.draft().phase(), DraftPhase::Dirty);
    assert!(store.is_empty());
}

#[test]
fn closing_one_bound_view_preserves_another_views_externalized_draft() {
    let runtime = runtime_with_nodes(&[NODE]);
    let first_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first draft view");
    let second_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second draft view");
    let first = instance(&first_view, "test.renderer", 1);
    let second = instance(&second_view, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let _first_subscription = store.bind(&first_view).expect("bind first draft view");
    let _second_subscription = store.bind(&second_view).expect("bind second draft view");
    let mut first_draft = RevisionedDraft::new(0, String::from("first remote"));
    first_draft.edit_local(String::from("first local"));
    let mut second_draft = RevisionedDraft::new(0, String::from("second remote"));
    second_draft.edit_local(String::from("second local"));

    assert!(store.externalize_overflow(&first, "prompt", &first_draft));
    assert!(store.externalize_overflow(&second, "prompt", &second_draft));
    assert_eq!(store.len(), 2);

    first_view.close().expect("close first draft view");

    assert_eq!(store.len(), 1);
    assert!(!store.discard_externalized(&first, "prompt"));
    let restored = store.restore(&second, "prompt", 0, String::from("second remote"));
    assert_eq!(restored.draft().local(), "second local");
    assert_eq!(restored.draft().phase(), DraftPhase::Dirty);
    assert!(store.is_empty());
}

#[test]
fn restore_uses_exact_field_identity_and_preserves_conflict_recovery() {
    let (_runtime, view) = runtime_and_view();
    let instance = instance(&view, "test.renderer", 1);
    let store = ExternalizedDraftStore::<String>::new();
    let mut prompt = RevisionedDraft::new(10, String::from("prompt base"));
    prompt.edit_local(String::from("prompt local"));
    let interrupted_attempt = submission(prompt.plan_submission_at(10)).attempt();
    prompt.mark_composing();
    prompt.reconcile_remote(11, String::from("prompt remote"));
    let conflict = DraftConflict::new(10, 11);
    assert_eq!(prompt.conflict(), Some(conflict));

    let mut title = RevisionedDraft::new(11, String::from("title remote"));
    title.edit_local(String::from("title local"));
    assert!(store.externalize_overflow(&instance, "prompt", &prompt));
    assert!(store.externalize_overflow(&instance, "title", &title));
    assert_eq!(store.len(), 2);

    let restored_title = store.restore(&instance, "title", 11, String::from("title remote"));
    assert_eq!(restored_title.draft().local(), "title local");
    assert_eq!(restored_title.draft().remote(), "title remote");
    assert_eq!(restored_title.draft().conflict(), None);
    assert!(restored_title.recovery().is_empty());
    assert_eq!(store.len(), 1);

    let restored_prompt = store.restore(&instance, "prompt", 11, String::from("prompt remote"));
    assert_eq!(restored_prompt.draft().local(), "prompt local");
    assert_eq!(restored_prompt.draft().remote(), "prompt remote");
    assert_eq!(restored_prompt.draft().conflict(), Some(conflict));
    assert_eq!(restored_prompt.draft().phase(), DraftPhase::Conflict);
    assert_eq!(
        restored_prompt.recovery().interrupted_attempt(),
        Some(interrupted_attempt)
    );
    assert!(restored_prompt.recovery().composition_interrupted());
    assert!(store.is_empty());
}

fn runtime_and_view() -> (GraphRuntime, ViewSession) {
    let runtime = runtime_with_nodes(&[NODE]);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("draft view");
    (runtime, view)
}

fn runtime_with_nodes(node_ids: &[u128]) -> GraphRuntime {
    let graph = node_ids
        .iter()
        .copied()
        .fold(
            GraphBuilder::new(GraphId::from_u128(0xd7af_7000)),
            |builder, node| builder.with_node(NodeId::from_u128(node), draft_node()),
        )
        .build()
        .expect("draft graph");
    GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("draft runtime")
}

fn draft_node() -> Node {
    Node {
        kind: NodeKindKey::new("test.draft"),
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
        size: Some(CanvasSize {
            width: 180.0,
            height: 96.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn instance(view: &ViewSession, renderer: &str, revision: u64) -> NodeInstanceKey {
    instance_for(view, NODE, renderer, revision)
}

fn instance_for(view: &ViewSession, node: u128, renderer: &str, revision: u64) -> NodeInstanceKey {
    view.frame(FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 800.0,
                height: 600.0,
            },
        },
        RendererStamp::new(renderer, revision),
    ))
    .expect("draft frame")
    .nodes()
    .iter()
    .find(|mounted| mounted.instance().node() == NodeId::from_u128(node))
    .expect("draft node")
    .instance()
    .clone()
}

fn submission(decision: DraftSubmissionDecision<String>) -> DraftSubmissionIntent<String> {
    match decision {
        DraftSubmissionDecision::Submit(intent) => intent,
        DraftSubmissionDecision::Rejected(rejection) => {
            panic!("expected draft submission, got {rejection:?}")
        }
    }
}
