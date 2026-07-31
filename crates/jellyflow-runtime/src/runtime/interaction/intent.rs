use std::collections::BTreeSet;
use std::sync::Arc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, GroupId, NodeExtent, NodeId, PortId,
};

use crate::runtime::adapter_protocol::PublishedEdgeRoute;
use crate::runtime::commands::{
    ConnectionEdit, GraphCommand, IncidentEdgePolicy, NodeGeometryEdit, TopologyEdit,
};
use crate::runtime::graph_runtime::{
    EdgeInstanceKey, GraphRevision, GraphRuntimePolicy, GraphSnapshot, GraphVersion,
    NodeLifetimeKey, PortInstanceKey, RuntimeProvenance,
};
use crate::runtime::policy::{resolve_edge_policy, resolve_node_policy};
use crate::runtime::view_session::ViewProvenance;
use crate::schema::NodeCreation;

use super::ConnectionGestureBinding;

/// The immutable selection observed when a selection gesture starts.
///
/// Marquee updates carry this baseline so every frame describes the selection relative to the
/// same authoritative starting point rather than incrementally mutating the previous frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionBaseline {
    provenance: ViewProvenance,
    graph_revision: GraphRevision,
    nodes: Arc<[NodeLifetimeKey]>,
    edges: Arc<[EdgeInstanceKey]>,
    groups: Arc<[GroupId]>,
}

impl SelectionBaseline {
    pub(crate) fn new(
        provenance: ViewProvenance,
        graph_revision: GraphRevision,
        nodes: impl IntoIterator<Item = NodeLifetimeKey>,
        edges: impl IntoIterator<Item = EdgeInstanceKey>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self {
            provenance,
            graph_revision,
            nodes: nodes.into_iter().collect::<Vec<_>>().into(),
            edges: edges.into_iter().collect::<Vec<_>>().into(),
            groups: groups.into_iter().collect::<Vec<_>>().into(),
        }
    }

    pub fn provenance(&self) -> ViewProvenance {
        self.provenance
    }

    pub fn graph_revision(&self) -> GraphRevision {
        self.graph_revision
    }

    pub fn nodes(&self) -> &[NodeLifetimeKey] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeInstanceKey] {
        &self.edges
    }

    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }

    #[cfg(test)]
    pub(crate) fn from_ids_for_test(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::new(
            ViewProvenance::unavailable(),
            0,
            nodes.into_iter().map(NodeLifetimeKey::unavailable),
            edges.into_iter().map(EdgeInstanceKey::unavailable),
            groups,
        )
    }
}

#[cfg(test)]
impl Default for SelectionBaseline {
    fn default() -> Self {
        Self::from_ids_for_test([], [], [])
    }
}

/// Closed connection target authority retained through freshness and settlement.
///
/// Keeping the target and its optional topology in one enum makes impossible combinations
/// unrepresentable: existing ports never carry topology and create-from-wire targets always carry
/// the complete, narrowly validated node creation.
#[derive(Debug, Clone)]
pub(super) enum ConnectionCommitTarget {
    Existing(PortInstanceKey),
    CreateFromWire {
        target: PortId,
        node: NodeId,
        ports: Arc<[PortId]>,
        edit: Arc<TopologyEdit>,
    },
}

impl ConnectionCommitTarget {
    pub(super) const fn existing(instance: PortInstanceKey) -> Self {
        Self::Existing(instance)
    }

    pub(super) fn from_node_creation(
        snapshot: &GraphSnapshot,
        target: PortId,
        creation: NodeCreation,
    ) -> Option<Self> {
        let node = creation.node_id();
        let ports = creation
            .ports()
            .iter()
            .map(|(port, model)| (model.node == node).then_some(*port))
            .collect::<Option<Vec<_>>>()?;
        if ports.iter().copied().collect::<BTreeSet<_>>().len() != ports.len() {
            return None;
        }
        if !ports.contains(&target)
            || snapshot.graph().node(&node).is_some()
            || ports
                .iter()
                .any(|port| snapshot.graph().port(port).is_some())
        {
            return None;
        }
        Some(Self::CreateFromWire {
            target,
            node,
            ports: ports.into(),
            edit: Arc::new(creation.into_topology_edit()),
        })
    }

    pub(super) const fn is_existing(&self) -> bool {
        matches!(self, Self::Existing(_))
    }

    pub(super) const fn is_create_from_wire(&self) -> bool {
        matches!(self, Self::CreateFromWire { .. })
    }

    pub(super) const fn port(&self) -> PortId {
        match self {
            Self::Existing(instance) => instance.port(),
            Self::CreateFromWire { target, .. } => *target,
        }
    }

    pub(super) fn is_current(&self, snapshot: &GraphSnapshot) -> bool {
        match self {
            Self::Existing(instance) => snapshot.port_instance_is_current(*instance),
            Self::CreateFromWire {
                target,
                node,
                ports,
                ..
            } => {
                ports.contains(target)
                    && snapshot.graph().node(node).is_none()
                    && ports
                        .iter()
                        .all(|port| snapshot.graph().port(port).is_none())
            }
        }
    }

    pub(super) fn installed_target(
        &self,
        snapshot: &GraphSnapshot,
        expected_revision: GraphRevision,
    ) -> Option<PortInstanceKey> {
        match self {
            Self::Existing(instance) => snapshot
                .port_instance_is_current(*instance)
                .then_some(*instance),
            Self::CreateFromWire {
                target,
                node,
                ports,
                ..
            } => {
                let owner = snapshot.node_lifetime_key(*node)?;
                if !snapshot.node_lifetime_was_introduced_at(owner, expected_revision) {
                    return None;
                }
                let mut installed_target = None;
                for port in ports.iter().copied() {
                    let instance = snapshot.port_instance_key(port)?;
                    if instance.owner() != owner
                        || !snapshot.port_instance_was_introduced_at(instance, expected_revision)
                    {
                        return None;
                    }
                    if port == *target {
                        installed_target = Some(instance);
                    }
                }
                installed_target
            }
        }
    }

    fn into_edit(self) -> TopologyEdit {
        match self {
            Self::Existing(_) => TopologyEdit::new(),
            Self::CreateFromWire { edit, .. } => {
                Arc::try_unwrap(edit).unwrap_or_else(|shared| shared.as_ref().clone())
            }
        }
    }
}

/// A session-owned graph mutation that is sealed inside an opaque submission before it leaves the
/// interaction runtime.
#[derive(Debug)]
pub(super) struct FreshGraphMutation(FreshGraphMutationAuthority);

