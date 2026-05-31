use std::collections::BTreeMap;

use crate::core::{
    CanvasPoint, Edge, Graph, GroupId, NodeId, Port, PortId, SymbolId, symbol_ref_node_data,
    symbol_ref_target_symbol_id,
};
use crate::ops::{GraphMutationBatchPlanner, GraphOp, GraphTransaction};

use super::model::GraphFragment;
use super::remap::IdRemapper;

/// Paste tuning for translating fragments into a destination graph.
#[derive(Debug, Clone, Copy)]
pub struct PasteTuning {
    /// Additional offset applied to every pasted node position.
    pub offset: CanvasPoint,
}

impl Default for PasteTuning {
    fn default() -> Self {
        Self {
            offset: CanvasPoint { x: 0.0, y: 0.0 },
        }
    }
}

impl GraphFragment {
    /// Remaps IDs and produces a transaction that inserts the fragment into a graph.
    ///
    /// The resulting transaction is deterministic for a given seed.
    pub fn to_paste_transaction(
        &self,
        remapper: &IdRemapper,
        tuning: PasteTuning,
    ) -> GraphTransaction {
        let mut tx = GraphTransaction::new();

        let mut group_map: BTreeMap<GroupId, GroupId> = BTreeMap::new();
        for group_id in self.groups.keys() {
            group_map.insert(*group_id, remapper.remap_group(*group_id));
        }

        let mut node_map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        for node_id in self.nodes.keys() {
            node_map.insert(*node_id, remapper.remap_node(*node_id));
        }

        let mut port_map: BTreeMap<PortId, PortId> = BTreeMap::new();
        for port_id in self.ports.keys() {
            port_map.insert(*port_id, remapper.remap_port(*port_id));
        }

        let mut symbol_map: BTreeMap<SymbolId, SymbolId> = BTreeMap::new();
        for symbol_id in self.symbols.keys() {
            symbol_map.insert(*symbol_id, remapper.remap_symbol(*symbol_id));
        }

        for (id, import) in &self.imports {
            tx.push(GraphOp::AddImport {
                id: *id,
                import: import.clone(),
            });
        }

        for (old_id, old_symbol) in &self.symbols {
            let new_id = symbol_map[old_id];
            tx.push(GraphOp::AddSymbol {
                id: new_id,
                symbol: old_symbol.clone(),
            });
        }

        for (old_id, group) in &self.groups {
            tx.push(GraphOp::AddGroup {
                id: group_map[old_id],
                group: group.clone(),
            });
        }

        let mut planning_graph = Graph::default();
        for (old_id, group) in &self.groups {
            planning_graph
                .groups
                .insert(group_map[old_id], group.clone());
        }

        let mut batch = GraphMutationBatchPlanner::new(&planning_graph);

        for (old_id, old_node) in &self.nodes {
            let new_id = node_map[old_id];
            let mut node = old_node.clone();

            if let Ok(Some(old_symbol_id)) = symbol_ref_target_symbol_id(*old_id, old_node)
                && let Some(new_symbol_id) = symbol_map.get(&old_symbol_id)
            {
                node.data = symbol_ref_node_data(*new_symbol_id);
            }

            node.pos = CanvasPoint {
                x: node.pos.x + tuning.offset.x,
                y: node.pos.y + tuning.offset.y,
            };
            node.parent = node
                .parent
                .and_then(|old_parent| group_map.get(&old_parent).copied());

            let mut ports: Vec<(PortId, Port)> = Vec::new();
            for old_port_id in &old_node.ports {
                if let Some(old_port) = self.ports.get(old_port_id) {
                    let mut port = old_port.clone();
                    port.node = new_id;
                    ports.push((port_map[old_port_id], port));
                }
            }

            batch
                .add_node_with_ports(new_id, node, ports)
                .expect("fragment paste should stage valid node and ports");
        }

        for (old_edge_id, old_edge) in &self.edges {
            let new_edge_id = remapper.remap_edge(*old_edge_id);
            let edge = Edge {
                kind: old_edge.kind,
                from: port_map[&old_edge.from],
                to: port_map[&old_edge.to],
                selectable: old_edge.selectable,
                deletable: old_edge.deletable,
                reconnectable: old_edge.reconnectable,
            };
            batch
                .add_edge(new_edge_id, edge)
                .expect("fragment paste should stage valid edges");
        }
        tx.ops.extend(batch.into_ops());

        for (old_id, note) in &self.sticky_notes {
            tx.push(GraphOp::AddStickyNote {
                id: remapper.remap_note(*old_id),
                note: note.clone(),
            });
        }

        tx
    }
}
