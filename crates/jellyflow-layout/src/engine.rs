use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphOp, GraphTransaction, NodeId,
};
use serde::{Deserialize, Serialize};

use crate::family::{LayoutEngineMetadata, LayoutFamilyId, LayoutFamilyMetadata};

/// Stable engine id for the built-in Dagre-compatible `dugong` engine.
pub const DUGONG_LAYOUT_ENGINE_ID: &str = "dugong";
/// Stable engine id for the built-in tidy tree engine.
pub const TIDY_TREE_LAYOUT_ENGINE_ID: &str = "tidy_tree";
/// Stable engine id for the built-in radial mind-map engine.
pub const MIND_MAP_RADIAL_LAYOUT_ENGINE_ID: &str = "mind_map_radial";
/// Stable engine id for the built-in freeform mind-map engine.
pub const MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID: &str = "mind_map_freeform";

/// Stable identifier for a layout engine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayoutEngineId(String);

impl LayoutEngineId {
    /// Creates a new engine id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the built-in `dugong` engine id.
    pub fn dugong() -> Self {
        Self::new(DUGONG_LAYOUT_ENGINE_ID)
    }

    /// Returns the built-in tidy tree engine id.
    pub fn tidy_tree() -> Self {
        Self::new(TIDY_TREE_LAYOUT_ENGINE_ID)
    }

    /// Returns the built-in radial mind-map engine id.
    pub fn mind_map_radial() -> Self {
        Self::new(MIND_MAP_RADIAL_LAYOUT_ENGINE_ID)
    }

    /// Returns the built-in freeform mind-map engine id.
    pub fn mind_map_freeform() -> Self {
        Self::new(MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID)
    }

    /// Returns this id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LayoutEngineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for LayoutEngineId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LayoutEngineId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Direction for a layered graph layout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutDirection {
    /// Top to bottom.
    #[default]
    TopToBottom,
    /// Bottom to top.
    BottomToTop,
    /// Left to right.
    LeftToRight,
    /// Right to left.
    RightToLeft,
}

/// Spacing knobs passed through to layout engines that support layered spacing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutSpacing {
    pub nodesep: f32,
    pub ranksep: f32,
    pub edgesep: f32,
}

impl Default for LayoutSpacing {
    fn default() -> Self {
        Self {
            nodesep: 50.0,
            ranksep: 50.0,
            edgesep: 20.0,
        }
    }
}

/// Options shared by Jellyflow layout adapters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    pub spacing: LayoutSpacing,
    pub margin: CanvasSize,
    pub default_node_size: CanvasSize,
    /// Fallback node origin used when a node has no per-node origin override.
    pub node_origin: (f32, f32),
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::TopToBottom,
            spacing: LayoutSpacing::default(),
            margin: CanvasSize {
                width: 0.0,
                height: 0.0,
            },
            default_node_size: CanvasSize {
                width: 172.0,
                height: 36.0,
            },
            node_origin: (0.0, 0.0),
        }
    }
}

impl LayoutOptions {
    /// Uses a different fallback node size for nodes without explicit or measured size.
    pub fn with_default_node_size(mut self, size: CanvasSize) -> Self {
        self.default_node_size = size;
        self
    }

    /// Uses a different layered layout direction.
    pub fn with_direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Uses a different fallback node origin.
    pub fn with_node_origin(mut self, node_origin: (f32, f32)) -> Self {
        self.node_origin = node_origin;
        self
    }
}

/// Which nodes a layout request should include.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutScope {
    /// Include all non-hidden nodes.
    #[default]
    All,
    /// Include only these nodes. Hidden nodes are still ignored.
    Nodes { nodes: BTreeSet<NodeId> },
}

impl LayoutScope {
    pub(crate) fn contains(&self, node: NodeId) -> bool {
        match self {
            Self::All => true,
            Self::Nodes { nodes } => nodes.contains(&node),
        }
    }
}