#[derive(Debug)]
enum FreshGraphMutationAuthority {
    Connection {
        binding: ConnectionGestureBinding,
        target: ConnectionCommitTarget,
        validated_version: GraphVersion,
    },
    MoveNodes(RelativeNodeMoveIntent),
    DeleteEdge(EdgeDeletionIntent),
    DeleteSelection(SelectionDeletionIntent),
}

impl FreshGraphMutation {
    pub(super) fn connection(
        binding: ConnectionGestureBinding,
        target: ConnectionCommitTarget,
        validated_version: GraphVersion,
    ) -> Self {
        Self(FreshGraphMutationAuthority::Connection {
            binding,
            target,
            validated_version,
        })
    }

    pub(super) fn move_nodes(intent: RelativeNodeMoveIntent) -> Self {
        Self(FreshGraphMutationAuthority::MoveNodes(intent))
    }

    pub(super) fn delete_edge(intent: EdgeDeletionIntent) -> Self {
        Self(FreshGraphMutationAuthority::DeleteEdge(intent))
    }

    pub(super) fn delete_selection(intent: SelectionDeletionIntent) -> Self {
        Self(FreshGraphMutationAuthority::DeleteSelection(intent))
    }

    pub(super) const fn requires_connection_revalidation(&self) -> bool {
        matches!(self.0, FreshGraphMutationAuthority::Connection { .. })
    }

    #[cfg(test)]
    pub(super) fn connection_facts_for_test(
        &self,
    ) -> Option<(ConnectionGestureBinding, PortId, GraphVersion, bool)> {
        match &self.0 {
            FreshGraphMutationAuthority::Connection {
                binding,
                target,
                validated_version,
            } => Some((
                binding.clone(),
                target.port(),
                *validated_version,
                target.is_create_from_wire(),
            )),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn move_nodes_for_test(&self) -> Option<&RelativeNodeMoveIntent> {
        match &self.0 {
            FreshGraphMutationAuthority::MoveNodes(intent) => Some(intent),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn delete_edge_for_test(&self) -> Option<EdgeDeletionIntent> {
        match &self.0 {
            FreshGraphMutationAuthority::DeleteEdge(intent) => Some(*intent),
            _ => None,
        }
    }

    /// Consumes this intent and binds it once to the supplied authoritative snapshot.
    pub(super) fn into_command(
        self,
        snapshot: &GraphSnapshot,
    ) -> Result<Option<GraphCommand>, FreshGraphMutationBindError> {
        match self.0 {
            FreshGraphMutationAuthority::Connection {
                binding,
                target,
                validated_version,
            } => {
                if binding.provenance() != snapshot.provenance() {
                    return Err(FreshGraphMutationBindError::ConnectionForeignRuntime {
                        edge: binding.edge_id(),
                    });
                }
                if !snapshot.node_lifetime_is_current(binding.owner()) {
                    return Err(FreshGraphMutationBindError::ConnectionBindingRetired {
                        edge: binding.edge_id(),
                    });
                }
                if binding
                    .reconnect_edge()
                    .is_some_and(|edge| !snapshot.edge_instance_is_current(edge))
                {
                    return Err(FreshGraphMutationBindError::ReconnectEdgeRetired {
                        edge: binding.edge_id(),
                    });
                }
                for port in [
                    Some(binding.fixed_port_instance()),
                    binding.detached_port_instance(),
                ]
                .into_iter()
                .flatten()
                {
                    if !snapshot.port_instance_is_current(port) {
                        return Err(FreshGraphMutationBindError::ConnectionPortRetired {
                            port: port.port(),
                        });
                    }
                }
                if !binding.is_current(snapshot) {
                    return Err(FreshGraphMutationBindError::ConnectionBindingRetired {
                        edge: binding.edge_id(),
                    });
                }
                match &target {
                    ConnectionCommitTarget::Existing(instance) => {
                        if instance.provenance() != snapshot.provenance() {
                            return Err(FreshGraphMutationBindError::ConnectionForeignRuntime {
                                edge: binding.edge_id(),
                            });
                        }
                        if !snapshot.port_instance_is_current(*instance) {
                            return Err(FreshGraphMutationBindError::ConnectionTargetRetired {
                                port: instance.port(),
                            });
                        }
                    }
                    ConnectionCommitTarget::CreateFromWire { target: port, .. } => {
                        if !target.is_current(snapshot) {
                            return Err(
                                FreshGraphMutationBindError::ConnectionTargetNoLongerProspective {
                                    port: *port,
                                },
                            );
                        }
                    }
                }
                if snapshot.version() != validated_version {
                    return Err(FreshGraphMutationBindError::RevisionChanged {
                        validated: validated_version,
                        current: snapshot.version(),
                    });
                }
                Ok(Some(GraphCommand::topology(
                    snapshot.version(),
                    target
                        .clone()
                        .into_edit()
                        .connection(binding.edit_for_target(target.port())),
                )))
            }
            FreshGraphMutationAuthority::MoveNodes(intent) => intent.into_command(snapshot),
            FreshGraphMutationAuthority::DeleteEdge(intent) => intent.into_command(snapshot),
            FreshGraphMutationAuthority::DeleteSelection(intent) => intent.into_command(snapshot),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FreshGraphMutationBindError {
    #[error(
        "interaction mutation was validated at {validated:?}, but the fresh snapshot is {current:?}"
    )]
    RevisionChanged {
        validated: GraphVersion,
        current: GraphVersion,
    },
    #[error("connection mutation for edge {edge:?} belongs to another runtime")]
    ConnectionForeignRuntime { edge: EdgeId },
    #[error("connection binding for edge {edge:?} was retired or replaced")]
    ConnectionBindingRetired { edge: EdgeId },
    #[error("connection port {port:?} was retired or replaced")]
    ConnectionPortRetired { port: PortId },
    #[error("connection target {port:?} was retired or replaced")]
    ConnectionTargetRetired { port: PortId },
    #[error("create-from-wire target {port:?} is no longer prospective")]
    ConnectionTargetNoLongerProspective { port: PortId },
    #[error("reconnect edge {edge:?} was retired or replaced before command binding")]
    ReconnectEdgeRetired { edge: EdgeId },
    #[error("edge deletion facts for {edge:?} belong to another runtime")]
    EdgeDeleteForeignRuntime { edge: EdgeId },
    #[error(
        "edge {edge:?} was replaced before deletion binding (expected {expected:?}, current {current:?})"
    )]
    EdgeDeleteReplaced {
        edge: EdgeId,
        expected: EdgeInstanceKey,
        current: EdgeInstanceKey,
    },
    #[error("node drag member {node:?} was retired before command binding")]
    NodeDragMemberUnavailable { node: NodeId },
    #[error("node drag member {node:?} belongs to another runtime")]
    NodeDragMemberForeignRuntime { node: NodeId },
    #[error(
        "node drag member {node:?} was replaced before command binding (expected {expected:?}, current {current:?})"
    )]
    NodeDragMemberReplaced {
        node: NodeId,
        expected: NodeLifetimeKey,
        current: NodeLifetimeKey,
    },
    #[error("node drag member {node:?} is no longer draggable")]
    NodeDragMemberNotDraggable { node: NodeId },
    #[error("node drag member {node:?} would move to a non-finite position")]
    NodeDragMemberPositionOverflow { node: NodeId },
    #[error("node move member {node:?} has a parent extent without a live parent group")]
    NodeMoveExtentUnavailable { node: NodeId },
    #[error("selected node {node:?} was retired before deletion binding")]
    SelectionNodeUnavailable { node: NodeId },
    #[error("selection deletion facts belong to another runtime")]
    SelectionForeignRuntime,
    #[error(
        "selected node {node:?} was replaced before deletion binding (expected {expected:?}, current {current:?})"
    )]
    SelectionNodeReplaced {
        node: NodeId,
        expected: NodeLifetimeKey,
        current: NodeLifetimeKey,
    },
    #[error("selected edge {edge:?} was retired before deletion binding")]
    SelectionEdgeUnavailable { edge: EdgeId },
    #[error(
        "selected edge {edge:?} was replaced before deletion binding (expected {expected:?}, current {current:?})"
    )]
    SelectionEdgeReplaced {
        edge: EdgeId,
        expected: EdgeInstanceKey,
        current: EdgeInstanceKey,
    },
}

