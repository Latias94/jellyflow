use super::super::fixtures::{default_editor_config, make_graph, make_store};

use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::NodeGraphStoreEvent;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeKind, EdgeReconnectable, EdgeViewDescriptor, Graph, Node, NodeId,
    NodeKindKey,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

mod commit_pipeline;
mod external_profile;
mod history;
mod rejections;
