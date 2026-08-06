use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use jellyflow_core::{
    BindingEndpoint, CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphMutationFootprint,
    Node, NodeId,
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
    /// Derives a conservative node scope from a graph mutation footprint and the current graph.
    ///
    /// Direct node changes are included. Port and edge changes expand to their owning endpoint
    /// nodes when that information is available in the current graph snapshot. Node ids that no
    /// longer exist in the supplied graph are omitted.
    pub fn from_footprint(graph: &Graph, footprint: &GraphMutationFootprint) -> Self {
        if footprint.graph {
            return Self::All;
        }

        let mut nodes = footprint
            .nodes
            .iter()
            .copied()
            .filter(|node| graph.nodes().contains_key(node))
            .collect::<BTreeSet<_>>();

        for port in &footprint.ports {
            if let Some(port) = graph.ports().get(port) {
                nodes.insert(port.node);
            }
        }

        for edge in &footprint.edges {
            if let Some(edge) = graph.edges().get(edge) {
                push_port_owner(graph, edge.from, &mut nodes);
                push_port_owner(graph, edge.to, &mut nodes);
            }
        }

        for binding in &footprint.bindings {
            if let Some(binding) = graph.bindings().get(binding) {
                push_binding_endpoint_nodes(graph, &binding.subject, &mut nodes);
                push_binding_endpoint_nodes(graph, &binding.target, &mut nodes);
            }
        }

        Self::Nodes { nodes }
    }

    /// Returns `true` when this scope selects no nodes.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Nodes { nodes } if nodes.is_empty())
    }

    /// Returns selected nodes when this is a node-limited scope.
    pub fn nodes(&self) -> Option<&BTreeSet<NodeId>> {
        match self {
            Self::All => None,
            Self::Nodes { nodes } => Some(nodes),
        }
    }

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

    /// Replaces this request's scope with nodes affected by a mutation footprint.
    pub fn with_dirty_scope_from_footprint(
        mut self,
        graph: &Graph,
        footprint: &GraphMutationFootprint,
    ) -> Self {
        self.scope = LayoutScope::from_footprint(graph, footprint);
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
}

/// One validated, visible node supplied to a layout engine.
#[derive(Debug, Clone, Copy)]
pub struct PreparedLayoutNode<'graph> {
    pub(crate) id: NodeId,
    pub(crate) node: &'graph Node,
    pub(crate) size: CanvasSize,
    pub(crate) origin: (f32, f32),
    pub(crate) current_center: CanvasPoint,
    pub(crate) pinned: bool,
}

impl<'graph> PreparedLayoutNode<'graph> {
    /// Returns this node's graph identity.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the underlying semantic graph node.
    pub fn node(&self) -> &'graph Node {
        self.node
    }

    /// Returns the resolved size used for this layout run.
    pub fn size(&self) -> CanvasSize {
        self.size
    }

    /// Returns the normalized graph-node origin.
    pub fn origin(&self) -> (f32, f32) {
        self.origin
    }

    /// Returns the node's current physical center in canvas space.
    pub fn current_center(&self) -> CanvasPoint {
        self.current_center
    }

    /// Returns whether this node is pinned for this layout run.
    pub fn is_pinned(&self) -> bool {
        self.pinned
    }
}

/// One validated edge whose endpoints are both present in the prepared layout input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedLayoutEdge {
    pub(crate) id: EdgeId,
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
}

impl PreparedLayoutEdge {
    /// Returns this edge's graph identity.
    pub fn id(&self) -> EdgeId {
        self.id
    }

    /// Returns the source node.
    pub fn source(&self) -> NodeId {
        self.source
    }

    /// Returns the target node.
    pub fn target(&self) -> NodeId {
        self.target
    }
}

/// Validated graph projection supplied to one layout engine invocation.
///
/// The registry constructs this value once per run. Engines cannot reinterpret scope, size
/// precedence, hidden nodes, pinning, or edge endpoint validity.
#[derive(Debug)]
pub struct PreparedLayoutInput<'graph> {
    pub(crate) options: LayoutOptions,
    pub(crate) nodes: Vec<PreparedLayoutNode<'graph>>,
    pub(crate) node_indices: BTreeMap<NodeId, usize>,
    pub(crate) edges: Vec<PreparedLayoutEdge>,
    pub(crate) outgoing: BTreeMap<NodeId, Vec<NodeId>>,
}