/// An edge deletion admitted from one authoritative published route.
///
/// The constructor is intentionally interaction-internal. Native adapters submit the published
/// route to `CanvasInteractionSession` instead of manufacturing deletion commands or policy facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EdgeDeletionIntent {
    instance: EdgeInstanceKey,
    validated_version: GraphVersion,
}

impl EdgeDeletionIntent {
    pub(super) fn from_route(
        route: &PublishedEdgeRoute,
        validated_version: GraphVersion,
    ) -> Option<Self> {
        route.facts().interaction.deletable.then_some(Self {
            instance: route.instance(),
            validated_version,
        })
    }

    pub(super) fn edge(self) -> EdgeId {
        self.instance.edge()
    }

    fn into_command(
        self,
        snapshot: &GraphSnapshot,
    ) -> Result<Option<GraphCommand>, FreshGraphMutationBindError> {
        let edge = self.instance.edge();
        if self.instance.provenance() != snapshot.provenance() {
            return Err(FreshGraphMutationBindError::EdgeDeleteForeignRuntime { edge });
        }
        match snapshot.edge_instance_key(edge) {
            None => return Ok(None),
            Some(current) if current != self.instance => {
                return Err(FreshGraphMutationBindError::EdgeDeleteReplaced {
                    edge,
                    expected: self.instance,
                    current,
                });
            }
            Some(_) => {}
        }
        if snapshot.version() != self.validated_version {
            return Err(FreshGraphMutationBindError::RevisionChanged {
                validated: self.validated_version,
                current: snapshot.version(),
            });
        }
        Ok(Some(
            GraphCommand::disconnect(snapshot.version(), edge).with_label("Delete edge"),
        ))
    }
}

/// One policy-filtered, revision-bound deletion of the current view selection.
///
/// Groups are intentionally absent: selection deletion currently owns graph nodes and edges only.
/// Explicit selected edges incident to an admitted node are omitted because node removal cascades
/// them in the same topology edit.
#[derive(Debug, Clone)]
pub(super) struct SelectionDeletionIntent {
    provenance: RuntimeProvenance,
    validated_version: GraphVersion,
    nodes: Arc<[NodeLifetimeKey]>,
    edges: Arc<[EdgeInstanceKey]>,
}

impl SelectionDeletionIntent {
    pub(super) fn from_selection(
        snapshot: &GraphSnapshot,
        policy: GraphRuntimePolicy,
        selected_nodes: &[NodeLifetimeKey],
        selected_edges: &[EdgeInstanceKey],
    ) -> Option<Self> {
        let graph = snapshot.graph();
        let mut nodes = selected_nodes
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|node| snapshot.node_lifetime_is_current(*node))
            .filter_map(|node| {
                let node_id = node.node();
                let record = graph.node(&node_id)?;
                if !resolve_node_policy(record, &policy).can_delete() {
                    return None;
                }
                Some(node)
            })
            .collect::<Vec<_>>();
        nodes.sort_unstable();
        let deleted_nodes = nodes
            .iter()
            .map(|member| member.node())
            .collect::<BTreeSet<_>>();

        let mut edges = selected_edges
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|edge| snapshot.edge_instance_is_current(*edge))
            .filter_map(|edge| {
                let edge_id = edge.edge();
                let record = graph.edge(&edge_id)?;
                if !resolve_edge_policy(record, &policy).can_delete() {
                    return None;
                }
                let incident_to_deleted_node = [record.from, record.to].into_iter().any(|port| {
                    graph
                        .port(&port)
                        .is_some_and(|port| deleted_nodes.contains(&port.node))
                });
                if incident_to_deleted_node {
                    return None;
                }
                Some(edge)
            })
            .collect::<Vec<_>>();
        edges.sort_unstable();

        (!nodes.is_empty() || !edges.is_empty()).then(|| Self {
            provenance: snapshot.provenance(),
            validated_version: snapshot.version(),
            nodes: nodes.into(),
            edges: edges.into(),
        })
    }

    pub(super) fn edges(&self) -> impl ExactSizeIterator<Item = EdgeId> + '_ {
        self.edges.iter().map(|member| member.edge())
    }

    fn into_command(
        self,
        snapshot: &GraphSnapshot,
    ) -> Result<Option<GraphCommand>, FreshGraphMutationBindError> {
        if self.provenance != snapshot.provenance() {
            return Err(FreshGraphMutationBindError::SelectionForeignRuntime);
        }
        for member in self.nodes.iter() {
            if member.provenance() != snapshot.provenance() {
                return Err(FreshGraphMutationBindError::SelectionForeignRuntime);
            }
            match snapshot.node_lifetime_key(member.node()) {
                None => {
                    return Err(FreshGraphMutationBindError::SelectionNodeUnavailable {
                        node: member.node(),
                    });
                }
                Some(current) if current != *member => {
                    return Err(FreshGraphMutationBindError::SelectionNodeReplaced {
                        node: member.node(),
                        expected: *member,
                        current,
                    });
                }
                Some(_) => {}
            }
        }
        for member in self.edges.iter() {
            if member.provenance() != snapshot.provenance() {
                return Err(FreshGraphMutationBindError::SelectionForeignRuntime);
            }
            match snapshot.edge_instance_key(member.edge()) {
                None => {
                    return Err(FreshGraphMutationBindError::SelectionEdgeUnavailable {
                        edge: member.edge(),
                    });
                }
                Some(current) if current != *member => {
                    return Err(FreshGraphMutationBindError::SelectionEdgeReplaced {
                        edge: member.edge(),
                        expected: *member,
                        current,
                    });
                }
                Some(_) => {}
            }
        }
        if snapshot.version() != self.validated_version {
            return Err(FreshGraphMutationBindError::RevisionChanged {
                validated: self.validated_version,
                current: snapshot.version(),
            });
        }

        let mut topology = TopologyEdit::new();
        for member in self.edges.iter() {
            topology = topology.connection(ConnectionEdit::disconnect(member.edge()));
        }
        for member in self.nodes.iter() {
            topology = topology.remove_node(member.node(), IncidentEdgePolicy::Cascade);
        }
        Ok((!topology.is_empty()).then(|| {
            GraphCommand::topology(snapshot.version(), topology).with_label("Delete selection")
        }))
    }
}

