use std::collections::BTreeSet;

use crate::core::{Graph, GroupId, NodeId, subgraph_target_graph_id, symbol_ref_target_symbol_id};

use super::model::GraphFragment;

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

struct FragmentCollector<'a> {
    graph: &'a Graph,
    groups: BTreeSet<GroupId>,
    nodes: BTreeSet<NodeId>,
    fragment: GraphFragment,
}

impl<'a> FragmentCollector<'a> {
    fn new(
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

    fn finish(mut self) -> GraphFragment {
        self.capture_selected_groups();
        self.include_group_children();
        self.capture_selected_nodes();
        self.capture_referenced_symbols();
        self.capture_referenced_imports();
        self.capture_node_ports();
        self.capture_internal_edges();
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
}

#[cfg(test)]
mod tests {
    use crate::core::{
        CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, Group, GroupId, Node,
        NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use crate::ops::fragment::GraphFragment;
    use serde_json::Value;

    fn node(kind: &str, pos: (f32, f32), parent: Option<GroupId>, ports: &[PortId]) -> Node {
        Node {
            kind: NodeKindKey::new(kind),
            kind_version: 1,
            pos: CanvasPoint { x: pos.0, y: pos.1 },
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: ports.to_vec(),
            data: Value::Null,
        }
    }

    fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: Value::Null,
        }
    }

    #[test]
    fn from_selection_detaches_nodes_when_group_is_not_selected() {
        let mut graph = Graph::default();

        let group_id = GroupId::new();
        graph.groups.insert(
            group_id,
            Group {
                title: "G".to_string(),
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 0.0 },
                    size: CanvasSize {
                        width: 100.0,
                        height: 100.0,
                    },
                },
                color: None,
            },
        );

        let node_id = NodeId::new();
        let in_port = PortId::new();
        graph
            .nodes
            .insert(node_id, node("A", (1.0, 2.0), Some(group_id), &[in_port]));
        graph
            .ports
            .insert(in_port, port(node_id, "in", PortDirection::In));

        let fragment = GraphFragment::from_selection(&graph, [node_id], std::iter::empty());
        assert!(fragment.groups.is_empty());
        assert_eq!(fragment.nodes.len(), 1);
        assert_eq!(fragment.nodes[&node_id].parent, None);
    }

    #[test]
    fn from_selection_keeps_parent_when_group_is_selected_and_includes_children() {
        let mut graph = Graph::default();

        let group_id = GroupId::new();
        graph.groups.insert(
            group_id,
            Group {
                title: "G".to_string(),
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 0.0 },
                    size: CanvasSize {
                        width: 100.0,
                        height: 100.0,
                    },
                },
                color: None,
            },
        );

        let a = NodeId::new();
        let a_out = PortId::new();
        graph
            .nodes
            .insert(a, node("A", (1.0, 2.0), Some(group_id), &[a_out]));
        graph
            .ports
            .insert(a_out, port(a, "out", PortDirection::Out));

        let b = NodeId::new();
        let b_in = PortId::new();
        graph
            .nodes
            .insert(b, node("B", (3.0, 4.0), Some(group_id), &[b_in]));
        graph.ports.insert(b_in, port(b, "in", PortDirection::In));

        let e = EdgeId::new();
        graph.edges.insert(
            e,
            Edge {
                kind: EdgeKind::Data,
                from: a_out,
                to: b_in,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        );

        let fragment = GraphFragment::from_selection(&graph, std::iter::empty(), [group_id]);
        assert_eq!(fragment.groups.len(), 1);
        assert!(fragment.nodes.contains_key(&a));
        assert!(fragment.nodes.contains_key(&b));
        assert_eq!(fragment.nodes[&a].parent, Some(group_id));
        assert_eq!(fragment.nodes[&b].parent, Some(group_id));
        assert_eq!(fragment.edges.len(), 1);
    }
}