/// A headless layout request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutRequest {
    pub options: LayoutOptions,
    #[serde(default)]
    pub scope: LayoutScope,
    /// Adapter-reported node sizes. Graph node sizes win over these facts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measured_node_sizes: BTreeMap<NodeId, CanvasSize>,
}

impl Default for LayoutRequest {
    fn default() -> Self {
        Self {
            options: LayoutOptions::default(),
            scope: LayoutScope::All,
            measured_node_sizes: BTreeMap::new(),
        }
    }
}

impl LayoutRequest {
    /// Creates a request for all visible nodes.
    pub fn all() -> Self {
        Self::default()
    }

    /// Creates a request for a selected set of nodes.
    pub fn nodes(nodes: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            scope: LayoutScope::Nodes {
                nodes: nodes.into_iter().collect(),
            },
            ..Self::default()
        }
    }

    /// Adds adapter-reported node sizes.
    pub fn with_measured_node_sizes(
        mut self,
        sizes: impl IntoIterator<Item = (NodeId, CanvasSize)>,
    ) -> Self {
        self.measured_node_sizes.extend(sizes);
        self
    }

    /// Sets layout options.
    pub fn with_options(mut self, options: LayoutOptions) -> Self {
        self.options = options;
        self
    }
}

/// Request for one selected layout engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEngineRequest {
    pub engine: LayoutEngineId,
    pub layout: LayoutRequest,
}

impl Default for LayoutEngineRequest {
    fn default() -> Self {
        Self {
            engine: LayoutEngineId::dugong(),
            layout: LayoutRequest::default(),
        }
    }
}

impl LayoutEngineRequest {
    /// Creates an engine request.
    pub fn new(engine: impl Into<LayoutEngineId>, layout: LayoutRequest) -> Self {
        Self {
            engine: engine.into(),
            layout,
        }
    }

    /// Creates a request for the built-in `dugong` engine.
    pub fn dugong(layout: LayoutRequest) -> Self {
        Self::new(LayoutEngineId::dugong(), layout)
    }

    /// Creates a request for the built-in tidy tree engine.
    pub fn tidy_tree(layout: LayoutRequest) -> Self {
        Self::new(LayoutEngineId::tidy_tree(), layout)
    }

    /// Creates a request for the built-in radial mind-map engine.
    pub fn mind_map_radial(layout: LayoutRequest) -> Self {
        Self::new(LayoutEngineId::mind_map_radial(), layout)
    }

    /// Creates a request for the built-in freeform mind-map engine.
    pub fn mind_map_freeform(layout: LayoutRequest) -> Self {
        Self::new(LayoutEngineId::mind_map_freeform(), layout)
    }

    /// Uses a different engine id.
    pub fn with_engine(mut self, engine: impl Into<LayoutEngineId>) -> Self {
        self.engine = engine.into();
        self
    }

    /// Sets the layout request.
    pub fn with_layout(mut self, layout: LayoutRequest) -> Self {
        self.layout = layout;
        self
    }
}

/// Runtime or host-owned context available to layout engines.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutContext {
    /// Runtime-reported node sizes. Graph node sizes and request-local measured sizes win.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measured_node_sizes: BTreeMap<NodeId, CanvasSize>,
    /// Nodes that engines should treat as fixed when they support pinning.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pinned_nodes: BTreeSet<NodeId>,
    /// Runtime fallback node origin, usually resolved from interaction state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_origin: Option<(f32, f32)>,
}

impl LayoutContext {
    /// Creates an empty layout context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds runtime-reported node sizes.
    pub fn with_measured_node_sizes(
        mut self,
        sizes: impl IntoIterator<Item = (NodeId, CanvasSize)>,
    ) -> Self {
        self.measured_node_sizes.extend(sizes);
        self
    }

    /// Adds nodes that engines should keep fixed when possible.
    pub fn with_pinned_nodes(mut self, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        self.pinned_nodes.extend(nodes);
        self
    }

    /// Sets the runtime fallback node origin.
    pub fn with_node_origin(mut self, node_origin: (f32, f32)) -> Self {
        self.node_origin = Some(node_origin);
        self
    }
}

