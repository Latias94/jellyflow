use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use jellyflow_runtime::{
    graph::EdgeEndpoint, graph::EdgeInstanceKey, graph::NodeLifetimeKey, graph::PortInstanceKey,
};
use open_gpui::{Context, FocusHandle, Window};

use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::OpenGpuiSceneHost;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OpenGpuiGraphFocusTarget {
    Node {
        lifetime: NodeLifetimeKey,
        presentation: OpenGpuiNodePresentation,
    },
    Port(PortInstanceKey),
    Edge(EdgeInstanceKey),
    ReconnectSource(EdgeInstanceKey),
    ReconnectTarget(EdgeInstanceKey),
}

impl OpenGpuiGraphFocusTarget {
    pub(super) fn reconnect(edge: EdgeInstanceKey, endpoint: EdgeEndpoint) -> Self {
        match endpoint {
            EdgeEndpoint::From => Self::ReconnectSource(edge),
            EdgeEndpoint::To => Self::ReconnectTarget(edge),
        }
    }

    fn is_retained_action(self) -> bool {
        matches!(
            self,
            Self::Edge(_) | Self::ReconnectSource(_) | Self::ReconnectTarget(_)
        )
    }

    fn logical_node(self) -> Option<NodeLifetimeKey> {
        match self {
            Self::Node { lifetime, .. } => Some(lifetime),
            Self::Port(_) | Self::Edge(_) | Self::ReconnectSource(_) | Self::ReconnectTarget(_) => {
                None
            }
        }
    }
}

impl Hash for OpenGpuiGraphFocusTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match *self {
            Self::Node {
                lifetime,
                presentation,
            } => {
                lifetime.hash(state);
                match presentation {
                    OpenGpuiNodePresentation::Detail => 0_u8,
                    OpenGpuiNodePresentation::Overview => 1_u8,
                }
                .hash(state);
            }
            Self::Port(port) => port.hash(state),
            Self::Edge(edge) | Self::ReconnectSource(edge) | Self::ReconnectTarget(edge) => {
                edge.hash(state);
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct OpenGpuiGraphFocusEntry {
    pub(super) target: OpenGpuiGraphFocusTarget,
    pub(super) handle: FocusHandle,
}

impl OpenGpuiGraphFocusEntry {
    pub(super) fn new(target: OpenGpuiGraphFocusTarget, handle: FocusHandle) -> Self {
        Self { target, handle }
    }
}

struct CurrentFocusIndex<'a> {
    current: &'a [OpenGpuiGraphFocusEntry],
    by_target: HashMap<OpenGpuiGraphFocusTarget, usize>,
    first_node_by_lifetime: HashMap<NodeLifetimeKey, usize>,
}

impl<'a> CurrentFocusIndex<'a> {
    fn new(current: &'a [OpenGpuiGraphFocusEntry]) -> Self {
        let mut by_target = HashMap::with_capacity(current.len());
        let mut first_node_by_lifetime = HashMap::with_capacity(current.len());
        for (index, entry) in current.iter().enumerate() {
            by_target.entry(entry.target).or_insert(index);
            if let Some(node) = entry.target.logical_node() {
                first_node_by_lifetime.entry(node).or_insert(index);
            }
        }
        Self {
            current,
            by_target,
            first_node_by_lifetime,
        }
    }

    fn entry_for_target(
        &self,
        target: OpenGpuiGraphFocusTarget,
    ) -> Option<&'a OpenGpuiGraphFocusEntry> {
        self.by_target
            .get(&target)
            .map(|index| &self.current[*index])
    }

    fn first_node_entry(&self, node: NodeLifetimeKey) -> Option<&'a OpenGpuiGraphFocusEntry> {
        self.first_node_by_lifetime
            .get(&node)
            .map(|index| &self.current[*index])
    }

    fn contains_handle(&self, handle: &FocusHandle) -> bool {
        self.current.iter().any(|entry| &entry.handle == handle)
    }

    fn replacement_for(
        &self,
        target: OpenGpuiGraphFocusTarget,
    ) -> Option<&'a OpenGpuiGraphFocusEntry> {
        match target.logical_node() {
            Some(node) => self.first_node_entry(node),
            None => self.entry_for_target(target),
        }
    }
}

