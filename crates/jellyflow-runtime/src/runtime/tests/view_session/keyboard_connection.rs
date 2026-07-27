use jellyflow_core::core::EdgeId;
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionSession, CommandSettlement, InteractionTerminalReason,
};
use crate::runtime::tests::fixtures::{current_port_instance, keyboard_connection_port_by_id};
use crate::runtime::view_session::{ViewSessionError, ViewSessionOptions};

use super::fixtures::graph_with_routed_edge;

#[test]
fn keyboard_connection_uses_exact_ports_and_the_fresh_connection_mutation_pipeline() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard connection runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard connection view");
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port)
        .expect("real source capability");
    let target =
        keyboard_connection_port_by_id(&view, ids.target_port).expect("real target capability");
    let edge = EdgeId::from_u128(0x301);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("keyboard interaction session");

    let activated = session
        .activate_connection_port(source, edge, NodeGraphConnectionMode::Strict)
        .expect("source activation");
    assert!(activated.effects().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
    assert_eq!(session.keyboard_connection_source(), Some(source));

    let reactivated = session
        .activate_keyboard_connection(source, edge, NodeGraphConnectionMode::Strict)
        .expect("source reactivation is a no-op");
    assert!(reactivated.effects().is_empty());
    assert_eq!(session.keyboard_connection_source(), Some(source));

    let submitted = session
        .activate_connection_port(target, edge, NodeGraphConnectionMode::Strict)
        .expect("compatible exact target");
    let [CanvasInteractionEffect::SubmitGraph { submission }] = submitted.effects() else {
        panic!("keyboard target must emit one ordinary graph mutation");
    };
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    let ticket = submission.ticket();
    let submitted = submitted.into_dispatch();
    let CanvasInteractionEffect::SubmitGraph { submission } = submitted
        .effects
        .into_iter()
        .next()
        .expect("keyboard mutation")
    else {
        unreachable!("the only effect was checked above")
    };
    let command = session
        .bind_submission(submission)
        .expect("fresh binding")
        .expect("accepted connection command");
    runtime
        .execute(command)
        .expect("keyboard connection execution")
        .applied()
        .expect("keyboard connection applies");
    session
        .settle(ticket, CommandSettlement::Applied)
        .expect("keyboard settlement");

    let graph = runtime.snapshot().expect("connected graph");
    let connected = graph.graph().edge(&edge).expect("new keyboard edge");
    assert_eq!(connected.from, ids.alternate_source_port);
    assert_eq!(connected.to, ids.target_port);
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn keyboard_connection_retains_structured_invalid_feedback_and_escape_cancels() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard feedback runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard feedback view");
    let source = keyboard_connection_port_by_id(&view, ids.alternate_source_port).unwrap();
    let invalid_target = keyboard_connection_port_by_id(&view, ids.source_port).unwrap();
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("keyboard feedback session");
    session
        .activate_keyboard_connection(
            source,
            EdgeId::from_u128(0x302),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap();

    let rejected = session
        .activate_keyboard_connection_target(invalid_target)
        .expect("policy rejection is structured state, not a session error");
    assert!(rejected.effects().is_empty());
    let feedback = session
        .keyboard_connection_feedback()
        .expect("invalid target feedback");
    assert_eq!(feedback.source(), ids.alternate_source_port);
    assert_eq!(feedback.target(), ids.source_port);
    assert!(!feedback.diagnostics().is_empty());
    assert_eq!(session.keyboard_connection_source(), Some(source));

    let cancelled = session.cancel().unwrap();
    assert!(cancelled.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
    assert_eq!(session.keyboard_connection_source(), None);
    assert_eq!(session.keyboard_connection_feedback(), None);
}

#[test]
fn keyboard_port_capabilities_are_view_scoped() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard capability runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let foreign = keyboard_connection_port_by_id(&first, ids.alternate_source_port).unwrap();
    let mut session = CanvasInteractionSession::attach(&second, CanvasInteractionConfig::default())
        .expect("second interaction session");

    assert!(matches!(
        session.activate_keyboard_connection(
            foreign,
            EdgeId::from_u128(0x303),
            NodeGraphConnectionMode::Strict,
        ),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn keyboard_port_capability_is_stale_after_same_id_replacement() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("keyboard replacement runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("keyboard replacement view");
    let stale_instance = current_port_instance(&view, ids.alternate_source_port);
    let stale = view.keyboard_connection_port(stale_instance).unwrap();
    let before = runtime.snapshot().unwrap();
    runtime
        .execute(GraphCommand::topology(
            before.version(),
            TopologyEdit::new().remove_node(ids.alternate_source, IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("source retirement applies");
    runtime
        .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
        .unwrap()
        .applied()
        .expect("source replacement applies");
    assert!(matches!(
        view.keyboard_connection_port(stale_instance),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    let current = keyboard_connection_port_by_id(&view, ids.alternate_source_port).unwrap();
    assert_ne!(stale, current);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("replacement interaction session");

    assert!(matches!(
        session.activate_keyboard_connection(
            stale,
            EdgeId::from_u128(0x304),
            NodeGraphConnectionMode::Strict,
        ),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}
