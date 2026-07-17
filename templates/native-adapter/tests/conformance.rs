use jellyflow::runtime::{
    FeedbackOperationReceipt, FrameNode, GraphCommand, GraphRuntime, HandlePosition,
    LayoutNodeStatus, NodeLayout, NodeLayoutBuilderError, PublishedPortOutcome, ViewPoint,
    ViewRect, ViewSessionOptions, ViewSize,
};
use jellyflow_native_adapter_template::{
    add_dynamic_port_command, base_port_id, dynamic_port_id, fixture_frame_request,
    fixture_node_id, native_catalog, native_graph,
};

fn measured_layout(
    node: &FrameNode,
    unavailable: Option<jellyflow::core::PortId>,
) -> Result<NodeLayout, NodeLayoutBuilderError> {
    let root = ViewRect::new(ViewPoint::new(24.0, 32.0), ViewSize::new(240.0, 128.0));
    let mut layout = node.begin_layout(root);
    for (index, port) in node.ports().iter().enumerate() {
        if unavailable == Some(port.id()) {
            layout.port_unavailable(port.layout_target())?;
            continue;
        }
        let center_y = root.origin.y + 44.0 + index as f32 * 32.0;
        let visual = ViewRect::new(
            ViewPoint::new(root.origin.x + root.size.width - 12.0, center_y - 6.0),
            ViewSize::new(12.0, 12.0),
        );
        let hit = ViewRect::new(
            ViewPoint::new(visual.origin.x - 4.0, visual.origin.y - 4.0),
            ViewSize::new(20.0, 20.0),
        );
        layout.port_rendered(
            port.layout_target(),
            visual,
            hit,
            ViewPoint::new(visual.origin.x + visual.size.width, center_y),
            HandlePosition::Right,
        )?;
    }
    layout.finish()
}

#[test]
fn semantic_catalog_creates_stable_real_ports() {
    let catalog = native_catalog().expect("semantic catalog");
    let graph = native_graph(&catalog).expect("canonical fixture graph");
    let node = graph
        .node(&fixture_node_id())
        .expect("catalog-created node");

    assert_eq!(node.ports, [base_port_id()]);
    assert_eq!(
        graph
            .port(&base_port_id())
            .expect("catalog-created real port")
            .node,
        fixture_node_id(),
    );
}

#[test]
fn command_and_layout_share_one_ordered_feedback_log() {
    let catalog = native_catalog().expect("semantic catalog");
    let graph = native_graph(&catalog).expect("canonical fixture graph");
    let runtime = GraphRuntime::builder(catalog)
        .build(graph)
        .expect("graph runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view session");

    let old_frame = view.frame(fixture_frame_request()).expect("owned frame");
    let old_node = old_frame
        .nodes()
        .iter()
        .find(|node| node.id() == fixture_node_id())
        .expect("admitted fixture node");
    let base = old_node
        .graph_snapshot()
        .graph()
        .port(&base_port_id())
        .expect("base port in owned frame");
    let collector = old_frame.collector();
    collector
        .command(add_dynamic_port_command(old_frame.graph_revision(), base))
        .expect("append native control command");
    collector
        .layout(measured_layout(old_node, None).expect("complete native layout"))
        .expect("append layout after command");

    let receipt = view
        .submit_feedback(collector.finish().expect("finish feedback once"))
        .expect("submit ordered feedback");
    assert_eq!(receipt.operations().len(), 2);
    assert!(matches!(
        &receipt.operations()[0],
        FeedbackOperationReceipt::Command { outcome }
            if outcome.as_ref().as_ref().is_ok_and(|outcome| outcome.applied().is_some())
    ));
    assert_eq!(
        receipt.operations()[1].layout_status(),
        Some(LayoutNodeStatus::Stale),
        "the preceding topology command makes geometry from the old frame stale",
    );
    assert!(receipt.request_next_frame());

    let snapshot = runtime.snapshot().expect("updated graph snapshot");
    assert_eq!(
        snapshot
            .graph()
            .node(&fixture_node_id())
            .expect("updated node")
            .ports,
        [base_port_id(), dynamic_port_id()],
    );
}

#[test]
fn fresh_layout_reports_every_real_port_outcome() {
    let catalog = native_catalog().expect("semantic catalog");
    let graph = native_graph(&catalog).expect("canonical fixture graph");
    let runtime = GraphRuntime::builder(catalog)
        .build(graph)
        .expect("graph runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view session");

    let command_frame = view.frame(fixture_frame_request()).expect("command frame");
    let command_node = command_frame
        .nodes()
        .iter()
        .find(|node| node.id() == fixture_node_id())
        .expect("command node");
    let base = command_node
        .graph_snapshot()
        .graph()
        .port(&base_port_id())
        .expect("base port in owned frame");
    let collector = command_frame.collector();
    collector
        .command(add_dynamic_port_command(
            command_frame.graph_revision(),
            base,
        ))
        .expect("append topology command");
    view.submit_feedback(collector.finish().expect("finish command feedback"))
        .expect("submit command feedback");

    let frame = view.frame(fixture_frame_request()).expect("fresh frame");
    let node = frame
        .nodes()
        .iter()
        .find(|node| node.id() == fixture_node_id())
        .expect("fresh dynamic node");
    let collector = frame.collector();
    collector
        .layout(
            measured_layout(node, Some(dynamic_port_id()))
                .expect("complete explicit port outcomes"),
        )
        .expect("append fresh layout");
    let receipt = view
        .submit_feedback(collector.finish().expect("finish layout feedback"))
        .expect("submit layout feedback");
    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Published),
    );

    let geometry = view.geometry_snapshot().expect("published geometry");
    assert!(matches!(
        geometry.port_outcome(base_port_id()),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(matches!(
        geometry.port_outcome(dynamic_port_id()),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.port(dynamic_port_id()).is_none());

    let incomplete_frame = view
        .frame(fixture_frame_request())
        .expect("validation frame");
    let incomplete_node = incomplete_frame
        .nodes()
        .iter()
        .find(|node| node.id() == fixture_node_id())
        .expect("validation node");
    let incomplete = incomplete_node.begin_layout(ViewRect::new(
        ViewPoint::new(24.0, 32.0),
        ViewSize::new(240.0, 128.0),
    ));
    assert!(matches!(
        incomplete.finish(),
        Err(NodeLayoutBuilderError::MissingPorts { ports }) if ports.len() == 2
    ));

    let undo_frame = view.frame(fixture_frame_request()).expect("undo frame");
    let collector = undo_frame.collector();
    collector
        .command(GraphCommand::undo(undo_frame.graph_revision()))
        .expect("append undo in feedback");
    view.submit_feedback(collector.finish().expect("finish undo feedback"))
        .expect("submit undo feedback");
    assert_eq!(
        runtime
            .snapshot()
            .expect("undo snapshot")
            .graph()
            .node(&fixture_node_id())
            .expect("restored node")
            .ports,
        [base_port_id()],
    );
}
