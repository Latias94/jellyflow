use std::collections::{BTreeMap, BTreeSet};

use crate::graph_kernel::{GraphOp, GraphTransaction};
use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, EdgeId, Graph, GraphLocalBindingTarget, NodeId, PortId,
    SymbolId, symbol_ref_target_symbol_id,
};

use super::index::GraphIndex;
use super::patch::{
    FactChange, FactChangeMask, GeometryImpact, NodeTranslation, RuntimeFactDelta, RuntimeFactKey,
    SessionImpact,
};

#[derive(Debug, Default)]
pub(super) struct IndexImpact {
    pub(super) ports: BTreeSet<PortId>,
    pub(super) edges: BTreeSet<EdgeId>,
    pub(super) bindings: BTreeSet<BindingId>,
    pub(super) symbol_ref_nodes: BTreeSet<NodeId>,
    pub(super) refresh_incident_nodes: BTreeSet<NodeId>,
    pub(super) reset_incident_nodes: BTreeSet<NodeId>,
}

#[derive(Debug, Default)]
pub(super) struct NodeIdentityImpact {
    pub(super) retired: BTreeSet<NodeId>,
    pub(super) added: BTreeSet<NodeId>,
}

#[derive(Debug)]
pub(super) struct InstalledTransactionImpact {
    pub(super) facts: Vec<RuntimeFactDelta>,
    pub(super) session: SessionImpact,
    pub(super) index: IndexImpact,
    pub(super) identities: NodeIdentityImpact,
}

#[derive(Debug, Default)]
struct SessionDirectives {
    geometry_withdrawals: BTreeSet<NodeId>,
    layout_dirty_nodes: BTreeSet<NodeId>,
    translated_nodes: BTreeSet<NodeId>,
    sanitize_view_state: bool,
}

#[derive(Debug, Default)]
struct ImpactSeed {
    fact_masks: BTreeMap<RuntimeFactKey, FactChangeMask>,
    index: IndexImpact,
    identities: NodeIdentityImpact,
    session: SessionDirectives,
}

impl InstalledTransactionImpact {
    pub(super) fn derive(
        before: &Graph,
        after: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
    ) -> Self {
        let mut seed = ImpactSeed::default();
        for operation in transaction.ops() {
            declare_operation_impact(operation, &mut seed);
        }

        seed.identities
            .retired
            .retain(|node| before.node(node).is_some());
        seed.identities
            .added
            .retain(|node| after.node(node).is_some());

        let facts = seed
            .fact_masks
            .into_iter()
            .map(|(key, change_mask)| {
                RuntimeFactDelta::new(key, fact_change(before, after, key), change_mask)
            })
            .collect::<Vec<_>>();
        let session = build_session_impact(
            before,
            after,
            before_index,
            transaction,
            &facts,
            &seed.index,
            &seed.identities,
            seed.session,
        );

        Self {
            facts,
            session,
            index: seed.index,
            identities: seed.identities,
        }
    }
}

