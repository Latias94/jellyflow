use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GraphId, GroupId, NodeId};

use crate::io::{
    ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed, edge_is_visible,
    retain_first,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewFacts {
    pub(crate) pan: CanvasPoint,
    pub(crate) zoom: f32,
    pub(crate) selected_nodes: Vec<NodeId>,
    pub(crate) selected_edges: Vec<EdgeId>,
    pub(crate) selected_groups: Vec<GroupId>,
    pub(crate) draw_order: Vec<NodeId>,
    pub(crate) edge_draw_order: Vec<EdgeId>,
    pub(crate) group_draw_order: Vec<GroupId>,
}

impl ViewFacts {
    pub(crate) fn from_seed(seed: ViewSessionSeed) -> Self {
        let (_, viewport, selection, stacking) = seed.into_parts();
        Self {
            pan: viewport.pan(),
            zoom: viewport.zoom(),
            selected_nodes: selection.nodes().to_vec(),
            selected_edges: selection.edges().to_vec(),
            selected_groups: selection.groups().to_vec(),
            draw_order: stacking.nodes().to_vec(),
            edge_draw_order: stacking.edges().to_vec(),
            group_draw_order: stacking.groups().to_vec(),
        }
    }

    pub(crate) fn into_seed(self, graph_id: GraphId) -> ViewSessionSeed {
        ViewSessionSeed::from_parts(
            graph_id,
            ViewportSeed::new(self.pan, self.zoom),
            ViewSelectionSeed::new(
                self.selected_nodes,
                self.selected_edges,
                self.selected_groups,
            ),
            ViewStackingSeed::new(self.draw_order, self.edge_draw_order, self.group_draw_order),
        )
    }

    pub(crate) fn to_seed(&self, graph_id: GraphId) -> ViewSessionSeed {
        self.clone().into_seed(graph_id)
    }

    pub(crate) fn set_viewport(&mut self, pan: CanvasPoint, zoom: f32) {
        let viewport = ViewportSeed::new(pan, zoom);
        self.pan = viewport.pan();
        self.zoom = viewport.zoom();
    }

    pub(crate) fn set_selection(
        &mut self,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) {
        self.selected_nodes = nodes;
        self.selected_edges = edges;
        self.selected_groups = groups;
    }

    pub(crate) fn selection(&self) -> ViewSelectionSeed {
        ViewSelectionSeed::new(
            self.selected_nodes.clone(),
            self.selected_edges.clone(),
            self.selected_groups.clone(),
        )
    }

    pub(crate) fn sanitize_for_graph(&mut self, graph: &Graph) {
        let mut seed = self.to_seed(graph.graph_id());
        seed.sanitize_for_graph(graph)
            .expect("view facts are always bound to the current graph");
        *self = Self::from_seed(seed);
    }

    pub(crate) fn sanitize_selection_for_graph(&mut self, graph: &Graph) {
        let visible_node = |id: &NodeId| graph.node(id).is_some_and(|node| !node.hidden);
        let visible_edge = |id: &EdgeId| edge_is_visible(graph, *id);

        self.selected_nodes.retain(visible_node);
        self.selected_edges.retain(visible_edge);
        self.selected_groups.retain(|id| graph.group(id).is_some());
        retain_first(&mut self.selected_nodes);
        retain_first(&mut self.selected_edges);
        retain_first(&mut self.selected_groups);
    }
}