/// A node position produced by a layout run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutNodePosition {
    pub node: NodeId,
    pub pos: CanvasPoint,
    pub center: CanvasPoint,
    pub size: CanvasSize,
}

/// A layout-produced edge route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEdgeRoute {
    pub edge: EdgeId,
    pub points: Vec<CanvasPoint>,
}

/// Result of a headless layout run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutResult {
    /// Engine-produced results contain at most one entry per node. If a caller manually constructs a
    /// result with duplicates, lookup returns the first entry and transaction conversion fails.
    pub nodes: Vec<LayoutNodePosition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_routes: Vec<LayoutEdgeRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<CanvasRect>,
}

impl LayoutResult {
    /// Finds one node position in this result.
    pub fn node_position(&self, node: NodeId) -> Option<LayoutNodePosition> {
        self.nodes
            .iter()
            .find(|position| position.node == node)
            .copied()
    }

    /// Converts node position changes into a Jellyflow transaction.
    pub fn to_transaction(&self, graph: &Graph) -> Result<GraphTransaction, LayoutError> {
        let mut seen = BTreeSet::new();
        let mut ops = Vec::new();

        for node in &self.nodes {
            if !seen.insert(node.node) {
                return Err(LayoutError::DuplicateResultNode(node.node));
            }

            let from = graph
                .nodes
                .get(&node.node)
                .ok_or(LayoutError::MissingTransactionNode(node.node))?
                .pos;
            if from != node.pos {
                ops.push(GraphOp::SetNodePos {
                    id: node.node,
                    from,
                    to: node.pos,
                });
            }
        }

        Ok(GraphTransaction::from_ops(ops).with_label("Layout graph"))
    }
}

/// Errors reported by layout projection, registry lookup, or layout output conversion.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutError {
    #[error("layout engine id is already registered: {0}")]
    DuplicateLayoutEngine(LayoutEngineId),
    #[error("layout family id is already registered: {0}")]
    DuplicateLayoutFamily(LayoutFamilyId),
    #[error("layout engine metadata is already registered: {0}")]
    DuplicateLayoutEngineMetadata(LayoutEngineId),
    #[error("layout engine is not registered: {0}")]
    MissingLayoutEngine(LayoutEngineId),
    #[error("layout default node size must be positive and finite: {0:?}")]
    InvalidDefaultNodeSize(CanvasSize),
    #[error("layout spacing values must be non-negative and finite: {0:?}")]
    InvalidSpacing(LayoutSpacing),
    #[error("layout margin must be non-negative and finite: {0:?}")]
    InvalidMargin(CanvasSize),
    #[error("layout node size must be positive and finite for node {node:?}: {size:?}")]
    InvalidNodeSize { node: NodeId, size: CanvasSize },
    #[error("layout scope references missing node: {0:?}")]
    MissingScopeNode(NodeId),
    #[error("layout edge references missing source port: {0:?}")]
    MissingSourcePort(EdgeId),
    #[error("layout edge references missing target port: {0:?}")]
    MissingTargetPort(EdgeId),
    #[error("layout edge source port references missing node: {edge:?}")]
    MissingSourceNode { edge: EdgeId },
    #[error("layout edge target port references missing node: {edge:?}")]
    MissingTargetNode { edge: EdgeId },
    #[error("layout engine did not return a node position for node {0:?}")]
    MissingNodePosition(NodeId),
    #[error("layout result contains a duplicate node position for node {0:?}")]
    DuplicateResultNode(NodeId),
    #[error("layout result references missing graph node: {0:?}")]
    MissingTransactionNode(NodeId),
    #[error("layout engine returned a non-finite node position for node {node:?}: ({x}, {y})")]
    NonFiniteNodePosition { node: NodeId, x: f64, y: f64 },
}

/// Headless layout engine contract.
pub trait LayoutEngine: Send + Sync {
    /// Stable engine id.
    fn id(&self) -> LayoutEngineId;

    /// Runs this layout engine.
    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError>;
}

