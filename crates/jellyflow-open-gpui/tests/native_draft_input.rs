use std::cell::RefCell;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{
    GraphCommand, GraphRevision, GraphRuntime, IncidentEdgePolicy, TopologyEdit, ViewSessionOptions,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::{
    DraftConflictResolution, DraftIntentRejection, DraftPhase, DraftReconcileDecision,
    DraftResolutionDecision, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry, OpenGpuiNodeRegistry, OpenGpuiSceneHost, RevisionedDraft,
};
use open_gpui::{
    AppContext, Context, Entity, EntityInputHandler, Focusable, ParentElement, Render, Styled,
    Task, Window, div, px,
};
use open_gpui_ui_components::{
    TextInput,
    gpui_adapter::{TextInputController, init_text_input},
};

const KIND: &str = "test.native-draft";
const RENDERER: &str = "test.native.draft";
const INPUT_SELECTOR: &str = "text-input:native-draft-prompt:root";

struct NativeDraftNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityRegistry,
    controller: Entity<TextInputController>,
    draft: RevisionedDraft<String>,
    focus_lease: Option<OpenGpuiActivityLease>,
    ime_lease: Option<OpenGpuiActivityLease>,
    draft_lease: Option<OpenGpuiActivityLease>,
    reapply_task: Option<Task<()>>,
}

impl NativeDraftNode {
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        activity: OpenGpuiNodeActivityRegistry,
        controller: Entity<TextInputController>,
        revision: GraphRevision,
        value: String,
    ) -> Self {
        Self {
            binding,
            activity,
            controller,
            draft: RevisionedDraft::new(revision, value),
            focus_lease: None,
            ime_lease: None,
            draft_lease: None,
            reapply_task: None,
        }
    }

    fn sync_from_controller(&mut self, cx: &mut Context<Self>) {
        let (value, composing) = {
            let controller = self.controller.read(cx);
            (
                controller.value().to_owned(),
                controller.marked_range_utf16().is_some(),
            )
        };
        self.draft.edit_local(value);
        if composing {
            self.draft.mark_composing();
        } else {
            self.draft.unmark_composing();
        }
        cx.notify();
    }

    fn sync_from_graph(&mut self, cx: &mut Context<Self>) {
        let (revision, value) = binding_value(&self.binding, cx);
        let decision = self.draft.reconcile_remote(revision, value);
        if matches!(decision, DraftReconcileDecision::RemoteApplied { .. }) {
            let value = self.draft.local().clone();
            self.controller
                .update(cx, |controller, cx| controller.set_value(value, cx));
        }
    }

    fn accept_remote(&mut self, cx: &mut Context<Self>) {
        assert!(matches!(
            self.draft.resolve_at(
                DraftConflictResolution::AcceptRemote,
                self.draft.remote_revision(),
            ),
            DraftResolutionDecision::AcceptedRemote { .. }
        ));
        let value = self.draft.local().clone();
        self.controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        cx.notify();
    }

    fn reapply_local(&mut self, cx: &mut Context<Self>) {
        let intent = match self.draft.resolve_at(
            DraftConflictResolution::ReapplyLocal,
            self.draft.remote_revision(),
        ) {
            DraftResolutionDecision::Reapply(intent) => intent,
            decision => panic!("expected a reapply intent, got {decision:?}"),
        };
        let attempt = intent.attempt();
        let expected_revision = intent.expected_revision();
        let value = intent.value().clone();
        let node = node_id();
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("live retained binding");
        assert_eq!(
            authoring.revision(),
            expected_revision,
            "draft intent must target the current authoring revision",
        );
        let ticket = authoring
            .enqueue(
                GraphCommand::set_node_data(expected_revision, node, value.into()),
                cx,
            )
            .expect("binding command enqueue");
        self.reapply_task = Some(cx.spawn(async move |node, cx| {
            let outcome = ticket.await.expect("binding command dispatch");
            let revision = outcome
                .applied()
                .expect("reapply command applies")
                .revision();
            node.update(cx, |node, cx| {
                node.draft.record_self_receipt(attempt, revision);
                node.reapply_task = None;
                cx.notify();
            })
            .expect("native draft node remains mounted");
        }));
    }

    fn sync_activity(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (focused, composing) = {
            let controller = self.controller.read(cx);
            (
                controller.focus_handle(cx).is_focused(window),
                controller.marked_range_utf16().is_some(),
            )
        };
        sync_lease(
            &self.activity,
            &mut self.focus_lease,
            OpenGpuiActivityKind::Focus,
            focused,
        );
        sync_lease(
            &self.activity,
            &mut self.ime_lease,
            OpenGpuiActivityKind::Ime,
            composing,
        );
        sync_lease(
            &self.activity,
            &mut self.draft_lease,
            OpenGpuiActivityKind::Draft,
            self.draft.phase() != DraftPhase::Clean,
        );
    }
}