/// Maximum number of nodes admitted into one atomic movement cohort.
///
/// This matches the default required-node materialization budget so every admitted member can be
/// retained by a conforming native adapter for the complete interaction lifetime.
pub const MAX_NODE_MOVE_COHORT_SIZE: usize =
    crate::runtime::adapter_protocol::DEFAULT_FRAME_REQUIRED_NODE_BUDGET;

#[derive(Debug, Clone)]
pub(super) struct NodeMoveCohort {
    members: Arc<[NodeLifetimeKey]>,
    nodes: Arc<[NodeId]>,
}

impl NodeMoveCohort {
    pub(super) fn new(members: impl IntoIterator<Item = NodeLifetimeKey>) -> Option<Self> {
        let mut members = members.into_iter().collect::<Vec<_>>();
        members.sort_unstable();
        if members
            .windows(2)
            .any(|pair| pair[0].node() == pair[1].node() && pair[0] != pair[1])
        {
            return None;
        }
        members.dedup();
        if members.is_empty() || members.len() > MAX_NODE_MOVE_COHORT_SIZE {
            return None;
        }
        let nodes = members
            .iter()
            .map(|member| member.node())
            .collect::<Vec<_>>();
        Some(Self {
            members: members.into(),
            nodes: nodes.into(),
        })
    }

    pub(super) fn members(&self) -> &[NodeLifetimeKey] {
        &self.members
    }

    pub(super) fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub(super) fn nodes_arc(&self) -> Arc<[NodeId]> {
        Arc::clone(&self.nodes)
    }

    pub(super) fn contains_member(&self, member: NodeLifetimeKey) -> bool {
        self.members.binary_search(&member).is_ok()
    }
}

/// Immutable movement limits for one admitted node cohort.
///
/// Pointer movement, alignment projection, and command binding all use this same constraint so an
/// optimistic node position cannot cross an extent that the authoritative command will enforce.
#[derive(Debug, Clone, Copy)]
pub(super) struct NodeMoveDeltaConstraint {
    representative: NodeId,
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl NodeMoveDeltaConstraint {
    pub(super) fn from_snapshot(
        snapshot: &GraphSnapshot,
        cohort: &NodeMoveCohort,
    ) -> Result<Self, FreshGraphMutationBindError> {
        let representative = cohort.nodes()[0];
        let mut constraint = Self {
            representative,
            min_x: f32::NEG_INFINITY,
            max_x: f32::INFINITY,
            min_y: f32::NEG_INFINITY,
            max_y: f32::INFINITY,
        };

        for member in cohort.members() {
            let node = member.node();
            if member.provenance() != snapshot.provenance() {
                return Err(FreshGraphMutationBindError::NodeDragMemberForeignRuntime { node });
            }
            let Some(current_lifetime) = snapshot.node_lifetime_key(node) else {
                return Err(FreshGraphMutationBindError::NodeDragMemberUnavailable { node });
            };
            if current_lifetime != *member {
                return Err(FreshGraphMutationBindError::NodeDragMemberReplaced {
                    node,
                    expected: *member,
                    current: current_lifetime,
                });
            }
            let current = snapshot
                .graph()
                .node(&node)
                .ok_or(FreshGraphMutationBindError::NodeDragMemberUnavailable { node })?;
            if !current.draggable.unwrap_or(true) {
                return Err(FreshGraphMutationBindError::NodeDragMemberNotDraggable { node });
            }
            let Some(extent) = movement_extent(snapshot, node, current.extent)? else {
                continue;
            };
            let Some((min_x, max_x, min_y, max_y)) =
                movement_delta_bounds(current.pos, current.origin, current.size, extent)
            else {
                return Err(FreshGraphMutationBindError::NodeDragMemberPositionOverflow { node });
            };
            constraint.min_x = constraint.min_x.max(min_x);
            constraint.max_x = constraint.max_x.min(max_x);
            constraint.min_y = constraint.min_y.max(min_y);
            constraint.max_y = constraint.max_y.min(max_y);
        }

        Ok(constraint)
    }

    #[cfg(test)]
    pub(super) fn unbounded_for_test(cohort: &NodeMoveCohort) -> Self {
        Self {
            representative: cohort.nodes()[0],
            min_x: f32::NEG_INFINITY,
            max_x: f32::INFINITY,
            min_y: f32::NEG_INFINITY,
            max_y: f32::INFINITY,
        }
    }

    pub(super) fn constrain(
        self,
        requested: CanvasPoint,
    ) -> Result<Option<CanvasPoint>, FreshGraphMutationBindError> {
        if !requested.is_finite() {
            return Err(
                FreshGraphMutationBindError::NodeDragMemberPositionOverflow {
                    node: self.representative,
                },
            );
        }
        let delta = CanvasPoint {
            x: clamp_shared_axis(requested.x, self.min_x, self.max_x),
            y: clamp_shared_axis(requested.y, self.min_y, self.max_y),
        };
        if !delta.is_finite() {
            return Err(
                FreshGraphMutationBindError::NodeDragMemberPositionOverflow {
                    node: self.representative,
                },
            );
        }
        Ok((delta.x != 0.0 || delta.y != 0.0).then_some(delta))
    }
}

/// Relative node movement captured independently from any stale press-time graph snapshot.
#[derive(Debug)]
pub(super) struct RelativeNodeMoveIntent {
    cohort: NodeMoveCohort,
    delta: CanvasPoint,
}

impl RelativeNodeMoveIntent {
    #[cfg(test)]
    fn new(members: impl IntoIterator<Item = NodeLifetimeKey>, delta: CanvasPoint) -> Option<Self> {
        if !delta.is_finite() || (delta.x == 0.0 && delta.y == 0.0) {
            return None;
        }
        Some(Self {
            cohort: NodeMoveCohort::new(members)?,
            delta,
        })
    }

