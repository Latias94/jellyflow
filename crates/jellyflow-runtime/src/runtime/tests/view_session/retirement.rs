use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::{Binding, BindingId, CanvasSize, NodeKindKey};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::io::{ViewSelectionSeed, ViewSessionSeed};
use crate::rules::EdgeEndpoint;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;

use super::fixtures::{
    graph_with_nodes, graph_with_routed_edge, publish_graph_layouts, view_options,
};

#[test]
fn removing_a_selected_measured_node_retires_only_affected_view_facts() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view_a = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id).with_selection(
            ViewSelectionSeed::new(vec![ids[0]], vec![], vec![]),
        )))
        .expect("view A");
    let view_b = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id).with_selection(
            ViewSelectionSeed::new(vec![ids[0], ids[1]], vec![], vec![]),
        )))
        .expect("view B");
    publish_graph_layouts(&view_a);
    publish_graph_layouts(&view_b);
    let stats_before = [view_a.stats().unwrap(), view_b.stats().unwrap()];

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .expect("remove command")
        .applied()
        .expect("remove applies");

    for (view, before) in [&view_a, &view_b].into_iter().zip(stats_before) {
        let geometry = view.geometry_snapshot().unwrap();
        assert!(geometry.node(ids[0]).is_none());
        assert!(geometry.node(ids[1]).is_some());
        assert_eq!(geometry.nodes().len(), 1);
        assert_eq!(
            view.stats().unwrap().spatial_updates - before.spatial_updates,
            1
        );
    }
    assert!(view_a.export_seed().unwrap().selection().nodes().is_empty());
    assert_eq!(view_b.export_seed().unwrap().selection().nodes(), [ids[1]]);
}

#[test]
fn authoritative_remove_and_readd_retires_old_geometry_and_node_subscription() {
    let (graph, ids) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let old_lifetime = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    assert_eq!(view.node_lifetime_key(ids[0]).unwrap(), Some(old_lifetime));
    publish_graph_layouts(&view);
    let old_calls = Rc::new(Cell::new(0));
    let callback_calls = old_calls.clone();
    view.subscribe_node(ids[0], move |_| {
        callback_calls.set(callback_calls.get() + 1);
    })
    .expect("old node subscription");
    let mut replacement = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&ids[0])
        .expect("node")
        .clone();
    replacement.kind = NodeKindKey::new("replacement.node");

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .expect("remove command")
        .applied()
        .expect("remove applies");
    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().add_node(ids[0], replacement, []),
        ))
        .expect("add replacement command")
        .applied()
        .expect("replacement applies");

    let new_lifetime = runtime.node_lifetime_key(ids[0]).unwrap().unwrap();
    assert_ne!(new_lifetime, old_lifetime);
    assert!(view.geometry_snapshot().unwrap().node(ids[0]).is_none());
    assert_eq!(old_calls.get(), 0);
}

#[test]
fn removing_a_node_withdraws_its_geometry_and_invalidates_only_its_incident_route() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    publish_graph_layouts(&view);
    let stats_before = view.stats().expect("view stats before removal");

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(ids.source, IncidentEdgePolicy::Cascade),
        ))
        .expect("remove command")
        .applied()
        .expect("remove applies");

    let geometry = view.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.source).is_none());
    assert!(geometry.node(ids.target).is_some());
    assert!(geometry.node(ids.alternate_source).is_some());
    assert!(geometry.route(ids.edge).is_none());
    let stats = view.stats().expect("view stats");
    assert_eq!(stats.spatial_updates - stats_before.spatial_updates, 1);
    assert_eq!(
        stats.route_invalidations - stats_before.route_invalidations,
        1
    );
}

#[test]
fn changing_node_size_withdraws_old_geometry_and_invalidates_only_incident_routes() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    publish_graph_layouts(&view);
    let stats_before = view.stats().expect("view stats before resize");

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_size(
            version,
            ids.source,
            Some(CanvasSize {
                width: 220.0,
                height: 140.0,
            }),
        ))
        .expect("resize command")
        .applied()
        .expect("resize applies");

    let geometry = view.geometry_snapshot().unwrap();
    assert!(geometry.node(ids.source).is_none());
    assert!(geometry.node(ids.target).is_some());
    assert!(geometry.node(ids.alternate_source).is_some());
    assert!(geometry.route(ids.edge).is_none());
    let stats = view.stats().expect("view stats");
    assert_eq!(stats.spatial_updates - stats_before.spatial_updates, 1);
    assert_eq!(
        stats.route_invalidations - stats_before.route_invalidations,
        1
    );
}

#[test]
fn reconnecting_an_edge_notifies_nodes_at_both_old_and_new_endpoints() {
    let (graph, ids) = graph_with_routed_edge();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let calls = [
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    ];
    for (node, calls) in [ids.source, ids.target, ids.alternate_source]
        .into_iter()
        .zip(calls.iter().cloned())
    {
        view.subscribe_node(node, move |_| calls.set(calls.get() + 1))
            .expect("node subscription");
    }

    let version = runtime.snapshot().unwrap().version();
    let outcome = runtime
        .execute(GraphCommand::reconnect(
            version,
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .expect("reconnect edge");
    let patch = outcome.applied().expect("reconnect applies").patch();

    assert_eq!(
        patch.affected_nodes(),
        &[ids.source, ids.target, ids.alternate_source]
    );
    assert_eq!(calls.map(|calls| calls.get()), [1, 1, 1]);
}

#[test]
fn cascading_binding_removal_notifies_the_surviving_endpoint_node() {
    let (mut graph, ids) = graph_with_nodes(2);
    let binding = BindingId::from_u128(0x302);
    let binding_value = Binding::new(
        jellyflow_core::core::BindingEndpoint::node(ids[0]),
        jellyflow_core::core::BindingEndpoint::node(ids[1]),
    );
    let mut builder = jellyflow_core::core::GraphBuilder::new(graph.graph_id());
    for (id, node) in graph.nodes() {
        builder.insert_node(*id, node.clone());
    }
    builder.insert_binding(binding, binding_value);
    graph = builder.build().expect("graph with binding");

    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let calls = Rc::new(Cell::new(0));
    let callback_calls = calls.clone();
    view.subscribe_node(ids[1], move |_| {
        callback_calls.set(callback_calls.get() + 1);
    })
    .expect("surviving node subscription");

    let version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::topology(
            version,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .expect("remove command")
        .applied()
        .expect("remove applies");

    assert!(runtime.snapshot().unwrap().graph().node(&ids[1]).is_some());
    assert_eq!(calls.get(), 1);
}
