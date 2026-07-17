use jellyflow_core::core::{CanvasPoint, EdgeId, Graph, GraphId, GroupId, NodeId};

use super::{edge_is_visible, retain_first};

const DEFAULT_ZOOM: f32 = 1.0;

/// Persisted viewport facts that may initialize a new view session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSeed {
    pan: CanvasPoint,
    zoom: f32,
}

impl ViewportSeed {
    /// Creates a viewport seed, replacing non-finite pan or non-positive zoom with defaults.
    pub fn new(pan: CanvasPoint, zoom: f32) -> Self {
        Self {
            pan: sanitize_pan(pan),
            zoom: sanitize_zoom(zoom),
        }
    }

    /// Returns the canvas-space pan.
    pub fn pan(self) -> CanvasPoint {
        self.pan
    }

    /// Returns the positive zoom factor.
    pub fn zoom(self) -> f32 {
        self.zoom
    }
}

impl Default for ViewportSeed {
    fn default() -> Self {
        Self::new(CanvasPoint::default(), DEFAULT_ZOOM)
    }
}

/// Persisted selection facts that may initialize a new view session.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ViewSelectionSeed {
    nodes: Vec<NodeId>,
    edges: Vec<EdgeId>,
    groups: Vec<GroupId>,
}

impl ViewSelectionSeed {
    /// Creates a selection seed. Graph-aware sanitization occurs when the seed is attached.
    pub fn new(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self {
            nodes,
            edges,
            groups,
        }
    }

    /// Returns selected node IDs in stable selection order.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Returns selected edge IDs in stable selection order.
    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Returns selected group IDs in stable selection order.
    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }
}

/// Persisted stacking facts that may initialize a new view session.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ViewStackingSeed {
    nodes: Vec<NodeId>,
    edges: Vec<EdgeId>,
    groups: Vec<GroupId>,
}

impl ViewStackingSeed {
    /// Creates a stacking seed. Graph-aware sanitization occurs when the seed is attached.
    pub fn new(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self {
            nodes,
            edges,
            groups,
        }
    }

    /// Returns node IDs in back-to-front order.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Returns edge IDs in back-to-front order.
    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Returns group IDs in back-to-front order.
    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }
}

/// Application-owned state that may initialize one runtime view session.
///
/// This type deliberately excludes geometry, routes, gestures, focus, input-method state, drafts,
/// theme data, caches, runtime IDs, revisions, and history. Persistence uses a separate frozen wire
/// DTO; this runtime seed does not implement Serde.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewSessionSeed {
    graph_id: GraphId,
    viewport: ViewportSeed,
    selection: ViewSelectionSeed,
    stacking: ViewStackingSeed,
}

impl ViewSessionSeed {
    /// Creates a seed with the default viewport and no selected or explicitly stacked elements.
    pub fn new(graph_id: GraphId) -> Self {
        Self::from_parts(
            graph_id,
            ViewportSeed::default(),
            ViewSelectionSeed::default(),
            ViewStackingSeed::default(),
        )
    }

    /// Creates a seed from its explicitly supported persisted facts.
    pub fn from_parts(
        graph_id: GraphId,
        viewport: ViewportSeed,
        selection: ViewSelectionSeed,
        stacking: ViewStackingSeed,
    ) -> Self {
        Self {
            graph_id,
            viewport,
            selection,
            stacking,
        }
    }

    /// Replaces the viewport facts.
    pub fn with_viewport(mut self, viewport: ViewportSeed) -> Self {
        self.viewport = viewport;
        self
    }

    /// Replaces the selection facts.
    pub fn with_selection(mut self, selection: ViewSelectionSeed) -> Self {
        self.selection = selection;
        self
    }

    /// Replaces the stacking facts.
    pub fn with_stacking(mut self, stacking: ViewStackingSeed) -> Self {
        self.stacking = stacking;
        self
    }

    /// Returns the graph document this seed belongs to.
    pub fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the persisted viewport seed.
    pub fn viewport(&self) -> ViewportSeed {
        self.viewport
    }

    /// Returns the persisted selection seed.
    pub fn selection(&self) -> &ViewSelectionSeed {
        &self.selection
    }

    /// Returns the persisted stacking seed.
    pub fn stacking(&self) -> &ViewStackingSeed {
        &self.stacking
    }

    /// Consumes the seed and returns its explicitly supported persisted facts.
    pub fn into_parts(self) -> (GraphId, ViewportSeed, ViewSelectionSeed, ViewStackingSeed) {
        (self.graph_id, self.viewport, self.selection, self.stacking)
    }

    /// Removes IDs that are stale, hidden, or duplicated for the target graph.
    ///
    /// The first occurrence of every retained ID wins so application-defined order remains stable.
    pub fn sanitize_for_graph(
        &mut self,
        graph: &Graph,
    ) -> Result<(), ViewSessionSeedGraphMismatch> {
        if self.graph_id != graph.graph_id() {
            return Err(ViewSessionSeedGraphMismatch {
                seed_graph_id: self.graph_id,
                graph_id: graph.graph_id(),
            });
        }

        let visible_node = |id: &NodeId| graph.node(id).is_some_and(|node| !node.hidden);
        let visible_edge = |id: &EdgeId| edge_is_visible(graph, *id);
        self.selection.nodes.retain(visible_node);
        self.selection.edges.retain(visible_edge);
        self.selection.groups.retain(|id| graph.group(id).is_some());
        self.stacking.nodes.retain(visible_node);
        self.stacking.edges.retain(visible_edge);
        self.stacking.groups.retain(|id| graph.group(id).is_some());

        retain_first(&mut self.selection.nodes);
        retain_first(&mut self.selection.edges);
        retain_first(&mut self.selection.groups);
        retain_first(&mut self.stacking.nodes);
        retain_first(&mut self.stacking.edges);
        retain_first(&mut self.stacking.groups);
        Ok(())
    }
}

/// A view-session seed was applied to a different graph document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("view-session seed graph {seed_graph_id} does not match graph {graph_id}")]
pub struct ViewSessionSeedGraphMismatch {
    /// Graph recorded by the seed.
    pub seed_graph_id: GraphId,
    /// Graph supplied for sanitization.
    pub graph_id: GraphId,
}

fn sanitize_pan(pan: CanvasPoint) -> CanvasPoint {
    if pan.is_finite() {
        pan
    } else {
        CanvasPoint::default()
    }
}

fn sanitize_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() && zoom > 0.0 {
        zoom
    } else {
        DEFAULT_ZOOM
    }
}
