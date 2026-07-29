use std::cell::RefCell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow_open_gpui::activity::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::authoring::{
    OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiNodeObservation,
};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::retained_field::{
    RetainedField, RetainedFieldAttemptState, RetainedFieldConflictChoice, RetainedFieldError,
    RetainedFieldKey, RetainedFieldPhase, RetainedFieldResolution, RetainedFieldRetention,
    RetainedFieldStore, RetainedFieldSubmitMode, RetainedFieldUpdate,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::GraphCommand, commands::IncidentEdgePolicy, commands::TopologyEdit,
    graph::GraphRuntime, view::RendererStamp, view::ViewSessionOptions,
};
use open_gpui::{
    App, AppContext, Context, Entity, EntityInputHandler, Focusable, ParentElement, Render, Styled,
    Window, div, px,
};
use open_gpui_ui_components::{
    TextInput,
    gpui_adapter::{TextInputController, init_text_input},
};

const KIND: &str = "test.native-draft";
const RENDERER: &str = "test.native.draft";
const INPUT_SELECTOR: &str = "text-input:native-draft-prompt:root";
const FIELD_KEY: &str = "prompt";

fn renderer_descriptor<F, V>(factory: F) -> OpenGpuiNodeRendererDescriptor
where
    F: Fn(Entity<OpenGpuiNodeBinding>, &mut Window, &mut App) -> Entity<V> + 'static,
    V: Render + 'static,
{
    OpenGpuiNodeRendererDescriptor::new(
        KIND,
        1,
        RendererStamp::new(RENDERER, 1),
        OpenGpuiNodeAdapterPresentation::new("Native draft"),
        move |binding, window, cx| Ok(factory(binding, window, cx)),
    )
}

struct NativeDraftNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityHandle,
    controller: Entity<TextInputController>,
    field: RetainedField<String>,
    focus_lease: Option<OpenGpuiActivityLease>,
    ime_lease: Option<OpenGpuiActivityLease>,
}

