use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;

use super::{NodeGraphDocumentSnapshot, ViewChange};

/// Store event emitted to subscribers.
#[derive(Clone, Copy)]
pub enum NodeGraphStoreEvent<'a> {
    DocumentReplaced {
        before: NodeGraphDocumentSnapshot<'a>,
        after: NodeGraphDocumentSnapshot<'a>,
    },
    GraphCommitted {
        patch: &'a NodeGraphPatch,
    },
    ViewChanged {
        before: &'a NodeGraphViewState,
        after: &'a NodeGraphViewState,
        changes: &'a [ViewChange],
    },
}
