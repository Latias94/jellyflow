use keyboard_types::Code as KeyCode;

use super::fixtures::make_graph;
use crate::io::NodeGraphViewState;
use crate::runtime::drag::{NODE_NUDGE_TRANSACTION_LABEL, NodeNudgeDirection, NodeNudgeRequest};
use crate::runtime::keyboard::{KeyboardActionOutcome, KeyboardDeleteAction, KeyboardIntent};

#[test]
fn keyboard_intent_routes_delete_selection_variants() {
    let (graph, a, _b, _out, _in, edge) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], vec![edge], Vec::new());
    let mut store = crate::runtime::store::NodeGraphStore::new(
        graph,
        view_state,
        crate::runtime::tests::fixtures::default_editor_config(),
    );

    let outcome = store
        .apply_keyboard_intent(KeyboardIntent::DeleteSelectionForKey(KeyCode::Backspace))
        .expect("key-bound delete should succeed")
        .expect("delete should commit");

    match outcome {
        KeyboardActionOutcome::DeleteSelection { action, dispatch } => {
            assert_eq!(
                action,
                KeyboardDeleteAction::KeyBoundSelectionDelete(KeyCode::Backspace)
            );
            assert_eq!(dispatch.committed().label(), Some("delete selection"));
        }
        other => panic!("unexpected keyboard outcome: {other:?}"),
    }
}

#[test]
fn keyboard_intent_routes_nudge_selection() {
    let (mut graph, a, _b, _out, _in, _edge) = make_graph();
    graph.nodes.get_mut(&a).expect("node").deletable = Some(true);
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![a], Vec::new(), Vec::new());
    let mut store = crate::runtime::store::NodeGraphStore::new(
        graph,
        view_state,
        crate::runtime::tests::fixtures::default_editor_config(),
    );

    let request = NodeNudgeRequest {
        direction: NodeNudgeDirection::Right,
        fast: false,
    };
    let outcome = store
        .apply_keyboard_intent(KeyboardIntent::NudgeSelection(request))
        .expect("nudge should succeed")
        .expect("nudge should commit");

    match outcome {
        KeyboardActionOutcome::NudgeSelection {
            request: actual,
            dispatch,
        } => {
            assert_eq!(actual, request);
            assert_eq!(
                dispatch.committed().label(),
                Some(NODE_NUDGE_TRANSACTION_LABEL)
            );
        }
        other => panic!("unexpected keyboard outcome: {other:?}"),
    }
}
