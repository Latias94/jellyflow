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
        let mut out = GraphFragment::default();

        let groups: BTreeSet<GroupId> = selected_groups.into_iter().collect();
        for group_id in &groups {
            if let Some(group) = graph.groups.get(group_id) {
                out.groups.insert(*group_id, group.clone());
            }
        }

        let mut nodes: BTreeSet<NodeId> = selected_nodes.into_iter().collect();
        if !groups.is_empty() {
            for (node_id, node) in &graph.nodes {
                if node.parent.is_some_and(|p| groups.contains(&p)) {
                    nodes.insert(*node_id);
                }
            }
        }

        for node_id in &nodes {
            if let Some(node) = graph.nodes.get(node_id) {
                let mut node = node.clone();
                if node.parent.is_some_and(|p| !groups.contains(&p)) {
                    node.parent = None;
                }
                out.nodes.insert(*node_id, node);
            }
        }

        for (node_id, node) in &out.nodes {
            let Ok(Some(symbol_id)) = symbol_ref_target_symbol_id(*node_id, node) else {
                continue;
            };
            if let Some(symbol) = graph.symbols.get(&symbol_id) {
                out.symbols.insert(symbol_id, symbol.clone());
            }
        }

        for (node_id, node) in &out.nodes {
            let Ok(Some(graph_id)) = subgraph_target_graph_id(*node_id, node) else {
                continue;
            };
            if let Some(import) = graph.imports.get(&graph_id) {
                out.imports.insert(graph_id, import.clone());
            }
        }

        for (port_id, port) in &graph.ports {
            if nodes.contains(&port.node) {
                out.ports.insert(*port_id, port.clone());
            }
        }

        for (edge_id, edge) in &graph.edges {
            let from_node = graph.ports.get(&edge.from).map(|p| p.node);
            let to_node = graph.ports.get(&edge.to).map(|p| p.node);
            if let (Some(from_node), Some(to_node)) = (from_node, to_node)
                && nodes.contains(&from_node)
                && nodes.contains(&to_node)
            {
                out.edges.insert(*edge_id, edge.clone());
            }
        }

        out
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
