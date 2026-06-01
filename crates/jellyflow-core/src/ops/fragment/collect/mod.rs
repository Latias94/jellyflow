mod collector;

use crate::core::{Graph, GroupId, NodeId};

use super::model::GraphFragment;
use collector::FragmentCollector;

impl GraphFragment {
    /// Builds a fragment from a set of nodes, capturing:
    /// - the selected nodes,
    /// - their ports,
    /// - edges that connect between selected nodes.
    ///
    /// Groups/notes are not inferred; callers may add them explicitly.
    ///
    /// Symbols are inferred for built-in symbol-ref nodes (`core::SYMBOL_REF_NODE_KIND`) so
    /// copy/paste can remain self-contained for the "blackboard variables" contract.
    pub fn from_nodes(graph: &Graph, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        Self::from_selection(graph, nodes, std::iter::empty())
    }

    /// Builds a fragment from a selection of nodes and groups.
    ///
    /// Captures:
    /// - selected groups,
    /// - selected nodes,
    /// - nodes inside selected groups,
    /// - ports for all captured nodes,
    /// - edges that connect between captured nodes.
    ///
    /// Nodes are detached from their parent group unless that group is included in the fragment.
    pub fn from_selection(
        graph: &Graph,
        selected_nodes: impl IntoIterator<Item = NodeId>,
        selected_groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        FragmentCollector::new(graph, selected_nodes, selected_groups).finish()
    }
}

#[cfg(test)]
mod tests;
