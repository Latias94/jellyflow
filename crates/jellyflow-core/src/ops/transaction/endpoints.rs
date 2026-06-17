use serde::{Deserialize, Serialize};

use crate::core::{Edge, PortId};

/// Edge endpoint pair (from/to ports).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeEndpoints {
    pub from: PortId,
    pub to: PortId,
}

impl EdgeEndpoints {
    pub fn new(from: PortId, to: PortId) -> Self {
        Self { from, to }
    }

    pub fn from_edge(edge: &Edge) -> Self {
        Self::new(edge.from, edge.to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EdgeKind, EdgeReconnectable};

    #[test]
    fn edge_endpoints_can_snapshot_edge_ports() {
        let from = PortId::new();
        let to = PortId::new();
        let edge = Edge {
            hidden: false,
            selectable: Some(true),
            focusable: None,
            interaction_width: None,
            deletable: Some(false),
            reconnectable: Some(EdgeReconnectable::Bool(true)),
            ..Edge::new(EdgeKind::Data, from, to)
        };

        assert_eq!(
            EdgeEndpoints::from_edge(&edge),
            EdgeEndpoints::new(from, to)
        );
    }
}
