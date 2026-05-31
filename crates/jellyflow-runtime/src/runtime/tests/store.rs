use super::fixtures::{default_editor_config, make_graph};

use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::NodeGraphStoreEvent;
use crate::runtime::lookups::ConnectionSide;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeKind, EdgeReconnectable, Graph, GraphId, Group,
    GroupId, Node, NodeId, NodeKindKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

mod dispatch;
mod lookups;
mod middleware;
mod replacement;
mod revisions;
mod subscriptions;
