use super::GraphDiffPlanner;
use crate::core::{EdgeId, Port, PortId};
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_ports(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, port_to) in &to.ports {
            if let Some(port_from) = from.ports.get(id) {
                self.diff_existing_port(*id, port_from, port_to);
            } else {
                self.tx.ops.push(GraphOp::AddPort {
                    id: *id,
                    port: port_to.clone(),
                });
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
        let removed_edge_ids = self.push_remove_port_for_replacement(id);

        self.tx.ops.push(GraphOp::AddPort {
            id,
            port: port_to.clone(),
        });

        self.restore_replaced_port_order(id, port_to);
        self.restore_edges_removed_with_port(removed_edge_ids);
    }

    fn push_remove_port_for_replacement(&mut self, id: PortId) -> Vec<EdgeId> {
        let mut removed_edge_ids = Vec::new();
        if let Ok(op) = GraphMutationPlanner::new(self.from).remove_port_op(id) {
            if let GraphOp::RemovePort { edges, .. } = &op {
                removed_edge_ids = edges.iter().map(|(id, _)| *id).collect();
                self.removed_edges_by_cascade
                    .extend(edges.iter().map(|(id, _)| *id));
            }
            self.tx.ops.push(op);
        }
        removed_edge_ids
    }

    fn restore_replaced_port_order(&mut self, id: PortId, port_to: &Port) {
        // `RemovePort` detaches the port id from `node.ports`, but `AddPort` does not
        // implicitly re-attach it.
        if let Some(node_to) = self.to.nodes.get(&port_to.node) {
            let mut from_ports = node_to.ports.clone();
            from_ports.retain(|p| *p != id);
            if from_ports != node_to.ports {
                self.tx.ops.push(GraphOp::SetNodePorts {
                    id: port_to.node,
                    from: from_ports,
                    to: node_to.ports.clone(),
                });
            }
        }
    }

    fn restore_edges_removed_with_port(&mut self, removed_edge_ids: Vec<EdgeId>) {
        // `RemovePort` cascades to incident edges. If those edges still exist in `to`, re-add
        // them to keep the patch apply-safe because edge diffing compares `from` vs `to`, not
        // the intermediate state created by the removal.
        for edge_id in removed_edge_ids {
            if let Some(edge_to) = self.to.edges.get(&edge_id) {
                self.tx.ops.push(GraphOp::AddEdge {
                    id: edge_id,
                    edge: edge_to.clone(),
                });
            }
        }
    }

    fn diff_port_metadata(&mut self, id: PortId, port_from: &Port, port_to: &Port) {
        if port_from.connectable != port_to.connectable {
            self.tx.ops.push(GraphOp::SetPortConnectable {
                id,
                from: port_from.connectable,
                to: port_to.connectable,
            });
        }
        if port_from.connectable_start != port_to.connectable_start {
            self.tx.ops.push(GraphOp::SetPortConnectableStart {
                id,
                from: port_from.connectable_start,
                to: port_to.connectable_start,
            });
        }
        if port_from.connectable_end != port_to.connectable_end {
            self.tx.ops.push(GraphOp::SetPortConnectableEnd {
                id,
                from: port_from.connectable_end,
                to: port_to.connectable_end,
            });
        }
        if port_from.ty != port_to.ty {
            self.tx.ops.push(GraphOp::SetPortType {
                id,
                from: port_from.ty.clone(),
                to: port_to.ty.clone(),
            });
        }
        if port_from.data != port_to.data {
            self.tx.ops.push(GraphOp::SetPortData {
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
                if let GraphOp::RemovePort { edges, .. } = &op {
                    self.removed_edges_by_cascade
                        .extend(edges.iter().map(|(id, _)| *id));
                }
                self.tx.ops.push(op);
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
