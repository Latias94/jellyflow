use keyboard_types::Code as KeyCode;

use super::fixtures::make_graph;
use super::harness::{HarnessEvent, InteractionHarness};
use crate::io::NodeGraphViewState;
use crate::runtime::delete::{DELETE_SELECTION_TRANSACTION_LABEL, DeleteSelectionError};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

#[test]
fn delete_selection_commits_selected_node_and_clears_stale_view_state() {
    let (graph, a, _b, _out, _in, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], vec![edge], Vec::new());
    view_state.draw_order = vec![a];
    view_state.edge_draw_order = vec![edge];
    let mut harness =
        InteractionHarness::with_view_state("delete selected node", graph, view_state);

    let plan = harness.store().plan_delete_selection();
    assert!(plan.is_accept());
    assert!(
        matches!(plan.ops(), [GraphOp::RemoveNode { id, edges, .. }]
            if *id == a && edges.iter().any(|(removed, _)| *removed == edge)),
        "delete selection should remove the selected node and cascade its edge: {:#?}",
        plan.ops(),
    );

    let outcome = harness
        .store_mut()
        .apply_delete_selection()
        .expect("delete selection dispatch succeeds")
        .expect("delete selection commits");

    assert_eq!(
        outcome.committed().label(),
        Some(DELETE_SELECTION_TRANSACTION_LABEL)
    );
    assert!(!harness.store().graph().nodes.contains_key(&a));
    assert!(!harness.store().graph().edges.contains_key(&edge));
    assert!(harness.store().view_state().selected_nodes.is_empty());
    assert!(harness.store().view_state().selected_edges.is_empty());
    assert!(harness.store().view_state().draw_order.is_empty());
    assert!(harness.store().view_state().edge_draw_order.is_empty());
    harness.assert_events(&[
        HarnessEvent::graph_commit(Some(DELETE_SELECTION_TRANSACTION_LABEL), &["remove_node"]),
        HarnessEvent::selection(Vec::new(), Vec::new(), Vec::new()),
    ]);
}

#[test]
fn delete_selection_can_remove_selected_edge_only() {
    let (graph, a, b, _out, _in, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(Vec::new(), vec![edge], Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("delete selected edge", graph, view_state);

    harness
        .store_mut()
        .apply_delete_selection()
        .expect("delete edge dispatch succeeds")
        .expect("delete edge commits");

    assert!(harness.store().graph().nodes.contains_key(&a));
    assert!(harness.store().graph().nodes.contains_key(&b));
    assert!(!harness.store().graph().edges.contains_key(&edge));
    harness.assert_events(&[
        HarnessEvent::graph_commit(Some(DELETE_SELECTION_TRANSACTION_LABEL), &["remove_edge"]),
        HarnessEvent::selection(Vec::new(), Vec::new(), Vec::new()),
    ]);
}

#[test]
fn delete_selection_key_gate_matches_xyflow_delete_key_config() {
    let (graph, a, _b, _out, _in, _edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], Vec::new(), Vec::new());
    let mut harness = InteractionHarness::with_view_state("delete key gate", graph, view_state);

    assert!(
        harness
            .store()
            .plan_delete_selection_for_key(KeyCode::Delete)
            .is_none(),
        "default delete key is Backspace"
    );
    assert!(
        harness
            .store()
            .plan_delete_selection_for_key(KeyCode::Backspace)
            .is_some()
    );

    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.delete_key = crate::io::NodeGraphDeleteKey::Delete;
    });
    assert!(
        harness
            .store()
            .plan_delete_selection_for_key(KeyCode::Backspace)
            .is_none()
    );
    assert!(
        harness
            .store()
            .plan_delete_selection_for_key(KeyCode::Delete)
            .is_some()
    );

    harness.store_mut().update_editor_config(|editor_config| {
        editor_config.interaction.disable_keyboard_a11y = true;
    });
    assert!(
        harness
            .store()
            .plan_delete_selection_for_key(KeyCode::Delete)
            .is_none(),
        "keyboard delete should be disabled with keyboard accessibility off"
    );
}

#[test]
fn delete_selection_rejects_policy_denied_selection_without_committing() {
    let (mut graph, a, _b, _out, _in, edge) = make_graph();
    graph.nodes.get_mut(&a).expect("node").deletable = Some(false);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], Vec::new(), Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("delete rejected node", graph, view_state);

    let err = harness
        .store_mut()
        .apply_delete_selection()
        .expect_err("delete selection should reject");
    let DeleteSelectionError::Rejected { diagnostics } = err else {
        panic!("unexpected delete error: {err:?}");
    };

    assert_eq!(diagnostics[0].key, "delete.node_not_deletable");
    assert!(harness.store().graph().nodes.contains_key(&a));
    assert!(harness.store().graph().edges.contains_key(&edge));
    assert!(harness.store().view_state().selected_nodes.contains(&a));
    harness.assert_events(&[]);
}

#[test]
fn delete_selection_empty_selection_is_noop() {
    let (graph, _a, _b, _out, _in, edge) = make_graph();
    let mut harness = InteractionHarness::new("delete empty selection", graph);

    let outcome = harness
        .store_mut()
        .apply_delete_selection()
        .expect("empty delete selection should not fail");

    assert!(outcome.is_none());
    assert!(harness.store().graph().edges.contains_key(&edge));
    harness.assert_events(&[]);
}

#[test]
fn graph_commit_sanitizes_selection_even_for_direct_transactions() {
    let (graph, a, _b, _out, _in, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], vec![edge], Vec::new());
    view_state.draw_order = vec![a];
    view_state.edge_draw_order = vec![edge];
    let mut harness =
        InteractionHarness::with_view_state("direct delete sanitizes view", graph, view_state);
    let tx = GraphMutationPlanner::new(harness.store().graph())
        .remove_node_tx(a, "external delete")
        .expect("remove node tx");

    harness
        .dispatch_transaction(&tx)
        .expect("external delete dispatch succeeds");

    assert!(harness.store().view_state().selected_nodes.is_empty());
    assert!(harness.store().view_state().selected_edges.is_empty());
    assert!(harness.store().view_state().draw_order.is_empty());
    assert!(harness.store().view_state().edge_draw_order.is_empty());
    harness.assert_events(&[
        HarnessEvent::graph_commit(Some("external delete"), &["remove_node"]),
        HarnessEvent::selection(Vec::new(), Vec::new(), Vec::new()),
    ]);
}
