use jellyflow_core::core::{Node, NodeId, Port, PortId, PortKey};

use crate::runtime::commands::{GraphCommand, TopologyEdit, TopologyPort};
use crate::runtime::graph_runtime::GraphVersion;

/// Concrete graph records created from one semantic node-kind definition.
///
/// A creation can be inspected or combined with other topology edits, but it does not expose
/// raw graph operations or transactions. Publishing it through
/// [`GraphRuntime`](crate::graph::GraphRuntime) therefore always requires an exact graph version
/// minted by a snapshot.
///
/// ```compile_fail
/// use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize};
/// use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
///
/// let mut catalog = NodeCatalog::new();
/// catalog.register(NodeKindSpec::builder("demo.node").build()).unwrap();
/// let creation = catalog
///     .instantiate_node(
///         &jellyflow_core::NodeKindKey::new("demo.node"),
///         CanvasRect {
///             origin: CanvasPoint::default(),
///             size: CanvasSize { width: 160.0, height: 96.0 },
///         },
///     )
///     .unwrap();
/// let transaction = creation.into_transaction();
/// ```
#[derive(Debug, Clone)]
pub struct NodeCreation {
    node_id: NodeId,
    node: Node,
    ports: Vec<(PortId, Port)>,
}

impl NodeCreation {
    pub(super) fn new(node_id: NodeId, node: Node, ports: Vec<(PortId, Port)>) -> Self {
        Self {
            node_id,
            node,
            ports,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_parts_for_test(
        node_id: NodeId,
        node: Node,
        ports: Vec<(PortId, Port)>,
    ) -> Self {
        Self::new(node_id, node, ports)
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn ports(&self) -> &[(PortId, Port)] {
        &self.ports
    }

    pub fn port_id(&self, key: &PortKey) -> Option<PortId> {
        self.ports
            .iter()
            .find_map(|(id, port)| (&port.key == key).then_some(*id))
    }

    pub fn into_parts(self) -> (NodeId, Node, Vec<(PortId, Port)>) {
        (self.node_id, self.node, self.ports)
    }

    pub fn into_topology_edit(self) -> TopologyEdit {
        let ports = self
            .ports
            .into_iter()
            .map(|(id, port)| TopologyPort::new(id, port));
        TopologyEdit::new().add_node(self.node_id, self.node, ports)
    }

    pub fn into_command(self, expected_version: GraphVersion) -> GraphCommand {
        GraphCommand::topology(expected_version, self.into_topology_edit())
    }
}