/// Caller-owned registry of layout engines.
#[derive(Default, Clone)]
pub struct LayoutEngineRegistry {
    engines: BTreeMap<LayoutEngineId, Arc<dyn LayoutEngine>>,
    families: BTreeMap<LayoutFamilyId, LayoutFamilyMetadata>,
    metadata: BTreeMap<LayoutEngineId, LayoutEngineMetadata>,
}

impl fmt::Debug for LayoutEngineRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayoutEngineRegistry")
            .field("engines", &self.engine_ids().collect::<Vec<_>>())
            .field("families", &self.family_ids().collect::<Vec<_>>())
            .finish()
    }
}

impl LayoutEngineRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an engine.
    pub fn insert<E>(&mut self, engine: E) -> Result<(), LayoutError>
    where
        E: LayoutEngine + 'static,
    {
        self.insert_shared(Arc::new(engine))
    }

    /// Registers a shared engine object.
    pub fn insert_shared(&mut self, engine: Arc<dyn LayoutEngine>) -> Result<(), LayoutError> {
        let id = engine.id();
        if self.engines.contains_key(&id) {
            return Err(LayoutError::DuplicateLayoutEngine(id));
        }
        self.engines.insert(id, engine);
        Ok(())
    }

    /// Registers layout family metadata.
    pub fn insert_family(&mut self, family: LayoutFamilyMetadata) -> Result<(), LayoutError> {
        let id = family.id.clone();
        if self.families.contains_key(&id) {
            return Err(LayoutError::DuplicateLayoutFamily(id));
        }
        self.families.insert(id, family);
        Ok(())
    }

    /// Registers layout engine discovery metadata.
    pub fn insert_metadata(&mut self, metadata: LayoutEngineMetadata) -> Result<(), LayoutError> {
        let id = metadata.engine.clone();
        if self.metadata.contains_key(&id) {
            return Err(LayoutError::DuplicateLayoutEngineMetadata(id));
        }
        self.metadata.insert(id, metadata);
        Ok(())
    }

    /// Returns an engine by id.
    pub fn get(&self, id: &LayoutEngineId) -> Option<&dyn LayoutEngine> {
        self.engines.get(id).map(Arc::as_ref)
    }

    /// Returns family metadata by id.
    pub fn family(&self, id: &LayoutFamilyId) -> Option<&LayoutFamilyMetadata> {
        self.families.get(id)
    }

    /// Returns engine discovery metadata by engine id.
    pub fn metadata(&self, id: &LayoutEngineId) -> Option<&LayoutEngineMetadata> {
        self.metadata.get(id)
    }

    /// Returns registered engine ids in deterministic order.
    pub fn engine_ids(&self) -> impl Iterator<Item = &LayoutEngineId> {
        self.engines.keys()
    }

    /// Returns registered family ids in deterministic order.
    pub fn family_ids(&self) -> impl Iterator<Item = &LayoutFamilyId> {
        self.families.keys()
    }

    /// Returns registered families in deterministic order.
    pub fn families(&self) -> impl Iterator<Item = &LayoutFamilyMetadata> {
        self.families.values()
    }

    /// Returns registered engine metadata in deterministic order.
    pub fn engine_metadata(&self) -> impl Iterator<Item = &LayoutEngineMetadata> {
        self.metadata.values()
    }

    /// Returns engine metadata for one family in deterministic engine-id order.
    pub fn engines_for_family(
        &self,
        family: &LayoutFamilyId,
    ) -> impl Iterator<Item = &LayoutEngineMetadata> {
        self.metadata
            .values()
            .filter(move |metadata| &metadata.family == family)
    }

    /// Runs the engine named by the request.
    pub fn layout(
        &self,
        graph: &Graph,
        request: &LayoutEngineRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let engine = self
            .get(&request.engine)
            .ok_or_else(|| LayoutError::MissingLayoutEngine(request.engine.clone()))?;
        engine.layout(graph, &request.layout, context)
    }
}

/// Runs a layout engine by id.
pub fn layout_graph_with_engine(
    graph: &Graph,
    request: &LayoutEngineRequest,
    registry: &LayoutEngineRegistry,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    registry.layout(graph, request, context)
}

