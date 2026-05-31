use crate::ops::GraphOp;

mod document;
mod edge;
mod node;
mod port;

pub(super) fn coalesce_setter_chains(ops: Vec<GraphOp>) -> Vec<GraphOp> {
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        if let Some(last) = out.last_mut()
            && try_coalesce_setter(last, &op)
        {
            continue;
        }
        out.push(op);
    }
    out
}

fn try_coalesce_setter(last: &mut GraphOp, next: &GraphOp) -> bool {
    node::try_coalesce_node_setter(last, next)
        || port::try_coalesce_port_setter(last, next)
        || edge::try_coalesce_edge_setter(last, next)
        || document::try_coalesce_document_setter(last, next)
}

fn coalesce_value<Id, T>(a: &Id, last_to: &mut T, b: &Id, from: &T, to: &T) -> bool
where
    Id: PartialEq,
    T: Clone + PartialEq,
{
    if a == b && last_to == from {
        *last_to = to.clone();
        true
    } else {
        false
    }
}
