mod bindings;
mod edges;
mod groups;
mod nodes;
mod ports;
mod sticky_notes;

use crate::core::Graph;

/// Plans graph mutations while preserving v1 `Graph` storage invariants.
pub struct GraphMutationPlanner<'a> {
    graph: &'a Graph,
}

impl<'a> GraphMutationPlanner<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Self { graph }
    }
}
