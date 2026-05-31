use super::GraphDiffPlanner;
use crate::core::EdgeId;
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_ports(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;
        let removed_ports_by_cascade = &self.removed_ports_by_cascade;
        let removed_edges_by_cascade = &mut self.removed_edges_by_cascade;

        for (id, port_to) in &to.ports {
            if let Some(port_from) = from.ports.get(id) {
                let structural_equal = port_from.node == port_to.node
                    && port_from.key == port_to.key
                    && port_from.dir == port_to.dir
                    && port_from.kind == port_to.kind
                    && port_from.capacity == port_to.capacity;

                if !structural_equal {
                    let mut removed_edge_ids: Vec<EdgeId> = Vec::new();
                    if let Ok(op) = GraphMutationPlanner::new(from).remove_port_op(*id) {
                        if let GraphOp::RemovePort { edges, .. } = &op {
                            removed_edge_ids = edges.iter().map(|(id, _)| *id).collect();
                            removed_edges_by_cascade.extend(edges.iter().map(|(id, _)| *id));
                        }
                        tx.ops.push(op);
                    }
                    tx.ops.push(GraphOp::AddPort {
                        id: *id,
                        port: port_to.clone(),
                    });

                    // Restore the port ordering for the owning node. `RemovePort` detaches the port id
                    // from `node.ports`, but `AddPort` does not implicitly re-attach it.
                    if let Some(node_to) = to.nodes.get(&port_to.node) {
                        let mut from_ports = node_to.ports.clone();
                        from_ports.retain(|p| p != id);
                        if from_ports != node_to.ports {
                            tx.ops.push(GraphOp::SetNodePorts {
                                id: port_to.node,
                                from: from_ports,
                                to: node_to.ports.clone(),
                            });
                        }
                    }

                    // `RemovePort` cascades to incident edges. If those edges still exist in `to`,
                    // re-add them to keep the patch apply-safe (edge diffing compares `from` vs `to`,
                    // not the intermediate state created by the removal).
                    for edge_id in removed_edge_ids {
                        if let Some(edge_to) = to.edges.get(&edge_id) {
                            tx.ops.push(GraphOp::AddEdge {
                                id: edge_id,
                                edge: edge_to.clone(),
                            });
                        }
                    }
                    continue;
                }

                if port_from.connectable != port_to.connectable {
                    tx.ops.push(GraphOp::SetPortConnectable {
                        id: *id,
                        from: port_from.connectable,
                        to: port_to.connectable,
                    });
                }
                if port_from.connectable_start != port_to.connectable_start {
                    tx.ops.push(GraphOp::SetPortConnectableStart {
                        id: *id,
                        from: port_from.connectable_start,
                        to: port_to.connectable_start,
                    });
                }
                if port_from.connectable_end != port_to.connectable_end {
                    tx.ops.push(GraphOp::SetPortConnectableEnd {
                        id: *id,
                        from: port_from.connectable_end,
                        to: port_to.connectable_end,
                    });
                }
                if port_from.ty != port_to.ty {
                    tx.ops.push(GraphOp::SetPortType {
                        id: *id,
                        from: port_from.ty.clone(),
                        to: port_to.ty.clone(),
                    });
                }
                if port_from.data != port_to.data {
                    tx.ops.push(GraphOp::SetPortData {
                        id: *id,
                        from: port_from.data.clone(),
                        to: port_to.data.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddPort {
                    id: *id,
                    port: port_to.clone(),
                });
            }
        }

        for id in from.ports.keys() {
            if !to.ports.contains_key(id) {
                if removed_ports_by_cascade.contains(id) {
                    continue;
                }
                if let Ok(op) = GraphMutationPlanner::new(from).remove_port_op(*id) {
                    if let GraphOp::RemovePort { edges, .. } = &op {
                        removed_edges_by_cascade.extend(edges.iter().map(|(id, _)| *id));
                    }
                    tx.ops.push(op);
                }
            }
        }
    }
}