impl Render for NativeDraftNode {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.sync_from_graph(cx);
        self.sync_activity(window, cx);
        div().w(px(280.0)).min_h(px(96.0)).p(px(12.0)).child(
            TextInput::new("native-draft-prompt", "Prompt").controller(self.controller.clone()),
        )
    }
}

#[open_gpui::test]
fn focused_composing_draft_survives_real_undo_and_redo_until_remote_is_accepted(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, host, node, controller, activity, cx) = mount_scene(cx);
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "committed".into(),
        ))
        .unwrap()
        .applied()
        .expect("seed command applies");
    draw(cx);
    focus_and_mark_local(&node, &controller, cx, "local 中文🙂");
    assert_eq!(activity.active_count(), 3);
    let input_focus = cx.read_entity(&controller, |controller, cx| controller.focus_handle(cx));

    runtime
        .execute(GraphCommand::undo(1))
        .unwrap()
        .applied()
        .expect("undo applies");
    draw(cx);

    cx.read_entity(&node, |node, _| {
        assert_eq!(node.draft.phase(), DraftPhase::ComposingConflict);
        assert_eq!(node.draft.local(), "local 中文🙂");
        assert_eq!(node.draft.remote(), "base");
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "local 中文🙂");
        assert!(controller.marked_range_utf16().is_some());
    });
    cx.update(|window, cx| {
        assert!(input_focus.is_focused(window));
        assert_eq!(window.focused(cx), Some(input_focus.clone()));
    });
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);

    runtime
        .execute(GraphCommand::redo(2))
        .unwrap()
        .applied()
        .expect("redo applies");
    draw(cx);

    cx.read_entity(&node, |node, _| {
        assert_eq!(node.draft.phase(), DraftPhase::ComposingConflict);
        assert_eq!(node.draft.local(), "local 中文🙂");
        assert_eq!(node.draft.remote(), "committed");
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "local 中文🙂");
        assert!(controller.marked_range_utf16().is_some());
    });
    cx.update(|window, cx| {
        assert!(input_focus.is_focused(window));
        assert_eq!(window.focused(cx), Some(input_focus.clone()));
    });
    assert_eq!(runtime.snapshot().unwrap().revision(), 3);

    cx.update_entity(&node, |node, cx| node.accept_remote(cx));
    draw(cx);
    cx.read_entity(&node, |node, _| {
        assert_eq!(node.draft.phase(), DraftPhase::Clean);
        assert_eq!(node.draft.local(), "committed");
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "committed");
        assert!(controller.marked_range_utf16().is_none());
    });
    assert_eq!(runtime.snapshot().unwrap().revision(), 3);
    assert_eq!(activity.active_count(), 1);

    runtime
        .execute(GraphCommand::topology(
            3,
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    draw(cx);
    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().strong_instances(), 0)
    });
}