    pub(super) fn from_cohort(cohort: NodeMoveCohort, delta: CanvasPoint) -> Option<Self> {
        if !delta.is_finite() || (delta.x == 0.0 && delta.y == 0.0) {
            return None;
        }
        Some(Self { cohort, delta })
    }

    #[cfg(test)]
    pub(super) fn nodes(&self) -> &[NodeId] {
        self.cohort.nodes()
    }

    #[cfg(test)]
    pub(super) fn delta(&self) -> CanvasPoint {
        self.delta
    }

    fn into_command(
        self,
        snapshot: &GraphSnapshot,
    ) -> Result<Option<GraphCommand>, FreshGraphMutationBindError> {
        let delta = feasible_cohort_delta(snapshot, &self.cohort, self.delta)?;
        let Some(delta) = delta else {
            return Ok(None);
        };
        let mut edits = Vec::with_capacity(self.cohort.members().len());
        for member in self.cohort.members() {
            let node = member.node();
            if member.provenance() != snapshot.provenance() {
                return Err(FreshGraphMutationBindError::NodeDragMemberForeignRuntime { node });
            }
            let Some(current_lifetime) = snapshot.node_lifetime_key(node) else {
                return Err(FreshGraphMutationBindError::NodeDragMemberUnavailable { node });
            };
            if current_lifetime != *member {
                return Err(FreshGraphMutationBindError::NodeDragMemberReplaced {
                    node,
                    expected: *member,
                    current: current_lifetime,
                });
            }
            let current = snapshot
                .graph()
                .node(&node)
                .ok_or(FreshGraphMutationBindError::NodeDragMemberUnavailable { node })?;
            if !current.draggable.unwrap_or(true) {
                return Err(FreshGraphMutationBindError::NodeDragMemberNotDraggable { node });
            }
            let position = CanvasPoint {
                x: current.pos.x + delta.x,
                y: current.pos.y + delta.y,
            };
            if !position.is_finite() {
                return Err(FreshGraphMutationBindError::NodeDragMemberPositionOverflow { node });
            }
            edits.push(NodeGeometryEdit::new(node).with_position(position));
        }
        Ok(Some(
            GraphCommand::set_node_geometry(snapshot.version(), edits).with_label("Move nodes"),
        ))
    }
}

/// Resolves one shared feasible movement delta against the fresh graph snapshot.
///
/// A cohort always moves as one atomic unit. Every member contributes an interval for each axis;
/// the requested delta is clamped to the intersection of those intervals. A malformed or missing
/// parent extent fails closed rather than allowing one member to escape its persisted boundary.
pub(super) fn feasible_cohort_delta(
    snapshot: &GraphSnapshot,
    cohort: &NodeMoveCohort,
    requested: CanvasPoint,
) -> Result<Option<CanvasPoint>, FreshGraphMutationBindError> {
    NodeMoveDeltaConstraint::from_snapshot(snapshot, cohort)?.constrain(requested)
}

fn movement_extent(
    snapshot: &GraphSnapshot,
    node: NodeId,
    extent: Option<NodeExtent>,
) -> Result<Option<CanvasRect>, FreshGraphMutationBindError> {
    match extent {
        None => Ok(None),
        Some(NodeExtent::Rect { rect }) => Ok(Some(rect)),
        Some(NodeExtent::Parent) => snapshot
            .graph()
            .node(&node)
            .and_then(|record| record.parent)
            .and_then(|parent| snapshot.graph().group(&parent).map(|group| group.rect))
            .map(Some)
            .ok_or(FreshGraphMutationBindError::NodeMoveExtentUnavailable { node }),
    }
}

fn movement_delta_bounds(
    position: CanvasPoint,
    origin: Option<jellyflow_core::core::NodeOrigin>,
    size: Option<CanvasSize>,
    extent: CanvasRect,
) -> Option<(f32, f32, f32, f32)> {
    if !position.is_finite() || !extent.is_finite() {
        return None;
    }
    let size = size.unwrap_or_default();
    if !size.width.is_finite() || !size.height.is_finite() || size.width < 0.0 || size.height < 0.0
    {
        return None;
    }
    let (origin_x, origin_y) = origin.map_or((0.0, 0.0), |origin| (origin.x, origin.y));
    let origin_x = origin_x.is_finite().then_some(origin_x.clamp(0.0, 1.0))?;
    let origin_y = origin_y.is_finite().then_some(origin_y.clamp(0.0, 1.0))?;
    let min_x = extent.origin.x + origin_x * size.width - position.x;
    let max_x = extent.origin.x + extent.size.width - (1.0 - origin_x) * size.width - position.x;
    let min_y = extent.origin.y + origin_y * size.height - position.y;
    let max_y = extent.origin.y + extent.size.height - (1.0 - origin_y) * size.height - position.y;
    (min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite())
        .then_some((min_x, max_x, min_y, max_y))
}

fn clamp_shared_axis(requested: f32, min: f32, max: f32) -> f32 {
    if min <= max {
        requested.clamp(min, max)
    } else {
        // The existing cohort is already mutually infeasible. Preserve document geometry instead
        // of moving a subset or selecting an arbitrary member's extent.
        0.0
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{
        CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeKindKey, Port,
        PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
    use crate::rules::EdgeEndpoint;
    use crate::runtime::commands::IncidentEdgePolicy;
    use crate::runtime::graph_runtime::GraphRuntime;

    #[test]
    fn relative_move_binds_once_to_fresh_positions_instead_of_press_time_positions() {
        let node = NodeId::from_u128(1);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node, test_node(CanvasPoint { x: 1.0, y: 2.0 }))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let admitted = runtime.snapshot().unwrap();
        let intent = RelativeNodeMoveIntent::new(
            [admitted
                .node_lifetime_key(node)
                .expect("admitted node identity")],
            CanvasPoint { x: 5.0, y: -1.0 },
        )
        .expect("relative intent");
        runtime
            .execute(GraphCommand::set_node_position(
                admitted.version(),
                node,
                CanvasPoint { x: 20.0, y: 30.0 },
            ))
            .into_result()
            .unwrap();
        let fresh = runtime.snapshot().unwrap();

        let command = FreshGraphMutation::move_nodes(intent)
            .into_command(&fresh)
            .unwrap()
            .expect("one movable node");
        assert_eq!(command.expected_version(), fresh.version());
        runtime.execute(command).into_result().unwrap();

        assert_eq!(
            runtime.snapshot().unwrap().graph().node(&node).unwrap().pos,
            CanvasPoint { x: 25.0, y: 29.0 }
        );
    }

    #[test]
    fn multi_node_move_binds_and_applies_as_one_atomic_command() {
        let first = NodeId::from_u128(1);
        let second = NodeId::from_u128(2);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(first, test_node(CanvasPoint { x: 1.0, y: 2.0 }))
            .with_node(second, test_node(CanvasPoint { x: 8.0, y: 13.0 }))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let admitted = runtime.snapshot().unwrap();
        let intent = RelativeNodeMoveIntent::new(
            [lifetime(&admitted, first), lifetime(&admitted, second)],
            CanvasPoint { x: 4.0, y: -3.0 },
        )
        .unwrap();

        let command = FreshGraphMutation::move_nodes(intent)
            .into_command(&admitted)
            .unwrap()
            .expect("complete cohort binds");
        let outcome = runtime.execute(command).into_result().unwrap();
        let receipt = outcome
            .applied()
            .expect("one atomic geometry command applies");
        assert_eq!(receipt.revision(), 1);
        let applied = runtime.snapshot().unwrap();
        assert_eq!(
            applied.graph().node(&first).unwrap().pos,
            CanvasPoint { x: 5.0, y: -1.0 }
        );
        assert_eq!(
            applied.graph().node(&second).unwrap().pos,
            CanvasPoint { x: 12.0, y: 10.0 }
        );
    }

    #[test]
    fn multi_node_move_rejects_the_whole_cohort_when_one_member_is_not_draggable() {
        let movable = NodeId::from_u128(1);
        let blocked = NodeId::from_u128(2);
        let mut blocked_node = test_node(CanvasPoint { x: 8.0, y: 13.0 });
        blocked_node.draggable = Some(false);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(movable, test_node(CanvasPoint { x: 1.0, y: 2.0 }))
            .with_node(blocked, blocked_node)
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let snapshot = runtime.snapshot().unwrap();
        let intent = RelativeNodeMoveIntent::new(
            [lifetime(&snapshot, movable), lifetime(&snapshot, blocked)],
            CanvasPoint { x: 4.0, y: -3.0 },
        )
        .unwrap();

        assert!(matches!(
            FreshGraphMutation::move_nodes(intent).into_command(&snapshot),
            Err(FreshGraphMutationBindError::NodeDragMemberNotDraggable { node })
                if node == blocked
        ));
        assert_eq!(runtime.snapshot().unwrap().revision(), 0);
        assert_eq!(
            runtime
                .snapshot()
                .unwrap()
                .graph()
                .node(&movable)
                .unwrap()
                .pos,
            CanvasPoint { x: 1.0, y: 2.0 }
        );
    }

    #[test]
    fn delayed_node_drag_bind_rejects_missing_and_rebuilt_members() {
        let node = NodeId::from_u128(1);
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(node, test_node(CanvasPoint { x: 1.0, y: 2.0 }))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let admitted = runtime.snapshot().unwrap();
        let member = lifetime(&admitted, node);
        runtime
            .execute(GraphCommand::topology(
                admitted.version(),
                TopologyEdit::new().remove_node(node, IncidentEdgePolicy::Reject),
            ))
            .into_result()
            .unwrap()
            .applied()
            .expect("node retirement applies");
        let retired = runtime.snapshot().unwrap();

        let missing =
            RelativeNodeMoveIntent::new([member], CanvasPoint { x: 4.0, y: -3.0 }).unwrap();
        assert!(matches!(
            FreshGraphMutation::move_nodes(missing).into_command(&retired),
            Err(FreshGraphMutationBindError::NodeDragMemberUnavailable { node: missing })
                if missing == node
        ));

        runtime
            .execute(GraphCommand::undo(retired.version()))
            .into_result()
            .unwrap()
            .applied()
            .expect("undo restores graph data under a fresh node lifetime");
        let rebuilt = runtime.snapshot().unwrap();
        assert_ne!(rebuilt.node_lifetime_key(node), Some(member));
        let stale = RelativeNodeMoveIntent::new([member], CanvasPoint { x: 4.0, y: -3.0 }).unwrap();
        assert!(matches!(
            FreshGraphMutation::move_nodes(stale).into_command(&rebuilt),
            Err(FreshGraphMutationBindError::NodeDragMemberReplaced { node: replaced, .. })
                if replaced == node
        ));
        assert_eq!(
            rebuilt.graph().node(&node).unwrap().pos,
            CanvasPoint { x: 1.0, y: 2.0 }
        );
    }

    #[test]
    fn edge_deletion_binds_once_treats_retired_as_satisfied_and_rejects_replacement() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(3);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        target.ports = vec![target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(target_node, target)
            .with_port(target_port, test_port(target_node, "in", PortDirection::In))
            .with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let initial = runtime.snapshot().unwrap();
        runtime
            .execute(GraphCommand::set_node_data(
                initial.version(),
                source_node,
                serde_json::json!({ "revision": 1 }),
            ))
            .into_result()
            .unwrap();

        let admitted = runtime.snapshot().unwrap();
        let intent = EdgeDeletionIntent {
            instance: admitted.edge_instance_key(edge).unwrap(),
            validated_version: admitted.version(),
        };
        let command = FreshGraphMutation::delete_edge(intent)
            .into_command(&runtime.snapshot().unwrap())
            .unwrap()
            .expect("existing edge produces a delete command");
        assert_eq!(command.expected_version(), admitted.version());
        assert_eq!(command.label(), Some("Delete edge"));
        runtime
            .execute(command)
            .into_result()
            .unwrap()
            .applied()
            .expect("edge deletion applies");
        assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());

        assert!(
            FreshGraphMutation::delete_edge(intent)
                .into_command(&runtime.snapshot().unwrap())
                .unwrap()
                .is_none()
        );
        runtime
            .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
            .into_result()
            .unwrap()
            .applied()
            .expect("one undo restores the deleted edge");
        assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_some());
        assert!(matches!(
            FreshGraphMutation::delete_edge(intent)
                .into_command(&runtime.snapshot().unwrap()),
            Err(FreshGraphMutationBindError::EdgeDeleteReplaced { edge: replaced, .. })
                if replaced == edge
        ));
    }

