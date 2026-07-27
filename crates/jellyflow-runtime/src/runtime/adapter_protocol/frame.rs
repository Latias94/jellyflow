use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasRect, CanvasSize, Node, NodeId, NodeKindKey, Port, PortId};

use crate::runtime::graph_runtime::{
    DocumentEpoch, GraphRevision, GraphSnapshot, GraphVersion, NodeLifetimeKey, PortInstanceKey,
};
use crate::runtime::view_session::{ViewGeneration, ViewId};
use crate::runtime::viewport::{ViewportSnapshot, ViewportTransform};

use super::feedback::{FeedbackCollector, NodeLayoutBuilder};

/// Default maximum number of viewport-admitted nodes materialized in one frame.
pub const DEFAULT_FRAME_VISIBLE_NODE_BUDGET: usize = 1_024;

/// Default maximum number of raw spatial-index entries scanned for visible-node admission.
///
/// Spatial cells may contain duplicate or broad-phase entries, so this is intentionally larger
/// than [`DEFAULT_FRAME_VISIBLE_NODE_BUDGET`]. Requests that need an exact result across a denser
/// viewport can opt into a larger limit explicitly.
pub const DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT: usize = 4_096;

/// Default maximum number of adapter-required nodes materialized in one frame.
///
/// This matches the reference Open GPUI host's protected-instance lease capacity.
pub const DEFAULT_FRAME_REQUIRED_NODE_BUDGET: usize = 64;

/// Default maximum number of routing dependency nodes materialized in one frame.
pub const DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET: usize = 512;

/// Geometry-affecting native environment generations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdapterEnvironment {
    dpi: u64,
    font: u64,
    locale: u64,
    metric_theme: u64,
}

impl AdapterEnvironment {
    pub fn new(dpi: u64, font: u64, locale: u64, metric_theme: u64) -> Self {
        Self {
            dpi,
            font,
            locale,
            metric_theme,
        }
    }

    pub fn dpi(self) -> u64 {
        self.dpi
    }

    pub fn font(self) -> u64 {
        self.font
    }

    pub fn locale(self) -> u64 {
        self.locale
    }

    pub fn metric_theme(self) -> u64 {
        self.metric_theme
    }
}

/// Adapter-local renderer identity for one native node implementation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RendererStamp {
    key: Arc<str>,
    incarnation: u64,
}

