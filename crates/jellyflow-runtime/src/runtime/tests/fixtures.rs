use jellyflow_core::core::{EdgeId, Graph, GraphBuilder, GroupId, NodeId, PortId};

use crate::runtime::graph_runtime::{NodeLifetimeKey, PortConnectionDragIntent, PortInstanceKey};
use crate::runtime::interaction::{KeyboardConnectionPort, NodeDragStart, SelectionMode};
use crate::runtime::view_session::{ViewSession, ViewSessionError};

pub(super) trait GraphFixtureMutExt {
    fn with_detached_builder<R>(&mut self, f: impl FnOnce(&mut GraphBuilder) -> R) -> R;
}

impl GraphFixtureMutExt for Graph {
    fn with_detached_builder<R>(&mut self, f: impl FnOnce(&mut GraphBuilder) -> R) -> R {
        let mut builder = GraphBuilder::from_detached_graph(self.clone());
        let result = f(&mut builder);
        *self = builder.build_unchecked();
        result
    }
}

pub(crate) fn current_node_lifetime(view: &ViewSession, node: NodeId) -> NodeLifetimeKey {
    view.graph_snapshot()
        .expect("test view snapshot")
        .node_lifetime_key(node)
        .expect("test node is current")
}

pub(crate) fn current_port_instance(view: &ViewSession, port: PortId) -> PortInstanceKey {
    view.graph_snapshot()
        .expect("test view snapshot")
        .port_instance_key(port)
        .expect("test port is current")
}

pub(crate) fn set_selection_by_id(
    view: &ViewSession,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
    groups: Vec<GroupId>,
) -> Result<(), ViewSessionError> {
    let snapshot = view.graph_snapshot()?;
    let nodes = nodes
        .into_iter()
        .map(|node| {
            snapshot
                .node_lifetime_key(node)
                .expect("selected test node is current")
        })
        .collect();
    let edges = edges
        .into_iter()
        .map(|edge| {
            snapshot
                .edge_instance_key(edge)
                .expect("selected test edge is current")
        })
        .collect();
    view.set_selection(nodes, edges, groups)
}

pub(crate) fn node_drag_start_by_id(
    view: &ViewSession,
    node: NodeId,
    selection_mode: SelectionMode,
) -> Result<Option<NodeDragStart>, ViewSessionError> {
    view.node_drag_start(current_node_lifetime(view, node), selection_mode)
}

pub(crate) fn port_connection_drag_intent_by_id(
    view: &ViewSession,
    port: PortId,
) -> Result<PortConnectionDragIntent, ViewSessionError> {
    view.port_connection_drag_intent(current_port_instance(view, port))
}

pub(crate) fn keyboard_connection_port_by_id(
    view: &ViewSession,
    port: PortId,
) -> Result<KeyboardConnectionPort, ViewSessionError> {
    view.keyboard_connection_port(current_port_instance(view, port))
}