impl NativeDraftNode {
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        activity: OpenGpuiNodeActivityHandle,
        controller: Entity<TextInputController>,
        field: RetainedField<String>,
    ) -> Self {
        Self {
            binding,
            activity,
            controller,
            field,
            focus_lease: None,
            ime_lease: None,
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
        self.field
            .edit(value)
            .expect("retain native input value before publishing it");
        self.field
            .set_composing(composing)
            .expect("retain native input composition before publishing it");
        cx.notify();
    }

    fn sync_from_graph(&mut self, cx: &mut Context<Self>) {
        let (_, observation) = binding_observation(&self.binding, cx);
        let update = self
            .field
            .observe(observation)
            .expect("retain graph reconciliation");
        if matches!(update, RetainedFieldUpdate::RemoteApplied) {
            let value = self.field.local().clone();
            self.controller
                .update(cx, |controller, cx| controller.set_value(value, cx));
        }
    }

    fn accept_remote(&mut self, cx: &mut Context<Self>) {
        let (authoring, observation) = binding_observation(&self.binding, cx);
        self.field
            .observe(observation)
            .expect("observe conflict authoring before resolution");
        assert!(matches!(
            self.field
                .resolve(&authoring, RetainedFieldConflictChoice::AcceptRemote),
            Ok(RetainedFieldResolution::AcceptedRemote { .. })
        ));
        let value = self.field.local().clone();
        self.controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        cx.notify();
    }

    fn reapply_local(&mut self, cx: &mut Context<Self>) {
        let (authoring, observation) = binding_observation(&self.binding, cx);
        self.field
            .observe(observation)
            .expect("observe current conflict authoring");
        let plan = self
            .field
            .plan(authoring, RetainedFieldSubmitMode::Reapply)
            .expect("explicit native input reapply plan");
        let expected_version = plan.authoring().version();
        let value = plan.value().clone();
        plan.dispatch(
            GraphCommand::set_node_data(expected_version, node_id(), value.into()),
            cx,
        )
        .expect("dispatch native input command");
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
    let (runtime, host, node, controller, activity, _store, cx) = mount_scene(cx);
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            node_id(),
            "committed".into(),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("seed command applies");
    draw(cx);
    focus_and_mark_local(&node, &controller, cx, "local 中文🙂");
    assert_eq!(activity.active_count(), 3);
    let input_focus = cx.read_entity(&controller, |controller, cx| controller.focus_handle(cx));

    runtime
        .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo applies");
    draw(cx);

    cx.read_entity(&node, |node, _| {
        assert_eq!(node.field.phase(), RetainedFieldPhase::ComposingConflict);
        assert_eq!(node.field.local(), "local 中文🙂");
        assert_eq!(node.field.remote(), "base");
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
        .execute(GraphCommand::redo(runtime.snapshot().unwrap().version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("redo applies");
    draw(cx);

    cx.read_entity(&node, |node, _| {
        assert_eq!(node.field.phase(), RetainedFieldPhase::ComposingConflict);
        assert_eq!(node.field.local(), "local 中文🙂");
        assert_eq!(node.field.remote(), "committed");
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
        assert_eq!(node.field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(node.field.local(), "committed");
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "committed");
        assert!(controller.marked_range_utf16().is_none());
    });
    assert_eq!(runtime.snapshot().unwrap().revision(), 3);
    assert_eq!(activity.active_count(), 1);

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    draw(cx);
    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().strong_instances(), 0)
    });
}

#[open_gpui::test]
fn composing_conflict_reapplies_only_after_explicit_unmark_and_revision_checked_command(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, _host, node, controller, activity, _store, cx) = mount_scene(cx);
    focus_and_mark_local(&node, &controller, cx, "local branch");
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            node_id(),
            "remote branch".into(),
        ))
        .into_result()
        .unwrap();
    draw(cx);

    cx.update_entity(&node, |node, cx| {
        let authoring = node.binding.read(cx).authoring().expect("live authoring");
        assert!(matches!(
            node.field.plan(authoring, RetainedFieldSubmitMode::Reapply),
            Err(RetainedFieldError::CompositionActive)
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
        assert_eq!(node.field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(node.field.base_revision(), 2);
    });
    cx.read_entity(&controller, |controller, _| {
        assert_eq!(controller.value(), "local branch");
        assert!(controller.marked_range_utf16().is_none());
    });
    assert_eq!(activity.active_count(), 1);
}

#[open_gpui::test]
fn internal_supervisor_owns_dispatch_ticket_and_releases_task_resources(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, host, node, _controller, activity, store, cx) = mount_scene(cx);

    cx.update_entity(&node, |node, cx| {
        dispatch_test_value(node, "supervised value", cx)
            .expect("internal supervisor retains the dispatched ticket");
    });
    assert_eq!(activity.active_count(), 2);

    draw(cx);
    draw(cx);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id())
            .unwrap()
            .data,
        serde_json::json!("supervised value")
    );
    cx.read_entity(&node, |node, _| {
        assert_eq!(node.field.attempt_state(), RetainedFieldAttemptState::Idle);
        assert_eq!(node.field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(node.field.retention(), RetainedFieldRetention::Idle);
    });
    assert_eq!(activity.active_count(), 0);
    assert!(store.is_empty());

    let settled_revision = runtime.snapshot().unwrap().revision();
    let settled_resources = cx.read_entity(&host, |host, _| host.resource_snapshot());
    for frame in 0..120 {
        draw(cx);
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.field.attempt_state(), RetainedFieldAttemptState::Idle);
            assert_eq!(node.field.phase(), RetainedFieldPhase::Clean);
            assert_eq!(node.field.retention(), RetainedFieldRetention::Idle);
        });
        assert_eq!(activity.active_count(), 0, "frame {frame}");
        assert_eq!(activity.protected_count(), 0, "frame {frame}");
        assert!(store.is_empty(), "frame {frame}");
    }
    assert_eq!(runtime.snapshot().unwrap().revision(), settled_revision);
    cx.read_entity(&host, |host, _| {
        let resources = host.resource_snapshot();
        assert_eq!(
            resources.strong_instances(),
            settled_resources.strong_instances()
        );
        assert_eq!(resources.protected_instances(), 0);
        assert_eq!(resources.protected_leases(), 0);
    });
}

