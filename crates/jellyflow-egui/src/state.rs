use std::collections::{BTreeMap, BTreeSet, HashMap};

use eframe::egui::{Pos2, Rect};

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, NodeId, NodeKindKey};
use jellyflow::layout::{LayoutPresetBuilder, LayoutScope};
use jellyflow::runtime::NodeGraphStore;
use jellyflow::runtime::runtime::connection::{ConnectionHandleRef, ResolvedConnectionTarget};
use jellyflow::runtime::runtime::drag::NodeDragPlan;
use jellyflow::runtime::runtime::geometry::{EdgePath, HandleBounds};
use jellyflow::runtime::runtime::measurement::LayoutFactsQueryResult;
use jellyflow::runtime::runtime::rendering::RenderingQueryResult;
use jellyflow::runtime::runtime::resize::{NodeResizeDirection, NodeResizePlan};
use jellyflow::runtime::runtime::viewport::ViewportTransform;

use crate::samples::SampleGraphKind;

/// Common layout presets exposed by the egui adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutPresetChoice {
    #[default]
    Workflow,
    Tree,
    MindMap,
    Freeform,
}

impl LayoutPresetChoice {
    pub const ALL: [Self; 4] = [Self::Workflow, Self::Tree, Self::MindMap, Self::Freeform];

    pub fn label(self) -> &'static str {
        match self {
            Self::Workflow => "Workflow",
            Self::Tree => "Tree",
            Self::MindMap => "Mind map",
            Self::Freeform => "Freeform",
        }
    }

    pub fn builder(self) -> LayoutPresetBuilder {
        match self {
            Self::Workflow => LayoutPresetBuilder::workflow(),
            Self::Tree => LayoutPresetBuilder::tree(),
            Self::MindMap => LayoutPresetBuilder::mind_map(),
            Self::Freeform => LayoutPresetBuilder::freeform(),
        }
    }
}

/// Adapter-owned hover target used for canvas highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverTarget {
    Node(NodeId),
    Edge(EdgeId),
    Handle(ConnectionHandleRef),
    ResizeHandle {
        node: NodeId,
        direction: NodeResizeDirection,
    },
}

/// Active canvas tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    Select,
    CreateNode,
    Drag,
    Resize,
    Connect,
    Pan,
}

/// Live interaction state for the canvas.
#[derive(Debug, Clone, Default)]
pub enum ActiveCanvasInteraction {
    #[default]
    None,
    NodeDrag {
        primary: NodeId,
        start_pointer: CanvasPoint,
        preview: Option<NodeDragPlan>,
    },
    NodeResize {
        node: NodeId,
        direction: NodeResizeDirection,
        start_pointer: CanvasPoint,
        current_pointer: CanvasPoint,
        preview: Option<NodeResizePlan>,
    },
    Connect {
        from: ConnectionHandleRef,
        start_pointer: CanvasPoint,
        current_pointer: CanvasPoint,
        target: Option<ResolvedConnectionTarget>,
    },
    SelectionBox {
        start_pointer: CanvasPoint,
        current_pointer: CanvasPoint,
        additive: bool,
    },
    Pan {
        current_pointer: CanvasPoint,
    },
}

impl ActiveCanvasInteraction {
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Selection tool state owned by the adapter.
#[derive(Debug, Clone, Default)]
pub struct InspectorState {
    pub data_buffer: String,
    pub data_buffer_node: Option<NodeId>,
    pub data_error: Option<String>,
}

/// Snapshot of canvas-derived geometry.
#[derive(Debug, Clone)]
pub struct CanvasSnapshot {
    pub viewport_rect: Rect,
    pub viewport_size: CanvasSize,
    pub transform: ViewportTransform,
    pub node_rects: BTreeMap<NodeId, CanvasRect>,
    pub handle_bounds: HashMap<ConnectionHandleRef, HandleBounds>,
    pub edge_paths: BTreeMap<EdgeId, EdgePath>,
    pub rendering: RenderingQueryResult,
    pub layout_facts: LayoutFactsQueryResult,
    pub visible_node_ids: Vec<NodeId>,
    pub visible_node_render_order: Vec<NodeId>,
    pub visible_edge_ids: Vec<EdgeId>,
    pub visible_edge_render_order: Vec<EdgeId>,
}

impl CanvasSnapshot {
    pub fn empty() -> Self {
        Self {
            viewport_rect: Rect::from_min_size(Pos2::ZERO, eframe::egui::vec2(1.0, 1.0)),
            viewport_size: CanvasSize {
                width: 1.0,
                height: 1.0,
            },
            transform: ViewportTransform {
                pan: CanvasPoint::default(),
                zoom: 1.0,
            },
            node_rects: BTreeMap::new(),
            handle_bounds: HashMap::new(),
            edge_paths: BTreeMap::new(),
            rendering: RenderingQueryResult::default(),
            layout_facts: LayoutFactsQueryResult::new(
                0,
                RenderingQueryResult::default(),
                Vec::new(),
                Vec::new(),
            ),
            visible_node_ids: Vec::new(),
            visible_node_render_order: Vec::new(),
            visible_edge_ids: Vec::new(),
            visible_edge_render_order: Vec::new(),
        }
    }

