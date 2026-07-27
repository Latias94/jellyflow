use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeRouteSpec, EdgeRouteSpecValidationError, GraphBuilder,
    GraphValidationError, Group, GroupId, NodeKindKey,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::io::ViewSessionSeed;
use crate::runtime::commands::{
    GraphCommand, GraphCommandOutcome, IncidentEdgePolicy, TopologyEdit,
};
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};
use crate::runtime::interaction::{SelectionMode, SelectionMutation};
use crate::runtime::tests::fixtures::set_selection_by_id;
use crate::runtime::view_session::ViewSessionError;
use crate::schema::NodeCatalogMigrationError;

use super::fixtures::{
    graph_with_nodes, graph_with_routed_edge, publish_graph_layouts, view_options,
};

#[test]
fn node_id_reuse_clears_selection_instead_of_selecting_the_successor() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&view, [ids[0]], [], Vec::new()).unwrap();
    let admitted = runtime.snapshot().unwrap();
    let old_lifetime = admitted.node_lifetime_key(ids[0]).unwrap();
    let mut replacement = admitted.graph().node(&ids[0]).unwrap().clone();
    replacement.kind = NodeKindKey::new("replacement.node");

    runtime
        .execute(GraphCommand::topology(
            admitted.version(),
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("node retirement applies");
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(ids[0], replacement, []),
        ))
        .unwrap()
        .applied()
        .expect("same-ID node successor applies");

    let replaced = runtime.snapshot().unwrap();
    assert!(!replaced.node_lifetime_is_current(old_lifetime));
    assert!(view.selection().unwrap().nodes().is_empty());
    assert!(matches!(
        view.set_selection(vec![old_lifetime], vec![], vec![]),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert!(matches!(
        view.downgrade()
            .apply_selection_mutation(SelectionMutation::Node {
                provenance: view.provenance().unwrap(),
                node: old_lifetime,
                mode: SelectionMode::Replace,
            }),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert!(view.selection().unwrap().nodes().is_empty());
}

#[test]
fn edge_id_reuse_clears_selection_and_route_paint_state() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    publish_graph_layouts(&view);
    set_selection_by_id(&view, [], [ids.edge], Vec::new()).unwrap();
    let admitted = runtime.snapshot().unwrap();
    let old_instance = admitted.edge_instance_key(ids.edge).unwrap();
    assert!(
        view.geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .unwrap()
            .facts()
            .interaction
            .selected
    );

    runtime
        .execute(GraphCommand::disconnect(admitted.version(), ids.edge))
        .unwrap()
        .applied()
        .expect("edge retirement applies");
    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap()
        .applied()
        .expect("same-ID edge successor applies");

    let replaced = runtime.snapshot().unwrap();
    assert!(!replaced.edge_instance_is_current(old_instance));
    assert!(view.selection().unwrap().edges().is_empty());
    assert!(matches!(
        view.set_selection(vec![], vec![old_instance], vec![]),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert!(matches!(
        view.downgrade()
            .apply_selection_mutation(SelectionMutation::Edge {
                provenance: view.provenance().unwrap(),
                instance: old_instance,
                mode: SelectionMode::Replace,
            }),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert!(
        !view
            .geometry_snapshot()
            .unwrap()
            .route(ids.edge)
            .unwrap()
            .facts()
            .interaction
            .selected
    );

    runtime
        .execute(GraphCommand::undo(replaced.version()))
        .unwrap()
        .applied()
        .expect("undo restores graph data under another edge lifetime");
    assert!(view.selection().unwrap().edges().is_empty());
}

#[test]
fn selection_mutations_reject_same_id_facts_from_another_runtime_or_view() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let first_runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let second_runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = first_runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    let second = second_runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&first, [ids[0]], [], Vec::new()).unwrap();
    let foreign_baseline = first.selection_baseline().unwrap();
    let foreign_node = first_runtime
        .snapshot()
        .unwrap()
        .node_lifetime_key(ids[0])
        .unwrap();

    assert!(matches!(
        second
            .downgrade()
            .apply_selection_mutation(SelectionMutation::Restore(foreign_baseline)),
        Err(ViewSessionError::ForeignInteractionFacts)
    ));
    assert!(matches!(
        second
            .downgrade()
            .apply_selection_mutation(SelectionMutation::Node {
                provenance: second.provenance().unwrap(),
                node: foreign_node,
                mode: SelectionMode::Replace,
            }),
        Err(ViewSessionError::ForeignInteractionFacts)
    ));
    assert!(matches!(
        second
            .downgrade()
            .apply_selection_mutation(SelectionMutation::Clear {
                provenance: first.provenance().unwrap(),
            }),
        Err(ViewSessionError::ForeignInteractionFacts)
    ));
    assert!(second.selection().unwrap().nodes().is_empty());
}

#[test]
fn non_identity_edits_preserve_exact_live_selection() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&view, [ids.source], [ids.edge], Vec::new()).unwrap();
    let admitted = runtime.snapshot().unwrap();
    let node_lifetime = admitted.node_lifetime_key(ids.source).unwrap();
    let edge_instance = admitted.edge_instance_key(ids.edge).unwrap();

    runtime
        .execute(GraphCommand::set_node_position(
            admitted.version(),
            ids.source,
            CanvasPoint { x: 12.0, y: 24.0 },
        ))
        .unwrap()
        .applied()
        .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            ids.source,
            serde_json::json!({ "retains": "selection" }),
        ))
        .unwrap()
        .applied()
        .unwrap();

    let current = runtime.snapshot().unwrap();
    assert!(current.node_lifetime_is_current(node_lifetime));
    assert!(current.edge_instance_is_current(edge_instance));
    let selection = view.selection().unwrap();
    assert_eq!(selection.nodes(), [ids.source]);
    assert_eq!(selection.edges(), [ids.edge]);
}

