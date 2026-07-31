use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::graph_kernel::GraphMutationFootprint;
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, EdgeId, Graph, GraphLocalBindingTarget, Node, NodeId, Port, PortId,
    Symbol, SymbolId,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::{ConnectDecision, Diagnostic, EdgeEndpoint};
use crate::runtime::graph_runtime::{
    GraphIndex, GraphRevision, GraphSnapshot, GraphVersion, RuntimePatch,
};

/// A graph mutation planned against one owned graph snapshot.
#[derive(Debug, Clone)]
pub struct GraphCommand {
    identity: GraphCommandIdentity,
    pub(super) expected_version: GraphVersion,
    pub(super) label: Option<String>,
    pub(super) record_history: bool,
    pub(super) action: GraphCommandAction,
}

struct GraphCommandIdentityMarker;

/// Opaque identity shared by one command and every outcome issued for it.
///
/// Cloning a command preserves this identity. Constructing another command, even with identical
/// payload and expected version, creates a different identity. This lets asynchronous consumers
/// reject a valid outcome that belongs to a different command without exposing a forgeable counter.
#[derive(Clone)]
pub struct GraphCommandIdentity(Arc<GraphCommandIdentityMarker>);

impl GraphCommandIdentity {
    pub(super) fn new() -> Self {
        Self(Arc::new(GraphCommandIdentityMarker))
    }
}

impl fmt::Debug for GraphCommandIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GraphCommandIdentity(<opaque>)")
    }
}

impl PartialEq for GraphCommandIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for GraphCommandIdentity {}

impl Hash for GraphCommandIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.0), state);
    }
}

#[derive(Debug, Clone)]
pub(super) enum GraphCommandAction {
    Undo,
    Redo,
    SetNodeData {
        node: NodeId,
        data: serde_json::Value,
    },
    SetNodeGeometry(Vec<NodeGeometryEdit>),
    SetPortData {
        port: PortId,
        data: serde_json::Value,
    },
    Symbols(SymbolEdit),
    Topology(TopologyEdit),
}

/// One node's optional position and size changes inside an atomic geometry command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGeometryEdit {
    node: NodeId,
    position: Option<CanvasPoint>,
    size: Option<Option<CanvasSize>>,
}

impl NodeGeometryEdit {
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            position: None,
            size: None,
        }
    }

    pub fn with_position(mut self, position: CanvasPoint) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets a persisted size, or clears it when `size` is `None`.
    pub fn with_size(mut self, size: Option<CanvasSize>) -> Self {
        self.size = Some(size);
        self
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn position(&self) -> Option<CanvasPoint> {
        self.position
    }

    /// Returns `None` when size is unchanged and `Some(None)` when it is cleared.
    pub fn size(&self) -> Option<Option<CanvasSize>> {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.position.is_none() && self.size.is_none()
    }
}

