use egui::{Id, Key, Sense, TextEdit, Ui, Vec2};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{
    EguiCanvasInteraction, EguiInteractionError, EguiInteractionOutput, EguiNativeFrameHost,
};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasPressTarget, CommandSettlement,
    InteractionGraphSubmission, InteractionGraphSubmissionBindError, InteractionTicketId,
};
use jellyflow_runtime::view::ViewSessionOptions;

#[test]
fn delete_and_backspace_submit_one_atomic_mixed_selection_command() {
    for key in [Key::Delete, Key::Backspace] {
        let mut harness = delete_harness(KeyboardFocus::Graph);

        harness.key_down(key);
        harness.step();

        let (ticket, submission) = take_single_submission(&mut harness);
        let ctx = harness.ctx.clone();
        let state = harness.state_mut();

        let command = state
            .interaction
            .bind_submission(submission)
            .expect("selection intent binds to its fresh revision")
            .expect("mixed selection emits one topology command");
        assert_eq!(command.label(), Some("Delete selection"));
        state
            .runtime
            .execute(command)
            .expect("selection command executes")
            .applied()
            .expect("selection command applies atomically");
        let SelectionDeleteState { interaction, .. } = state;
        interaction
            .settle(&ctx, ticket, CommandSettlement::Applied)
            .expect("adapter observes the command receipt");

        let state = harness.state();
        let graph = state.runtime.snapshot().expect("applied graph");
        assert!(graph.graph().node(&state.ids.node).is_none());
        assert!(graph.graph().edge(&state.ids.edge).is_none());
        assert_eq!(state.runtime.history_state().unwrap().undo_len(), 1);
    }
}

#[test]
fn text_edit_focus_does_not_delete_the_graph_selection() {
    let mut harness = delete_harness(KeyboardFocus::TextEdit);

    harness.key_down(Key::Delete);
    harness.step();

    let state = harness.state_mut();
    assert!(
        state
            .last_output
            .take()
            .expect("each frame emits interaction output")
            .effects()
            .is_empty()
    );
    let snapshot = state.runtime.snapshot().expect("unchanged graph");
    assert!(snapshot.graph().node(&state.ids.node).is_some());
    assert!(snapshot.graph().edge(&state.ids.edge).is_some());
    assert_eq!(state.runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(
        state.host.view().selection().unwrap().nodes(),
        [state.ids.node]
    );
    assert_eq!(
        state.host.view().selection().unwrap().edges(),
        [state.ids.edge]
    );
}

#[test]
fn cancelled_submission_cannot_enter_the_egui_command_transport() {
    let mut harness = delete_harness(KeyboardFocus::Graph);

    harness.key_down(Key::Delete);
    harness.step();

    let (ticket, submission) = take_single_submission(&mut harness);
    let ctx = harness.ctx.clone();
    let state = harness.state_mut();
    state
        .interaction
        .cancel(&ctx)
        .expect("cancellation reaches the bound interaction session");

    assert!(matches!(
        state
            .interaction
            .bind_submission(submission),
        Err(EguiInteractionError::Submission(
            InteractionGraphSubmissionBindError::Superseded {
                ticket: rejected,
                current: None,
            }
        )) if rejected == ticket
    ));
    let snapshot = state.runtime.snapshot().expect("unchanged graph snapshot");
    assert!(snapshot.graph().node(&state.ids.node).is_some());
    assert!(snapshot.graph().edge(&state.ids.edge).is_some());
    assert_eq!(state.runtime.history_state().unwrap().undo_len(), 0);
}

#[derive(Clone, Copy)]
enum KeyboardFocus {
    Graph,
    TextEdit,
}

#[derive(Clone, Copy)]
struct DeleteIds {
    node: NodeId,
    edge: EdgeId,
}

struct SelectionDeleteState {
    runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    interaction: EguiCanvasInteraction,
    ids: DeleteIds,
    focus_pending: Option<KeyboardFocus>,
    text: String,
    last_output: Option<EguiInteractionOutput>,
}

fn delete_harness(focus: KeyboardFocus) -> Harness<'static, SelectionDeleteState> {
    let node = NodeId::from_u128(1);
    let source_node = NodeId::from_u128(2);
    let target_node = NodeId::from_u128(3);
    let source = PortId::from_u128(11);
    let target = PortId::from_u128(12);
    let edge = EdgeId::from_u128(21);
    let graph = GraphBuilder::new(GraphId::from_u128(1))
        .with_node(node, test_node(Vec::new()))
        .with_node(source_node, test_node(vec![source]))
        .with_node(target_node, test_node(vec![target]))
        .with_port(source, test_port(source_node, PortDirection::Out))
        .with_port(target, test_port(target_node, PortDirection::In))
        .with_edge(edge, Edge::new(EdgeKind::Data, source, target))
        .build()
        .expect("selection deletion graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("selection deletion runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("selection deletion view");
    let snapshot = view.graph_snapshot().expect("selection snapshot");
    let node_lifetime = snapshot
        .node_lifetime_key(node)
        .expect("selected node is current");
    let edge_instance = snapshot
        .edge_instance_key(edge)
        .expect("selected edge is current");
    view.set_selection(vec![node_lifetime], vec![edge_instance], Vec::new())
        .expect("seed mixed graph selection");
    let interaction = EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default())
        .expect("selection deletion interaction");
    let state = SelectionDeleteState {
        runtime,
        host: EguiNativeFrameHost::new(view),
        interaction,
        ids: DeleteIds { node, edge },
        focus_pending: Some(focus),
        text: "editable".to_owned(),
        last_output: None,
    };
    Harness::builder()
        .with_size(Vec2::new(720.0, 420.0))
        .build_ui_state(render_selection_delete, state)
}

fn render_selection_delete(ui: &mut Ui, state: &mut SelectionDeleteState) {
    let scene_rect = ui.max_rect();
    state
        .interaction
        .begin_frame(ui.ctx(), scene_rect)
        .expect("begin selection deletion frame");
    let graph = ui.interact(scene_rect, Id::new("graph-keyboard-scope"), Sense::click());
    let text = ui.add(
        TextEdit::singleline(&mut state.text)
            .id(Id::new("selection-delete-text-editor"))
            .desired_width(180.0),
    );
    match state.focus_pending.take() {
        Some(KeyboardFocus::Graph) => graph.request_focus(),
        Some(KeyboardFocus::TextEdit) => text.request_focus(),
        None => {}
    }
    state
        .interaction
        .handle_response(&graph, CanvasPressTarget::Pan)
        .expect("graph response maps keyboard input");
    state
        .interaction
        .handle_response(&text, CanvasPressTarget::NativeControl)
        .expect("native text response remains isolated");
    state.last_output = Some(
        state
            .interaction
            .end_frame(ui.ctx())
            .expect("end selection deletion frame"),
    );
}

fn take_single_submission(
    harness: &mut Harness<'static, SelectionDeleteState>,
) -> (InteractionTicketId, InteractionGraphSubmission) {
    let mut effects = harness
        .state_mut()
        .last_output
        .take()
        .expect("each frame emits interaction output")
        .into_effects();
    assert_eq!(effects.len(), 1, "one key press submits one graph mutation");
    match effects.pop().expect("one submission") {
        CanvasInteractionEffect::SubmitGraph { submission } => (submission.ticket(), submission),
        CanvasInteractionEffect::Search(_) => {
            panic!("selection deletion must remain one adapter-owned graph submission")
        }
    }
}

fn test_node(ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new("test.egui-selection-delete"),
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
