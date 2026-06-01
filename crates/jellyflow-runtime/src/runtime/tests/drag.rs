use super::fixtures::default_editor_config;
use super::harness::{HarnessEvent, InteractionHarness};

use crate::io::NodeGraphViewState;
use crate::runtime::drag::{
    NODE_DRAG_TRANSACTION_LABEL, NodeDragItem, NodeDragRequest, plan_node_drag,
};
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphId, Group, GroupId, Node, NodeId, NodeKindKey,
};
use jellyflow_core::ops::GraphOp;

#[test]
fn single_node_drag_commits_set_node_pos_transaction_and_trace() {
    let fixture = drag_fixture();
    let mut harness = InteractionHarness::new("single node drag", fixture.graph);
    let target = CanvasPoint { x: 30.0, y: 40.0 };

    let plan = plan_node_drag(
        harness.store().graph(),
        harness.store().view_state(),
        &harness.store().resolved_interaction_state(),
        NodeDragRequest {
            node: fixture.enabled,
            to: target,
        },
    )
    .expect("enabled node drag plan");

    assert_eq!(plan.node, fixture.enabled);
    assert_eq!(plan.from, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to, target);
    assert_eq!(
        plan.transaction().label(),
        Some(NODE_DRAG_TRANSACTION_LABEL)
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [GraphOp::SetNodePos { id, from, to }]
                if *id == fixture.enabled
                    && *from == CanvasPoint { x: 10.0, y: 20.0 }
                    && *to == target
        ),
        "drag plan should be a single SetNodePos op: {:#?}",
        plan.transaction().ops(),
    );

    let outcome = harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("drag dispatch succeeds")
        .expect("drag dispatch commits");

    assert_eq!(
        outcome.committed().label(),
        Some(NODE_DRAG_TRANSACTION_LABEL)
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 30.0, y: 40.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        &["set_node_pos"],
    )]);
}

#[test]
fn single_node_drag_skips_non_draggable_hidden_noop_missing_and_invalid_targets() {
    let fixture = drag_fixture();
    let store = NodeGraphStore::new(fixture.graph, Default::default(), default_editor_config());
    let interaction = store.resolved_interaction_state();

    for request in [
        NodeDragRequest {
            node: fixture.disabled,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.hidden,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.missing,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        },
        NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 10.0, y: 20.0 },
        },
        NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint {
                x: f32::INFINITY,
                y: 40.0,
            },
        },
    ] {
        assert!(
            plan_node_drag(store.graph(), store.view_state(), &interaction, request).is_none(),
            "request should not produce a drag plan: {request:?}",
        );
    }
}

#[test]
fn single_node_drag_respects_global_nodes_draggable_policy_without_committing() {
    let fixture = drag_fixture();
    let mut editor_config = default_editor_config();
    editor_config.interaction.nodes_draggable = false;
    let mut harness = InteractionHarness::new("global drag disabled", fixture.graph);
    harness.store_mut().replace_editor_config(editor_config);

    let result = harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: CanvasPoint { x: 30.0, y: 40.0 },
        })
        .expect("disabled drag does not error");

    assert!(result.is_none());
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 10.0, y: 20.0 },
    );
    harness.assert_events(&[]);
}