impl GraphCommand {
    pub fn undo(expected_version: GraphVersion) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: Some("undo".to_owned()),
            record_history: true,
            action: GraphCommandAction::Undo,
        }
    }

    pub fn redo(expected_version: GraphVersion) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: Some("redo".to_owned()),
            record_history: true,
            action: GraphCommandAction::Redo,
        }
    }

    pub fn set_node_data(
        expected_version: GraphVersion,
        node: NodeId,
        data: serde_json::Value,
    ) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: None,
            record_history: true,
            action: GraphCommandAction::SetNodeData { node, data },
        }
    }

    pub fn set_node_position(
        expected_version: GraphVersion,
        node: NodeId,
        position: CanvasPoint,
    ) -> Self {
        Self::set_node_geometry(
            expected_version,
            [NodeGeometryEdit::new(node).with_position(position)],
        )
    }

    pub fn set_node_size(
        expected_version: GraphVersion,
        node: NodeId,
        size: Option<CanvasSize>,
    ) -> Self {
        Self::set_node_geometry(
            expected_version,
            [NodeGeometryEdit::new(node).with_size(size)],
        )
    }

    /// Applies adapter-resolved position and size edits as one revision and one undo boundary.
    ///
    /// Values are committed exactly. The runtime preserves `Node::extent` and
    /// `Node::expand_parent` as adapter-facing interaction preferences but does not clamp geometry
    /// or mutate parent groups implicitly.
    pub fn set_node_geometry(
        expected_version: GraphVersion,
        edits: impl IntoIterator<Item = NodeGeometryEdit>,
    ) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: None,
            record_history: true,
            action: GraphCommandAction::SetNodeGeometry(edits.into_iter().collect()),
        }
    }

    pub fn set_port_data(
        expected_version: GraphVersion,
        port: PortId,
        data: serde_json::Value,
    ) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: None,
            record_history: true,
            action: GraphCommandAction::SetPortData { port, data },
        }
    }

    /// Applies one revision-checked, atomic edit to graph-scoped symbols.
    pub fn symbols(expected_version: GraphVersion, edit: SymbolEdit) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: None,
            record_history: true,
            action: GraphCommandAction::Symbols(edit),
        }
    }

    pub fn topology(expected_version: GraphVersion, edit: TopologyEdit) -> Self {
        Self {
            identity: GraphCommandIdentity::new(),
            expected_version,
            label: None,
            record_history: true,
            action: GraphCommandAction::Topology(edit),
        }
    }

    pub fn connection(expected_version: GraphVersion, edit: ConnectionEdit) -> Self {
        Self::topology(expected_version, TopologyEdit::new().connection(edit))
    }

    pub fn connect(
        expected_version: GraphVersion,
        edge: EdgeId,
        from: PortId,
        to: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::connection(
            expected_version,
            ConnectionEdit::connect(edge, from, to, mode),
        )
    }

    pub fn disconnect(expected_version: GraphVersion, edge: EdgeId) -> Self {
        Self::connection(expected_version, ConnectionEdit::disconnect(edge))
    }

    pub fn reconnect(
        expected_version: GraphVersion,
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        new_port: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::connection(
            expected_version,
            ConnectionEdit::reconnect(edge, endpoint, new_port, mode),
        )
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Omits a derived mutation from user-facing undo and redo history.
    ///
    /// The command still advances the graph revision and publishes the normal patch. Use this only
    /// for state that is deterministically reconstructed from authoritative graph or adapter facts,
    /// such as a content-driven node size. Ordinary user edits should remain undoable.
    #[must_use]
    pub fn without_history(mut self) -> Self {
        self.record_history = false;
        self
    }

    pub(crate) fn records_history(&self) -> bool {
        self.record_history
    }

    pub fn expected_version(&self) -> GraphVersion {
        self.expected_version
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the direct model IDs known from this command's value.
    ///
    /// This declaration does not inspect the current graph or command history, so it does not
    /// expand indirect effects for operations such as node removal, disconnection, undo, or redo.
    /// The runtime execution path supplements it with the complete affected and conflict
    /// footprint derived from the current state.
    pub fn declared_footprint(&self) -> GraphMutationFootprint {
        let mut footprint = GraphMutationFootprint::new();
        match &self.action {
            GraphCommandAction::Undo | GraphCommandAction::Redo => {}
            GraphCommandAction::SetNodeData { node, .. } => footprint.touch_node(*node),
            GraphCommandAction::SetNodeGeometry(edits) => {
                for edit in edits {
                    footprint.touch_node(edit.node);
                }
            }
            GraphCommandAction::SetPortData { port, .. } => footprint.touch_port(*port),
            GraphCommandAction::Symbols(edit) => edit.append_declared_footprint(&mut footprint),
            GraphCommandAction::Topology(edit) => edit.append_declared_footprint(&mut footprint),
        }
        footprint
    }

    pub(crate) fn conflict_footprint(
        &self,
        graph: &Graph,
        index: &GraphIndex,
    ) -> GraphMutationFootprint {
        let mut footprint = self.declared_footprint();
        match &self.action {
            GraphCommandAction::Undo | GraphCommandAction::Redo => footprint.touch_graph(),
            GraphCommandAction::SetNodeData { .. }
            | GraphCommandAction::SetNodeGeometry(_)
            | GraphCommandAction::SetPortData { .. } => {}
            GraphCommandAction::Symbols(edit) => {
                edit.append_conflict_footprint(index, &mut footprint);
            }
            GraphCommandAction::Topology(edit) => {
                edit.append_conflict_footprint(graph, index, &mut footprint);
            }
        }
        footprint
    }

    pub(crate) fn execution_kind(&self) -> GraphCommandExecutionKind {
        match &self.action {
            GraphCommandAction::Undo => GraphCommandExecutionKind::Undo,
            GraphCommandAction::Redo => GraphCommandExecutionKind::Redo,
            GraphCommandAction::SetNodeData { .. }
            | GraphCommandAction::SetNodeGeometry(_)
            | GraphCommandAction::SetPortData { .. }
            | GraphCommandAction::Symbols(_)
            | GraphCommandAction::Topology(_) => GraphCommandExecutionKind::Mutation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphCommandExecutionKind {
    Mutation,
    Undo,
    Redo,
}

/// Policy used when a topology edit removes a node or port with incident edges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IncidentEdgePolicy {
    #[default]
    Reject,
    Cascade,
}

/// A rules-checked connection change compiled by the runtime into reversible graph operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEdit {
    Connect {
        edge: EdgeId,
        from: PortId,
        to: PortId,
        mode: NodeGraphConnectionMode,
    },
    Disconnect {
        edge: EdgeId,
    },
    Reconnect {
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        new_port: PortId,
        mode: NodeGraphConnectionMode,
    },
}

#[derive(Debug, Clone)]
pub struct ConnectionEvaluation {
    version: GraphVersion,
    decision: ConnectDecision,
    diagnostics: Vec<Diagnostic>,
    planning_work: GraphCommandPlanningWork,
}

impl ConnectionEvaluation {
    pub(crate) fn accepted(version: GraphVersion, planning_work: GraphCommandPlanningWork) -> Self {
        Self {
            version,
            decision: ConnectDecision::Accept,
            diagnostics: Vec::new(),
            planning_work,
        }
    }

    pub(crate) fn rejected(
        version: GraphVersion,
        diagnostics: Vec<Diagnostic>,
        planning_work: GraphCommandPlanningWork,
    ) -> Self {
        Self {
            version,
            decision: ConnectDecision::Reject,
            diagnostics,
            planning_work,
        }
    }

    pub fn revision(&self) -> GraphRevision {
        self.version.revision()
    }

    pub fn version(&self) -> GraphVersion {
        self.version
    }

    pub fn decision(&self) -> ConnectDecision {
        self.decision
    }

    pub fn is_accept(&self) -> bool {
        self.decision == ConnectDecision::Accept
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns the exact command-planning work used to reach this decision.
    #[must_use]
    pub const fn planning_work(&self) -> GraphCommandPlanningWork {
        self.planning_work
    }
}

impl ConnectionEdit {
    pub fn connect(edge: EdgeId, from: PortId, to: PortId, mode: NodeGraphConnectionMode) -> Self {
        Self::Connect {
            edge,
            from,
            to,
            mode,
        }
    }

    pub fn disconnect(edge: EdgeId) -> Self {
        Self::Disconnect { edge }
    }

    pub fn reconnect(
        edge: EdgeId,
        endpoint: EdgeEndpoint,
        new_port: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Self {
        Self::Reconnect {
            edge,
            endpoint,
            new_port,
            mode,
        }
    }

    fn append_declared_footprint(self, footprint: &mut GraphMutationFootprint) {
        match self {
            Self::Connect { edge, from, to, .. } => {
                footprint.touch_edge(edge);
                footprint.touch_port(from);
                footprint.touch_port(to);
            }
            Self::Disconnect { edge } => footprint.touch_edge(edge),
            Self::Reconnect { edge, new_port, .. } => {
                footprint.touch_edge(edge);
                footprint.touch_port(new_port);
            }
        }
    }
}

/// An ordered, atomic edit to graph-scoped symbols.
///
/// Updates replace the complete [`Symbol`] value while the runtime preserves stable [`SymbolId`]
/// identity and emits only fields that actually changed.
#[derive(Debug, Clone, Default)]
pub struct SymbolEdit {
    pub(super) operations: Vec<SymbolOperation>,
}

#[derive(Debug, Clone)]
pub(super) enum SymbolOperation {
    Add { id: SymbolId, symbol: Symbol },
    Update { id: SymbolId, symbol: Symbol },
    Remove { id: SymbolId },
}

impl SymbolEdit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(mut self, id: SymbolId, symbol: Symbol) -> Self {
        self.operations.push(SymbolOperation::Add { id, symbol });
        self
    }

    pub fn update(mut self, id: SymbolId, symbol: Symbol) -> Self {
        self.operations.push(SymbolOperation::Update { id, symbol });
        self
    }

    pub fn remove(mut self, id: SymbolId) -> Self {
        self.operations.push(SymbolOperation::Remove { id });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    fn append_declared_footprint(&self, footprint: &mut GraphMutationFootprint) {
        for operation in &self.operations {
            let id = match operation {
                SymbolOperation::Add { id, .. }
                | SymbolOperation::Update { id, .. }
                | SymbolOperation::Remove { id } => *id,
            };
            footprint.touch_symbol(id);
        }
    }

    fn append_conflict_footprint(
        &self,
        index: &GraphIndex,
        footprint: &mut GraphMutationFootprint,
    ) {
        for operation in &self.operations {
            if let SymbolOperation::Remove { id } = operation {
                for node in index.symbol_referrers(*id) {
                    footprint.touch_node(node);
                }
            }
        }
    }
}

/// One port definition in the final ordered topology of a node.
#[derive(Debug, Clone)]
pub struct TopologyPort {
    pub(super) id: PortId,
    pub(super) port: Port,
}

impl TopologyPort {
    pub fn new(id: PortId, port: Port) -> Self {
        Self { id, port }
    }

    pub fn id(&self) -> PortId {
        self.id
    }

    pub fn port(&self) -> &Port {
        &self.port
    }
}

/// An ordered, atomic topology change.
#[derive(Debug, Clone, Default)]
pub struct TopologyEdit {
    pub(super) operations: Vec<TopologyOperation>,
}

#[derive(Debug, Clone)]
pub(super) enum TopologyOperation {
    SetNodeData {
        node: NodeId,
        data: serde_json::Value,
    },
    AddNode {
        id: NodeId,
        node: Node,
        ports: Vec<TopologyPort>,
    },
    RemoveNode {
        id: NodeId,
        incident_edges: IncidentEdgePolicy,
    },
    ReplaceNodePorts {
        node: NodeId,
        ports: Vec<TopologyPort>,
        incident_edges: IncidentEdgePolicy,
    },
    Connection(ConnectionEdit),
}

impl TopologyEdit {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates durable node payload in the same atomic unit as topology changes.
    pub fn set_node_data(mut self, node: NodeId, data: serde_json::Value) -> Self {
        self.operations
            .push(TopologyOperation::SetNodeData { node, data });
        self
    }

    /// Adds a node and its complete ordered port set.
    ///
    /// `node.ports` is derived from `ports`; any value supplied in the node snapshot is replaced.
    /// Registered kinds must use the runtime catalog's canonical key and latest version. Unknown
    /// kinds remain opaque and are preserved exactly.
    pub fn add_node(
        mut self,
        id: NodeId,
        node: Node,
        ports: impl IntoIterator<Item = TopologyPort>,
    ) -> Self {
        self.operations.push(TopologyOperation::AddNode {
            id,
            node,
            ports: ports.into_iter().collect(),
        });
        self
    }

    pub fn remove_node(mut self, id: NodeId, incident_edges: IncidentEdgePolicy) -> Self {
        self.operations
            .push(TopologyOperation::RemoveNode { id, incident_edges });
        self
    }

    /// Replaces one node's complete ordered port set while retaining facts for stable port IDs.
    pub fn replace_node_ports(
        mut self,
        node: NodeId,
        ports: impl IntoIterator<Item = TopologyPort>,
        incident_edges: IncidentEdgePolicy,
    ) -> Self {
        self.operations.push(TopologyOperation::ReplaceNodePorts {
            node,
            ports: ports.into_iter().collect(),
            incident_edges,
        });
        self
    }

    pub fn connection(mut self, edit: ConnectionEdit) -> Self {
        self.operations.push(TopologyOperation::Connection(edit));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    fn append_declared_footprint(&self, footprint: &mut GraphMutationFootprint) {
        for operation in &self.operations {
            match operation {
                TopologyOperation::SetNodeData { node, .. } => footprint.touch_node(*node),
                TopologyOperation::AddNode { id, ports, .. } => {
                    footprint.touch_node(*id);
                    for port in ports {
                        footprint.touch_port(port.id);
                    }
                }
                TopologyOperation::RemoveNode { id, .. } => footprint.touch_node(*id),
                TopologyOperation::ReplaceNodePorts { node, ports, .. } => {
                    footprint.touch_node(*node);
                    for port in ports {
                        footprint.touch_port(port.id);
                    }
                }
                TopologyOperation::Connection(edit) => {
                    edit.append_declared_footprint(footprint);
                }
            }
        }
    }

    fn append_conflict_footprint(
        &self,
        graph: &Graph,
        index: &GraphIndex,
        footprint: &mut GraphMutationFootprint,
    ) {
        for operation in &self.operations {
            match operation {
                TopologyOperation::SetNodeData { .. } => {}
                TopologyOperation::AddNode { id, node, ports } => {
                    append_node_dependencies(graph, index, *id, footprint);
                    footprint.touch_node_snapshot(*id, node);
                    for port in ports {
                        append_port_dependencies(graph, index, port.id, true, footprint);
                        footprint.touch_port_snapshot(port.id, &port.port);
                    }
                }
                TopologyOperation::RemoveNode { id, .. }
                | TopologyOperation::ReplaceNodePorts { node: id, .. } => {
                    append_node_dependencies(graph, index, *id, footprint);
                    if let TopologyOperation::ReplaceNodePorts { ports, .. } = operation {
                        for port in ports {
                            append_port_dependencies(graph, index, port.id, true, footprint);
                            footprint.touch_port_snapshot(port.id, &port.port);
                        }
                    }
                }
                TopologyOperation::Connection(edit) => {
                    append_connection_dependencies(graph, index, *edit, footprint);
                }
            }
        }
    }
}

fn append_connection_dependencies(
    graph: &Graph,
    index: &GraphIndex,
    edit: ConnectionEdit,
    footprint: &mut GraphMutationFootprint,
) {
    match edit {
        ConnectionEdit::Connect { edge, from, to, .. } => {
            append_edge_dependencies(graph, index, edge, footprint);
            append_port_dependencies(graph, index, from, true, footprint);
            append_port_dependencies(graph, index, to, true, footprint);
        }
        ConnectionEdit::Disconnect { edge } => {
            append_edge_dependencies(graph, index, edge, footprint);
        }
        ConnectionEdit::Reconnect { edge, new_port, .. } => {
            append_edge_dependencies(graph, index, edge, footprint);
            append_port_dependencies(graph, index, new_port, true, footprint);
        }
    }
}

fn append_node_dependencies(
    graph: &Graph,
    index: &GraphIndex,
    node_id: NodeId,
    footprint: &mut GraphMutationFootprint,
) {
    footprint.touch_node(node_id);
    let Some(node) = graph.node(&node_id) else {
        return;
    };
    footprint.touch_node_snapshot(node_id, node);
    append_binding_dependencies(
        graph,
        index,
        [GraphLocalBindingTarget::node(node_id)],
        footprint,
    );
    for port in &node.ports {
        append_port_dependencies(graph, index, *port, true, footprint);
    }
}

fn append_port_dependencies(
    graph: &Graph,
    index: &GraphIndex,
    port_id: PortId,
    include_incident_edges: bool,
    footprint: &mut GraphMutationFootprint,
) {
    footprint.touch_port(port_id);
    if let Some(port) = graph.port(&port_id) {
        footprint.touch_port_snapshot(port_id, port);
    }
    append_binding_dependencies(
        graph,
        index,
        [GraphLocalBindingTarget::port(port_id)],
        footprint,
    );
    if include_incident_edges {
        for edge in index.incident_edges(port_id) {
            append_edge_dependencies(graph, index, edge, footprint);
        }
    }
}

fn append_edge_dependencies(
    graph: &Graph,
    index: &GraphIndex,
    edge_id: EdgeId,
    footprint: &mut GraphMutationFootprint,
) {
    footprint.touch_edge(edge_id);
    if let Some(edge) = graph.edge(&edge_id) {
        footprint.touch_edge_snapshot(edge_id, edge);
        append_port_dependencies(graph, index, edge.from, false, footprint);
        append_port_dependencies(graph, index, edge.to, false, footprint);
    }
    append_binding_dependencies(
        graph,
        index,
        [GraphLocalBindingTarget::edge(edge_id)],
        footprint,
    );
}

fn append_binding_dependencies(
    graph: &Graph,
    index: &GraphIndex,
    targets: impl IntoIterator<Item = GraphLocalBindingTarget>,
    footprint: &mut GraphMutationFootprint,
) {
    let bindings = targets
        .into_iter()
        .flat_map(|target| index.bindings_for_target(target))
        .collect::<BTreeSet<_>>();
    for binding_id in bindings {
        if let Some(binding) = graph.binding(&binding_id) {
            footprint.touch_binding_snapshot(binding_id, binding);
        } else {
            footprint.touch_binding(binding_id);
        }
    }
}

/// Operation-local work performed while planning one graph command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphCommandPlanningWork {
    fact_reads: u64,
    index_bucket_reads: u64,
    index_entries_read: u64,
    operations_emitted: u64,
    full_type_revalidations: u64,
}

impl GraphCommandPlanningWork {
    pub(crate) const fn new(
        fact_reads: u64,
        index_bucket_reads: u64,
        index_entries_read: u64,
        operations_emitted: u64,
        full_type_revalidations: u64,
    ) -> Self {
        Self {
            fact_reads,
            index_bucket_reads,
            index_entries_read,
            operations_emitted,
            full_type_revalidations,
        }
    }

    pub(crate) const fn empty() -> Self {
        Self::new(0, 0, 0, 0, 0)
    }

    /// Returns the number of authoritative graph facts read while planning.
    #[must_use]
    pub const fn fact_reads(self) -> u64 {
        self.fact_reads
    }

    /// Returns the number of graph-index buckets opened while planning.
    #[must_use]
    pub const fn index_bucket_reads(self) -> u64 {
        self.index_bucket_reads
    }

    /// Returns the number of entries examined inside graph-index buckets.
    #[must_use]
    pub const fn index_entries_read(self) -> u64 {
        self.index_entries_read
    }

    /// Returns the number of transaction operations emitted by the planner.
    #[must_use]
    pub const fn operations_emitted(self) -> u64 {
        self.operations_emitted
    }

    /// Returns the number of whole-graph type revalidations requested by planning.
    #[must_use]
    pub const fn full_type_revalidations(self) -> u64 {
        self.full_type_revalidations
    }
}

#[derive(Debug, Clone)]
pub struct GraphCommandReceipt {
    identity: GraphCommandIdentity,
    version: GraphVersion,
    patch: RuntimePatch,
    footprint: GraphMutationFootprint,
    planning_work: GraphCommandPlanningWork,
}

impl GraphCommandReceipt {
    pub(crate) fn new(
        identity: GraphCommandIdentity,
        version: GraphVersion,
        patch: RuntimePatch,
        footprint: GraphMutationFootprint,
        planning_work: GraphCommandPlanningWork,
    ) -> Self {
        Self {
            identity,
            version,
            patch,
            footprint,
            planning_work,
        }
    }

    pub fn version(&self) -> GraphVersion {
        self.version
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn revision(&self) -> GraphRevision {
        self.patch.to_revision()
    }

    pub fn patch(&self) -> &RuntimePatch {
        &self.patch
    }

    pub fn footprint(&self) -> &GraphMutationFootprint {
        &self.footprint
    }

    /// Returns work performed while planning this applied command.
    #[must_use]
    pub const fn planning_work(&self) -> GraphCommandPlanningWork {
        self.planning_work
    }
}

#[derive(Debug, Clone)]
pub struct GraphCommandNoop {
    identity: GraphCommandIdentity,
    snapshot: GraphSnapshot,
    footprint: GraphMutationFootprint,
    planning_work: GraphCommandPlanningWork,
}

impl GraphCommandNoop {
    pub(crate) fn new(
        identity: GraphCommandIdentity,
        snapshot: GraphSnapshot,
        footprint: GraphMutationFootprint,
        planning_work: GraphCommandPlanningWork,
    ) -> Self {
        Self {
            identity,
            snapshot,
            footprint,
            planning_work,
        }
    }

    pub fn revision(&self) -> GraphRevision {
        self.snapshot.revision()
    }

    pub fn version(&self) -> GraphVersion {
        self.snapshot.version()
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn footprint(&self) -> &GraphMutationFootprint {
        &self.footprint
    }

    /// Returns the authoritative graph state that proves this command was a semantic no-op.
    pub fn snapshot(&self) -> &GraphSnapshot {
        &self.snapshot
    }

    /// Returns work performed while proving this command was a semantic no-op.
    #[must_use]
    pub const fn planning_work(&self) -> GraphCommandPlanningWork {
        self.planning_work
    }
}

#[derive(Debug, Clone)]
pub struct GraphCommandConflict {
    identity: GraphCommandIdentity,
    expected_version: GraphVersion,
    current: GraphSnapshot,
    footprint: GraphMutationFootprint,
}

impl GraphCommandConflict {
    pub(crate) fn new(
        identity: GraphCommandIdentity,
        expected_version: GraphVersion,
        current: GraphSnapshot,
        footprint: GraphMutationFootprint,
    ) -> Self {
        Self {
            identity,
            expected_version,
            current,
            footprint,
        }
    }

    pub fn expected_revision(&self) -> GraphRevision {
        self.expected_version.revision()
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn expected_version(&self) -> GraphVersion {
        self.expected_version
    }

    pub fn current_version(&self) -> GraphVersion {
        self.current.version()
    }

    pub fn current(&self) -> &GraphSnapshot {
        &self.current
    }

    pub fn footprint(&self) -> &GraphMutationFootprint {
        &self.footprint
    }
}

#[derive(Debug, Clone)]
pub struct GraphCommandRejection {
    identity: GraphCommandIdentity,
    version: GraphVersion,
    message: String,
    diagnostics: Vec<Diagnostic>,
    planning_work: GraphCommandPlanningWork,
}

impl GraphCommandRejection {
    pub(crate) fn new(
        identity: GraphCommandIdentity,
        version: GraphVersion,
        message: impl Into<String>,
        diagnostics: Vec<Diagnostic>,
        planning_work: GraphCommandPlanningWork,
    ) -> Self {
        Self {
            identity,
            version,
            message: message.into(),
            diagnostics,
            planning_work,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn identity(&self) -> &GraphCommandIdentity {
        &self.identity
    }

    pub fn version(&self) -> GraphVersion {
        self.version
    }

    pub fn revision(&self) -> GraphRevision {
        self.version.revision()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns command-planning work completed before this command was rejected.
    #[must_use]
    pub const fn planning_work(&self) -> GraphCommandPlanningWork {
        self.planning_work
    }
}

#[derive(Debug, Clone)]
pub enum GraphCommandOutcome {
    Applied(GraphCommandReceipt),
    Noop(GraphCommandNoop),
    Conflict(GraphCommandConflict),
    Rejected(GraphCommandRejection),
}

impl GraphCommandOutcome {
    pub fn identity(&self) -> &GraphCommandIdentity {
        match self {
            Self::Applied(receipt) => receipt.identity(),
            Self::Noop(noop) => noop.identity(),
            Self::Conflict(conflict) => conflict.identity(),
            Self::Rejected(rejection) => rejection.identity(),
        }
    }

    /// Returns the authoritative runtime version observed after this command completed.
    pub fn version(&self) -> GraphVersion {
        match self {
            Self::Applied(receipt) => receipt.version(),
            Self::Noop(noop) => noop.version(),
            Self::Conflict(conflict) => conflict.current_version(),
            Self::Rejected(rejection) => rejection.version(),
        }
    }

    pub fn applied(&self) -> Option<&GraphCommandReceipt> {
        match self {
            Self::Applied(receipt) => Some(receipt),
            Self::Noop(_) | Self::Conflict(_) | Self::Rejected(_) => None,
        }
    }

    pub fn conflict(&self) -> Option<&GraphCommandConflict> {
        match self {
            Self::Conflict(conflict) => Some(conflict),
            Self::Applied(_) | Self::Noop(_) | Self::Rejected(_) => None,
        }
    }

    pub fn rejection(&self) -> Option<&GraphCommandRejection> {
        match self {
            Self::Rejected(rejection) => Some(rejection),
            Self::Applied(_) | Self::Noop(_) | Self::Conflict(_) => None,
        }
    }
}
