use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::ids::{GroupId, NodeKindKey, PortId};

use super::geometry::{CanvasPoint, CanvasRect, CanvasSize};

fn is_false(v: &bool) -> bool {
    !*v
}

/// Node instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Node kind identifier.
    pub kind: NodeKindKey,
    /// Node kind version (for per-kind migrations).
    pub kind_version: u32,
    /// Top-left position in canvas space.
    pub pos: CanvasPoint,

    /// Optional node origin override (XyFlow `node.origin`).
    ///
    /// When omitted, runtime uses the global `NodeGraphInteractionState.node_origin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<NodeOrigin>,

    /// Whether the node can be selected (XyFlow `node.selectable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.elements_selectable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,

    /// Whether the node can receive keyboard focus (XyFlow `node.focusable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.nodes_focusable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,

    /// Whether the node can be dragged with pointer interactions (XyFlow `node.draggable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.nodes_draggable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draggable: Option<bool>,

    /// Whether the node can be used for creating connections via editor interactions (XyFlow
    /// `node.connectable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.nodes_connectable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable: Option<bool>,

    /// Whether the node can be deleted via editor interactions (XyFlow `node.deletable`).
    ///
    /// When omitted, the global `NodeGraphInteractionState.nodes_deletable` decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletable: Option<bool>,

    /// Optional group container id (subflow / parent frame).
    ///
    /// This is an editor-structure concept (XyFlow `parentId` mental model) and is intentionally
    /// orthogonal to semantic subgraphs (see ADR 0126).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<GroupId>,

    /// Optional per-node movement/resize extent override.
    ///
    /// This mirrors XyFlow's `node.extent` concept. It is an editor-structure constraint (UI-facing),
    /// not a semantic graph rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<NodeExtent>,

    /// Whether moving/resizing this node can expand its parent container (if any).
    ///
    /// This mirrors XyFlow's `node.expandParent` behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand_parent: Option<bool>,

    /// Optional explicit node size in logical px at zoom=1 (semantic sizing).
    ///
    /// The editor converts this into canvas space by dividing by the current zoom so node content
    /// remains readable under semantic zoom.
    ///
    /// When `None`, the editor derives the size from measured geometry or style defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<CanvasSize>,

    /// Whether the node is hidden (XyFlow `node.hidden`).
    ///
    /// Hidden nodes are excluded from derived geometry (hit-testing, rendering, fit-view).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,

    /// Whether the node is collapsed.
    #[serde(default)]
    pub collapsed: bool,

    /// Stable port ordering for this node (UI-facing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortId>,

    /// Opaque node payload (domain-owned).
    ///
    /// This must be preserved for unknown node kinds.
    #[serde(default)]
    pub data: Value,
}

impl Node {
    /// Clears the ordered port list.
    pub fn clear_ports(&mut self) {
        self.ports.clear();
    }

    /// Retains ports that satisfy `f`.
    pub fn retain_ports(&mut self, f: impl FnMut(&PortId) -> bool) {
        self.ports.retain(f);
    }
}

/// Per-node origin override, expressed as a normalized fraction of the node rect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeOrigin {
    pub x: f32,
    pub y: f32,
}

impl NodeOrigin {
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub fn as_tuple(self) -> (f32, f32) {
        (self.x, self.y)
    }
}

/// Per-node movement/resize extent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeExtent {
    /// Constrain to the node's parent container (if any).
    Parent,
    /// Constrain to the given rect in canvas space.
    Rect { rect: CanvasRect },
}
