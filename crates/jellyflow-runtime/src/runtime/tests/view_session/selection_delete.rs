use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Group,
    GroupId, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::ViewSessionSeed;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, FreshGraphMutationBindError, InteractionGraphSubmission,
    InteractionGraphSubmissionBindError, InteractionTicketId,
};
use crate::runtime::tests::fixtures::set_selection_by_id;
use crate::runtime::view_session::ViewSessionOptions;
use crate::runtime::viewport::ViewportTransform;

use super::fixtures::node;

#[test]
fn mixed_selection_delete_is_one_policy_filtered_topology_and_one_undo_boundary() {
    let fixture = deletion_fixture();
    let runtime = GraphRuntime::builder(Default::default())
        .build(fixture.graph)
        .unwrap();
    let view = runtime
        .attach_view(ViewSessionOptions {
            seed: Some(ViewSessionSeed::new(fixture.graph_id)),
        })
        .unwrap();
    set_selection_by_id(
        &view,
        [fixture.deletable_node, fixture.blocked_node],
        [fixture.incident_blocked_edge, fixture.explicit_edge],
        vec![fixture.group],
    )
    .unwrap();
    let mut session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();

    let update = session.delete_selection().unwrap();
    let mut submissions = submit_effects(update);
    assert_eq!(submissions.len(), 1);
    let (ticket, submission) = submissions.pop().unwrap();

    // The selected incident edge is non-deletable on its own, but deleting its admitted owner
    // necessarily cascades it. It must not be emitted as a duplicate explicit disconnect.
    let command = session
        .bind_submission(submission)
        .unwrap()
        .expect("non-empty selection command");
    view.execute(command).unwrap().applied().unwrap();
    session.settle(ticket, CommandSettlement::Applied).unwrap();

    let applied = view.graph_snapshot().unwrap();
    assert!(applied.graph().node(&fixture.deletable_node).is_none());
    assert!(applied.graph().node(&fixture.blocked_node).is_some());
    assert!(applied.graph().node(&fixture.retained_node).is_some());
    assert!(
        applied
            .graph()
            .edge(&fixture.incident_blocked_edge)
            .is_none()
    );
    assert!(applied.graph().edge(&fixture.explicit_edge).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(view.selection().unwrap().nodes(), [fixture.blocked_node]);
    assert_eq!(view.selection().unwrap().groups(), [fixture.group]);

    view.execute(GraphCommand::undo(applied.version()))
        .unwrap()
        .applied()
        .unwrap();
    let restored = view.graph_snapshot().unwrap();
    assert!(restored.graph().node(&fixture.deletable_node).is_some());
    assert!(
        restored
            .graph()
            .edge(&fixture.incident_blocked_edge)
            .is_some()
    );
    assert!(restored.graph().edge(&fixture.explicit_edge).is_some());
}

#[test]
fn empty_foreign_stale_and_active_selection_deletes_do_not_gain_authority() {
    let fixture = deletion_fixture();
    let runtime = GraphRuntime::builder(Default::default())
        .build(fixture.graph)
        .unwrap();
    let first = runtime
        .attach_view(ViewSessionOptions {
            seed: Some(ViewSessionSeed::new(fixture.graph_id)),
        })
        .unwrap();
    let second = runtime
        .attach_view(ViewSessionOptions {
            seed: Some(ViewSessionSeed::new(fixture.graph_id)),
        })
        .unwrap();
    let mut session =
        CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default()).unwrap();

    assert!(session.delete_selection().unwrap().effects().is_empty());
    set_selection_by_id(&second, [fixture.deletable_node], [], Vec::new()).unwrap();
    assert!(session.delete_selection().unwrap().effects().is_empty());
    let foreign_sample = second
        .viewport_pointer_sample(crate::runtime::adapter_protocol::ViewPoint::new(0.0, 0.0))
        .unwrap();
    let viewport_before_foreign_press = first.viewport_snapshot().unwrap();
    assert!(matches!(
        session.press(CanvasPressTarget::Pan, foreign_sample),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        first.viewport_snapshot().unwrap(),
        viewport_before_foreign_press
    );

    set_selection_by_id(&first, [fixture.deletable_node], [], Vec::new()).unwrap();
    let stale_sample = first
        .viewport_pointer_sample(crate::runtime::adapter_protocol::ViewPoint::new(0.0, 0.0))
        .unwrap();
    let changed_viewport = first
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 24.0, y: -12.0 }, 2.0).unwrap())
        .unwrap();
    session.press(CanvasPressTarget::Pan, stale_sample).unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );
    assert_eq!(first.viewport_snapshot().unwrap(), changed_viewport);
    assert!(matches!(
        session.delete_selection(),
        Err(CanvasInteractionError::AlreadyActive)
    ));
    session.cancel().unwrap();

    let update = session.delete_selection().unwrap();
    let mut submissions = submit_effects(update);
    assert_eq!(submissions.len(), 1);
    let (_, submission) = submissions.pop().unwrap();
    let before_retirement = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::topology(
            before_retirement.version(),
            TopologyEdit::new().remove_node(fixture.deletable_node, IncidentEdgePolicy::Cascade),
        ))
        .unwrap()
        .applied()
        .unwrap();
    let retired = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::undo(retired.version()))
        .unwrap()
        .applied()
        .unwrap();
    assert!(matches!(
        session.bind_submission(submission),
        Err(InteractionGraphSubmissionBindError::Mutation(
            FreshGraphMutationBindError::SelectionNodeReplaced { node, .. },
        ))
            if node == fixture.deletable_node
    ));

    first.close().unwrap();
    assert!(matches!(
        session.delete_selection(),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
}

#[test]
fn selection_delete_binding_uses_the_session_runtime_even_when_graph_ids_match() {
    let first_fixture = deletion_fixture();
    let second_fixture = deletion_fixture();
    let first_runtime = GraphRuntime::builder(Default::default())
        .build(first_fixture.graph)
        .unwrap();
    let second_runtime = GraphRuntime::builder(Default::default())
        .build(second_fixture.graph)
        .unwrap();
    let first_view = first_runtime
        .attach_view(ViewSessionOptions {
            seed: Some(ViewSessionSeed::new(first_fixture.graph_id)),
        })
        .unwrap();
    set_selection_by_id(&first_view, [first_fixture.deletable_node], [], Vec::new()).unwrap();
    let mut session =
        CanvasInteractionSession::attach(&first_view, CanvasInteractionConfig::default()).unwrap();
    let mut submissions = submit_effects(session.delete_selection().unwrap());
    let (_, submission) = submissions.pop().expect("one admitted deletion");
    assert!(submissions.is_empty());

    let before = second_runtime.snapshot().unwrap();
    let before_history = second_runtime.history_state().unwrap();
    let command = session
        .bind_submission(submission)
        .expect("the session obtains its own authoritative snapshot")
        .expect("the selected node produces a deletion command");
    first_runtime
        .execute(command)
        .expect("the command belongs to the session runtime")
        .applied()
        .expect("the session-owned command applies");
    assert!(
        first_runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&first_fixture.deletable_node)
            .is_none()
    );
    let after = second_runtime.snapshot().unwrap();
    assert!(before.ptr_eq(&after));
    assert_eq!(second_runtime.history_state().unwrap(), before_history);
    assert!(after.graph().node(&second_fixture.deletable_node).is_some());
}

