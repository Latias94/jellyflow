use crate::ops::GraphOp;

pub(super) fn invert_port_op(op: &GraphOp) -> Vec<GraphOp> {
    match op {
        GraphOp::AddPort { id, port } => vec![GraphOp::RemovePort {
            id: *id,
            port: port.clone(),
            edges: Vec::new(),
        }],
        GraphOp::RemovePort { id, port, edges } => {
            let mut out: Vec<GraphOp> = Vec::new();
            out.push(GraphOp::AddPort {
                id: *id,
                port: port.clone(),
            });
            for (edge_id, edge) in edges {
                out.push(GraphOp::AddEdge {
                    id: *edge_id,
                    edge: edge.clone(),
                });
            }
            out
        }
        GraphOp::SetPortConnectable { id, from, to } => vec![GraphOp::SetPortConnectable {
            id: *id,
            from: *to,
            to: *from,
        }],
        GraphOp::SetPortConnectableStart { id, from, to } => {
            vec![GraphOp::SetPortConnectableStart {
                id: *id,
                from: *to,
                to: *from,
            }]
        }
        GraphOp::SetPortConnectableEnd { id, from, to } => {
            vec![GraphOp::SetPortConnectableEnd {
                id: *id,
                from: *to,
                to: *from,
            }]
        }
        GraphOp::SetPortType { id, from, to } => vec![GraphOp::SetPortType {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        GraphOp::SetPortData { id, from, to } => vec![GraphOp::SetPortData {
            id: *id,
            from: to.clone(),
            to: from.clone(),
        }],
        _ => unreachable!("port invert handler received non-port operation"),
    }
}
