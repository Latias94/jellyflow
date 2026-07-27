use std::collections::BTreeSet;

use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GraphId, GroupId, NodeId};

use crate::io::{
    ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed, edge_is_visible,
    retain_first,
};
use crate::runtime::graph_runtime::{EdgeInstanceKey, GraphSnapshot, NodeLifetimeKey};
use crate::runtime::viewport::ViewportTransform;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewFacts {
    pub(crate) pan: CanvasPoint,
    pub(crate) zoom: f32,
    pub(crate) selected_nodes: Vec<NodeLifetimeKey>,
    pub(crate) selected_edges: Vec<EdgeInstanceKey>,
    pub(crate) selected_groups: Vec<GroupId>,
    pub(crate) draw_order: Vec<NodeId>,
    pub(crate) edge_draw_order: Vec<EdgeId>,
    pub(crate) group_draw_order: Vec<GroupId>,
}

impl ViewFacts {
    pub(crate) fn from_seed(seed: ViewSessionSeed, snapshot: &GraphSnapshot) -> Self {
        let (_, viewport, selection, stacking) = seed.into_parts();
        let mut facts = Self {
            pan: viewport.pan(),
            zoom: viewport.zoom(),
            selected_nodes: selection
                .nodes()
                .iter()
                .filter_map(|node| snapshot.node_lifetime_key(*node))
                .collect(),
            selected_edges: selection
                .edges()
                .iter()
                .filter_map(|edge| snapshot.edge_instance_key(*edge))
                .collect(),
            selected_groups: selection.groups().to_vec(),
            draw_order: stacking.nodes().to_vec(),
            edge_draw_order: stacking.edges().to_vec(),
            group_draw_order: stacking.groups().to_vec(),
        };
        facts.reconcile_selection(snapshot, &BTreeSet::new(), false);
        facts
    }

    pub(crate) fn into_seed(self, graph_id: GraphId) -> ViewSessionSeed {
        ViewSessionSeed::from_parts(
            graph_id,
            ViewportSeed::new(self.pan, self.zoom),
            ViewSelectionSeed::new(
                self.selected_nodes
                    .into_iter()
                    .map(NodeLifetimeKey::node)
                    .collect(),
                self.selected_edges
                    .into_iter()
                    .map(EdgeInstanceKey::edge)
                    .collect(),
                self.selected_groups,
            ),
            ViewStackingSeed::new(self.draw_order, self.edge_draw_order, self.group_draw_order),
        )
    }

    pub(crate) fn set_viewport(&mut self, transform: ViewportTransform) {
        self.pan = transform.pan;
        self.zoom = transform.zoom;
    }

    pub(crate) fn set_live_selection(
        &mut self,
        snapshot: &GraphSnapshot,
        nodes: Vec<NodeLifetimeKey>,
        edges: Vec<EdgeInstanceKey>,
        groups: Vec<GroupId>,
    ) {
        self.selected_nodes = nodes;
        self.selected_edges = edges;
        self.selected_groups = groups;
        self.reconcile_selection(snapshot, &BTreeSet::new(), false);
    }

    pub(crate) fn selection(&self) -> ViewSelectionSeed {
        ViewSelectionSeed::new(
            self.selected_nodes.iter().map(|node| node.node()).collect(),
            self.selected_edges.iter().map(|edge| edge.edge()).collect(),
            self.selected_groups.clone(),
        )
    }

    pub(crate) fn reconcile_for_snapshot(
        &mut self,
        snapshot: &GraphSnapshot,
        retired_groups: &BTreeSet<GroupId>,
        clear_groups: bool,
    ) {
        self.reconcile_selection(snapshot, retired_groups, clear_groups);
        self.sanitize_draw_order(snapshot.graph());
    }

    pub(crate) fn reconcile_selection(
        &mut self,
        snapshot: &GraphSnapshot,
        retired_groups: &BTreeSet<GroupId>,
        clear_groups: bool,
    ) {
        let graph = snapshot.graph();
        self.selected_nodes.retain(|key| {
            snapshot.node_lifetime_is_current(*key)
                && graph.node(&key.node()).is_some_and(|node| !node.hidden)
        });
        self.selected_edges.retain(|key| {
            snapshot.edge_instance_is_current(*key) && edge_is_visible(graph, key.edge())
        });
        if clear_groups {
            self.selected_groups.clear();
        } else {
            self.selected_groups
                .retain(|group| !retired_groups.contains(group) && graph.group(group).is_some());
        }
        retain_first(&mut self.selected_nodes);
        retain_first(&mut self.selected_edges);
        retain_first(&mut self.selected_groups);
    }

    fn sanitize_draw_order(&mut self, graph: &Graph) {
        self.draw_order
            .retain(|id| graph.node(id).is_some_and(|node| !node.hidden));
        self.edge_draw_order
            .retain(|id| edge_is_visible(graph, *id));
        self.group_draw_order.retain(|id| graph.group(id).is_some());
        retain_first(&mut self.draw_order);
        retain_first(&mut self.edge_draw_order);
        retain_first(&mut self.group_draw_order);
    }
}