#[test]
fn document_reset_and_undo_never_restore_selection_by_id() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&view, [ids[0]], [], Vec::new()).unwrap();

    let same_document = runtime.catalog().canonicalize_graph(&graph).unwrap();
    runtime.replace_document(same_document).unwrap();
    assert!(view.selection().unwrap().nodes().is_empty());

    set_selection_by_id(&view, [ids[0]], [], Vec::new()).unwrap();
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .unwrap();
    assert!(view.selection().unwrap().nodes().is_empty());
    runtime
        .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
        .unwrap()
        .applied()
        .unwrap();
    assert!(view.selection().unwrap().nodes().is_empty());
}

#[test]
fn stale_baseline_can_restore_live_entities_but_not_revisionless_group_ids() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let group = GroupId::from_u128(0x44);
    let mut builder = GraphBuilder::from_detached_graph(graph);
    builder.insert_group(
        group,
        Group {
            title: "Selection group".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 320.0,
                    height: 180.0,
                },
            },
            color: None,
        },
    );
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&view, [ids[0]], [], vec![group]).unwrap();
    let baseline = view.selection_baseline().unwrap();

    runtime
        .execute(GraphCommand::set_node_position(
            runtime.snapshot().unwrap().version(),
            ids[0],
            CanvasPoint { x: 20.0, y: 30.0 },
        ))
        .unwrap()
        .applied()
        .unwrap();
    set_selection_by_id(&view, [], [], Vec::new()).unwrap();
    view.downgrade()
        .apply_selection_mutation(SelectionMutation::Restore(baseline))
        .unwrap();

    let restored = view.selection().unwrap();
    assert_eq!(restored.nodes(), [ids[0]]);
    assert!(restored.groups().is_empty());
}

#[test]
fn hiding_a_selected_node_sanitizes_its_incident_selected_edge_in_the_same_patch() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .unwrap();
    set_selection_by_id(&view, [ids.source], [ids.edge], Vec::new()).unwrap();

    let mut replacement = GraphBuilder::new(graph_id);
    for (id, record) in graph.nodes() {
        let mut record = record.clone();
        if *id == ids.source {
            record.hidden = true;
        }
        replacement.insert_node(*id, record);
    }
    for (id, record) in graph.ports() {
        replacement.insert_port(*id, record.clone());
    }
    for (id, record) in graph.edges() {
        replacement.insert_edge(*id, record.clone());
    }
    let replacement = replacement.build().unwrap();
    let document = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    runtime.replace_document(document).unwrap();

    let selection = view.selection().unwrap();
    assert!(selection.nodes().is_empty());
    assert!(selection.edges().is_empty());
}

