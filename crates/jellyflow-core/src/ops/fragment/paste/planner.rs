use crate::core::{
    Binding, BindingEndpoint, CanvasPoint, Edge, Graph, GraphLocalBindingTarget, Node, NodeId,
    Port, PortId,
};
use crate::core::{symbol_ref_node_data, symbol_ref_target_symbol_id};
use crate::ops::{GraphMutationBatchPlanner, GraphOp, GraphTransaction};

use super::super::{model::GraphFragment, remap::IdRemapper};
use super::remapped_ids::RemappedFragmentIds;
use super::tuning::PasteTuning;

pub(super) struct FragmentPastePlanner<'a> {
    fragment: &'a GraphFragment,
    ids: RemappedFragmentIds,
    tuning: PasteTuning,
    tx: GraphTransaction,
}

impl<'a> FragmentPastePlanner<'a> {
    pub(super) fn new(
        fragment: &'a GraphFragment,
        remapper: &'a IdRemapper,
        tuning: PasteTuning,
    ) -> Self {
        Self {
            fragment,
            ids: RemappedFragmentIds::new(fragment, remapper),
            tuning,
            tx: GraphTransaction::new(),
        }
    }

    pub(super) fn finish(mut self) -> GraphTransaction {
        self.push_imports();
        self.push_symbols();
        self.push_groups();
        self.push_batch_insert_ops();
        self.push_sticky_notes();
        self.push_bindings();
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
            self.push_op(GraphOp::AddSymbol {
                id: self.ids.symbol(*old_id),
                symbol: old_symbol.clone(),
            });
        }
    }

    fn push_groups(&mut self) {
        for (old_id, group) in &self.fragment.groups {
            self.push_op(GraphOp::AddGroup {
                id: self.ids.group(*old_id),
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
            planning_graph.insert_group(self.ids.group(*old_id), group.clone());
        }
        planning_graph
    }

    fn stage_nodes(&self, batch: &mut GraphMutationBatchPlanner<'_>) {
        for (old_id, old_node) in &self.fragment.nodes {
            batch
                .add_node_with_ports(
                    self.ids.node(*old_id),
                    self.remapped_node(*old_id, old_node),
                    self.remapped_node_ports(old_node, self.ids.node(*old_id)),
                )
                .expect("fragment paste should stage valid node and ports");
        }
    }

    fn stage_edges(&self, batch: &mut GraphMutationBatchPlanner<'_>) {
        for (old_edge_id, old_edge) in &self.fragment.edges {
            batch
                .add_edge(self.ids.edge(*old_edge_id), self.remapped_edge(old_edge))
                .expect("fragment paste should stage valid edges");
        }
    }

    fn push_sticky_notes(&mut self) {
        for (old_id, note) in &self.fragment.sticky_notes {
            self.push_op(GraphOp::AddStickyNote {
                id: self.ids.sticky_note(*old_id),
                note: note.clone(),
            });
        }
    }

    fn push_bindings(&mut self) {
        for (old_id, binding) in &self.fragment.bindings {
            if let Some(binding) = self.remapped_binding(binding) {
                self.push_op(GraphOp::AddBinding {
                    id: self.ids.binding(*old_id),
                    binding,
                });
            }
        }
    }

    fn remapped_node(&self, old_id: NodeId, old_node: &Node) -> Node {
        let mut node = old_node.clone();

        if let Ok(Some(old_symbol_id)) = symbol_ref_target_symbol_id(old_id, old_node)
            && let Some(new_symbol_id) = self.ids.maybe_symbol(old_symbol_id)
        {
            node.data = symbol_ref_node_data(new_symbol_id);
        }

        node.pos = CanvasPoint {
            x: node.pos.x + self.tuning.offset.x,
            y: node.pos.y + self.tuning.offset.y,
        };
        node.parent = node
            .parent
            .and_then(|old_parent| self.ids.maybe_group(old_parent));
        node
    }

    fn remapped_node_ports(&self, old_node: &Node, new_id: NodeId) -> Vec<(PortId, Port)> {
        let mut ports: Vec<(PortId, Port)> = Vec::new();
        for old_port_id in &old_node.ports {
            if let Some(old_port) = self.fragment.ports.get(old_port_id) {
                let mut port = old_port.clone();
                port.node = new_id;
                ports.push((self.ids.port(*old_port_id), port));
            }
        }
        ports
    }

    fn remapped_edge(&self, old_edge: &Edge) -> Edge {
        Edge {
            kind: old_edge.kind,
            from: self.ids.port(old_edge.from),
            to: self.ids.port(old_edge.to),
            hidden: old_edge.hidden,
            selectable: old_edge.selectable,
            focusable: old_edge.focusable,
            interaction_width: old_edge.interaction_width,
            deletable: old_edge.deletable,
            reconnectable: old_edge.reconnectable,
        }
    }

    fn remapped_binding(&self, binding: &Binding) -> Option<Binding> {
        Some(Binding {
            subject: self.remapped_binding_endpoint(&binding.subject)?,
            target: self.remapped_binding_endpoint(&binding.target)?,
            kind: binding.kind.clone(),
            meta: binding.meta.clone(),
        })
    }

    fn remapped_binding_endpoint(&self, endpoint: &BindingEndpoint) -> Option<BindingEndpoint> {
        match endpoint {
            BindingEndpoint::Source { anchor } => Some(BindingEndpoint::Source {
                anchor: anchor.clone(),
            }),
            BindingEndpoint::GraphLocal { target } => Some(BindingEndpoint::GraphLocal {
                target: self.remapped_graph_local_target(*target)?,
            }),
        }
    }

    fn remapped_graph_local_target(
        &self,
        target: GraphLocalBindingTarget,
    ) -> Option<GraphLocalBindingTarget> {
        match target {
            GraphLocalBindingTarget::Graph => Some(GraphLocalBindingTarget::Graph),
            GraphLocalBindingTarget::Node { id } => self
                .ids
                .maybe_node(id)
                .map(|id| GraphLocalBindingTarget::Node { id }),
            GraphLocalBindingTarget::Port { id } => self
                .ids
                .maybe_port(id)
                .map(|id| GraphLocalBindingTarget::Port { id }),
            GraphLocalBindingTarget::Edge { id } => self
                .ids
                .maybe_edge(id)
                .map(|id| GraphLocalBindingTarget::Edge { id }),
            GraphLocalBindingTarget::Group { id } => self
                .ids
                .maybe_group(id)
                .map(|id| GraphLocalBindingTarget::Group { id }),
            GraphLocalBindingTarget::StickyNote { id } => self
                .ids
                .maybe_sticky_note(id)
                .map(|id| GraphLocalBindingTarget::StickyNote { id }),
        }
    }
}
