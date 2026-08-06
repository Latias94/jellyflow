use egui::{Id, Key, Modifiers, Popup, Sense, TextEdit, Ui, Vec2};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasSize, EdgeId, GraphBuilder, GraphId, Node, NodeGraphConnectionMode, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{EguiCanvasInteraction, EguiInteractionOutput, EguiNativeFrameHost};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase, CanvasPressTarget,
    InteractionGraphSubmission, InteractionSubmissionSettlementEvidence,
};
use jellyflow_runtime::view::ViewSessionOptions;

#[test]
fn text_edit_owns_escape_even_when_the_same_frame_focuses_the_graph() {
    let mut harness = keyboard_harness(FocusOwner::TextEdit);
    arm_keyboard_connection(&mut harness);

    harness.state_mut().focus_graph_during_next_frame = true;
    harness.key_down(Key::Escape);
    harness.step();

    assert!(take_effects(&mut harness).is_empty());
    assert_eq!(
        harness.state().interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag,
        "the TextEdit that owned Escape at dispatch must keep the graph connection armed"
    );
    assert!(
        harness
            .ctx
            .memory(|memory| memory.has_focus(Id::new(GRAPH_ID)))
    );
}

#[test]
fn an_open_popup_owns_escape_even_when_the_graph_is_focused() {
    let mut harness = keyboard_harness(FocusOwner::Graph);
    arm_keyboard_connection(&mut harness);
    Popup::open_id(&harness.ctx, Id::new(POPUP_ID));

    harness.key_down(Key::Escape);
    harness.step();

    assert!(take_effects(&mut harness).is_empty());
    assert_eq!(
        harness.state().interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag,
        "an open popup must consume Escape before the graph cancellation path"
    );
}

#[test]
fn graph_arrow_keys_each_submit_one_directional_nudge() {
    assert_nudge(
        Key::ArrowRight,
        Modifiers::NONE,
        CanvasPoint { x: 10.0, y: 0.0 },
    );
    assert_nudge(
        Key::ArrowUp,
        Modifiers::SHIFT,
        CanvasPoint { x: 0.0, y: -50.0 },
    );
}

#[test]
fn focused_non_text_native_control_keeps_delete_arrow_and_escape_native() {
    for key in [Key::Delete, Key::ArrowRight] {
        let mut harness = keyboard_harness(FocusOwner::NativeControl);
        harness.key_down(key);
        harness.step();

        assert!(take_effects(&mut harness).is_empty(), "{key:?} leaked");
        assert_selected_node_unchanged(&harness);
    }

    let mut harness = keyboard_harness(FocusOwner::NativeControl);
    arm_keyboard_connection(&mut harness);
    harness.key_down(Key::Escape);
    harness.step();

    assert!(take_effects(&mut harness).is_empty());
    assert_eq!(
        harness.state().interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag,
        "Escape owned by a non-TextEdit native control must not cancel the graph session"
    );
}

#[test]
fn tab_never_generates_a_graph_effect() {
    let mut harness = keyboard_harness(FocusOwner::Graph);

    harness.key_down(Key::Tab);
    harness.step();

    assert!(take_effects(&mut harness).is_empty());
    assert_selected_node_unchanged(&harness);
}

const GRAPH_ID: &str = "native-keyboard-graph";
const TEXT_EDIT_ID: &str = "native-keyboard-text-edit";
const NATIVE_CONTROL_ID: &str = "native-keyboard-control";
const POPUP_ID: &str = "native-keyboard-popup";

#[derive(Debug, Clone, Copy)]
enum FocusOwner {
    Graph,
    TextEdit,
    NativeControl,
}

#[derive(Debug, Clone, Copy)]
struct KeyboardIds {
    selected_node: NodeId,
    source_port: PortId,
    prospective_edge: EdgeId,
}

struct NativeKeyboardState {
    runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    interaction: EguiCanvasInteraction,
    ids: KeyboardIds,
    focus_pending: Option<FocusOwner>,
    focus_graph_during_next_frame: bool,
    text: String,
    last_output: Option<EguiInteractionOutput>,
}

fn keyboard_harness(focus: FocusOwner) -> Harness<'static, NativeKeyboardState> {
    let selected_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let source_port = PortId::from_u128(11);
    let target_port = PortId::from_u128(12);
    let prospective_edge = EdgeId::from_u128(21);
    let graph = GraphBuilder::new(GraphId::from_u128(0xE6_15))
        .with_node(
            selected_node,
            test_node(vec![source_port], CanvasPoint::default()),
        )
        .with_node(
            target_node,
            test_node(vec![target_port], CanvasPoint { x: 240.0, y: 0.0 }),
        )
        .with_port(source_port, test_port(selected_node, PortDirection::Out))
        .with_port(target_port, test_port(target_node, PortDirection::In))
        .build()
        .expect("native keyboard graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("native keyboard runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("native keyboard view");
    let selected_lifetime = view
        .graph_snapshot()
        .expect("native keyboard snapshot")
        .node_lifetime_key(selected_node)
        .expect("selected node is current");
    view.set_selection(vec![selected_lifetime], Vec::new(), Vec::new())
        .expect("seed selected node");
    let interaction = EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default())
        .expect("native keyboard interaction");
    let state = NativeKeyboardState {
        runtime,
        host: EguiNativeFrameHost::new(view),
        interaction,
        ids: KeyboardIds {
            selected_node,
            source_port,
            prospective_edge,
        },
        focus_pending: Some(focus),
        focus_graph_during_next_frame: false,
        text: "editable".to_owned(),
        last_output: None,
    };
    Harness::builder()
        .with_size(Vec2::new(720.0, 420.0))
        .build_ui_state(render_keyboard_fixture, state)
}