/// Runs a layout engine and converts the node positions into a Jellyflow transaction.
pub fn layout_graph_to_transaction_with_engine(
    graph: &Graph,
    request: &LayoutEngineRequest,
    registry: &LayoutEngineRegistry,
    context: &LayoutContext,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_engine(graph, request, registry, context)?.to_transaction(graph)
}

pub(crate) fn validate_request(graph: &Graph, request: &LayoutRequest) -> Result<(), LayoutError> {
    if !request.options.default_node_size.is_positive_finite() {
        return Err(LayoutError::InvalidDefaultNodeSize(
            request.options.default_node_size,
        ));
    }
    if !is_non_negative_finite_spacing(request.options.spacing) {
        return Err(LayoutError::InvalidSpacing(request.options.spacing));
    }
    if !is_non_negative_finite_size(request.options.margin) {
        return Err(LayoutError::InvalidMargin(request.options.margin));
    }

    if let LayoutScope::Nodes { nodes } = &request.scope {
        for node in nodes {
            if !graph.nodes.contains_key(node) {
                return Err(LayoutError::MissingScopeNode(*node));
            }
        }
    }

    Ok(())
}

pub(crate) fn resolve_node_size(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
    node: NodeId,
) -> Result<CanvasSize, LayoutError> {
    let size = graph
        .nodes
        .get(&node)
        .and_then(|node| node.size)
        .or_else(|| request.measured_node_sizes.get(&node).copied())
        .or_else(|| context.measured_node_sizes.get(&node).copied())
        .unwrap_or(request.options.default_node_size);

    if size.is_positive_finite() {
        Ok(size)
    } else {
        Err(LayoutError::InvalidNodeSize { node, size })
    }
}

pub(crate) fn resolve_node_origin(
    origin: Option<jellyflow_core::NodeOrigin>,
    request_fallback: (f32, f32),
    context: &LayoutContext,
) -> (f32, f32) {
    let fallback = context.node_origin.unwrap_or(request_fallback);
    let (x, y) = origin.map(|origin| origin.as_tuple()).unwrap_or(fallback);
    (normalize_origin_component(x), normalize_origin_component(y))
}

pub(crate) fn position_from_center(
    center: CanvasPoint,
    size: CanvasSize,
    origin: (f32, f32),
) -> CanvasPoint {
    CanvasPoint {
        x: center.x - size.width * (0.5 - origin.0),
        y: center.y - size.height * (0.5 - origin.1),
    }
}

pub(crate) fn node_rect_from_position(node: &LayoutNodePosition) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: node.center.x - node.size.width * 0.5,
            y: node.center.y - node.size.height * 0.5,
        },
        size: node.size,
    }
}

pub(crate) fn union_bounds(bounds: Option<CanvasRect>, next: CanvasRect) -> Option<CanvasRect> {
    if !next.is_positive_finite() {
        return bounds;
    }

    let Some(bounds) = bounds else {
        return Some(next);
    };

    let min_x = bounds.origin.x.min(next.origin.x);
    let min_y = bounds.origin.y.min(next.origin.y);
    let max_x = (bounds.origin.x + bounds.size.width).max(next.origin.x + next.size.width);
    let max_y = (bounds.origin.y + bounds.size.height).max(next.origin.y + next.size.height);

    Some(CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: max_x - min_x,
            height: max_y - min_y,
        },
    })
}

fn is_non_negative_finite_spacing(spacing: LayoutSpacing) -> bool {
    spacing.nodesep.is_finite()
        && spacing.ranksep.is_finite()
        && spacing.edgesep.is_finite()
        && spacing.nodesep >= 0.0
        && spacing.ranksep >= 0.0
        && spacing.edgesep >= 0.0
}

fn is_non_negative_finite_size(size: CanvasSize) -> bool {
    size.is_finite() && size.width >= 0.0 && size.height >= 0.0
}

fn normalize_origin_component(component: f32) -> f32 {
    if component.is_finite() {
        component.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