#[derive(Default)]
pub(super) struct OpenGpuiGraphFocusCoordinator {
    actions: BTreeMap<OpenGpuiGraphFocusTarget, FocusHandle>,
    previous: Vec<OpenGpuiGraphFocusEntry>,
    return_target: Option<OpenGpuiGraphFocusTarget>,
}

impl OpenGpuiGraphFocusCoordinator {
    pub(super) fn has_entries(&self) -> bool {
        !self.actions.is_empty() || !self.previous.is_empty() || self.return_target.is_some()
    }

    pub(super) fn action_handle(
        &mut self,
        target: OpenGpuiGraphFocusTarget,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> FocusHandle {
        self.actions
            .entry(target)
            .or_insert_with(|| cx.focus_handle())
            .clone()
            .tab_stop(true)
    }

    pub(super) fn exact_focused_target(&self, window: &Window) -> Option<OpenGpuiGraphFocusTarget> {
        self.previous
            .iter()
            .find_map(|entry| entry.handle.is_focused(window).then_some(entry.target))
    }

    pub(super) fn remember_return_target(&mut self, target: OpenGpuiGraphFocusTarget) {
        self.return_target = Some(target);
    }

    pub(super) fn reconcile(
        &mut self,
        current: Vec<OpenGpuiGraphFocusEntry>,
        root: &FocusHandle,
        allow_return: bool,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) {
        {
            let current_index = CurrentFocusIndex::new(&current);
            if allow_return
                && let Some(target) = self.return_target.take()
                && (root.is_focused(window) || window.focused(cx).is_none())
            {
                if let Some(entry) = current_index.entry_for_target(target) {
                    window.focus(&entry.handle, cx);
                } else {
                    window.focus(root, cx);
                }
            } else if allow_return {
                self.return_target = None;
            }

            if let Some(index) = self.disappeared_focus_index(&current_index, window, cx) {
                if let Some(fallback) = self.fallback_for(index, &current_index) {
                    window.focus(&fallback.handle, cx);
                } else {
                    window.focus(root, cx);
                }
            }
        }

        let retained = current
            .iter()
            .filter_map(|entry| entry.target.is_retained_action().then_some(entry.target))
            .collect::<BTreeSet<_>>();
        self.actions.retain(|target, _| retained.contains(target));
        self.previous = current;
    }

    fn disappeared_focus_index(
        &self,
        current: &CurrentFocusIndex<'_>,
        window: &Window,
        cx: &open_gpui::App,
    ) -> Option<usize> {
        self.disappeared_focus_index_by(
            current,
            |handle| handle.is_focused(window),
            |handle| handle.contains_focused(window, cx),
        )
    }

    fn disappeared_focus_index_by(
        &self,
        current: &CurrentFocusIndex<'_>,
        mut is_focused: impl FnMut(&FocusHandle) -> bool,
        mut contains_focused: impl FnMut(&FocusHandle) -> bool,
    ) -> Option<usize> {
        if let Some(index) = self
            .previous
            .iter()
            .position(|entry| is_focused(&entry.handle))
        {
            return (!current.contains_handle(&self.previous[index].handle)).then_some(index);
        }

        self.previous.iter().enumerate().find_map(|(index, entry)| {
            let node = entry.target.logical_node()?;
            let same_entry_survives = current
                .entry_for_target(entry.target)
                .is_some_and(|current| current.handle == entry.handle);
            if same_entry_survives {
                return None;
            }
            let presentation_changed = current.first_node_entry(node).is_some();
            (presentation_changed && contains_focused(&entry.handle)).then_some(index)
        })
    }

    fn fallback_for<'a>(
        &self,
        removed: usize,
        current: &CurrentFocusIndex<'a>,
    ) -> Option<&'a OpenGpuiGraphFocusEntry> {
        let removed_target = self.previous[removed].target;
        if let Some(replacement) = current.replacement_for(removed_target) {
            return Some(replacement);
        }
        self.previous[removed + 1..]
            .iter()
            .find_map(|previous| current.entry_for_target(previous.target))
            .or_else(|| {
                self.previous[..removed]
                    .iter()
                    .rev()
                    .find_map(|previous| current.entry_for_target(previous.target))
            })
    }

    pub(super) fn retire(&mut self) {
        self.actions.clear();
        self.previous.clear();
        self.return_target = None;
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::{
        CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node, NodeId,
        NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    };
    use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
    use jellyflow_runtime::{graph::GraphRuntime, view::ViewSessionOptions};
    use open_gpui::{Context, IntoElement, Render};

    use super::*;

    struct FocusTestView;

    impl Render for FocusTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            open_gpui::div()
        }
    }

    fn node_target(
        lifetime: NodeLifetimeKey,
        presentation: OpenGpuiNodePresentation,
    ) -> OpenGpuiGraphFocusTarget {
        OpenGpuiGraphFocusTarget::Node {
            lifetime,
            presentation,
        }
    }

    struct FocusIdentities {
        nodes: [NodeLifetimeKey; 2],
        ports: [PortInstanceKey; 2],
        edge: EdgeInstanceKey,
    }

    fn focus_identities() -> FocusIdentities {
        const KIND: &str = "test.focus-node";
        let nodes = [NodeId::from_u128(1), NodeId::from_u128(2)];
        let ports = [PortId::from_u128(11), PortId::from_u128(12)];
        let edge = EdgeId::from_u128(21);
        let mut catalog = NodeCatalog::new();
        catalog
            .register(
                NodeKindSpec::builder(KIND)
                    .port(NodePortSpec::new(
                        "out",
                        PortDirection::Out,
                        PortKind::Data,
                        PortCapacity::Single,
                    ))
                    .port(NodePortSpec::new(
                        "in",
                        PortDirection::In,
                        PortKind::Data,
                        PortCapacity::Single,
                    ))
                    .build(),
            )
            .expect("unique focus node kind");
        let mut graph = GraphBuilder::new(GraphId::from_u128(1));
        graph.insert_node(nodes[0], focus_node(KIND, ports[0], 0.0));
        graph.insert_node(nodes[1], focus_node(KIND, ports[1], 200.0));
        graph.insert_port(ports[0], focus_port(nodes[0], "out", PortDirection::Out));
        graph.insert_port(ports[1], focus_port(nodes[1], "in", PortDirection::In));
        graph.insert_edge(edge, Edge::new(EdgeKind::Data, ports[0], ports[1]));
        let runtime = GraphRuntime::builder(catalog)
            .build(graph.build().expect("valid focus graph"))
            .expect("valid focus runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("focus view");
        let snapshot = view.graph_snapshot().expect("focus snapshot");
        FocusIdentities {
            nodes: nodes.map(|node| {
                snapshot
                    .node_lifetime_key(node)
                    .expect("current node lifetime")
            }),
            ports: ports.map(|port| {
                view.port_instance_key(port)
                    .expect("port identity query")
                    .expect("current port identity")
            }),
            edge: view
                .edge_instance_key(edge)
                .expect("edge identity query")
                .expect("current edge identity"),
        }
    }

    fn focus_node(kind: &str, port: PortId, x: f32) -> Node {
        Node {
            kind: NodeKindKey::new(kind),
            kind_version: 1,
            pos: CanvasPoint { x, y: 0.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 100.0,
                height: 60.0,
            }),
            hidden: false,
            collapsed: false,
            ports: vec![port],
            data: serde_json::Value::Null,
        }
    }

    fn focus_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }

    fn entry(target: OpenGpuiGraphFocusTarget, handle: &FocusHandle) -> OpenGpuiGraphFocusEntry {
        OpenGpuiGraphFocusEntry::new(target, handle.clone())
    }

    #[open_gpui::test]
    fn exact_focused_handle_survives_lod_replacement_and_a_new_handle_receives_focus(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let node = focus_identities().nodes[0];
            let old_handle = cx.focus_handle();
            let replacement_handle = cx.focus_handle();
            let detail = node_target(node, OpenGpuiNodePresentation::Detail);
            let overview = node_target(node, OpenGpuiNodePresentation::Overview);
            let coordinator = OpenGpuiGraphFocusCoordinator {
                previous: vec![entry(detail, &old_handle)],
                ..OpenGpuiGraphFocusCoordinator::default()
            };
            window.focus(&old_handle, cx);

            let surviving_handle = vec![entry(overview, &old_handle)];
            let surviving_index = CurrentFocusIndex::new(&surviving_handle);
            assert_eq!(
                coordinator.disappeared_focus_index(&surviving_index, window, cx),
                None,
                "a presentation change must not move an exact surviving focus handle"
            );

            let replaced_handle = vec![entry(overview, &replacement_handle)];
            let replaced_index = CurrentFocusIndex::new(&replaced_handle);
            let removed = coordinator
                .disappeared_focus_index(&replaced_index, window, cx)
                .expect("focused handle disappeared");
            assert_eq!(removed, 0);
            assert_eq!(
                coordinator
                    .fallback_for(removed, &replaced_index)
                    .map(|entry| entry.handle.clone()),
                Some(replacement_handle)
            );
        });
    }

    #[open_gpui::test]
    fn lod_replacement_uses_the_first_graph_order_target_for_the_logical_node(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|_, cx| {
            let node = focus_identities().nodes[0];
            let old_handle = cx.focus_handle();
            let overview_handle = cx.focus_handle();
            let later_detail_handle = cx.focus_handle();
            let detail = node_target(node, OpenGpuiNodePresentation::Detail);
            let overview = node_target(node, OpenGpuiNodePresentation::Overview);
            let coordinator = OpenGpuiGraphFocusCoordinator {
                previous: vec![entry(detail, &old_handle)],
                ..OpenGpuiGraphFocusCoordinator::default()
            };
            let current = vec![
                entry(overview, &overview_handle),
                entry(detail, &later_detail_handle),
            ];
            let current_index = CurrentFocusIndex::new(&current);

            let removed = coordinator
                .disappeared_focus_index_by(
                    &current_index,
                    |_| false,
                    |handle| handle == &old_handle,
                )
                .expect("focused descendant lost its detail host");
            assert_eq!(
                coordinator
                    .fallback_for(removed, &current_index)
                    .map(|entry| entry.handle.clone()),
                Some(overview_handle),
                "overview, including PinnedDetail, follows graph order"
            );

            let detail_only = vec![entry(detail, &later_detail_handle)];
            let detail_index = CurrentFocusIndex::new(&detail_only);
            assert_eq!(
                coordinator
                    .fallback_for(removed, &detail_index)
                    .map(|entry| entry.handle.clone()),
                Some(later_detail_handle),
                "overview-to-detail replacement resolves through the same logical node index"
            );
        });
    }

    #[open_gpui::test]
    fn reused_node_id_does_not_receive_focus_from_a_retired_lifetime(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let retired = focus_identities().nodes[0];
            let successor = focus_identities().nodes[0];
            assert_eq!(retired.node(), successor.node());
            assert_ne!(retired, successor);
            let old_handle = cx.focus_handle();
            let successor_handle = cx.focus_handle();
            let old_target = node_target(retired, OpenGpuiNodePresentation::Detail);
            let successor_target = node_target(successor, OpenGpuiNodePresentation::Overview);
            let coordinator = OpenGpuiGraphFocusCoordinator {
                previous: vec![entry(old_target, &old_handle)],
                ..OpenGpuiGraphFocusCoordinator::default()
            };
            window.focus(&old_handle, cx);
            let current = vec![entry(successor_target, &successor_handle)];
            let current_index = CurrentFocusIndex::new(&current);

            let removed = coordinator
                .disappeared_focus_index(&current_index, window, cx)
                .expect("retired node focus disappeared");
            assert!(coordinator.fallback_for(removed, &current_index).is_none());
        });
    }

    #[open_gpui::test]
    fn disappearing_port_and_edge_keep_previous_graph_order_fallback_semantics(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|_, cx| {
            let identities = focus_identities();
            let node_a = identities.nodes[0];
            let node_b = identities.nodes[1];
            let port_a = OpenGpuiGraphFocusTarget::Port(identities.ports[0]);
            let port_b = OpenGpuiGraphFocusTarget::Port(identities.ports[1]);
            let edge = OpenGpuiGraphFocusTarget::Edge(identities.edge);
            let node_a = node_target(node_a, OpenGpuiNodePresentation::Detail);
            let node_b = node_target(node_b, OpenGpuiNodePresentation::Detail);
            let node_a_handle = cx.focus_handle();
            let port_a_handle = cx.focus_handle();
            let edge_handle = cx.focus_handle();
            let node_b_handle = cx.focus_handle();
            let port_b_handle = cx.focus_handle();
            let coordinator = OpenGpuiGraphFocusCoordinator {
                previous: vec![
                    entry(node_a, &node_a_handle),
                    entry(port_a, &port_a_handle),
                    entry(edge, &edge_handle),
                    entry(node_b, &node_b_handle),
                    entry(port_b, &port_b_handle),
                ],
                ..OpenGpuiGraphFocusCoordinator::default()
            };

            let without_edge = vec![
                entry(port_b, &port_b_handle),
                entry(node_a, &node_a_handle),
                entry(port_a, &port_a_handle),
                entry(node_b, &node_b_handle),
            ];
            let without_edge_index = CurrentFocusIndex::new(&without_edge);
            assert_eq!(
                coordinator
                    .fallback_for(2, &without_edge_index)
                    .map(|entry| entry.target),
                Some(node_b),
                "current render order must not replace previous-order keyboard fallback"
            );

            let without_trailing_port = vec![
                entry(node_a, &node_a_handle),
                entry(port_a, &port_a_handle),
                entry(node_b, &node_b_handle),
            ];
            let without_trailing_port_index = CurrentFocusIndex::new(&without_trailing_port);
            assert_eq!(
                coordinator
                    .fallback_for(4, &without_trailing_port_index)
                    .map(|entry| entry.target),
                Some(node_b),
                "a trailing removed port falls back to the nearest surviving predecessor"
            );
        });
    }

    #[open_gpui::test]
    fn return_target_resolves_exactly_and_empty_current_focuses_root(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let target = OpenGpuiGraphFocusTarget::Edge(focus_identities().edge);
            let root = cx.focus_handle();
            let target_handle = cx.focus_handle();
            let mut coordinator = OpenGpuiGraphFocusCoordinator::default();
            coordinator.remember_return_target(target);
            window.focus(&root, cx);

            coordinator.reconcile(vec![entry(target, &target_handle)], &root, true, window, cx);
            assert_eq!(window.focused(cx), Some(target_handle));

            coordinator.reconcile(Vec::new(), &root, false, window, cx);
            assert_eq!(window.focused(cx), Some(root));
        });
    }

    #[open_gpui::test]
    fn retained_return_and_actions_do_not_upgrade_to_same_id_successors(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_, cx) = cx.add_window_view(|_, _| FocusTestView);
        cx.update(|window, cx| {
            let retired = focus_identities();
            let successor = focus_identities();
            assert_eq!(retired.ports[0].port(), successor.ports[0].port());
            assert_eq!(retired.edge.edge(), successor.edge.edge());
            assert_ne!(retired.ports[0], successor.ports[0]);
            assert_ne!(retired.edge, successor.edge);
            for (retired_target, successor_target) in [
                (
                    OpenGpuiGraphFocusTarget::Port(retired.ports[0]),
                    OpenGpuiGraphFocusTarget::Port(successor.ports[0]),
                ),
                (
                    OpenGpuiGraphFocusTarget::Edge(retired.edge),
                    OpenGpuiGraphFocusTarget::Edge(successor.edge),
                ),
                (
                    OpenGpuiGraphFocusTarget::ReconnectSource(retired.edge),
                    OpenGpuiGraphFocusTarget::ReconnectSource(successor.edge),
                ),
                (
                    OpenGpuiGraphFocusTarget::ReconnectTarget(retired.edge),
                    OpenGpuiGraphFocusTarget::ReconnectTarget(successor.edge),
                ),
            ] {
                let root = cx.focus_handle();
                let retired_handle = cx.focus_handle();
                let successor_handle = cx.focus_handle();
                let mut coordinator = OpenGpuiGraphFocusCoordinator::default();
                if retired_target.is_retained_action() {
                    coordinator.actions.insert(retired_target, retired_handle);
                }
                coordinator.remember_return_target(retired_target);
                window.focus(&root, cx);

                coordinator.reconcile(
                    vec![entry(successor_target, &successor_handle)],
                    &root,
                    true,
                    window,
                    cx,
                );

                assert_eq!(window.focused(cx), Some(root));
                assert!(!coordinator.actions.contains_key(&retired_target));
                assert!(!coordinator.actions.contains_key(&successor_target));
            }
        });
    }
}
