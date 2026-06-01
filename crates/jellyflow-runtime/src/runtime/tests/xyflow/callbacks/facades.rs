use crate::runtime::xyflow::callbacks::{DeleteChange, SelectionChange};
use jellyflow_core::core::{EdgeId, GroupId, NodeId, StickyNoteId};

#[test]
fn delete_change_facade_consumes_parts() {
    let node = NodeId::new();
    let edge = EdgeId::new();
    let group = GroupId::new();
    let sticky_note = StickyNoteId::new();
    let change = DeleteChange::from_parts(vec![node], vec![edge], vec![group], vec![sticky_note]);

    let (nodes, edges, groups, sticky_notes) = change.into_parts();

    assert_eq!(nodes, vec![node]);
    assert_eq!(edges, vec![edge]);
    assert_eq!(groups, vec![group]);
    assert_eq!(sticky_notes, vec![sticky_note]);
}

#[test]
fn selection_change_facade_exposes_and_consumes_parts() {
    let node = NodeId::new();
    let edge = EdgeId::new();
    let group = GroupId::new();
    let change = SelectionChange::new(vec![node], vec![edge], vec![group]);

    assert!(!change.is_empty());
    assert_eq!(change.nodes(), &[node]);
    assert_eq!(change.edges(), &[edge]);
    assert_eq!(change.groups(), &[group]);

    let (nodes, edges, groups) = change.into_parts();

    assert_eq!(nodes, vec![node]);
    assert_eq!(edges, vec![edge]);
    assert_eq!(groups, vec![group]);
}