#[test]
fn revisioned_position_command_derives_and_undoes_the_authoritative_inverse() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let lifetime = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    let point = CanvasPoint { x: 10.0, y: 20.0 };

    runtime
        .execute(GraphCommand::set_node_position(
            runtime.snapshot().unwrap().version(),
            ids[0],
            point,
        ))
        .expect("execute")
        .applied()
        .expect("position command applies");

    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids[0])
            .unwrap()
            .pos,
        point
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.node_lifetime_key(ids[0]).unwrap(), Some(lifetime));
    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::undo(version))
        .unwrap()
        .applied()
        .expect("undo applies");
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids[0])
            .unwrap()
            .pos,
        CanvasPoint::default()
    );
    assert_eq!(runtime.node_lifetime_key(ids[0]).unwrap(), Some(lifetime));
}

#[test]
fn non_finite_command_is_rejected_without_runtime_session_or_history_side_effects() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let events = Rc::new(Cell::new(0));
    let callback_events = events.clone();
    view.subscribe(move |_| callback_events.set(callback_events.get() + 1))
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let history = runtime.history_state().unwrap();
    let runtime_stats = runtime.stats().unwrap();
    let view_stats = view.stats().unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            before.version(),
            ids[0],
            CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
        ))
        .expect("invalid command returns a structured outcome");

    assert!(outcome.rejection().is_some());
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap(), history);
    assert_eq!(runtime.stats().unwrap(), runtime_stats);
    assert_eq!(view.stats().unwrap(), view_stats);
    assert_eq!(events.get(), 0);
}

#[test]
fn invalid_route_initial_graph_is_rejected_before_runtime_state_is_created() {
    let (graph, ids) = graph_with_routed_edge();
    let mut graph = GraphBuilder::from_detached_graph(graph);
    graph.edge_mut(&ids.edge).expect("fixture edge").view.route =
        Some(EdgeRouteSpec::Bezier { curvature: -1.0 });

    let error = match GraphRuntime::builder(Default::default()).build(graph.build_unchecked()) {
        Ok(_) => panic!("runtime must reject an invalid initial graph route"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        GraphRuntimeError::Canonicalization(NodeCatalogMigrationError::InvalidSource { errors })
            if errors.iter().any(|error| matches!(
                error,
                GraphValidationError::EdgeRouteSpecInvalid {
                    edge,
                    source: EdgeRouteSpecValidationError::BezierCurvature { .. },
                } if *edge == ids.edge
            ))
    ));
}

#[test]
fn net_noop_does_not_build_or_deliver_a_patch() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let before = runtime.snapshot().unwrap();
    let installed_impact_before = runtime.installed_impact_state_for_test();
    let outcome = runtime
        .execute(GraphCommand::set_node_position(
            before.version(),
            ids[0],
            CanvasPoint::default(),
        ))
        .unwrap();

    assert!(matches!(outcome, GraphCommandOutcome::Noop(_)));
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(
        runtime.installed_impact_state_for_test(),
        installed_impact_before,
        "normalized no-op commands must not update indexes or node identities"
    );
}

#[test]
fn node_binding_subscription_is_retired_before_reused_node_id_gets_a_new_incarnation() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let old_lifetime = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    let old_calls = Rc::new(Cell::new(0));
    let callback_calls = old_calls.clone();
    view.subscribe_node(ids[0], move |_| {
        callback_calls.set(callback_calls.get() + 1)
    })
    .unwrap();

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("remove applies");
    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::undo(version))
        .unwrap()
        .applied()
        .expect("undo applies");
    let new_lifetime = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    assert_ne!(new_lifetime, old_lifetime);

    let new_calls = Rc::new(Cell::new(0));
    let callback_calls = new_calls.clone();
    view.subscribe_node(ids[0], move |event| {
        assert_eq!(event.lifetime(), new_lifetime);
        callback_calls.set(callback_calls.get() + 1);
    })
    .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();

    assert_eq!(old_calls.get(), 0);
    assert_eq!(new_calls.get(), 1);
}
