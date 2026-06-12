use super::GraphDiffPlanner;
use crate::core::{EdgeId, Port, PortId};
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_ports(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, port_to) in &to.ports {
            if self.removed_ports_by_cascade.contains(id) {
                self.push_op(GraphOp::AddPort {
                    id: *id,
                    port: port_to.clone(),
                });
                if from.nodes.contains_key(&port_to.node) {
                    self.nodes_requiring_port_order_restore.insert(port_to.node);
                }
                continue;
            }

            if let Some(port_from) = from.ports.get(id) {
                self.diff_existing_port(*id, port_from, port_to);
            } else {
                self.push_op(GraphOp::AddPort {
                    id: *id,
                    port: port_to.clone(),
                });
                if from.nodes.contains_key(&port_to.node) {
                    self.nodes_requiring_port_order_restore.insert(port_to.node);
                }
            }
        }

        self.diff_removed_ports();
    }

    fn diff_existing_port(&mut self, id: PortId, port_from: &Port, port_to: &Port) {
        if !ports_are_structurally_equal(port_from, port_to) {
            self.replace_structural_port(id, port_to);
            return;
        }

        self.diff_port_metadata(id, port_from, port_to);
    }

    fn replace_structural_port(&mut self, id: PortId, port_to: &Port) {
        self.detach_replaced_port_order(id);
        let removed_edge_ids = self.push_remove_port_for_replacement(id);

        self.push_op(GraphOp::AddPort {
            id,
            port: port_to.clone(),
        });

        self.replaced_ports_requiring_port_order_restore.insert(id);
        self.nodes_requiring_port_order_restore.insert(port_to.node);
        self.restore_edges_removed_with_port(removed_edge_ids);
    }

    fn detach_replaced_port_order(&mut self, id: PortId) {
        let Some(port_from) = self.from.ports.get(&id) else {
            return;
        };
        let Some(node_from) = self.from.nodes.get(&port_from.node) else {
            return;
        };

        let mut detached_ports = node_from.ports.clone();
        detached_ports.retain(|port_id| *port_id != id);
        if detached_ports != node_from.ports {
            self.push_op(GraphOp::SetNodePorts {
                id: port_from.node,
                from: node_from.ports.clone(),
                to: detached_ports,
            });
        }
    }

    fn push_remove_port_for_replacement(&mut self, id: PortId) -> Vec<EdgeId> {
        let mut removed_edge_ids = Vec::new();
        if let Ok(op) = GraphMutationPlanner::new(self.from).remove_port_op(id) {
            let op = self.with_target_removed_bindings(op);
            if let GraphOp::RemovePort {
                edges, bindings, ..
            } = &op
            {
                removed_edge_ids = edges.iter().map(|(id, _)| *id).collect();
                self.removed_edges_by_cascade
                    .extend(edges.iter().map(|(id, _)| *id));
                self.removed_bindings_by_cascade
                    .extend(bindings.iter().map(|(id, _)| *id));
            }
            self.push_op(op);
        }
        removed_edge_ids
    }

    fn restore_edges_removed_with_port(&mut self, removed_edge_ids: Vec<EdgeId>) {
        // `RemovePort` cascades to incident edges. If those edges still exist in `to`, re-add
        // them to keep the patch apply-safe because edge diffing compares `from` vs `to`, not
        // the intermediate state created by the removal.
        for edge_id in removed_edge_ids {
            if let Some(edge_to) = self.to.edges.get(&edge_id) {
                self.push_op(GraphOp::AddEdge {
                    id: edge_id,
                    edge: edge_to.clone(),
                });
                self.restored_edges_by_cascade.insert(edge_id);
            }
        }
    }

    pub(super) fn restore_target_port_orders(&mut self) {
        let node_ids: Vec<_> = self
            .nodes_requiring_port_order_restore
            .iter()
            .copied()
            .collect();
        for node_id in node_ids {
            let Some(node_from) = self.from.nodes.get(&node_id) else {
                continue;
            };
            let Some(node_to) = self.to.nodes.get(&node_id) else {
                continue;
            };
            let stable_ports = self.stable_restored_port_order(&node_from.ports);
            if stable_ports != node_to.ports {
                self.push_op(GraphOp::SetNodePorts {
                    id: node_id,
                    from: stable_ports,
                    to: node_to.ports.clone(),
                });
            }
        }
    }

    fn diff_port_metadata(&mut self, id: PortId, port_from: &Port, port_to: &Port) {
        if port_from.connectable != port_to.connectable {
            self.push_op(GraphOp::SetPortConnectable {
                id,
                from: port_from.connectable,
                to: port_to.connectable,
            });
        }
        if port_from.connectable_start != port_to.connectable_start {
            self.push_op(GraphOp::SetPortConnectableStart {
                id,
                from: port_from.connectable_start,
                to: port_to.connectable_start,
            });
        }
        if port_from.connectable_end != port_to.connectable_end {
            self.push_op(GraphOp::SetPortConnectableEnd {
                id,
                from: port_from.connectable_end,
                to: port_to.connectable_end,
            });
        }
        if port_from.ty != port_to.ty {
            self.push_op(GraphOp::SetPortType {
                id,
                from: port_from.ty.clone(),
                to: port_to.ty.clone(),
            });
        }
        if port_from.data != port_to.data {
            self.push_op(GraphOp::SetPortData {
                id,
                from: port_from.data.clone(),
                to: port_to.data.clone(),
            });
        }
    }

    fn diff_removed_ports(&mut self) {
        let from = self.from;
        let to = self.to;

        for id in from.ports.keys() {
            if to.ports.contains_key(id) || self.removed_ports_by_cascade.contains(id) {
                continue;
            }

            if let Ok(op) = GraphMutationPlanner::new(from).remove_port_op(*id) {
                let op = self.with_target_removed_bindings(op);
                if let GraphOp::RemovePort {
                    edges, bindings, ..
                } = &op
                {
                    self.removed_edges_by_cascade
                        .extend(edges.iter().map(|(id, _)| *id));
                    self.removed_bindings_by_cascade
                        .extend(bindings.iter().map(|(id, _)| *id));
                }
                self.push_op(op);
            }
        }
    }
}

fn ports_are_structurally_equal(from: &Port, to: &Port) -> bool {
    from.node == to.node
        && from.key == to.key
        && from.dir == to.dir
        && from.kind == to.kind
        && from.capacity == to.capacity
}
