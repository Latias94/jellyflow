use jellyflow_core::core::{Node, NodeId, Port, PortId, PortKey};

use crate::runtime::commands::{GraphCommand, TopologyEdit, TopologyPort};
use crate::runtime::graph_runtime::GraphRevision;

/// Concrete graph records created from one semantic node-kind definition.
///
/// A creation can be inspected or combined with other topology edits, but it does not expose
/// raw graph operations or transactions. Publishing it through [`GraphRuntime`](crate::GraphRuntime)
/// therefore always requires an expected graph revision.
///
/// ```compile_fail
/// use jellyflow_core::CanvasPoint;
/// use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
///
/// let mut catalog = NodeCatalog::new();
/// catalog.register(NodeKindSpec::builder("demo.node", "Node", "demo.node").build()).unwrap();
/// let creation = catalog
///     .instantiate_node(&jellyflow_core::NodeKindKey::new("demo.node"), CanvasPoint::default())
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

    pub fn into_command(self, expected_revision: GraphRevision) -> GraphCommand {
        GraphCommand::topology(expected_revision, self.into_topology_edit())
    }
}