#[open_gpui::test]
fn composing_conflict_reapplies_only_after_explicit_unmark_and_revision_checked_command(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, _host, node, controller, activity, cx) = mount_scene(cx);
    focus_and_mark_local(&node, &controller, cx, "local branch");
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            node_id(),
            "remote branch".into(),
        ))
        .unwrap();
    draw(cx);

    cx.update_entity(&node, |node, _| {
        assert!(matches!(
            node.draft.resolve_at(
                DraftConflictResolution::ReapplyLocal,
                node.draft.remote_revision(),
            ),
            DraftResolutionDecision::Rejected(DraftIntentRejection::CompositionActive)
        ));
    });
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);

    cx.update(|window, cx| {
        controller.update(cx, |controller, cx| {
            EntityInputHandler::unmark_text(controller, window, cx)
        });
    });
    cx.update_entity(&node, |node, cx| node.sync_from_controller(cx));
    cx.update_entity(&node, |node, cx| node.reapply_local(cx));
    draw(cx);
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);

    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .unwrap()
            .data,
        serde_json::json!("local branch")
    );
    cx.read_entity(&node, |node, _| {
        assert_eq!(node.draft.phase(), DraftPhase::Clean);
        assert_eq!(node.draft.base_revision(), 2);
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "local branch");
        assert!(controller.marked_range_utf16().is_none());
    });
    assert_eq!(activity.active_count(), 1);
}

#[open_gpui::test]
fn focused_native_input_survives_culling_then_blurs_parks_and_hard_retires(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, host, node, controller, activity, cx) = mount_scene(cx);

    let input = cx.debug_bounds(INPUT_SELECTOR).expect("native text input");
    cx.simulate_click(input.center(), Default::default());
    draw(cx);

    let input_focus = cx.read_entity(&controller, |controller, cx| controller.focus_handle(cx));
    let weak_node = node.downgrade();
    let weak_controller = controller.downgrade();
    let weak_input_focus = input_focus.downgrade();
    cx.update(|window, cx| {
        assert!(input_focus.is_focused(window));
        assert_eq!(window.focused(cx), Some(input_focus.clone()));
    });
    assert_eq!(activity.active_count(), 1);
    assert_eq!(activity.protected_count(), 1);

    cx.update_entity(&host, |host, cx| {
        host.set_frame_bounds(offscreen_viewport());
        cx.notify();
    });
    draw(cx);

    cx.update(|window, cx| {
        assert!(input_focus.is_focused(window));
        assert_eq!(window.focused(cx), Some(input_focus.clone()));
    });
    assert_eq!(activity.active_count(), 1);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 1);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().protected_instances(), 1);
        assert_eq!(host.stats().protected_leases(), 1);
    });

    cx.update(|window, _| window.blur());
    draw(cx);
    assert_eq!(activity.active_count(), 0);
    assert_eq!(activity.protected_count(), 0);
    cx.update(|window, cx| {
        assert!(!input_focus.is_focused(window));
        assert!(window.focused(cx).is_none());
    });

    cx.update_entity(&host, |_, cx| cx.notify());
    draw(cx);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 1);
        assert_eq!(host.stats().eligible_parked_nodes(), 1);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().strong_instances(), 1);
    });
    cx.update(|window, cx| {
        assert!(!input_focus.is_focused(window));
        assert!(window.focused(cx).is_none());
    });

    drop(input_focus);
    drop(controller);
    drop(node);
    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    draw(cx);

    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    assert_eq!(activity.protected_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.stats().active_nodes(), 0);
        assert_eq!(host.stats().parked_nodes(), 0);
        assert_eq!(host.stats().strong_instances(), 0);
        assert_eq!(host.stats().protected_instances(), 0);
        assert_eq!(host.stats().protected_leases(), 0);
        assert_eq!(host.stats().hard_retirements(), 1);
    });
    cx.update(|window, cx| assert!(window.focused(cx).is_none()));
    weak_node.assert_released();
    weak_controller.assert_released();
    assert!(weak_input_focus.upgrade().is_none());
}