fn submit_effects(
    update: CanvasInteractionUpdate,
) -> Vec<(InteractionTicketId, InteractionGraphSubmission)> {
    update
        .into_dispatch()
        .effects
        .into_iter()
        .filter_map(|effect| match effect {
            CanvasInteractionEffect::SubmitGraph { submission } => {
                Some((submission.ticket(), submission))
            }
            CanvasInteractionEffect::Search(_) => None,
        })
        .collect()
}

struct DeletionFixture {
    graph: jellyflow_core::core::Graph,
    graph_id: GraphId,
    deletable_node: NodeId,
    blocked_node: NodeId,
    retained_node: NodeId,
    incident_blocked_edge: EdgeId,
    explicit_edge: EdgeId,
    group: GroupId,
}

fn deletion_fixture() -> DeletionFixture {
    let graph_id = GraphId::from_u128(0xd311);
    let deletable_node = NodeId::from_u128(1);
    let blocked_node = NodeId::from_u128(2);
    let retained_node = NodeId::from_u128(3);
    let source = PortId::from_u128(11);
    let blocked_in = PortId::from_u128(12);
    let blocked_out = PortId::from_u128(13);
    let retained_in = PortId::from_u128(14);
    let incident_blocked_edge = EdgeId::from_u128(21);
    let explicit_edge = EdgeId::from_u128(22);
    let group = GroupId::from_u128(31);
    let mut first = node("test.node", 0.0);
    first.deletable = Some(true);
    first.ports = vec![source];
    let mut second = node("test.node", 200.0);
    second.deletable = Some(false);
    second.ports = vec![blocked_in, blocked_out];
    let mut third = node("test.node", 400.0);
    third.ports = vec![retained_in];
    let mut incident = Edge::new(EdgeKind::Data, source, blocked_in);
    incident.deletable = Some(false);
    let graph = GraphBuilder::new(graph_id)
        .with_node(deletable_node, first)
        .with_node(blocked_node, second)
        .with_node(retained_node, third)
        .with_port(source, port(deletable_node, PortDirection::Out))
        .with_port(blocked_in, port(blocked_node, PortDirection::In))
        .with_port(blocked_out, port(blocked_node, PortDirection::Out))
        .with_port(retained_in, port(retained_node, PortDirection::In))
        .with_edge(incident_blocked_edge, incident)
        .with_edge(
            explicit_edge,
            Edge::new(EdgeKind::Data, blocked_out, retained_in),
        )
        .with_group(
            group,
            Group {
                title: "Keep".to_owned(),
                rect: CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 640.0,
                        height: 240.0,
                    },
                },
                color: None,
            },
        )
        .build()
        .unwrap();
    DeletionFixture {
        graph,
        graph_id,
        deletable_node,
        blocked_node,
        retained_node,
        incident_blocked_edge,
        explicit_edge,
        group,
    }
}

fn port(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(match direction {
            PortDirection::In => "in",
            PortDirection::Out => "out",
        }),
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
