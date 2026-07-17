//! Headless layout planning for revisioned graph commands.

use std::collections::BTreeSet;

use jellyflow_core::core::Graph;
use jellyflow_layout::{LayoutError, LayoutResult};

use crate::runtime::commands::{GraphCommand, NodeGeometryEdit};
use crate::runtime::graph_runtime::GraphRevision;

/// Converts a validated layout result into one revision-checked geometry command.
pub fn layout_result_to_command(
    expected_revision: GraphRevision,
    graph: &Graph,
    layout: &LayoutResult,
) -> Result<GraphCommand, LayoutError> {
    let mut seen = BTreeSet::new();
    let mut edits = Vec::with_capacity(layout.nodes.len());
    for node in &layout.nodes {
        if !seen.insert(node.node) {
            return Err(LayoutError::DuplicateResultNode(node.node));
        }
        if !graph.nodes().contains_key(&node.node) {
            return Err(LayoutError::MissingResultNode(node.node));
        }
        edits.push(NodeGeometryEdit::new(node.node).with_position(node.pos));
    }
    Ok(GraphCommand::set_node_geometry(expected_revision, edits).with_label("Layout graph"))
}