#[test]
fn multi_selection_drag_moves_primary_and_selected_nodes_with_sorted_ops() {
    let fixture = drag_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![
            fixture.selected_high,
            fixture.disabled,
            fixture.child_in_selected_group,
            fixture.selected_low,
        ],
        Vec::new(),
        vec![fixture.selected_group],
    );
    let mut harness =
        InteractionHarness::with_view_state("multi selection drag", fixture.graph, view_state);
    let target = CanvasPoint { x: 30.0, y: 40.0 };

    let plan = harness
        .store()
        .plan_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("multi selection drag plan");

    assert_eq!(plan.node, fixture.enabled);
    assert_eq!(plan.from, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(plan.to, target);
    assert_eq!(
        plan.items(),
        &[
            NodeDragItem {
                node: fixture.selected_low,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 20.0, y: 20.0 },
            },
            NodeDragItem {
                node: fixture.enabled,
                from: CanvasPoint { x: 10.0, y: 20.0 },
                to: CanvasPoint { x: 30.0, y: 40.0 },
            },
            NodeDragItem {
                node: fixture.selected_high,
                from: CanvasPoint { x: 100.0, y: 0.0 },
                to: CanvasPoint { x: 120.0, y: 20.0 },
            },
        ],
    );
    assert!(
        matches!(
            plan.transaction().ops(),
            [
                GraphOp::SetNodePos { id: low, .. },
                GraphOp::SetNodePos { id: primary, .. },
                GraphOp::SetNodePos { id: high, .. },
            ] if *low == fixture.selected_low
                && *primary == fixture.enabled
                && *high == fixture.selected_high
        ),
        "multi-drag ops should be sorted by node id: {:#?}",
        plan.transaction().ops(),
    );

    harness
        .store_mut()
        .apply_node_drag(NodeDragRequest {
            node: fixture.enabled,
            to: target,
        })
        .expect("multi drag dispatch succeeds")
        .expect("multi drag dispatch commits");

    assert_eq!(
        harness.store().graph().nodes[&fixture.selected_low].pos,
        CanvasPoint { x: 20.0, y: 20.0 },
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.enabled].pos,
        CanvasPoint { x: 30.0, y: 40.0 },
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.selected_high].pos,
        CanvasPoint { x: 120.0, y: 20.0 },
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.disabled].pos,
        CanvasPoint { x: 200.0, y: 0.0 },
    );
    assert_eq!(
        harness.store().graph().nodes[&fixture.child_in_selected_group].pos,
        CanvasPoint { x: 300.0, y: 0.0 },
    );
    harness.assert_events(&[HarnessEvent::graph_commit(
        Some(NODE_DRAG_TRANSACTION_LABEL),
        &["set_node_pos", "set_node_pos", "set_node_pos"],
    )]);
}

struct DragFixture {
    graph: Graph,
    enabled: NodeId,
    disabled: NodeId,
    hidden: NodeId,
    missing: NodeId,
    selected_low: NodeId,
    selected_high: NodeId,
    child_in_selected_group: NodeId,
    selected_group: GroupId,
}

fn drag_fixture() -> DragFixture {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let selected_low = NodeId::from_u128(5);
    let enabled = NodeId::from_u128(10);
    let disabled = NodeId::from_u128(20);
    let hidden = NodeId::from_u128(30);
    let missing = NodeId::from_u128(40);
    let selected_high = NodeId::from_u128(60);
    let child_in_selected_group = NodeId::from_u128(70);
    let selected_group = GroupId::from_u128(100);

    graph.nodes.insert(
        selected_low,
        node(CanvasPoint { x: 0.0, y: 0.0 }, None, false),
    );
    graph
        .nodes
        .insert(enabled, node(CanvasPoint { x: 10.0, y: 20.0 }, None, false));
    graph.nodes.insert(
        disabled,
        node(CanvasPoint { x: 200.0, y: 0.0 }, Some(false), false),
    );
    graph
        .nodes
        .insert(hidden, node(CanvasPoint { x: 10.0, y: 20.0 }, None, true));
    graph.nodes.insert(
        selected_high,
        node(CanvasPoint { x: 100.0, y: 0.0 }, None, false),
    );
    graph.groups.insert(
        selected_group,
        Group {
            title: "Selected Group".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 280.0, y: -20.0 },
                size: CanvasSize {
                    width: 80.0,
                    height: 80.0,
                },
            },
            color: None,
        },
    );
    graph.nodes.insert(
        child_in_selected_group,
        node_with_parent(
            CanvasPoint { x: 300.0, y: 0.0 },
            None,
            false,
            selected_group,
        ),
    );

    DragFixture {
        graph,
        enabled,
        disabled,
        hidden,
        missing,
        selected_low,
        selected_high,
        child_in_selected_group,
        selected_group,
    }
}

fn node_with_parent(
    pos: CanvasPoint,
    draggable: Option<bool>,
    hidden: bool,
    parent: GroupId,
) -> Node {
    let mut node = node(pos, draggable, hidden);
    node.parent = Some(parent);
    node
}

fn node(pos: CanvasPoint, draggable: Option<bool>, hidden: bool) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        selectable: None,
        draggable,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: None,
        hidden,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}