impl PreparedLayoutInput<'_> {
    /// Returns the options for this layout run.
    pub fn options(&self) -> LayoutOptions {
        self.options
    }

    /// Iterates visible nodes in deterministic graph order.
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = PreparedLayoutNode<'_>> + '_ {
        self.nodes.iter().copied()
    }

    /// Returns one prepared node by identity.
    pub fn node(&self, node: NodeId) -> Option<PreparedLayoutNode<'_>> {
        self.node_indices
            .get(&node)
            .and_then(|index| self.nodes.get(*index))
            .copied()
    }

    /// Returns visible edges in deterministic graph order.
    pub fn edges(&self) -> &[PreparedLayoutEdge] {
        &self.edges
    }

    /// Returns visible outgoing neighbors for one node.
    pub fn outgoing(&self, node: NodeId) -> &[NodeId] {
        self.outgoing.get(&node).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Returns whether any visible node is pinned.
    pub fn has_pinned_nodes(&self) -> bool {
        self.nodes.iter().any(|node| node.pinned)
    }
}

/// A node position produced by a layout run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutNodePosition {
    /// Node receiving this placement.
    pub node: NodeId,
    /// Canvas anchor position using the graph node's normalized origin.
    pub pos: CanvasPoint,
    /// Center of the node's physical canvas rectangle.
    pub center: CanvasPoint,
    /// Size of the node's physical canvas rectangle.
    pub size: CanvasSize,
}

/// A layout-produced edge route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEdgeRoute {
    pub edge: EdgeId,
    pub points: Vec<CanvasPoint>,
}

/// One engine-produced center placement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutNodePlacement {
    pub node: NodeId,
    pub center: CanvasPoint,
}

/// Whether the registry should normalize the result to the requested margin.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCoordinateFrame {
    /// Translate nodes and routes together so result bounds begin at the requested margin.
    #[default]
    NormalizeToMargin,
    /// Preserve the engine's canvas coordinates exactly.
    PreserveCanvasCoordinates,
}

/// Minimal output produced by a layout engine before registry finalization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutPlacements {
    pub nodes: Vec<LayoutNodePlacement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_routes: Vec<LayoutEdgeRoute>,
    #[serde(default)]
    pub coordinate_frame: LayoutCoordinateFrame,
}

impl LayoutPlacements {
    /// Creates placements normalized to the request margin.
    pub fn normalize_to_margin(nodes: impl IntoIterator<Item = LayoutNodePlacement>) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
            edge_routes: Vec::new(),
            coordinate_frame: LayoutCoordinateFrame::NormalizeToMargin,
        }
    }

    /// Creates placements that preserve their canvas coordinates.
    pub fn preserve_canvas_coordinates(
        nodes: impl IntoIterator<Item = LayoutNodePlacement>,
    ) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
            edge_routes: Vec::new(),
            coordinate_frame: LayoutCoordinateFrame::PreserveCanvasCoordinates,
        }
    }

    /// Adds engine-produced edge routes.
    pub fn with_edge_routes(mut self, routes: impl IntoIterator<Item = LayoutEdgeRoute>) -> Self {
        self.edge_routes.extend(routes);
        self
    }
}

/// Result of a headless layout run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutResult {
    /// Results returned through a [`LayoutEngineRegistry`] contain one valid entry for every
    /// visible node in the requested scope.
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
}

/// Errors reported by layout projection, registry lookup, or layout result validation.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutError {
    #[error("layout engine id is already registered: {0}")]
    DuplicateLayoutEngine(LayoutEngineId),
    #[error("layout family metadata conflicts with an existing family: {0}")]
    ConflictingLayoutFamily(LayoutFamilyId),
    #[error(
        "layout engine {engine} declares family {metadata_family}, but its descriptor carries {descriptor_family}"
    )]
    LayoutEngineFamilyMismatch {
        engine: LayoutEngineId,
        metadata_family: LayoutFamilyId,
        descriptor_family: LayoutFamilyId,
    },
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
    #[error("layout placements reference a node outside the active scope: {0:?}")]
    UnexpectedResultNode(NodeId),
    #[error("layout result contains invalid geometry for node {0:?}")]
    InvalidResultNodeGeometry(NodeId),
    #[error("layout result contains a duplicate edge route for edge {0:?}")]
    DuplicateResultEdgeRoute(EdgeId),
    #[error("layout result contains an invalid edge route for edge {0:?}")]
    InvalidResultEdgeRoute(EdgeId),
    #[error("layout result contains invalid bounds")]
    InvalidResultBounds,
    #[error("freeform layout could not resolve overlap for node {0:?}")]
    UnresolvedOverlap(NodeId),
    #[error("layout engine returned a non-finite node position for node {node:?}: ({x}, {y})")]
    NonFiniteNodePosition { node: NodeId, x: f64, y: f64 },
}