#[open_gpui::test]
fn focused_native_input_survives_culling_then_blurs_parks_and_hard_retires(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let (runtime, host, node, controller, activity, _store, cx) = mount_scene(cx);

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
        assert_eq!(host.resource_snapshot().active_nodes(), 1);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
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
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 1);
        assert_eq!(host.resource_snapshot().eligible_parked_nodes(), 1);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 1);
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(node_id(), IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap();
    draw(cx);

    assert!(activity.is_revoked());
    assert_eq!(activity.active_count(), 0);
    assert_eq!(activity.protected_count(), 0);
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().active_nodes(), 0);
        assert_eq!(host.resource_snapshot().parked_nodes(), 0);
        assert_eq!(host.resource_snapshot().strong_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
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
        assert_eq!(node.field.phase(), RetainedFieldPhase::Composing);
    });
    draw(cx);
}

fn dispatch_test_value(
    node: &mut NativeDraftNode,
    value: &str,
    cx: &mut Context<NativeDraftNode>,
) -> Result<(), RetainedFieldError> {
    let (authoring, observation) = binding_observation(&node.binding, cx);
    node.field.observe(observation)?;
    node.field.edit(value.to_owned())?;
    let plan = node
        .field
        .plan(authoring, RetainedFieldSubmitMode::Commit)?;
    let version = plan.authoring().version();
    let command = GraphCommand::set_node_data(version, node_id(), plan.value().clone().into());
    plan.dispatch(command, cx)
}

fn mount_scene(
    cx: &mut open_gpui::TestAppContext,
) -> (
    Rc<GraphRuntime>,
    Entity<OpenGpuiSceneHost>,
    Entity<NativeDraftNode>,
    Entity<TextInputController>,
    OpenGpuiNodeActivityHandle,
    RetainedFieldStore<String>,
    &mut open_gpui::VisualTestContext,
) {
    let node_catalog = catalog();
    let runtime = Rc::new(
        GraphRuntime::builder(node_catalog.clone())
            .build(graph())
            .unwrap(),
    );
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let captured_node = Rc::new(RefCell::new(None));
    let captured_controller = Rc::new(RefCell::new(None));
    let captured_activity = Rc::new(RefCell::new(None));
    let factory_node = captured_node.clone();
    let factory_controller = captured_controller.clone();
    let factory_activity = captured_activity.clone();
    let field_store = RetainedFieldStore::new();
    let factory_field_store = field_store.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(renderer_descriptor(move |binding, _window, cx| {
            let activity = binding.read(cx).activity().clone();
            let (_authoring, observation) = binding_observation(&binding, cx);
            let value = observation.value().clone();
            let controller = cx.new(|cx| TextInputController::with_value(value.clone(), cx));
            let field = RetainedField::mount(
                observation,
                RetainedFieldKey::new(FIELD_KEY).expect("valid native input field key"),
                factory_field_store.clone(),
            )
            .expect("mount native retained field");
            let node = cx.new(|_| {
                NativeDraftNode::new(binding, activity.clone(), controller.clone(), field)
            });
            *factory_node.borrow_mut() = Some(node.clone());
            *factory_controller.borrow_mut() = Some(controller);
            *factory_activity.borrow_mut() = Some(activity);
            node
        }))
        .expect("unique native-draft renderer");

    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&node_catalog).expect("valid test registry"),
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
    (runtime, host, node, controller, activity, field_store, cx)
}

fn sync_lease(
    activity: &OpenGpuiNodeActivityHandle,
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

fn binding_observation(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &open_gpui::App,
) -> (OpenGpuiNodeAuthoring, OpenGpuiNodeObservation<String>) {
    let authoring = binding
        .read(cx)
        .authoring()
        .expect("live binding authoring");
    let observation =
        authoring.observe_field(|node| node.data.as_str().expect("string prompt").to_owned());
    (authoring, observation)
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();
}

fn graph() -> jellyflow_core::Graph {
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
        .register(NodeKindSpec::builder(KIND).build())
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