    pub fn screen_point_to_canvas(&self, point: Pos2) -> CanvasPoint {
        let local_x = point.x - self.viewport_rect.min.x;
        let local_y = point.y - self.viewport_rect.min.y;
        CanvasPoint {
            x: local_x / self.transform.zoom - self.transform.pan.x,
            y: local_y / self.transform.zoom - self.transform.pan.y,
        }
    }

    pub fn canvas_point_to_screen(&self, point: CanvasPoint) -> Pos2 {
        Pos2::new(
            self.viewport_rect.min.x + (point.x + self.transform.pan.x) * self.transform.zoom,
            self.viewport_rect.min.y + (point.y + self.transform.pan.y) * self.transform.zoom,
        )
    }

    pub fn node_screen_rect(&self, node: NodeId) -> Option<Rect> {
        let rect = self.node_rects.get(&node)?;
        let min = self.canvas_point_to_screen(rect.origin);
        let max = self.canvas_point_to_screen(CanvasPoint {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height,
        });
        Some(Rect::from_min_max(min, max))
    }

    pub fn handle_screen_rect(&self, handle: ConnectionHandleRef) -> Option<Rect> {
        let bounds = self.handle_bounds.get(&handle)?;
        let node_rect = self.node_rects.get(&handle.node)?;
        let origin = CanvasPoint {
            x: node_rect.origin.x + bounds.rect.origin.x,
            y: node_rect.origin.y + bounds.rect.origin.y,
        };
        let min = self.canvas_point_to_screen(origin);
        let max = self.canvas_point_to_screen(CanvasPoint {
            x: origin.x + bounds.rect.size.width,
            y: origin.y + bounds.rect.size.height,
        });
        Some(Rect::from_min_max(min, max))
    }

    pub fn edge_paths(&self) -> impl Iterator<Item = (&EdgeId, &EdgePath)> {
        self.edge_paths.iter()
    }
}

/// Adapter-level runtime state shared across panels.
#[derive(Debug, Clone)]
pub struct JellyflowEguiState {
    pub palette_filter: String,
    pub pending_create_kind: Option<NodeKindKey>,
    pub selected_sample: SampleGraphKind,
    pub selected_layout_preset: LayoutPresetChoice,
    pub canvas_tool: CanvasTool,
    pub canvas: CanvasInteractionState,
    pub inspector: InspectorState,
    pub status_message: Option<String>,
}

impl Default for JellyflowEguiState {
    fn default() -> Self {
        Self {
            palette_filter: String::new(),
            pending_create_kind: None,
            selected_sample: SampleGraphKind::default(),
            selected_layout_preset: LayoutPresetChoice::default(),
            canvas_tool: CanvasTool::Select,
            canvas: CanvasInteractionState::default(),
            inspector: InspectorState::default(),
            status_message: None,
        }
    }
}

/// Live canvas state and cached snapshot.
#[derive(Debug, Clone, Default)]
pub struct CanvasInteractionState {
    pub hovered: Option<HoverTarget>,
    pub active: ActiveCanvasInteraction,
    pub snapshot: CanvasSnapshot,
    pub node_measurements: HashMap<NodeId, (CanvasSize, Vec<(ConnectionHandleRef, HandleBounds)>)>,
}

impl CanvasInteractionState {
    pub fn set_active(&mut self, active: ActiveCanvasInteraction) {
        self.active = active;
    }

    pub fn clear_active(&mut self) {
        self.active = ActiveCanvasInteraction::None;
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_active()
    }
}

impl Default for CanvasSnapshot {
    fn default() -> Self {
        Self::empty()
    }
}

impl JellyflowEguiState {
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }
}

/// Derives a reasonable layout scope from the current selection.
pub fn layout_scope_for_selection(store: &NodeGraphStore) -> LayoutScope {
    let nodes = store
        .view_state()
        .selected_nodes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if nodes.is_empty() {
        LayoutScope::All
    } else {
        LayoutScope::Nodes { nodes }
    }
}

/// Extracts the first selected node, if there is exactly one.
pub fn single_selected_node(store: &NodeGraphStore) -> Option<NodeId> {
    let selected = &store.view_state().selected_nodes;
    (selected.len() == 1).then_some(selected[0])
}

/// Returns the selected node ids in deterministic order.
pub fn selected_nodes(store: &NodeGraphStore) -> Vec<NodeId> {
    let mut nodes = store.view_state().selected_nodes.clone();
    nodes.sort();
    nodes.dedup();
    nodes
}

/// Returns the selected edge ids in deterministic order.
pub fn selected_edges(store: &NodeGraphStore) -> Vec<EdgeId> {
    let mut edges = store.view_state().selected_edges.clone();
    edges.sort();
    edges.dedup();
    edges
}