impl RendererStamp {
    pub fn new(key: impl Into<Arc<str>>, incarnation: u64) -> Self {
        Self {
            key: key.into(),
            incarnation,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn incarnation(&self) -> u64 {
        self.incarnation
    }
}

/// Full identity of one native node instance inside one view.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeInstanceKey {
    view: ViewId,
    renderer: RendererStamp,
    lifetime: NodeLifetimeKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLayoutTarget {
    pub(crate) instance: NodeInstanceKey,
    pub(crate) layout_generation: u64,
    pub(crate) frame_sequence: u64,
}

impl NodeLayoutTarget {
    pub fn instance(&self) -> &NodeInstanceKey {
        &self.instance
    }

    pub fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    pub(crate) fn frame_sequence(&self) -> u64 {
        self.frame_sequence
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortLayoutTarget {
    pub(crate) node: Arc<NodeLayoutTarget>,
    pub(crate) port_instance: PortInstanceKey,
}

impl PortLayoutTarget {
    pub fn instance(&self) -> &NodeInstanceKey {
        self.node.instance()
    }

    pub fn layout_generation(&self) -> u64 {
        self.node.layout_generation()
    }

    pub fn port(&self) -> PortId {
        self.port_instance.port()
    }

    pub fn port_instance(&self) -> PortInstanceKey {
        self.port_instance
    }

    pub(crate) fn frame_sequence(&self) -> u64 {
        self.node.frame_sequence()
    }

    #[cfg(test)]
    pub(crate) fn with_port_for_test(&self, port: PortId) -> Self {
        Self {
            node: Arc::clone(&self.node),
            port_instance: self.port_instance.with_port_for_test(port),
        }
    }
}

impl NodeInstanceKey {
    pub(crate) fn new(
        view: ViewId,
        renderer: RendererStamp,
        lifetime: NodeLifetimeKey,
    ) -> Option<Self> {
        (view.runtime_provenance() == lifetime.provenance()).then_some(Self {
            view,
            renderer,
            lifetime,
        })
    }

    pub fn view(&self) -> ViewId {
        self.view
    }

    pub fn node(&self) -> NodeId {
        self.lifetime.node()
    }

    pub fn renderer(&self) -> &RendererStamp {
        &self.renderer
    }

    /// Returns the exact semantic lifetime represented by this native node instance.
    pub fn lifetime(&self) -> NodeLifetimeKey {
        self.lifetime
    }
}

/// Independent node-materialization limits for one native frame.
///
/// Node materialization limits apply after hidden, invalid, and duplicate nodes are removed. The
/// visible scan limit applies earlier to raw spatial entries. A value of zero disables that class
/// for the request. Callers may opt into larger limits explicitly, but the defaults remain finite
/// so a viewport or high-fan-out node cannot mount or scan an unbounded native tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMaterializationBudget {
    visible_nodes: usize,
    visible_node_scan_limit: usize,
    required_nodes: usize,
    routing_dependency_nodes: usize,
}

impl FrameMaterializationBudget {
    #[must_use]
    pub const fn new(
        visible_nodes: usize,
        required_nodes: usize,
        routing_dependency_nodes: usize,
    ) -> Self {
        Self {
            visible_nodes,
            visible_node_scan_limit: DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT,
            required_nodes,
            routing_dependency_nodes,
        }
    }

    #[must_use]
    pub const fn visible_nodes(self) -> usize {
        self.visible_nodes
    }

    /// Returns the maximum number of raw spatial-index entries examined for visible admission.
    #[must_use]
    pub const fn visible_node_scan_limit(self) -> usize {
        self.visible_node_scan_limit
    }

    /// Overrides the raw spatial-index entry limit for visible admission.
    ///
    /// This limit bounds broad-phase scanning and candidate construction independently from the
    /// number of nodes ultimately materialized. A value of zero disables the spatial scan and
    /// reports truncation whenever the viewport has possible candidates. A truncated query keeps
    /// exact visibility checks and stable render ordering within its deterministic scanned prefix,
    /// but an unexamined node may precede that prefix in global selected/draw order.
    #[must_use]
    pub const fn with_visible_node_scan_limit(mut self, limit: usize) -> Self {
        self.visible_node_scan_limit = limit;
        self
    }

    #[must_use]
    pub const fn required_nodes(self) -> usize {
        self.required_nodes
    }

    #[must_use]
    pub const fn routing_dependency_nodes(self) -> usize {
        self.routing_dependency_nodes
    }
}

impl Default for FrameMaterializationBudget {
    fn default() -> Self {
        Self::new(
            DEFAULT_FRAME_VISIBLE_NODE_BUDGET,
            DEFAULT_FRAME_REQUIRED_NODE_BUDGET,
            DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET,
        )
    }
}

/// Structured evidence that a frame request exceeded one or more materialization limits.
///
/// Counts include valid, unique candidates known to the admission pass but omitted by their class
/// budget. When [`Self::visible_scan_truncated`] is false, `visible_nodes` is the exact omitted
/// count. When it is true, `visible_nodes` is a lower bound from the scanned prefix and additional
/// visible candidates may exist in unexamined spatial entries. Incident routing discovery is also
/// bounded, so [`Self::incident_scan_truncated`] reports the equivalent routing degradation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameMaterializationOverflow {
    visible_nodes: usize,
    required_nodes: usize,
    routing_dependency_nodes: usize,
    visible_scan_truncated: bool,
    incident_scan_truncated: bool,
}

impl FrameMaterializationOverflow {
    pub(crate) const fn new(
        visible_nodes: usize,
        required_nodes: usize,
        routing_dependency_nodes: usize,
        visible_scan_truncated: bool,
        incident_scan_truncated: bool,
    ) -> Self {
        Self {
            visible_nodes,
            required_nodes,
            routing_dependency_nodes,
            visible_scan_truncated,
            incident_scan_truncated,
        }
    }

    #[must_use]
    pub const fn visible_nodes(self) -> usize {
        self.visible_nodes
    }

    #[must_use]
    pub const fn required_nodes(self) -> usize {
        self.required_nodes
    }

    #[must_use]
    pub const fn routing_dependency_nodes(self) -> usize {
        self.routing_dependency_nodes
    }

    /// Returns whether visible admission stopped at its raw spatial-entry limit.
    ///
    /// When true, [`Self::visible_nodes`] remains a known lower bound rather than an exact count,
    /// and ordering is guaranteed only within the deterministic scanned prefix.
    #[must_use]
    pub const fn visible_scan_truncated(self) -> bool {
        self.visible_scan_truncated
    }

    #[must_use]
    pub const fn incident_scan_truncated(self) -> bool {
        self.incident_scan_truncated
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.visible_nodes == 0
            && self.required_nodes == 0
            && self.routing_dependency_nodes == 0
            && !self.visible_scan_truncated
            && !self.incident_scan_truncated
    }
}

/// Inputs that determine which concrete nodes are materialized for a native frame.
#[derive(Debug, Clone)]
pub struct FrameRequest {
    pub(crate) viewport: CanvasRect,
    pub(crate) overscan: f32,
    pub(crate) default_renderer: RendererStamp,
    pub(crate) renderers: BTreeMap<NodeId, RendererStamp>,
    pub(crate) kind_renderers: Arc<BTreeMap<NodeKindKey, RendererStamp>>,
    pub(crate) routing_dependencies: BTreeSet<NodeId>,
    pub(crate) incident_routing_dependencies: bool,
    pub(crate) required_nodes: BTreeSet<NodeId>,
    pub(crate) kind_default_sizes: Arc<BTreeMap<NodeKindKey, CanvasSize>>,
    pub(crate) fallback_size: CanvasSize,
    pub(crate) materialization_budget: FrameMaterializationBudget,
}

impl FrameRequest {
    pub fn new(viewport: CanvasRect, default_renderer: RendererStamp) -> Self {
        Self {
            viewport,
            overscan: 0.0,
            default_renderer,
            renderers: BTreeMap::new(),
            kind_renderers: Arc::new(BTreeMap::new()),
            routing_dependencies: BTreeSet::new(),
            incident_routing_dependencies: false,
            required_nodes: BTreeSet::new(),
            kind_default_sizes: Arc::new(BTreeMap::new()),
            fallback_size: CanvasSize {
                width: 160.0,
                height: 96.0,
            },
            materialization_budget: FrameMaterializationBudget::default(),
        }
    }

    pub fn with_overscan(mut self, overscan: f32) -> Self {
        self.overscan = if overscan.is_finite() && overscan > 0.0 {
            overscan
        } else {
            0.0
        };
        self
    }

    /// Replaces the viewport while retaining immutable renderer and sizing policy.
    #[must_use]
    pub fn with_viewport(mut self, viewport: CanvasRect) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn with_renderer(mut self, node: NodeId, renderer: RendererStamp) -> Self {
        self.renderers.insert(node, renderer);
        self
    }

    /// Registers one renderer for every node of a semantic kind.
    ///
    /// A node-specific renderer registered through [`Self::with_renderer`] takes precedence.
    pub fn with_kind_renderer(mut self, kind: NodeKindKey, renderer: RendererStamp) -> Self {
        Arc::make_mut(&mut self.kind_renderers).insert(kind, renderer);
        self
    }

    pub fn with_routing_dependencies(mut self, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        self.routing_dependencies.extend(nodes);
        self
    }

    /// Derives routing dependencies from the incident edges of initially admitted nodes.
    ///
    /// The runtime resolves opposite edge endpoints through its graph index and forces those
    /// nodes into the frame even when they fall outside the viewport. This is opt-in so callers
    /// that already provide explicit routing dependencies retain the existing admission semantics.
    pub fn with_incident_routing_dependencies(mut self) -> Self {
        self.incident_routing_dependencies = true;
        self
    }

    /// Forces adapter-owned active instances into the frame even when they are outside culling.
    ///
    /// This is separate from routing dependencies: focus, IME, pointer capture, and unresolved
    /// native drafts require a live component but do not imply an edge-routing dependency.
    pub fn with_required_nodes(mut self, nodes: impl IntoIterator<Item = NodeId>) -> Self {
        self.required_nodes.extend(nodes);
        self
    }

    /// Overrides the independent visible, required, and routing-dependency limits for this frame.
    #[must_use]
    pub fn with_materialization_budget(mut self, budget: FrameMaterializationBudget) -> Self {
        self.materialization_budget = budget;
        self
    }

    pub fn with_kind_default(mut self, kind: NodeKindKey, size: CanvasSize) -> Self {
        if size.is_positive_finite() {
            Arc::make_mut(&mut self.kind_default_sizes).insert(kind, size);
        }
        self
    }

    pub fn with_fallback_size(mut self, size: CanvasSize) -> Self {
        if size.is_positive_finite() {
            self.fallback_size = size;
        }
        self
    }

    pub(crate) fn renderer_for(&self, node: NodeId, kind: &NodeKindKey) -> RendererStamp {
        self.renderers
            .get(&node)
            .or_else(|| self.kind_renderers.get(kind))
            .cloned()
            .unwrap_or_else(|| self.default_renderer.clone())
    }
}

/// Opaque, one-shot capability bound to one materialized frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameToken {
    pub(crate) view: ViewId,
    pub(crate) generation: ViewGeneration,
    pub(crate) sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBoundsSource {
    Published,
    Persisted,
    KindDefault,
    ProtocolFallback,
}

#[derive(Debug, Clone)]
pub struct FramePort {
    snapshot: GraphSnapshot,
    layout_target: PortLayoutTarget,
}

impl FramePort {
    pub(crate) fn new(
        snapshot: GraphSnapshot,
        node_target: Arc<NodeLayoutTarget>,
        port_instance: PortInstanceKey,
    ) -> Self {
        Self {
            snapshot,
            layout_target: PortLayoutTarget {
                node: node_target,
                port_instance,
            },
        }
    }

    pub fn id(&self) -> PortId {
        self.layout_target.port()
    }

    pub fn instance(&self) -> PortInstanceKey {
        self.layout_target.port_instance()
    }

    pub fn port(&self) -> Option<&Port> {
        self.snapshot.graph().port(&self.id())
    }

    pub fn layout_target(&self) -> &PortLayoutTarget {
        &self.layout_target
    }
}

#[derive(Debug, Clone)]
pub struct FrameNode {
    snapshot: GraphSnapshot,
    layout_target: Arc<NodeLayoutTarget>,
    selected: bool,
    binding_revision: GraphRevision,
    ports: Vec<FramePort>,
    bounds: CanvasRect,
    bounds_source: FrameBoundsSource,
}

impl FrameNode {
    pub(crate) fn new(
        snapshot: GraphSnapshot,
        layout_target: Arc<NodeLayoutTarget>,
        selected: bool,
        binding_revision: GraphRevision,
        ports: Vec<FramePort>,
        bounds: CanvasRect,
        bounds_source: FrameBoundsSource,
    ) -> Self {
        Self {
            snapshot,
            layout_target,
            selected,
            binding_revision,
            ports,
            bounds,
            bounds_source,
        }
    }

    pub fn id(&self) -> NodeId {
        self.layout_target.instance().node()
    }

    pub fn instance(&self) -> &NodeInstanceKey {
        self.layout_target.instance()
    }

    pub fn layout_target(&self) -> &NodeLayoutTarget {
        &self.layout_target
    }

    pub fn renderer(&self) -> &RendererStamp {
        self.layout_target.instance().renderer()
    }

    pub fn layout_generation(&self) -> u64 {
        self.layout_target.layout_generation()
    }

    /// Whether this node was selected when this frame was materialized.
    ///
    /// Selection is view-local frame state and does not advance the graph or binding revision.
    pub fn selected(&self) -> bool {
        self.selected
    }

    /// Latest graph revision that changed facts observable by this node binding.
    pub fn binding_revision(&self) -> GraphRevision {
        self.binding_revision
    }

    /// Immutable graph snapshot shared by every record materialized from this frame.
    pub fn graph_snapshot(&self) -> &GraphSnapshot {
        &self.snapshot
    }

    pub fn node(&self) -> Option<&Node> {
        self.snapshot.graph().node(&self.id())
    }

    pub fn ports(&self) -> &[FramePort] {
        &self.ports
    }

    pub fn bounds(&self) -> CanvasRect {
        self.bounds
    }

    pub fn bounds_source(&self) -> FrameBoundsSource {
        self.bounds_source
    }

    /// Begins a complete layout report for this node and all of its real ports.
    pub fn begin_layout(&self, bounds: super::ViewRect) -> NodeLayoutBuilder {
        NodeLayoutBuilder::new(
            &self.layout_target,
            bounds,
            self.ports.iter().map(|port| port.layout_target.clone()),
        )
    }
}

#[derive(Debug, Clone)]
pub struct AdapterFrame {
    collector: FeedbackCollector,
    graph_version: GraphVersion,
    document_epoch: DocumentEpoch,
    viewport: ViewportSnapshot,
    nodes: Vec<FrameNode>,
    materialization_overflow: FrameMaterializationOverflow,
}

impl AdapterFrame {
    pub(crate) fn new(
        token: FrameToken,
        graph_version: GraphVersion,
        document_epoch: DocumentEpoch,
        viewport: ViewportSnapshot,
        nodes: Vec<FrameNode>,
        materialization_overflow: FrameMaterializationOverflow,
    ) -> Self {
        Self {
            collector: FeedbackCollector::new(token),
            graph_version,
            document_epoch,
            viewport,
            nodes,
            materialization_overflow,
        }
    }

    pub fn graph_revision(&self) -> GraphRevision {
        self.graph_version.revision()
    }

    pub fn graph_version(&self) -> GraphVersion {
        self.graph_version
    }

    pub fn document_epoch(&self) -> DocumentEpoch {
        self.document_epoch
    }

    /// Exact viewport identity captured by this frame and its one-shot feedback token.
    pub fn viewport_snapshot(&self) -> ViewportSnapshot {
        self.viewport
    }

    /// Viewport transform captured by this frame.
    pub fn transform(&self) -> ViewportTransform {
        self.viewport.transform()
    }

    /// Concrete admitted nodes in back-to-front paint order.
    ///
    /// The runtime applies the view's explicit draw order, deterministic fallback ID order, and
    /// selected-node elevation after viewport, routing, and required-node admission. Adapters can
    /// paint this slice directly without re-sorting it.
    pub fn nodes(&self) -> &[FrameNode] {
        &self.nodes
    }

    /// Returns structured counts for node candidates omitted by frame materialization budgets.
    #[must_use]
    pub const fn materialization_overflow(&self) -> FrameMaterializationOverflow {
        self.materialization_overflow
    }

    pub fn collector(&self) -> FeedbackCollector {
        self.collector.clone()
    }
}
