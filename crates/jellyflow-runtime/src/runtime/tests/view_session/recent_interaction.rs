use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use crate::runtime::adapter_protocol::{FrameRequest, RendererStamp, ViewPoint};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionSession, CanvasPressTarget, SelectionMode,
};
use crate::runtime::tests::fixtures::{node_drag_start_by_id, set_selection_by_id};
use crate::runtime::view_session::{ViewSession, ViewSessionOptions};

use super::fixtures::graph_with_nodes;

#[test]
fn recent_node_elevation_is_view_local_and_selection_scoped() {
    let (graph, nodes) = graph_with_nodes(2);
    let first_node = nodes[0];
    let second_node = nodes[1];
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("recent interaction runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    for view in [&first, &second] {
        set_selection_by_id(view, nodes.clone(), [], Vec::new()).expect("selected nodes");
    }
    assert_eq!(frame_order(&first), vec![first_node, second_node]);
    assert_eq!(frame_order(&second), vec![first_node, second_node]);

    let start = node_drag_start_by_id(&first, first_node, SelectionMode::Replace)
        .expect("drag facts")
        .expect("draggable node");
    let mut interaction =
        CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default())
            .expect("first interaction session");
    let press = interaction
        .press(
            CanvasPressTarget::NodeDrag(start),
            first
                .viewport_pointer_sample(ViewPoint::new(20.0, 20.0))
                .expect("pointer sample"),
        )
        .expect("node press");
    assert!(press.damage().nodes);
    assert!(press.damage().ports);
    interaction.cancel().unwrap();

    assert_eq!(frame_order(&first), vec![second_node, first_node]);
    assert!(
        first
            .export_seed()
            .expect("exported view seed")
            .stacking()
            .nodes()
            .is_empty(),
        "transient interaction elevation must not become persisted stacking",
    );
    assert_eq!(
        frame_order(&second),
        vec![first_node, second_node],
        "another view must not inherit transient interaction order",
    );

    set_selection_by_id(&first, [], [], Vec::new()).expect("clear selection");
    set_selection_by_id(&first, nodes, [], Vec::new()).expect("restore selection");
    assert_eq!(frame_order(&first), vec![first_node, second_node]);
}

fn frame_order(view: &ViewSession) -> Vec<NodeId> {
    view.frame(FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint { x: -20.0, y: -20.0 },
            size: CanvasSize {
                width: 320.0,
                height: 180.0,
            },
        },
        RendererStamp::new("test.node", 1),
    ))
    .expect("materialized frame")
    .nodes()
    .iter()
    .map(|node| node.id())
    .collect()
}
