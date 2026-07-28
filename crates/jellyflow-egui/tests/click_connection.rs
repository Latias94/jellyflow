use egui::Context;
use jellyflow_core::{
    CanvasPoint, CanvasSize, EdgeId, GraphBuilder, GraphId, Node, NodeGraphConnectionMode, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::EguiCanvasInteraction;
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
};
use jellyflow_runtime::view::ViewSessionOptions;

#[test]
fn egui_non_drag_port_activation_uses_runtime_source_target_authority() {
    let source_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let source_port = PortId::from_u128(11);
    let target_port = PortId::from_u128(12);
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xE601));
    graph.insert_node(source_node, test_node(vec![source_port], 0.0));
    graph.insert_node(target_node, test_node(vec![target_port], 240.0));
    graph.insert_port(source_port, test_port(source_node, PortDirection::Out));
    graph.insert_port(target_port, test_port(target_node, PortDirection::In));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.build().expect("valid click-connect graph"))
        .expect("runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    let mut interaction =
        EguiCanvasInteraction::new(&view, CanvasInteractionConfig::default()).unwrap();
    let ctx = Context::default();
    let edge = EdgeId::from_u128(21);
    let source_instance = view
        .port_instance_key(source_port)
        .unwrap()
        .expect("current source port");
    let target_instance = view
        .port_instance_key(target_port)
        .unwrap()
        .expect("current target port");

    let armed = interaction
        .activate_connection_port(&ctx, source_instance, edge, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert!(armed.effects().is_empty());
    assert_eq!(
        interaction.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );

    let submitted = interaction
        .activate_connection_port(&ctx, target_instance, edge, NodeGraphConnectionMode::Strict)
        .unwrap();
    let mut effects = submitted.into_effects();
    assert_eq!(effects.len(), 1);
    let CanvasInteractionEffect::SubmitGraph { submission } =
        effects.pop().expect("one connection submission")
    else {
        unreachable!("non-drag target activation cannot emit search")
    };
    let ticket = submission.ticket();
    let command = interaction
        .bind_submission(submission)
        .unwrap()
        .expect("fresh connection command");
    let completion = runtime.execute(command);
    assert!(completion.applied_revision().is_some());
    interaction
        .complete_submission(&ctx, ticket, completion)
        .unwrap();

    let graph = runtime.snapshot().unwrap();
    let edge = graph.graph().edge(&edge).expect("committed edge");
    assert_eq!(edge.from, source_port);
    assert_eq!(edge.to, target_port);
}

fn test_node(ports: Vec<PortId>, x: f32) -> Node {
    Node {
        kind: NodeKindKey::new("test.egui-click-connect"),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
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
