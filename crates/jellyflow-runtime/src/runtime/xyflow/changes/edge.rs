use serde::{Deserialize, Serialize};

use jellyflow_core::core::{Edge, EdgeId, EdgeKind, EdgeReconnectable, PortId};

/// Changes targeting edges (graph-owned).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EdgeChange {
    Add {
        id: EdgeId,
        edge: Edge,
    },
    Remove {
        id: EdgeId,
    },

    Kind {
        id: EdgeId,
        kind: EdgeKind,
    },
    Selectable {
        id: EdgeId,
        selectable: Option<bool>,
    },
    Deletable {
        id: EdgeId,
        deletable: Option<bool>,
    },
    Reconnectable {
        id: EdgeId,
        reconnectable: Option<EdgeReconnectable>,
    },
    Endpoints {
        id: EdgeId,
        from: PortId,
        to: PortId,
    },
}