fn focus_and_mark_local(
    node: &Entity<NativeDraftNode>,
    controller: &Entity<TextInputController>,
    cx: &mut open_gpui::VisualTestContext,
    value: &str,
) {
    let input = cx.debug_bounds(INPUT_SELECTOR).expect("native text input");
    cx.simulate_click(input.center(), Default::default());
    cx.update(|window, cx| {
        controller.update(cx, |controller, cx| {
            assert!(controller.accepts_editing());
            let full_range = 0..controller.value().encode_utf16().count();
            EntityInputHandler::replace_and_mark_text_in_range(
                controller,
                Some(full_range),
                value,
                Some(0..0),
                window,
                cx,
            )
        });
    });
    cx.read_entity(controller, |controller, _| {
        assert_eq!(controller.value(), value);
        assert!(controller.marked_range_utf16().is_some());
    });
    cx.update_entity(node, |node, cx| node.sync_from_controller(cx));
    cx.read_entity(node, |node, _| {
        assert_eq!(node.draft.phase(), DraftPhase::Composing);
    });
    draw(cx);
}

fn mount_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    Entity<NativeDraftNode>,
    Entity<TextInputController>,
    OpenGpuiNodeActivityRegistry,
    &mut open_gpui::VisualTestContext,
) {
    let runtime = Rc::new(GraphRuntime::builder(catalog()).build(graph()).unwrap());
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let captured_node = Rc::new(RefCell::new(None));
    let captured_controller = Rc::new(RefCell::new(None));
    let captured_activity = Rc::new(RefCell::new(None));
    let factory_node = captured_node.clone();
    let factory_controller = captured_controller.clone();
    let factory_activity = captured_activity.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry.register(RENDERER, move |binding, _window, cx| {
        let activity = binding.read(cx).activity().clone();
        let (revision, value) = binding_value(&binding, cx);
        let controller = cx.new(|cx| TextInputController::with_value(value.clone(), cx));
        let node = cx.new(|_| {
            NativeDraftNode::new(
                binding,
                activity.clone(),
                controller.clone(),
                revision,
                value,
            )
        });
        *factory_node.borrow_mut() = Some(node.clone());
        *factory_controller.borrow_mut() = Some(controller);
        *factory_activity.borrow_mut() = Some(activity);
        node
    });

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let node = captured_node
        .borrow_mut()
        .take()
        .expect("native draft node");
    let controller = captured_controller
        .borrow_mut()
        .take()
        .expect("text input controller");
    let activity = captured_activity
        .borrow_mut()
        .take()
        .expect("node activity registry");
    (runtime, host, node, controller, activity, cx)
}

fn sync_lease(
    activity: &OpenGpuiNodeActivityRegistry,
    lease: &mut Option<OpenGpuiActivityLease>,
    kind: OpenGpuiActivityKind,
    active: bool,
) {
    if active && lease.is_none() {
        *lease = Some(activity.acquire(kind).expect("activity lease"));
    } else if !active {
        lease.take();
    }
}

fn binding_value(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &open_gpui::App,
) -> (GraphRevision, String) {
    let authoring = binding
        .read(cx)
        .authoring()
        .expect("live binding authoring");
    let value = authoring
        .node()
        .data
        .as_str()
        .expect("string prompt")
        .to_owned();
    (authoring.revision(), value)
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
}

fn graph() -> jellyflow::core::Graph {
    GraphBuilder::new(GraphId::from_u128(0xd4_af7))
        .with_node(
            node_id(),
            Node {
                kind: NodeKindKey::new(KIND),
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
                    width: 280.0,
                    height: 96.0,
                }),
                hidden: false,
                collapsed: false,
                ports: Vec::new(),
                data: serde_json::json!("base"),
            },
        )
        .build()
        .unwrap()
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND, "Native draft", RENDERER)
                .default_size(CanvasSize {
                    width: 280.0,
                    height: 96.0,
                })
                .build(),
        )
        .unwrap();
    catalog
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}

fn offscreen_viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: 10_000.0,
            y: 10_000.0,
        },
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}

fn node_id() -> NodeId {
    NodeId::from_u128(1)
}
