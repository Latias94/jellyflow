use std::collections::BTreeMap;

use crate::core::{
    CanvasPoint, Edge, Graph, GroupId, Node, NodeId, Port, PortId, SymbolId, symbol_ref_node_data,
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
        FragmentPastePlanner::new(self, remapper, tuning).finish()
    }
}

struct FragmentPastePlanner<'a> {
    fragment: &'a GraphFragment,
    remapper: &'a IdRemapper,
    tuning: PasteTuning,
    group_map: BTreeMap<GroupId, GroupId>,
    node_map: BTreeMap<NodeId, NodeId>,
    port_map: BTreeMap<PortId, PortId>,
    symbol_map: BTreeMap<SymbolId, SymbolId>,
    tx: GraphTransaction,
}

impl<'a> FragmentPastePlanner<'a> {
    fn new(fragment: &'a GraphFragment, remapper: &'a IdRemapper, tuning: PasteTuning) -> Self {
        Self {
            fragment,
            remapper,
            tuning,
            group_map: remapped_groups(fragment, remapper),
            node_map: remapped_nodes(fragment, remapper),
            port_map: remapped_ports(fragment, remapper),
            symbol_map: remapped_symbols(fragment, remapper),
            tx: GraphTransaction::new(),
        }
    }

    fn finish(mut self) -> GraphTransaction {
        self.push_imports();
        self.push_symbols();
        self.push_groups();
        self.push_batch_insert_ops();
        self.push_sticky_notes();
        self.tx
    }

    fn push_op(&mut self, op: GraphOp) {
        self.tx.push(op);
    }

    fn extend_ops(&mut self, ops: impl IntoIterator<Item = GraphOp>) {
        self.tx.extend(ops);
    }

    fn push_imports(&mut self) {
        for (id, import) in &self.fragment.imports {
            self.push_op(GraphOp::AddImport {
                id: *id,
                import: import.clone(),
            });
        }
    }

    fn push_symbols(&mut self) {
        for (old_id, old_symbol) in &self.fragment.symbols {
            let new_id = self.symbol_map[old_id];
            self.push_op(GraphOp::AddSymbol {
                id: new_id,
                symbol: old_symbol.clone(),
            });
        }
    }

    fn push_groups(&mut self) {
        for (old_id, group) in &self.fragment.groups {
            self.push_op(GraphOp::AddGroup {
                id: self.group_map[old_id],
                group: group.clone(),
            });
        }
    }

    fn push_batch_insert_ops(&mut self) {
        let planning_graph = self.planning_graph();
        let mut batch = GraphMutationBatchPlanner::new(&planning_graph);
        self.stage_nodes(&mut batch);
        self.stage_edges(&mut batch);
        self.extend_ops(batch.into_ops());
    }

    fn planning_graph(&self) -> Graph {
        let mut planning_graph = Graph::default();
        for (old_id, group) in &self.fragment.groups {
            planning_graph
                .groups
                .insert(self.group_map[old_id], group.clone());
        }
        planning_graph
    }

    fn stage_nodes(&self, batch: &mut GraphMutationBatchPlanner<'_>) {
        for (old_id, old_node) in &self.fragment.nodes {
            let new_id = self.node_map[old_id];
            batch
                .add_node_with_ports(
                    new_id,
                    self.remapped_node(*old_id, old_node),
                    self.remapped_node_ports(old_node, new_id),
                )
                .expect("fragment paste should stage valid node and ports");
        }
    }

    fn stage_edges(&self, batch: &mut GraphMutationBatchPlanner<'_>) {
        for (old_edge_id, old_edge) in &self.fragment.edges {
            batch
                .add_edge(
                    self.remapper.remap_edge(*old_edge_id),
                    self.remapped_edge(old_edge),
                )
                .expect("fragment paste should stage valid edges");
        }
    }

    fn push_sticky_notes(&mut self) {
        for (old_id, note) in &self.fragment.sticky_notes {
            self.push_op(GraphOp::AddStickyNote {
                id: self.remapper.remap_note(*old_id),
                note: note.clone(),
            });
        }
    }

    fn remapped_node(&self, old_id: NodeId, old_node: &Node) -> Node {
        let mut node = old_node.clone();

        if let Ok(Some(old_symbol_id)) = symbol_ref_target_symbol_id(old_id, old_node)
            && let Some(new_symbol_id) = self.symbol_map.get(&old_symbol_id)
        {
            node.data = symbol_ref_node_data(*new_symbol_id);
        }

        node.pos = CanvasPoint {
            x: node.pos.x + self.tuning.offset.x,
            y: node.pos.y + self.tuning.offset.y,
        };
        node.parent = node
            .parent
            .and_then(|old_parent| self.group_map.get(&old_parent).copied());
        node
    }

    fn remapped_node_ports(&self, old_node: &Node, new_id: NodeId) -> Vec<(PortId, Port)> {
        let mut ports: Vec<(PortId, Port)> = Vec::new();
        for old_port_id in &old_node.ports {
            if let Some(old_port) = self.fragment.ports.get(old_port_id) {
                let mut port = old_port.clone();
                port.node = new_id;
                ports.push((self.port_map[old_port_id], port));
            }
        }
        ports
    }

    fn remapped_edge(&self, old_edge: &Edge) -> Edge {
        Edge {
            kind: old_edge.kind,
            from: self.port_map[&old_edge.from],
            to: self.port_map[&old_edge.to],
            selectable: old_edge.selectable,
            deletable: old_edge.deletable,
            reconnectable: old_edge.reconnectable,
        }
    }
}

fn remapped_groups(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<GroupId, GroupId> {
    let mut map = BTreeMap::new();
    for group_id in fragment.groups.keys() {
        map.insert(*group_id, remapper.remap_group(*group_id));
    }
    map
}

fn remapped_nodes(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<NodeId, NodeId> {
    let mut map = BTreeMap::new();
    for node_id in fragment.nodes.keys() {
        map.insert(*node_id, remapper.remap_node(*node_id));
    }
    map
}

fn remapped_ports(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<PortId, PortId> {
    let mut map = BTreeMap::new();
    for port_id in fragment.ports.keys() {
        map.insert(*port_id, remapper.remap_port(*port_id));
    }
    map
}

fn remapped_symbols(
    fragment: &GraphFragment,
    remapper: &IdRemapper,
) -> BTreeMap<SymbolId, SymbolId> {
    let mut map = BTreeMap::new();
    for symbol_id in fragment.symbols.keys() {
        map.insert(*symbol_id, remapper.remap_symbol(*symbol_id));
    }
    map
}
