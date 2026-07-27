use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, NodeExtent, NodeId,
};

use crate::runtime::adapter_protocol::{ViewPoint, ViewSize};
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionSession, CanvasPressTarget, MAX_NODE_MOVE_COHORT_SIZE,
    NodeDragStart, NodeNudgeDirection, SelectionMode,
};
use crate::runtime::tests::fixtures::{node_drag_start_by_id, set_selection_by_id};
use crate::runtime::view_session::{ViewSessionError, ViewSessionOptions};
use crate::runtime::viewport::{ViewportIntent, ViewportPointerSample, ViewportZoomRequest};

use super::fixtures::{graph_with_nodes, node, publish_graph_layouts};

fn drag_nodes(start: &NodeDragStart) -> Vec<NodeId> {
    start.members().iter().map(|member| member.node()).collect()
}

#[test]
fn node_drag_start_filters_ineligible_selected_peers_and_owns_selection_semantics() {
    let clicked = NodeId::from_u128(1);
    let eligible = NodeId::from_u128(2);
    let hidden = NodeId::from_u128(3);
    let not_selectable = NodeId::from_u128(4);
    let not_draggable = NodeId::from_u128(5);
    let unselected = NodeId::from_u128(6);
    let mut hidden_node = node("test.node", 0.0);
    hidden_node.hidden = true;
    let mut non_selectable_node = node("test.node", 0.0);
    non_selectable_node.selectable = Some(false);
    let mut non_draggable_node = node("test.node", 0.0);
    non_draggable_node.draggable = Some(false);
    let graph = GraphBuilder::new(GraphId::from_u128(0x240))
        .with_node(clicked, node("test.node", 0.0))
        .with_node(eligible, node("test.node", 20.0))
        .with_node(hidden, hidden_node)
        .with_node(not_selectable, non_selectable_node)
        .with_node(not_draggable, non_draggable_node)
        .with_node(unselected, node("test.node", 40.0))
        .build()
        .unwrap();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    set_selection_by_id(
        &view,
        [clicked, eligible, hidden, not_selectable, not_draggable],
        [],
        Vec::new(),
    )
    .unwrap();

    let selected_start = node_drag_start_by_id(&view, clicked, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    assert_eq!(drag_nodes(&selected_start), [clicked, eligible]);
    assert!(selected_start.clicked_was_selected());

    let replace_start = node_drag_start_by_id(&view, unselected, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    assert_eq!(drag_nodes(&replace_start), [unselected]);
    assert!(!replace_start.clicked_was_selected());

    let additive_start = node_drag_start_by_id(&view, unselected, SelectionMode::Add)
        .unwrap()
        .unwrap();
    assert_eq!(drag_nodes(&additive_start), [clicked, eligible, unselected]);

    let toggle_drag_start = node_drag_start_by_id(&view, unselected, SelectionMode::Toggle)
        .unwrap()
        .unwrap();
    assert_eq!(
        drag_nodes(&toggle_drag_start),
        [clicked, eligible, unselected]
    );

    for ineligible in [hidden, not_selectable, not_draggable] {
        assert!(
            node_drag_start_by_id(&view, ineligible, SelectionMode::Replace)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn node_drag_admission_is_atomic_at_the_materialization_capacity() {
    assert_eq!(MAX_NODE_MOVE_COHORT_SIZE, 64);
    let (graph, nodes) = graph_with_nodes(MAX_NODE_MOVE_COHORT_SIZE + 1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    set_selection_by_id(
        &view,
        nodes[..MAX_NODE_MOVE_COHORT_SIZE].to_vec(),
        [],
        Vec::new(),
    )
    .unwrap();
    let admitted = node_drag_start_by_id(&view, nodes[0], SelectionMode::Replace)
        .unwrap()
        .expect("the complete in-budget cohort is admitted");
    assert_eq!(drag_nodes(&admitted), nodes[..MAX_NODE_MOVE_COHORT_SIZE]);
    let snapshot = runtime.snapshot().unwrap();
    assert!(
        admitted
            .members()
            .iter()
            .all(|member| snapshot.node_lifetime_key(member.node()) == Some(*member))
    );

    set_selection_by_id(&view, nodes.clone(), [], Vec::new()).unwrap();
    assert!(
        node_drag_start_by_id(&view, nodes[0], SelectionMode::Replace)
            .unwrap()
            .is_none(),
        "an over-budget cohort must be rejected instead of truncated"
    );
}

#[test]
fn node_facts_from_another_runtime_cannot_arm_move_or_activate_matching_ids() {
    let node_id = NodeId::from_u128(1);
    let graph = || {
        GraphBuilder::new(GraphId::from_u128(0x241))
            .with_node(node_id, node("test.node", 0.0))
            .build()
            .unwrap()
    };
    let source = GraphRuntime::builder(Default::default())
        .build(graph())
        .unwrap();
    let target = GraphRuntime::builder(Default::default())
        .build(graph())
        .unwrap();
    let source_view = source.attach_view(ViewSessionOptions::default()).unwrap();
    let target_view = target.attach_view(ViewSessionOptions::default()).unwrap();
    let start = node_drag_start_by_id(&source_view, node_id, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let source_key = start.clicked_lifetime();
    let target_snapshot = target.snapshot().unwrap();
    let target_key = target_snapshot.node_lifetime_key(node_id).unwrap();
    assert_eq!(source_key.node(), target_key.node());
    assert_ne!(source_key.provenance(), target_key.provenance());
    let mut session = interaction_for(&target_view);
    let target_position = pointer_sample(&target_view, 0.0, 0.0);

    assert!(matches!(
        session.press(CanvasPressTarget::NodeDrag(start.clone()), target_position,),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert!(matches!(
        session.activate_node(source_key, SelectionMode::Replace),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
}

#[test]
fn node_drag_start_is_bound_to_the_exact_view_generation() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x242))
                .with_node(node_id, node("test.node", 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let source_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let target_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let start = node_drag_start_by_id(&source_view, node_id, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let mut session = interaction_for(&target_view);
    let target_position = pointer_sample(&target_view, 0.0, 0.0);

    assert!(matches!(
        session.press(CanvasPressTarget::NodeDrag(start), target_position,),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn replacement_under_the_same_node_id_stales_drag_and_activation_facts() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x243))
                .with_node(node_id, node("test.node", 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let start = node_drag_start_by_id(&view, node_id, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let stale_key = start.clicked_lifetime();
    replace_node_by_undo(&runtime, node_id);
    let mut session = interaction_for(&view);
    let start_position = pointer_sample(&view, 0.0, 0.0);

    assert!(matches!(
        view.node_drag_start(stale_key, SelectionMode::Replace),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert!(matches!(
        session.press(CanvasPressTarget::NodeDrag(start), start_position,),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert!(matches!(
        session.activate_node(stale_key, SelectionMode::Replace),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
}

#[test]
fn replacement_after_press_aborts_before_drag_activation() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x244))
                .with_node(node_id, node("test.node", 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let start = node_drag_start_by_id(&view, node_id, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let mut session = interaction_for(&view);
    let start_position = pointer_sample(&view, 0.0, 0.0);
    let end_position = pointer_sample(&view, 10.0, 0.0);
    session
        .press(CanvasPressTarget::NodeDrag(start), start_position)
        .unwrap();
    replace_node_by_undo(&runtime, node_id);
    assert!(matches!(
        session.move_to(end_position),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert!(view.selection().unwrap().nodes().is_empty());
    assert!(session.release(end_position).unwrap().effects().is_empty());
}

#[test]
fn replacement_before_click_release_keeps_the_interaction_retriable_after_selection_failure() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x245))
                .with_node(node_id, node("test.node", 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let start = node_drag_start_by_id(&view, node_id, SelectionMode::Replace)
        .unwrap()
        .unwrap();
    let mut session = interaction_for(&view);
    let position = pointer_sample(&view, 0.0, 0.0);
    session
        .press(CanvasPressTarget::NodeDrag(start), position)
        .unwrap();
    replace_node_by_undo(&runtime, node_id);

    assert!(matches!(
        session.release(position),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );
    assert!(view.selection().unwrap().nodes().is_empty());
    session.cancel().unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn keyboard_nudge_uses_one_shared_extent_feasible_delta_and_one_command() {
    let first = NodeId::from_u128(1);
    let second = NodeId::from_u128(2);
    let extent = CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 270.0,
            height: 100.0,
        },
    };
    let mut first_node = node("test.node", 100.0);
    first_node.extent = Some(NodeExtent::Rect { rect: extent });
    let mut second_node = node("test.node", 20.0);
    second_node.extent = Some(NodeExtent::Rect { rect: extent });
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x245))
                .with_node(first, first_node)
                .with_node(second, second_node)
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    set_selection_by_id(&view, [first, second], [], Vec::new()).unwrap();
    let mut session = interaction_for(&view);

    let update = session
        .nudge_selection(NodeNudgeDirection::Right, true)
        .expect("nudge is admitted");
    let (ticket, submission) = update
        .into_dispatch()
        .effects
        .into_iter()
        .find_map(|effect| match effect {
            CanvasInteractionEffect::SubmitGraph { submission } => {
                Some((submission.ticket(), submission))
            }
            _ => None,
        })
        .expect("one atomic node move");
    let command = session
        .bind_submission(submission)
        .unwrap()
        .expect("shared feasible move remains non-zero");
    runtime.execute(command).unwrap().applied().unwrap();
    session
        .settle(
            ticket,
            crate::runtime::interaction::CommandSettlement::Applied,
        )
        .unwrap();

    let moved = runtime.snapshot().unwrap();
    assert_eq!(moved.graph().node(&first).unwrap().pos.x, 110.0);
    assert_eq!(moved.graph().node(&second).unwrap().pos.x, 30.0);
}

#[test]
fn keyboard_nudge_rejects_a_selected_non_draggable_cohort_without_effects() {
    let node_id = NodeId::from_u128(1);
    let mut blocked = node("test.node", 0.0);
    blocked.draggable = Some(false);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x246))
                .with_node(node_id, blocked)
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    set_selection_by_id(&view, [node_id], [], Vec::new()).unwrap();
    let mut session = interaction_for(&view);

    assert!(matches!(
        session.nudge_selection(NodeNudgeDirection::Right, false),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn keyboard_nudge_shift_acceleration_uses_the_configured_multiplier() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x248))
                .with_node(node_id, node("test.node", 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    set_selection_by_id(&view, [node_id], [], Vec::new()).unwrap();
    let mut session = interaction_for(&view);

    apply_nudge(&runtime, &mut session, false);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id)
            .unwrap()
            .pos
            .x,
        10.0,
    );
    apply_nudge(&runtime, &mut session, true);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id)
            .unwrap()
            .pos
            .x,
        60.0,
    );
}

#[test]
fn reveal_node_pans_the_owned_view_without_changing_zoom() {
    let node_id = NodeId::from_u128(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            GraphBuilder::new(GraphId::from_u128(0x247))
                .with_node(node_id, node("test.node", 1_000.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&view);
    view.apply_viewport_intent(ViewportIntent::zoom(ViewportZoomRequest::new(
        CanvasPoint::default(),
        2.0,
        0.25,
        4.0,
    )))
    .expect("test zoom applies");
    let node = runtime
        .snapshot()
        .unwrap()
        .node_lifetime_key(node_id)
        .expect("current node lifetime");
    let mut session = interaction_for(&view);
    let viewport = ViewSize::new(500.0, 300.0);

    let before = view.viewport_snapshot().unwrap();
    let update = session
        .reveal_node(node, viewport, 20.0)
        .expect("measured node can be revealed");
    let after = view.viewport_snapshot().unwrap();
    assert!(update.damage().viewport);
    assert_eq!(after.transform().zoom, before.transform().zoom);
    assert_ne!(after.transform().pan, before.transform().pan);

    let graph = runtime.snapshot().unwrap();
    let record = graph.graph().node(&node_id).unwrap();
    let min = after
        .transform()
        .screen_point_for_canvas(record.pos)
        .unwrap();
    let max = after
        .transform()
        .screen_point_for_canvas(CanvasPoint {
            x: record.pos.x + record.size.unwrap().width,
            y: record.pos.y + record.size.unwrap().height,
        })
        .unwrap();
    assert!(min.x >= 20.0 && min.y >= 20.0);
    assert!(max.x <= viewport.width - 20.0);
    assert!(max.y <= viewport.height - 20.0);

    let settled = view.viewport_snapshot().unwrap();
    let repeated = session
        .reveal_node(node, viewport, 20.0)
        .expect("already visible node is a no-op");
    assert!(!repeated.damage().viewport);
    assert_eq!(view.viewport_snapshot().unwrap(), settled);
}

fn interaction_for(view: &crate::runtime::view_session::ViewSession) -> CanvasInteractionSession {
    CanvasInteractionSession::attach(view, CanvasInteractionConfig::default()).unwrap()
}

fn apply_nudge(runtime: &GraphRuntime, session: &mut CanvasInteractionSession, accelerated: bool) {
    let update = session
        .nudge_selection(NodeNudgeDirection::Right, accelerated)
        .expect("nudge is admitted");
    let mut effects = update.into_dispatch().effects.into_iter();
    let Some(CanvasInteractionEffect::SubmitGraph { submission }) = effects.next() else {
        panic!("nudge must emit exactly one graph mutation");
    };
    let ticket = submission.ticket();
    assert!(effects.next().is_none());
    let command = session
        .bind_submission(submission)
        .unwrap()
        .expect("nudge command");
    runtime.execute(command).unwrap().applied().unwrap();
    session
        .settle(
            ticket,
            crate::runtime::interaction::CommandSettlement::Applied,
        )
        .unwrap();
}

fn pointer_sample(
    view: &crate::runtime::view_session::ViewSession,
    x: f32,
    y: f32,
) -> ViewportPointerSample {
    view.viewport_pointer_sample(ViewPoint::new(x, y)).unwrap()
}

fn replace_node_by_undo(runtime: &GraphRuntime, node: NodeId) {
    let before = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::topology(
            before.version(),
            TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("node removal applies");
    let retired = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::undo(retired.version()))
        .unwrap()
        .applied()
        .expect("undo restores a successor node");
}