fn declare_operation_impact(operation: &GraphOp, seed: &mut ImpactSeed) {
    match operation {
        GraphOp::AddNode { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::IDENTITY);
            seed.identities.added.insert(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.index.refresh_incident_nodes.insert(*id);
            seed.index.reset_incident_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::RemoveNode {
            id,
            ports,
            edges,
            bindings,
            ..
        } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::IDENTITY);
            seed.identities.retired.insert(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.index.refresh_incident_nodes.insert(*id);
            seed.index.reset_incident_nodes.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, _) in ports {
                seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
                seed.index.ports.insert(*id);
            }
            for (id, _) in edges {
                seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
                seed.index.edges.insert(*id);
            }
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetNodePos { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::POSITION);
            seed.session.translated_nodes.insert(*id);
        }
        GraphOp::SetNodeOrigin { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::POSITION);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeKind { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.hard_replace(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::SetNodeKindVersion { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.hard_replace(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::SetNodeSelectable { id, .. }
        | GraphOp::SetNodeDraggable { id, .. }
        | GraphOp::SetNodeConnectable { id, .. }
        | GraphOp::SetNodeDeletable { id, .. }
        | GraphOp::SetNodeExtent { id, .. }
        | GraphOp::SetNodeExpandParent { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::PRESENTATION);
        }
        GraphOp::SetNodeParent { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::STRUCTURE);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodePorts { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::STRUCTURE);
            seed.index.refresh_incident_nodes.insert(*id);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeSize { id, .. } | GraphOp::SetNodeCollapsed { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeHidden { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::VISIBILITY);
            seed.session.sanitize_view_state = true;
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeData { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::PAYLOAD);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }

        GraphOp::AddPort { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
            seed.index.ports.insert(*id);
        }
        GraphOp::RemovePort {
            id,
            edges,
            bindings,
            ..
        } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
            seed.index.ports.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, _) in edges {
                seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
                seed.index.edges.insert(*id);
            }
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetPortConnectable { id, .. }
        | GraphOp::SetPortConnectableStart { id, .. }
        | GraphOp::SetPortConnectableEnd { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::PRESENTATION);
        }
        GraphOp::SetPortType { id, .. } | GraphOp::SetPortData { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddEdge { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
            seed.index.edges.insert(*id);
        }
        GraphOp::RemoveEdge { id, bindings, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
            seed.index.edges.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetEdgeEndpoints { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::STRUCTURE);
            seed.index.edges.insert(*id);
            seed.session.sanitize_view_state = true;
        }
        GraphOp::SetEdgeHidden { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::VISIBILITY);
            seed.session.sanitize_view_state = true;
        }
        GraphOp::SetEdgeData { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::PAYLOAD);
        }
        GraphOp::SetEdgeKind { id, .. }
        | GraphOp::SetEdgeSelectable { id, .. }
        | GraphOp::SetEdgeInteractionWidth { id, .. }
        | GraphOp::SetEdgeDeletable { id, .. }
        | GraphOp::SetEdgeReconnectable { id, .. }
        | GraphOp::SetEdgeView { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::PRESENTATION);
        }

        GraphOp::AddImport { id, .. } | GraphOp::RemoveImport { id, .. } => {
            seed.fact(RuntimeFactKey::Import(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::SetImportAlias { id, .. } => {
            seed.fact(RuntimeFactKey::Import(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddSymbol { id, .. } | GraphOp::RemoveSymbol { id, .. } => {
            seed.fact(RuntimeFactKey::Symbol(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::SetSymbolName { id, .. }
        | GraphOp::SetSymbolType { id, .. }
        | GraphOp::SetSymbolDefaultValue { id, .. }
        | GraphOp::SetSymbolMeta { id, .. } => {
            seed.fact(RuntimeFactKey::Symbol(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddGroup { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::RemoveGroup {
            id,
            detached,
            bindings,
            ..
        } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::IDENTITY);
            seed.session.sanitize_view_state = true;
            for (node, _) in detached {
                seed.fact(RuntimeFactKey::Node(*node), FactChangeMask::STRUCTURE);
                seed.withdraw_and_dirty(*node);
            }
            for (binding, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*binding), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*binding);
            }
        }
        GraphOp::SetGroupRect { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::POSITION);
        }
        GraphOp::SetGroupTitle { id, .. } | GraphOp::SetGroupColor { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::PRESENTATION);
        }

        GraphOp::AddStickyNote { id, .. } => {
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::RemoveStickyNote { id, bindings, .. } => {
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::IDENTITY);
            for (binding, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*binding), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*binding);
            }
        }
        GraphOp::SetStickyNoteRect { id, .. } => {
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::POSITION);
        }
        GraphOp::SetStickyNoteText { id, .. } | GraphOp::SetStickyNoteColor { id, .. } => {
            seed.fact(
                RuntimeFactKey::StickyNote(*id),
                FactChangeMask::PRESENTATION,
            );
        }

        GraphOp::AddBinding { id, .. } | GraphOp::RemoveBinding { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
            seed.index.bindings.insert(*id);
        }
        GraphOp::SetBindingSubject { id, .. } | GraphOp::SetBindingTarget { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::PAYLOAD);
            seed.index.bindings.insert(*id);
        }
        GraphOp::SetBindingKind { id, .. } | GraphOp::SetBindingMeta { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::PAYLOAD);
        }
    }
}

impl ImpactSeed {
    fn fact(&mut self, key: RuntimeFactKey, mask: FactChangeMask) {
        self.fact_masks.entry(key).or_default().merge(mask);
    }

    fn hard_replace(&mut self, node: NodeId) {
        self.identities.retired.insert(node);
        self.identities.added.insert(node);
    }