/// Headless layout engine contract.
pub trait LayoutEngine: Send + Sync {
    /// Produces center placements from one validated graph projection.
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError>;
}

/// Atomically registered layout engine and discovery metadata.
#[derive(Clone)]
pub struct LayoutEngineDescriptor {
    family: LayoutFamilyMetadata,
    metadata: LayoutEngineMetadata,
    engine: Arc<dyn LayoutEngine>,
}

impl fmt::Debug for LayoutEngineDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayoutEngineDescriptor")
            .field("family", &self.family)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl LayoutEngineDescriptor {
    /// Creates a descriptor from one concrete engine.
    pub fn new<E>(family: LayoutFamilyMetadata, metadata: LayoutEngineMetadata, engine: E) -> Self
    where
        E: LayoutEngine + 'static,
    {
        Self::from_shared(family, metadata, Arc::new(engine))
    }

    /// Creates a descriptor from one shared engine object.
    pub fn from_shared(
        family: LayoutFamilyMetadata,
        metadata: LayoutEngineMetadata,
        engine: Arc<dyn LayoutEngine>,
    ) -> Self {
        Self {
            family,
            metadata,
            engine,
        }
    }

    /// Returns this engine's stable identity.
    pub fn id(&self) -> &LayoutEngineId {
        &self.metadata.engine
    }

    /// Returns this engine's family metadata.
    pub fn family(&self) -> &LayoutFamilyMetadata {
        &self.family
    }

    /// Returns this engine's discovery metadata.
    pub fn metadata(&self) -> &LayoutEngineMetadata {
        &self.metadata
    }
}

/// Caller-owned registry of layout engines.
#[derive(Default, Clone)]
pub struct LayoutEngineRegistry {
    engines: BTreeMap<LayoutEngineId, LayoutEngineDescriptor>,
    families: BTreeMap<LayoutFamilyId, LayoutFamilyMetadata>,
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

    /// Atomically registers an engine together with its family and discovery metadata.
    pub fn register(&mut self, descriptor: LayoutEngineDescriptor) -> Result<(), LayoutError> {
        let id = descriptor.id().clone();
        if self.engines.contains_key(&id) {
            return Err(LayoutError::DuplicateLayoutEngine(id));
        }
        if descriptor.metadata.family != descriptor.family.id {
            return Err(LayoutError::LayoutEngineFamilyMismatch {
                engine: id,
                metadata_family: descriptor.metadata.family.clone(),
                descriptor_family: descriptor.family.id.clone(),
            });
        }
        if let Some(existing) = self.families.get(&descriptor.family.id)
            && existing != &descriptor.family
        {
            return Err(LayoutError::ConflictingLayoutFamily(
                descriptor.family.id.clone(),
            ));
        }

        self.families
            .entry(descriptor.family.id.clone())
            .or_insert_with(|| descriptor.family.clone());
        self.engines.insert(id, descriptor);
        Ok(())
    }

    /// Returns whether one engine is registered.
    pub fn contains(&self, id: &LayoutEngineId) -> bool {
        self.engines.contains_key(id)
    }

    /// Returns family metadata by id.
    pub fn family(&self, id: &LayoutFamilyId) -> Option<&LayoutFamilyMetadata> {
        self.families.get(id)
    }

    /// Returns engine discovery metadata by engine id.
    pub fn metadata(&self, id: &LayoutEngineId) -> Option<&LayoutEngineMetadata> {
        self.engines.get(id).map(LayoutEngineDescriptor::metadata)
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
        self.engines.values().map(LayoutEngineDescriptor::metadata)
    }

    /// Returns engine metadata for one family in deterministic engine-id order.
    pub fn engines_for_family(
        &self,
        family: &LayoutFamilyId,
    ) -> impl Iterator<Item = &LayoutEngineMetadata> {
        self.engines
            .values()
            .map(LayoutEngineDescriptor::metadata)
            .filter(move |metadata| &metadata.family == family)
    }

    /// Runs the engine named by the request.
    pub fn layout(
        &self,
        graph: &Graph,
        request: &LayoutEngineRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let descriptor = self
            .engines
            .get(&request.engine)
            .ok_or_else(|| LayoutError::MissingLayoutEngine(request.engine.clone()))?;
        let run = crate::projection::PreparedLayoutRun::prepare(graph, &request.layout, context)?;
        let placements = descriptor.engine.place(run.input())?;
        run.finish(placements)
    }
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
            if !graph.nodes().contains_key(node) {
                return Err(LayoutError::MissingScopeNode(*node));
            }
        }
    }

    Ok(())
}