    #[test]
    fn connection_bind_rejects_a_snapshot_newer_than_the_validated_probe() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let runtime = GraphRuntime::builder(Default::default())
            .build(connection_graph(
                source_node,
                source_port,
                target_node,
                target_port,
                None,
            ))
            .unwrap();
        let validated = runtime.snapshot().unwrap();
        let mutation = FreshGraphMutation::connection(
            ConnectionGestureBinding::connect(
                validated.port_instance_key(source_port).unwrap(),
                EdgeEndpoint::From,
                EdgeId::from_u128(3),
                NodeGraphConnectionMode::Strict,
            ),
            ConnectionCommitTarget::existing(validated.port_instance_key(target_port).unwrap()),
            validated.version(),
        );
        runtime
            .execute(GraphCommand::set_node_data(
                validated.version(),
                source_node,
                serde_json::json!({ "advanced": true }),
            ))
            .into_result()
            .unwrap()
            .applied()
            .expect("unrelated command advances the graph version");
        let current = runtime.snapshot().unwrap();

        assert!(matches!(
            mutation.into_command(&current),
            Err(FreshGraphMutationBindError::RevisionChanged {
                validated: actual_validated,
                current: actual_current,
            }) if actual_validated == validated.version() && actual_current == current.version()
        ));
    }

    #[test]
    fn connection_and_edge_delete_intents_reject_same_id_facts_from_another_runtime() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(3);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        target.ports = vec![target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(target_node, target)
            .with_port(target_port, test_port(target_node, "in", PortDirection::In))
            .with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port))
            .build()
            .unwrap();
        let first = GraphRuntime::builder(Default::default())
            .build(graph.clone())
            .unwrap();
        let second = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let first_snapshot = first.snapshot().unwrap();
        let second_snapshot = second.snapshot().unwrap();
        let foreign_instance = first_snapshot.edge_instance_key(edge).unwrap();

        let connect = FreshGraphMutation::connection(
            ConnectionGestureBinding::connect(
                first_snapshot.port_instance_key(source_port).unwrap(),
                EdgeEndpoint::From,
                EdgeId::from_u128(4),
                NodeGraphConnectionMode::Strict,
            ),
            ConnectionCommitTarget::existing(
                second_snapshot.port_instance_key(target_port).unwrap(),
            ),
            second_snapshot.version(),
        );
        assert!(matches!(
            connect.into_command(&second_snapshot),
            Err(FreshGraphMutationBindError::ConnectionForeignRuntime { edge: rejected })
                if rejected == EdgeId::from_u128(4)
        ));

        let reconnect = FreshGraphMutation::connection(
            ConnectionGestureBinding::reconnect(
                foreign_instance,
                EdgeEndpoint::To,
                first_snapshot.port_instance_key(target_port).unwrap(),
                first_snapshot.port_instance_key(source_port).unwrap(),
                NodeGraphConnectionMode::Strict,
            )
            .unwrap(),
            ConnectionCommitTarget::existing(
                second_snapshot.port_instance_key(target_port).unwrap(),
            ),
            second_snapshot.version(),
        );
        assert!(matches!(
            reconnect.into_command(&second_snapshot),
            Err(FreshGraphMutationBindError::ConnectionForeignRuntime { edge: rejected })
                if rejected == edge
        ));

        let deletion = EdgeDeletionIntent {
            instance: foreign_instance,
            validated_version: second_snapshot.version(),
        };
        assert!(matches!(
            FreshGraphMutation::delete_edge(deletion).into_command(&second_snapshot),
            Err(FreshGraphMutationBindError::EdgeDeleteForeignRuntime { edge: rejected })
                if rejected == edge
        ));
        assert!(second_snapshot.graph().edge(&edge).is_some());
    }

    #[test]
    fn reconnect_bind_rejects_an_edge_id_reused_by_undo() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(3);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        target.ports = vec![target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(target_node, target)
            .with_port(target_port, test_port(target_node, "in", PortDirection::In))
            .with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let initial = runtime.snapshot().unwrap();
        let origin = initial
            .edge_instance_key(edge)
            .expect("initial edge identity");
        let detached = initial.port_instance_key(target_port).unwrap();
        let fixed = initial.port_instance_key(source_port).unwrap();

        runtime
            .execute(GraphCommand::disconnect(initial.version(), edge))
            .into_result()
            .unwrap()
            .applied()
            .expect("edge retirement applies");
        let retired = runtime.snapshot().unwrap();
        assert_eq!(retired.edge_instance_key(edge), None);

        runtime
            .execute(GraphCommand::undo(retired.version()))
            .into_result()
            .unwrap()
            .applied()
            .expect("undo restores the edge id");
        let restored = runtime.snapshot().unwrap();
        assert_ne!(
            restored.edge_instance_key(edge),
            Some(origin),
            "undo restores graph data but not the retired edge lifetime"
        );

        let mutation = FreshGraphMutation::connection(
            ConnectionGestureBinding::reconnect(
                origin,
                EdgeEndpoint::To,
                detached,
                fixed,
                NodeGraphConnectionMode::Strict,
            )
            .unwrap(),
            ConnectionCommitTarget::existing(restored.port_instance_key(target_port).unwrap()),
            restored.version(),
        );
        assert!(matches!(
            mutation.into_command(&restored),
            Err(FreshGraphMutationBindError::ReconnectEdgeRetired { edge: rejected })
                if rejected == edge
        ));
    }

    #[test]
    fn reconnect_bind_accepts_an_unchanged_edge_lifetime_after_an_unrelated_revision() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(3);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        target.ports = vec![target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(target_node, target)
            .with_port(target_port, test_port(target_node, "in", PortDirection::In))
            .with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let initial = runtime.snapshot().unwrap();
        let origin = initial
            .edge_instance_key(edge)
            .expect("initial edge identity");
        let detached = initial.port_instance_key(target_port).unwrap();
        let fixed = initial.port_instance_key(source_port).unwrap();

        runtime
            .execute(GraphCommand::set_node_data(
                initial.version(),
                source_node,
                serde_json::json!({ "unrelated": true }),
            ))
            .into_result()
            .unwrap()
            .applied()
            .expect("unrelated mutation applies");
        let fresh = runtime.snapshot().unwrap();
        assert_eq!(fresh.edge_instance_key(edge), Some(origin));

        let mutation = FreshGraphMutation::connection(
            ConnectionGestureBinding::reconnect(
                origin,
                EdgeEndpoint::To,
                detached,
                fixed,
                NodeGraphConnectionMode::Strict,
            )
            .unwrap(),
            ConnectionCommitTarget::existing(fresh.port_instance_key(target_port).unwrap()),
            fresh.version(),
        );
        assert!(
            mutation
                .into_command(&fresh)
                .expect("stable edge lifetime binds")
                .is_some()
        );
    }

    #[test]
    fn create_from_wire_binds_node_and_connection_as_one_undoable_topology_command() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let edge = EdgeId::from_u128(1);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let snapshot = runtime.snapshot().unwrap();
        let creation = test_creation(
            target_node,
            target_port,
            CanvasPoint { x: 200.0, y: 0.0 },
            "in",
            PortDirection::In,
        );
        let target =
            ConnectionCommitTarget::from_node_creation(&snapshot, target_port, creation).unwrap();
        let mutation = FreshGraphMutation::connection(
            ConnectionGestureBinding::connect(
                snapshot.port_instance_key(source_port).unwrap(),
                EdgeEndpoint::From,
                edge,
                NodeGraphConnectionMode::Strict,
            ),
            target,
            snapshot.version(),
        );

        let command = mutation
            .into_command(&runtime.snapshot().unwrap())
            .unwrap()
            .expect("atomic create-from-wire command");
        assert_eq!(command.expected_version(), snapshot.version());
        let applied_version = runtime
            .execute(command)
            .into_result()
            .unwrap()
            .applied()
            .expect("create-from-wire applies")
            .version();
        let applied = runtime.snapshot().unwrap();
        assert!(applied.graph().node(&target_node).is_some());
        assert!(applied.graph().port(&target_port).is_some());
        assert!(applied.graph().edge(&edge).is_some());

        runtime
            .execute(GraphCommand::undo(applied_version))
            .into_result()
            .unwrap()
            .applied()
            .expect("one undo reverts the atomic command");
        let undone = runtime.snapshot().unwrap();
        assert!(undone.graph().node(&target_node).is_none());
        assert!(undone.graph().port(&target_port).is_none());
        assert!(undone.graph().edge(&edge).is_none());
        assert!(undone.graph().node(&source_node).is_some());
    }

    #[test]
    fn reconnect_from_wire_keeps_edge_identity_and_replaces_only_the_detached_endpoint() {
        let source_node = NodeId::from_u128(1);
        let source_port = PortId::from_u128(1);
        let old_target_node = NodeId::from_u128(2);
        let old_target_port = PortId::from_u128(2);
        let new_target_node = NodeId::from_u128(3);
        let new_target_port = PortId::from_u128(3);
        let edge = EdgeId::from_u128(1);
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut old_target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        old_target.ports = vec![old_target_port];
        let graph = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(old_target_node, old_target)
            .with_port(
                old_target_port,
                test_port(old_target_node, "old-in", PortDirection::In),
            )
            .with_edge(
                edge,
                Edge::new(EdgeKind::Data, source_port, old_target_port),
            )
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let snapshot = runtime.snapshot().unwrap();
        let creation = test_creation(
            new_target_node,
            new_target_port,
            CanvasPoint { x: 400.0, y: 0.0 },
            "new-in",
            PortDirection::In,
        );
        let target =
            ConnectionCommitTarget::from_node_creation(&snapshot, new_target_port, creation)
                .unwrap();
        let mutation = FreshGraphMutation::connection(
            ConnectionGestureBinding::reconnect(
                snapshot.edge_instance_key(edge).unwrap(),
                EdgeEndpoint::To,
                snapshot.port_instance_key(old_target_port).unwrap(),
                snapshot.port_instance_key(source_port).unwrap(),
                NodeGraphConnectionMode::Strict,
            )
            .unwrap(),
            target,
            snapshot.version(),
        );

        let command = mutation
            .into_command(&runtime.snapshot().unwrap())
            .unwrap()
            .expect("atomic reconnect-from-wire command");
        let applied_version = runtime
            .execute(command)
            .into_result()
            .unwrap()
            .applied()
            .expect("reconnect-from-wire applies")
            .version();
        let applied = runtime.snapshot().unwrap();
        assert_eq!(applied.graph().edges().len(), 1);
        let reconnected = applied.graph().edge(&edge).expect("stable edge identity");
        assert_eq!(reconnected.from, source_port);
        assert_eq!(reconnected.to, new_target_port);

        runtime
            .execute(GraphCommand::undo(applied_version))
            .into_result()
            .unwrap()
            .applied()
            .expect("one undo restores reconnect-from-wire");
        let undone = runtime.snapshot().unwrap();
        assert!(undone.graph().node(&new_target_node).is_none());
        assert_eq!(undone.graph().edges().len(), 1);
        let restored = undone.graph().edge(&edge).expect("restored edge identity");
        assert_eq!(restored.from, source_port);
        assert_eq!(restored.to, old_target_port);
    }

    fn lifetime(snapshot: &GraphSnapshot, node: NodeId) -> NodeLifetimeKey {
        snapshot
            .node_lifetime_key(node)
            .expect("fixture node lifetime")
    }

    fn connection_graph(
        source_node: NodeId,
        source_port: PortId,
        target_node: NodeId,
        target_port: PortId,
        edge: Option<EdgeId>,
    ) -> Graph {
        let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
        source.ports = vec![source_port];
        let mut target = test_node(CanvasPoint { x: 200.0, y: 0.0 });
        target.ports = vec![target_port];
        let mut builder = GraphBuilder::new(GraphId::from_u128(1))
            .with_node(source_node, source)
            .with_port(
                source_port,
                test_port(source_node, "out", PortDirection::Out),
            )
            .with_node(target_node, target)
            .with_port(target_port, test_port(target_node, "in", PortDirection::In));
        if let Some(edge) = edge {
            builder = builder.with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port));
        }
        builder.build().unwrap()
    }

    fn test_creation(
        node: NodeId,
        port: PortId,
        position: CanvasPoint,
        key: &str,
        direction: PortDirection,
    ) -> NodeCreation {
        let mut model = test_node(position);
        model.ports = vec![port];
        NodeCreation::from_parts_for_test(
            node,
            model,
            vec![(port, test_port(node, key, direction))],
        )
    }

    fn test_node(pos: CanvasPoint) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos,
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 96.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }

    fn test_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }
}