fn render_keyboard_fixture(ui: &mut Ui, state: &mut NativeKeyboardState) {
    let scene_rect = ui.max_rect();
    state
        .interaction
        .begin_frame(ui.ctx(), scene_rect)
        .expect("begin native keyboard frame");

    let graph = ui.interact(scene_rect, Id::new(GRAPH_ID), Sense::click());
    let text_edit = ui.add(
        TextEdit::singleline(&mut state.text)
            .id(Id::new(TEXT_EDIT_ID))
            .desired_width(180.0),
    );
    let native_control = ui.interact(
        text_edit.rect.translate(Vec2::new(0.0, 48.0)),
        Id::new(NATIVE_CONTROL_ID),
        Sense::click(),
    );

    match state.focus_pending.take() {
        Some(FocusOwner::Graph) => graph.request_focus(),
        Some(FocusOwner::TextEdit) => text_edit.request_focus(),
        Some(FocusOwner::NativeControl) => native_control.request_focus(),
        None => {}
    }
    if std::mem::take(&mut state.focus_graph_during_next_frame) {
        graph.request_focus();
    }

    state
        .interaction
        .handle_response(&graph, CanvasPressTarget::Pan)
        .expect("graph response maps keyboard input");
    state
        .interaction
        .handle_response(&text_edit, CanvasPressTarget::NativeControl)
        .expect("TextEdit response remains native-owned");
    state
        .interaction
        .handle_response(&native_control, CanvasPressTarget::NativeControl)
        .expect("ordinary native control response remains native-owned");
    let output = state
        .interaction
        .end_frame(ui.ctx())
        .expect("end native keyboard frame");
    if let Some(accumulated) = state.last_output.as_mut() {
        accumulated.merge(output);
    } else {
        state.last_output = Some(output);
    }
}

fn arm_keyboard_connection(harness: &mut Harness<'static, NativeKeyboardState>) {
    let ctx = harness.ctx.clone();
    let state = harness.state_mut();
    let source = state
        .host
        .view()
        .port_instance_key(state.ids.source_port)
        .expect("source port lookup")
        .expect("source port is current");
    let output = state
        .interaction
        .activate_connection_port(
            &ctx,
            source,
            state.ids.prospective_edge,
            NodeGraphConnectionMode::Strict,
        )
        .expect("arm keyboard connection");
    assert!(output.effects().is_empty());
    assert_eq!(
        state.interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
}

fn assert_nudge(key: Key, modifiers: Modifiers, expected: CanvasPoint) {
    let mut harness = keyboard_harness(FocusOwner::Graph);
    harness.key_down_modifiers(modifiers, key);
    harness.step();

    let mut effects = take_effects(&mut harness);
    assert_eq!(effects.len(), 1, "one key press must submit one nudge");
    let CanvasInteractionEffect::SubmitGraph { submission } =
        effects.pop().expect("one nudge submission")
    else {
        panic!("keyboard nudge cannot start graph search");
    };
    execute_submission(&mut harness, submission);

    let state = harness.state();
    let snapshot = state.runtime.snapshot().expect("nudged graph snapshot");
    assert_eq!(
        snapshot
            .graph()
            .node(&state.ids.selected_node)
            .expect("selected node remains present")
            .pos,
        expected
    );
    assert_eq!(state.runtime.history_state().unwrap().undo_len(), 1);
}

fn execute_submission(
    harness: &mut Harness<'static, NativeKeyboardState>,
    submission: InteractionGraphSubmission,
) {
    let ctx = harness.ctx.clone();
    let state = harness.state_mut();
    let mut bound = state
        .interaction
        .bind_submission(submission)
        .expect("fresh nudge submission binds");
    let command = bound
        .take_command()
        .expect("nudge submission creates a graph command");
    let completion = state.runtime.execute(command);
    assert!(completion.applied_revision().is_some());
    let settled = state
        .interaction
        .settle_submission(
            &ctx,
            bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        )
        .expect("nudge completion returns the session to idle");
    assert!(!settled.renderer_work_pending());
    assert!(settled.terminal_query_error().is_none());
}

fn take_effects(
    harness: &mut Harness<'static, NativeKeyboardState>,
) -> Vec<CanvasInteractionEffect> {
    let mut output = harness
        .state_mut()
        .last_output
        .take()
        .expect("each frame emits interaction output");
    output.take_effects()
}

fn assert_selected_node_unchanged(harness: &Harness<'static, NativeKeyboardState>) {
    let state = harness.state();
    let snapshot = state.runtime.snapshot().expect("unchanged graph snapshot");
    assert_eq!(
        snapshot
            .graph()
            .node(&state.ids.selected_node)
            .expect("selected node was not deleted")
            .pos,
        CanvasPoint::default()
    );
    assert_eq!(state.runtime.history_state().unwrap().undo_len(), 0);
}

fn test_node(ports: Vec<PortId>, pos: CanvasPoint) -> Node {
    Node {
        kind: NodeKindKey::new("test.egui-native-keyboard"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn test_port(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("port"),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}