    fn withdraw_and_dirty(&mut self, node: NodeId) {
        self.session.geometry_withdrawals.insert(node);
        self.session.layout_dirty_nodes.insert(node);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_session_impact(
    before: &Graph,
    after: &Graph,
    before_index: &GraphIndex,
    transaction: &GraphTransaction,
    facts: &[RuntimeFactDelta],
    index: &IndexImpact,
    identities: &NodeIdentityImpact,
    directives: SessionDirectives,
) -> SessionImpact {
    let footprint = transaction.footprint();
    let mut geometry = GeometryImpact::from_footprint(&footprint);
    let mut affected_nodes = BTreeSet::new();
    let mut retired_nodes = identities.retired.clone();
    let mut geometry_withdrawals = directives.geometry_withdrawals;
    let mut layout_dirty_nodes = directives.layout_dirty_nodes;
    let mut route_edges = BTreeSet::new();
    let mut position_only_nodes = BTreeSet::new();
    let staged_symbol_referrers = staged_symbol_referrers(after, &index.symbol_ref_nodes);

    for fact in facts {
        match fact.key() {
            RuntimeFactKey::Node(node) => {
                affected_nodes.insert(node);
                if fact.change() == FactChange::Removed {
                    retired_nodes.insert(node);
                }
                if fact.change() == FactChange::Updated
                    && fact.change_mask().is_only(FactChangeMask::POSITION)
                {
                    position_only_nodes.insert(node);
                }
            }
            RuntimeFactKey::Port(port) => {
                let owners = [before.port(&port), after.port(&port)]
                    .into_iter()
                    .flatten()
                    .map(|port| port.node)
                    .collect::<BTreeSet<_>>();
                affected_nodes.extend(owners.iter().copied());
                let live_owners = owners
                    .into_iter()
                    .filter(|node| after.node(node).is_some())
                    .collect::<BTreeSet<_>>();
                geometry_withdrawals.extend(live_owners.iter().copied());
                layout_dirty_nodes.extend(live_owners);
            }
            RuntimeFactKey::Edge(edge) => {
                for current in [before.edge(&edge), after.edge(&edge)]
                    .into_iter()
                    .flatten()
                {
                    affected_nodes.extend(
                        [
                            before.port(&current.from),
                            before.port(&current.to),
                            after.port(&current.from),
                            after.port(&current.to),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                    );
                }
                if fact.change() != FactChange::Updated
                    || !fact.change_mask().is_only(FactChangeMask::PAYLOAD)
                {
                    route_edges.insert(edge);
                }
            }
            RuntimeFactKey::Binding(binding) => {
                for current in [before.binding(&binding), after.binding(&binding)]
                    .into_iter()
                    .flatten()
                {
                    extend_binding_nodes(&mut affected_nodes, before, after, current);
                    extend_binding_geometry(&mut geometry, current);
                }
            }
            RuntimeFactKey::Symbol(symbol) => {
                let referrers = before_index
                    .symbol_referrers(symbol)
                    .chain(
                        staged_symbol_referrers
                            .get(&symbol)
                            .into_iter()
                            .flatten()
                            .copied(),
                    )
                    .collect::<BTreeSet<_>>();
                affected_nodes.extend(referrers.iter().copied());
                layout_dirty_nodes.extend(referrers);
            }
            RuntimeFactKey::Import(_)
            | RuntimeFactKey::Group(_)
            | RuntimeFactKey::StickyNote(_) => {}
        }
    }

    let translated_nodes = directives
        .translated_nodes
        .into_iter()
        .filter_map(|node| {
            let before = before.node(&node)?;
            let after = after.node(&node)?;
            Some(NodeTranslation {
                node,
                delta: CanvasPoint {
                    x: after.pos.x - before.pos.x,
                    y: after.pos.y - before.pos.y,
                },
            })
        })
        .collect::<Vec<_>>();

    for node in translated_nodes.iter().map(|translation| translation.node) {
        route_edges.extend(before_index.incident_edges_for_node(node));
    }
    for node in &layout_dirty_nodes {
        route_edges.extend(before_index.incident_edges_for_node(*node));
    }

    geometry.include_nodes(affected_nodes.iter().copied());
    geometry.mark_translation_only(
        translated_nodes
            .iter()
            .map(|translation| translation.node)
            .filter(|node| {
                !retired_nodes.contains(node)
                    && !geometry_withdrawals.contains(node)
                    && !layout_dirty_nodes.contains(node)
                    && position_only_nodes.contains(node)
            }),
    );
    if geometry.is_global() {
        geometry.make_global();
    }

    SessionImpact {
        affected_nodes: affected_nodes.into_iter().collect(),
        retired_nodes: retired_nodes.into_iter().collect(),
        geometry_withdrawals: geometry_withdrawals.into_iter().collect(),
        layout_dirty_nodes: layout_dirty_nodes.into_iter().collect(),
        translated_nodes,
        route_edges: route_edges.into_iter().collect(),
        sanitize_view_state: directives.sanitize_view_state,
        geometry,
    }
}

fn fact_change(before: &Graph, after: &Graph, key: RuntimeFactKey) -> FactChange {
    let (before, after) = match key {
        RuntimeFactKey::Import(id) => (before.import(&id).is_some(), after.import(&id).is_some()),
        RuntimeFactKey::Symbol(id) => (before.symbol(&id).is_some(), after.symbol(&id).is_some()),
        RuntimeFactKey::Node(id) => (before.node(&id).is_some(), after.node(&id).is_some()),
        RuntimeFactKey::Port(id) => (before.port(&id).is_some(), after.port(&id).is_some()),
        RuntimeFactKey::Edge(id) => (before.edge(&id).is_some(), after.edge(&id).is_some()),
        RuntimeFactKey::Group(id) => (before.group(&id).is_some(), after.group(&id).is_some()),
        RuntimeFactKey::StickyNote(id) => (
            before.sticky_note(&id).is_some(),
            after.sticky_note(&id).is_some(),
        ),
        RuntimeFactKey::Binding(id) => {
            (before.binding(&id).is_some(), after.binding(&id).is_some())
        }
    };
    match (before, after) {
        (false, true) => FactChange::Added,
        (true, false) => FactChange::Removed,
        _ => FactChange::Updated,
    }
}

fn staged_symbol_referrers(
    after: &Graph,
    changed_nodes: &BTreeSet<NodeId>,
) -> BTreeMap<SymbolId, BTreeSet<NodeId>> {
    let mut referrers = BTreeMap::<SymbolId, BTreeSet<NodeId>>::new();
    for node in changed_nodes {
        let Some(symbol) = after
            .node(node)
            .and_then(|record| symbol_ref_target_symbol_id(*node, record).ok().flatten())
        else {
            continue;
        };
        referrers.entry(symbol).or_default().insert(*node);
    }
    referrers
}

fn extend_binding_nodes(
    nodes: &mut BTreeSet<NodeId>,
    before: &Graph,
    after: &Graph,
    binding: &Binding,
) {
    for target in [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    {
        match target {
            GraphLocalBindingTarget::Node { id } => {
                nodes.insert(id);
            }
            GraphLocalBindingTarget::Port { id } => {
                nodes.extend(
                    [before.port(&id), after.port(&id)]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                );
            }
            GraphLocalBindingTarget::Edge { id } => {
                for edge in [before.edge(&id), after.edge(&id)].into_iter().flatten() {
                    nodes.extend(
                        [
                            before.port(&edge.from),
                            before.port(&edge.to),
                            after.port(&edge.from),
                            after.port(&edge.to),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                    );
                }
            }
            GraphLocalBindingTarget::Graph
            | GraphLocalBindingTarget::Group { .. }
            | GraphLocalBindingTarget::StickyNote { .. } => {}
        }
    }
}

fn extend_binding_geometry(impact: &mut GeometryImpact, binding: &Binding) {
    for target in [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    {
        match target {
            GraphLocalBindingTarget::Graph => impact.make_global(),
            GraphLocalBindingTarget::Node { id } => impact.include_nodes([id]),
            GraphLocalBindingTarget::Port { id } => impact.include_port(id),
            GraphLocalBindingTarget::Edge { id } => impact.include_incident_edge(id),
            GraphLocalBindingTarget::Group { .. } | GraphLocalBindingTarget::StickyNote { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_kernel::EdgeEndpoints;
    use jellyflow_core::core::{
        BindingEndpoint, CanvasRect, CanvasSize, Edge, EdgeKind, EdgeReconnectable,
        EdgeViewDescriptor, GraphBuilder, GraphId, GraphImport, Group, GroupId, Node, NodeExtent,
        NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection, PortKey, PortKind,
        SYMBOL_REF_NODE_KIND, StickyNote, StickyNoteId, Symbol, symbol_ref_node_data,
    };
    use jellyflow_core::types::TypeDesc;
    use serde_json::{Value, json};

    fn node_at(pos: CanvasPoint, ports: Vec<PortId>) -> Node {
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
            size: None,
            hidden: false,
            collapsed: false,
            ports,
            data: Value::Null,
        }
    }

    fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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
            data: Value::Null,
        }
    }

    fn rect(x: f32) -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x, y: x + 1.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        }
    }

    fn all_operation_variants() -> Vec<GraphOp> {
        let node_id = NodeId::from_u128(1);
        let port_id = PortId::from_u128(2);
        let other_port_id = PortId::from_u128(3);
        let edge_id = EdgeId::from_u128(4);
        let graph_id = GraphId::from_u128(5);
        let symbol_id = SymbolId::from_u128(6);
        let group_id = GroupId::from_u128(7);
        let other_group_id = GroupId::from_u128(8);
        let note_id = StickyNoteId::from_u128(9);
        let binding_id = BindingId::from_u128(10);
        let node = node_at(CanvasPoint::default(), vec![port_id]);
        let port = port(node_id, "value", PortDirection::Out);
        let edge = Edge::new(EdgeKind::Data, port_id, other_port_id);
        let import = GraphImport::default();
        let symbol = Symbol {
            name: "value".into(),
            ty: Some(TypeDesc::Float),
            default_value: Some(json!(0.0)),
            meta: Value::Null,
        };
        let group = Group {
            title: "group".into(),
            rect: rect(0.0),
            color: None,
        };
        let note = StickyNote {
            text: "note".into(),
            rect: rect(0.0),
            color: None,
        };
        let subject = BindingEndpoint::node(node_id);
        let target = BindingEndpoint::source_payload("fixture://source", Value::Null);
        let binding = Binding::new(subject.clone(), target.clone());

        vec![
            GraphOp::AddNode {
                id: node_id,
                node: node.clone(),
            },
            GraphOp::RemoveNode {
                id: node_id,
                node: node.clone(),
                ports: vec![],
                edges: vec![],
                bindings: vec![],
            },
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 1.0, y: 2.0 },
            },
            GraphOp::SetNodeOrigin {
                id: node_id,
                from: None,
                to: Some(NodeOrigin { x: 0.5, y: 0.5 }),
            },
            GraphOp::SetNodeKind {
                id: node_id,
                from: NodeKindKey::new("from"),
                to: NodeKindKey::new("to"),
            },
            GraphOp::SetNodeKindVersion {
                id: node_id,
                from: 1,
                to: 2,
            },
            GraphOp::SetNodeSelectable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeDraggable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeConnectable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeDeletable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeParent {
                id: node_id,
                from: None,
                to: Some(group_id),
            },
            GraphOp::SetNodeExtent {
                id: node_id,
                from: None,
                to: Some(NodeExtent::Parent),
            },
            GraphOp::SetNodeExpandParent {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeSize {
                id: node_id,
                from: None,
                to: Some(CanvasSize {
                    width: 10.0,
                    height: 20.0,
                }),
            },
            GraphOp::SetNodeHidden {
                id: node_id,
                from: false,
                to: true,
            },
            GraphOp::SetNodeCollapsed {
                id: node_id,
                from: false,
                to: true,
            },
            GraphOp::SetNodePorts {
                id: node_id,
                from: vec![port_id],
                to: vec![other_port_id],
            },
            GraphOp::SetNodeData {
                id: node_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddPort {
                id: port_id,
                port: port.clone(),
            },
            GraphOp::RemovePort {
                id: port_id,
                port: port.clone(),
                edges: vec![],
                bindings: vec![],
            },
            GraphOp::SetPortConnectable {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortConnectableStart {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortConnectableEnd {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortType {
                id: port_id,
                from: None,
                to: Some(TypeDesc::Float),
            },
            GraphOp::SetPortData {
                id: port_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddEdge {
                id: edge_id,
                edge: edge.clone(),
            },
            GraphOp::RemoveEdge {
                id: edge_id,
                edge: edge.clone(),
                bindings: vec![],
            },
            GraphOp::SetEdgeKind {
                id: edge_id,
                from: EdgeKind::Data,
                to: EdgeKind::Exec,
            },
            GraphOp::SetEdgeSelectable {
                id: edge_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetEdgeHidden {
                id: edge_id,
                from: false,
                to: true,
            },
            GraphOp::SetEdgeInteractionWidth {
                id: edge_id,
                from: None,
                to: Some(12.0),
            },
            GraphOp::SetEdgeDeletable {
                id: edge_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetEdgeReconnectable {
                id: edge_id,
                from: None,
                to: Some(EdgeReconnectable::Bool(true)),
            },
            GraphOp::SetEdgeData {
                id: edge_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::SetEdgeView {
                id: edge_id,
                from: EdgeViewDescriptor::default(),
                to: EdgeViewDescriptor::new().with_label("value"),
            },
            GraphOp::SetEdgeEndpoints {
                id: edge_id,
                from: EdgeEndpoints::new(port_id, other_port_id),
                to: EdgeEndpoints::new(other_port_id, port_id),
            },
            GraphOp::AddImport {
                id: graph_id,
                import: import.clone(),
            },
            GraphOp::RemoveImport {
                id: graph_id,
                import: import.clone(),
            },
            GraphOp::SetImportAlias {
                id: graph_id,
                from: None,
                to: Some("alias".into()),
            },
            GraphOp::AddSymbol {
                id: symbol_id,
                symbol: symbol.clone(),
            },
            GraphOp::RemoveSymbol {
                id: symbol_id,
                symbol: symbol.clone(),
            },
            GraphOp::SetSymbolName {
                id: symbol_id,
                from: "from".into(),
                to: "to".into(),
            },
            GraphOp::SetSymbolType {
                id: symbol_id,
                from: None,
                to: Some(TypeDesc::Float),
            },
            GraphOp::SetSymbolDefaultValue {
                id: symbol_id,
                from: None,
                to: Some(json!(1.0)),
            },
            GraphOp::SetSymbolMeta {
                id: symbol_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddGroup {
                id: group_id,
                group: group.clone(),
            },
            GraphOp::RemoveGroup {
                id: group_id,
                group: group.clone(),
                detached: vec![(node_id, Some(other_group_id))],
                bindings: vec![],
            },
            GraphOp::SetGroupRect {
                id: group_id,
                from: rect(0.0),
                to: rect(1.0),
            },
            GraphOp::SetGroupTitle {
                id: group_id,
                from: "from".into(),
                to: "to".into(),
            },
            GraphOp::SetGroupColor {
                id: group_id,
                from: None,
                to: Some("red".into()),
            },
            GraphOp::AddStickyNote {
                id: note_id,
                note: note.clone(),
            },
            GraphOp::RemoveStickyNote {
                id: note_id,
                note: note.clone(),
                bindings: vec![],
            },
            GraphOp::SetStickyNoteText {
                id: note_id,
                from: "from".into(),
                to: "to".into(),
            },
            GraphOp::SetStickyNoteRect {
                id: note_id,
                from: rect(0.0),
                to: rect(1.0),
            },
            GraphOp::SetStickyNoteColor {
                id: note_id,
                from: None,
                to: Some("yellow".into()),
            },
            GraphOp::AddBinding {
                id: binding_id,
                binding: binding.clone(),
            },
            GraphOp::RemoveBinding {
                id: binding_id,
                binding: binding.clone(),
            },
            GraphOp::SetBindingSubject {
                id: binding_id,
                from: subject.clone(),
                to: BindingEndpoint::port(port_id),
            },
            GraphOp::SetBindingTarget {
                id: binding_id,
                from: target.clone(),
                to: BindingEndpoint::edge(edge_id),
            },
            GraphOp::SetBindingKind {
                id: binding_id,
                from: None,
                to: Some("depends_on".into()),
            },
            GraphOp::SetBindingMeta {
                id: binding_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
        ]
    }

    #[test]
    fn every_graph_operation_variant_declares_installed_impact() {
        let operations = all_operation_variants();
        let names = operations
            .iter()
            .map(|operation| {
                serde_json::to_value(operation).expect("serialize graph operation")["op"]
                    .as_str()
                    .expect("tagged operation name")
                    .to_owned()
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(operations.len(), 61);
        assert_eq!(
            names.len(),
            61,
            "the fixture must contain every variant once"
        );
        for operation in &operations {
            let mut seed = ImpactSeed::default();
            declare_operation_impact(operation, &mut seed);
            assert!(
                !seed.fact_masks.is_empty(),
                "{operation:?} did not declare a runtime fact"
            );
        }
    }

    #[test]
    fn interleaved_position_updates_report_the_authoritative_net_delta() {
        let node_id = NodeId::from_u128(0x101);
        let before = GraphBuilder::new(GraphId::from_u128(0x102))
            .with_node(node_id, node_at(CanvasPoint::default(), vec![]))
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 10.0, y: 2.0 },
            },
            GraphOp::SetNodeData {
                id: node_id,
                from: Value::Null,
                to: json!({"interleaved": true}),
            },
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint { x: 10.0, y: 2.0 },
                to: CanvasPoint { x: 25.0, y: 5.0 },
            },
        ]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply fixture transaction");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.facts.len(), 1);
        assert!(
            impact.facts[0]
                .change_mask()
                .contains(FactChangeMask::POSITION)
        );
        assert!(
            impact.facts[0]
                .change_mask()
                .contains(FactChangeMask::PAYLOAD)
        );
        assert_eq!(impact.session.translated_nodes.len(), 1);
        assert_eq!(impact.session.translated_nodes[0].node, node_id);
        assert_eq!(
            impact.session.translated_nodes[0].delta,
            CanvasPoint { x: 25.0, y: 5.0 }
        );
        assert!(!impact.session.geometry.preserves_translated_layout(
            node_id,
            std::iter::empty(),
            std::iter::empty()
        ));
    }

    #[test]
    fn unrelated_interleaved_edit_preserves_net_translation_layout() {
        let moved = NodeId::from_u128(0x111);
        let other = NodeId::from_u128(0x112);
        let before = GraphBuilder::new(GraphId::from_u128(0x113))
            .with_node(moved, node_at(CanvasPoint::default(), vec![]))
            .with_node(other, node_at(CanvasPoint { x: 100.0, y: 0.0 }, vec![]))
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetNodePos {
                id: moved,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 10.0, y: 2.0 },
            },
            GraphOp::SetNodeSelectable {
                id: other,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodePos {
                id: moved,
                from: CanvasPoint { x: 10.0, y: 2.0 },
                to: CanvasPoint { x: 25.0, y: 5.0 },
            },
        ]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply fixture transaction");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.session.translated_nodes.len(), 1);
        assert_eq!(impact.session.translated_nodes[0].node, moved);
        assert_eq!(
            impact.session.translated_nodes[0].delta,
            CanvasPoint { x: 25.0, y: 5.0 }
        );
        assert!(impact.session.layout_dirty_nodes.is_empty());
        assert!(impact.session.geometry.preserves_translated_layout(
            moved,
            std::iter::empty(),
            std::iter::empty()
        ));
    }

    #[test]
    fn cascaded_port_removal_does_not_redirty_a_retired_node() {
        let node_id = NodeId::from_u128(0x121);
        let port_id = PortId::from_u128(0x122);
        let node = node_at(CanvasPoint::default(), vec![port_id]);
        let port = port(node_id, "value", PortDirection::Out);
        let before = GraphBuilder::new(GraphId::from_u128(0x123))
            .with_node(node_id, node.clone())
            .with_port(port_id, port.clone())
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([GraphOp::RemoveNode {
            id: node_id,
            node,
            ports: vec![(port_id, port)],
            edges: vec![],
            bindings: vec![],
        }]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("remove fixture node");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.session.affected_nodes, vec![node_id]);
        assert_eq!(impact.session.retired_nodes, vec![node_id]);
        assert!(impact.session.geometry_withdrawals.is_empty());
        assert!(impact.session.layout_dirty_nodes.is_empty());
        assert_eq!(impact.index.ports, BTreeSet::from([port_id]));
    }

    fn indexed_fixture() -> (Graph, GraphTransaction) {
        let symbol_a = SymbolId::from_u128(0x201);
        let symbol_b = SymbolId::from_u128(0x202);
        let source_node = NodeId::from_u128(0x203);
        let first_target = NodeId::from_u128(0x204);
        let second_target = NodeId::from_u128(0x205);
        let source_port = PortId::from_u128(0x206);
        let first_port = PortId::from_u128(0x207);
        let second_port = PortId::from_u128(0x208);
        let edge_id = EdgeId::from_u128(0x209);
        let binding_id = BindingId::from_u128(0x20a);
        let mut symbol_node = node_at(CanvasPoint::default(), vec![source_port]);
        symbol_node.kind = NodeKindKey::new(SYMBOL_REF_NODE_KIND);
        symbol_node.data = symbol_ref_node_data(symbol_a);
        let symbol = |name: &str| Symbol {
            name: name.into(),
            ty: Some(TypeDesc::Float),
            default_value: None,
            meta: Value::Null,
        };
        let binding = Binding::new(
            BindingEndpoint::port(first_port),
            BindingEndpoint::source_payload("fixture://source", Value::Null),
        );
        let graph = GraphBuilder::new(GraphId::from_u128(0x20b))
            .with_symbol(symbol_a, symbol("a"))
            .with_symbol(symbol_b, symbol("b"))
            .with_node(source_node, symbol_node)
            .with_node(
                first_target,
                node_at(CanvasPoint { x: 50.0, y: 0.0 }, vec![first_port]),
            )
            .with_node(
                second_target,
                node_at(CanvasPoint { x: 50.0, y: 50.0 }, vec![second_port]),
            )
            .with_port(source_port, port(source_node, "out", PortDirection::Out))
            .with_port(first_port, port(first_target, "in", PortDirection::In))
            .with_port(second_port, port(second_target, "in", PortDirection::In))
            .with_edge(edge_id, Edge::new(EdgeKind::Data, source_port, first_port))
            .with_binding(binding_id, binding.clone())
            .build()
            .expect("valid indexed fixture");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetEdgeEndpoints {
                id: edge_id,
                from: EdgeEndpoints::new(source_port, first_port),
                to: EdgeEndpoints::new(source_port, second_port),
            },
            GraphOp::SetBindingSubject {
                id: binding_id,
                from: binding.subject,
                to: BindingEndpoint::port(second_port),
            },
            GraphOp::SetNodeData {
                id: source_node,
                from: symbol_ref_node_data(symbol_a),
                to: symbol_ref_node_data(symbol_b),
            },
        ]);
        (graph, transaction)
    }

    fn apply_and_verify_index(
        before: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
    ) -> (Graph, GraphIndex) {
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply indexed fixture transaction");
        let impact = InstalledTransactionImpact::derive(before, &after, before_index, transaction);
        let mut updated = before_index.clone();
        updated.apply_impact(&after, &impact.index);
        assert_eq!(updated, GraphIndex::from_graph(&after));
        (after, updated)
    }

    #[test]
    fn incremental_index_matches_rebuild_across_execute_undo_and_redo() {
        let (before, transaction) = indexed_fixture();
        let before_index = GraphIndex::from_graph(&before);

        let (after, after_index) = apply_and_verify_index(&before, &before_index, &transaction);
        let inverse = transaction.inverse();
        let (undone, undone_index) = apply_and_verify_index(&after, &after_index, &inverse);
        assert_eq!(undone_index, before_index);
        let (redone, redone_index) = apply_and_verify_index(&undone, &undone_index, &transaction);
        assert_eq!(redone_index, after_index);
        assert_eq!(redone.nodes().len(), after.nodes().len());
    }

    #[test]
    fn one_node_edit_in_a_large_graph_keeps_impact_local() {
        const NODE_COUNT: u128 = 10_000;
        let target = NodeId::from_u128(0x301);
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x302));
        builder.insert_node(target, node_at(CanvasPoint::default(), vec![]));
        for index in 0..NODE_COUNT - 1 {
            builder.insert_node(
                NodeId::from_u128(0x1_000 + index),
                node_at(
                    CanvasPoint {
                        x: index as f32,
                        y: 0.0,
                    },
                    vec![],
                ),
            );
        }
        let before = builder.build().expect("valid large graph");
        let transaction = GraphTransaction::from_ops([GraphOp::SetNodeData {
            id: target,
            from: Value::Null,
            to: json!({"changed": true}),
        }]);
        let mut after = before.clone();
        transaction.apply_to(&mut after).expect("apply local edit");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.facts.len(), 1);
        assert_eq!(impact.session.affected_nodes, vec![target]);
        assert_eq!(impact.session.layout_dirty_nodes, vec![target]);
        assert!(impact.index.ports.is_empty());
        assert!(impact.index.edges.is_empty());
        assert!(impact.index.bindings.is_empty());
        assert_eq!(impact.index.symbol_ref_nodes, BTreeSet::from([target]));
    }

    #[test]
    fn graph_and_import_footprints_require_global_geometry_invalidation() {
        let mut footprint = crate::graph_kernel::GraphMutationFootprint::new();
        footprint.touch_import(GraphId::from_u128(0x401));
        assert!(GeometryImpact::from_footprint(&footprint).is_global());

        let mut footprint = crate::graph_kernel::GraphMutationFootprint::new();
        footprint.touch_graph();
        assert!(GeometryImpact::from_footprint(&footprint).is_global());
    }

    #[test]
    fn graph_binding_requires_global_geometry_invalidation() {
        let binding = Binding::graph_local_to_source(
            GraphLocalBindingTarget::Graph,
            "fixture://graph",
            Value::Null,
        );
        let mut impact = GeometryImpact::default();

        extend_binding_geometry(&mut impact, &binding);

        assert!(impact.is_global());
        assert!(impact.nodes().is_empty());
        assert!(impact.ports().is_empty());
        assert!(impact.incident_edges().is_empty());
    }
}
