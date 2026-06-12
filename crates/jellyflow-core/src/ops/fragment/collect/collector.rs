use std::collections::BTreeSet;

use crate::core::{
    BindingEndpoint, Graph, GraphLocalBindingTarget, GroupId, NodeId, subgraph_target_graph_id,
    symbol_ref_target_symbol_id,
};

use super::super::model::GraphFragment;

pub(super) struct FragmentCollector<'a> {
    graph: &'a Graph,
    groups: BTreeSet<GroupId>,
    nodes: BTreeSet<NodeId>,
    fragment: GraphFragment,
}

impl<'a> FragmentCollector<'a> {
    pub(super) fn new(
        graph: &'a Graph,
        selected_nodes: impl IntoIterator<Item = NodeId>,
        selected_groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self {
            graph,
            groups: selected_groups.into_iter().collect(),
            nodes: selected_nodes.into_iter().collect(),
            fragment: GraphFragment::default(),
        }
    }

    pub(super) fn finish(mut self) -> GraphFragment {
        self.capture_selected_groups();
        self.include_group_children();
        self.capture_selected_nodes();
        self.capture_referenced_symbols();
        self.capture_referenced_imports();
        self.capture_node_ports();
        self.capture_internal_edges();
        self.capture_bindings();
        self.fragment
    }

    fn capture_selected_groups(&mut self) {
        for group_id in &self.groups {
            if let Some(group) = self.graph.groups.get(group_id) {
                self.fragment.groups.insert(*group_id, group.clone());
            }
        }
    }

    fn include_group_children(&mut self) {
        if self.groups.is_empty() {
            return;
        }

        for (node_id, node) in &self.graph.nodes {
            if node
                .parent
                .is_some_and(|parent| self.groups.contains(&parent))
            {
                self.nodes.insert(*node_id);
            }
        }
    }

    fn capture_selected_nodes(&mut self) {
        for node_id in &self.nodes {
            if let Some(node) = self.graph.nodes.get(node_id) {
                let mut node = node.clone();
                if node
                    .parent
                    .is_some_and(|parent| !self.groups.contains(&parent))
                {
                    node.parent = None;
                }
                self.fragment.nodes.insert(*node_id, node);
            }
        }
    }

    fn capture_referenced_symbols(&mut self) {
        for (node_id, node) in &self.fragment.nodes {
            let Ok(Some(symbol_id)) = symbol_ref_target_symbol_id(*node_id, node) else {
                continue;
            };
            if let Some(symbol) = self.graph.symbols.get(&symbol_id) {
                self.fragment.symbols.insert(symbol_id, symbol.clone());
            }
        }
    }

    fn capture_referenced_imports(&mut self) {
        for (node_id, node) in &self.fragment.nodes {
            let Ok(Some(graph_id)) = subgraph_target_graph_id(*node_id, node) else {
                continue;
            };
            if let Some(import) = self.graph.imports.get(&graph_id) {
                self.fragment.imports.insert(graph_id, import.clone());
            }
        }
    }

    fn capture_node_ports(&mut self) {
        for (port_id, port) in &self.graph.ports {
            if self.nodes.contains(&port.node) {
                self.fragment.ports.insert(*port_id, port.clone());
            }
        }
    }

    fn capture_internal_edges(&mut self) {
        for (edge_id, edge) in &self.graph.edges {
            let from_node = self.graph.ports.get(&edge.from).map(|port| port.node);
            let to_node = self.graph.ports.get(&edge.to).map(|port| port.node);
            if let (Some(from_node), Some(to_node)) = (from_node, to_node)
                && self.nodes.contains(&from_node)
                && self.nodes.contains(&to_node)
            {
                self.fragment.edges.insert(*edge_id, edge.clone());
            }
        }
    }

    fn capture_bindings(&mut self) {
        for (binding_id, binding) in &self.graph.bindings {
            if self.binding_endpoint_can_survive(&binding.subject)
                && self.binding_endpoint_can_survive(&binding.target)
            {
                self.fragment.bindings.insert(*binding_id, binding.clone());
            }
        }
    }

    fn binding_endpoint_can_survive(&self, endpoint: &BindingEndpoint) -> bool {
        match endpoint {
            BindingEndpoint::Source { .. } => true,
            BindingEndpoint::GraphLocal { target } => {
                self.graph_local_target_is_in_fragment(*target)
            }
        }
    }

    fn graph_local_target_is_in_fragment(&self, target: GraphLocalBindingTarget) -> bool {
        match target {
            GraphLocalBindingTarget::Graph => true,
            GraphLocalBindingTarget::Node { id } => self.fragment.nodes.contains_key(&id),
            GraphLocalBindingTarget::Port { id } => self.fragment.ports.contains_key(&id),
            GraphLocalBindingTarget::Edge { id } => self.fragment.edges.contains_key(&id),
            GraphLocalBindingTarget::Group { id } => self.fragment.groups.contains_key(&id),
            GraphLocalBindingTarget::StickyNote { id } => {
                self.fragment.sticky_notes.contains_key(&id)
            }
        }
    }
}