fn push_port_owner(graph: &Graph, port: jellyflow_core::PortId, nodes: &mut BTreeSet<NodeId>) {
    if let Some(port) = graph.ports().get(&port)
        && graph.nodes().contains_key(&port.node)
    {
        nodes.insert(port.node);
    }
}

fn push_edge_endpoint_nodes(graph: &Graph, edge: EdgeId, nodes: &mut BTreeSet<NodeId>) {
    if let Some(edge) = graph.edges().get(&edge) {
        push_port_owner(graph, edge.from, nodes);
        push_port_owner(graph, edge.to, nodes);
    }
}

fn push_binding_endpoint_nodes(
    graph: &Graph,
    endpoint: &BindingEndpoint,
    nodes: &mut BTreeSet<NodeId>,
) {
    let Some(target) = endpoint.graph_local_target() else {
        return;
    };

    match target {
        jellyflow_core::GraphLocalBindingTarget::Graph => {}
        jellyflow_core::GraphLocalBindingTarget::Node { id } => {
            if graph.nodes().contains_key(&id) {
                nodes.insert(id);
            }
        }
        jellyflow_core::GraphLocalBindingTarget::Port { id } => push_port_owner(graph, id, nodes),
        jellyflow_core::GraphLocalBindingTarget::Edge { id } => {
            push_edge_endpoint_nodes(graph, id, nodes);
        }
        jellyflow_core::GraphLocalBindingTarget::Group { .. }
        | jellyflow_core::GraphLocalBindingTarget::StickyNote { .. } => {}
    }
}

pub(crate) fn resolve_node_size(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
    node: NodeId,
) -> Result<CanvasSize, LayoutError> {
    let size = graph
        .nodes()
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

pub(crate) fn normalized_node_origin(origin: Option<jellyflow_core::NodeOrigin>) -> (f32, f32) {
    let (x, y) = origin.map(|origin| origin.as_tuple()).unwrap_or((0.0, 0.0));
    (normalize_origin_component(x), normalize_origin_component(y))
}

pub(crate) fn position_from_center(
    center: CanvasPoint,
    size: CanvasSize,
    origin: (f32, f32),
) -> Option<CanvasPoint> {
    checked_canvas_point(
        f64::from(center.x) - f64::from(size.width) * (0.5 - f64::from(origin.0)),
        f64::from(center.y) - f64::from(size.height) * (0.5 - f64::from(origin.1)),
    )
}

pub(crate) fn node_rect_from_position(node: &LayoutNodePosition) -> Option<CanvasRect> {
    let rect = CanvasRect {
        origin: checked_canvas_point(
            f64::from(node.center.x) - f64::from(node.size.width) * 0.5,
            f64::from(node.center.y) - f64::from(node.size.height) * 0.5,
        )?,
        size: node.size,
    };
    rect.is_positive_finite().then_some(rect)
}

pub(crate) fn union_bounds(bounds: Option<CanvasRect>, next: CanvasRect) -> Option<CanvasRect> {
    if !next.is_positive_finite() {
        return None;
    }
    let Some(bounds) = bounds else {
        return Some(next);
    };
    if !bounds.is_positive_finite() {
        return None;
    }

    let min_x = bounds.origin.x.min(next.origin.x);
    let min_y = bounds.origin.y.min(next.origin.y);
    let bounds_max = bounds.checked_max()?;
    let next_max = next.checked_max()?;
    let max_x = bounds_max.x.max(next_max.x);
    let max_y = bounds_max.y.max(next_max.y);

    let rect = CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: checked_f32(f64::from(max_x) - f64::from(min_x))?,
            height: checked_f32(f64::from(max_y) - f64::from(min_y))?,
        },
    };
    rect.is_positive_finite().then_some(rect)
}

pub(crate) fn translate_point(point: CanvasPoint, delta: CanvasPoint) -> Option<CanvasPoint> {
    checked_canvas_point(
        f64::from(point.x) + f64::from(delta.x),
        f64::from(point.y) + f64::from(delta.y),
    )
}

pub(crate) fn checked_f32(value: f64) -> Option<f32> {
    (value.is_finite() && value >= f64::from(f32::MIN) && value <= f64::from(f32::MAX))
        .then_some(value as f32)
}

pub(crate) fn checked_canvas_point(x: f64, y: f64) -> Option<CanvasPoint> {
    Some(CanvasPoint {
        x: checked_f32(x)?,
        y: checked_f32(y)?,
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
