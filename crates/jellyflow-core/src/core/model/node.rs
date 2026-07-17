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

    /// Optional node origin override. Omitted nodes use top-left origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<NodeOrigin>,

    /// Optional adapter-facing selection override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,

    /// Optional adapter-facing drag override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draggable: Option<bool>,

    /// Whether graph connections may use this node. Runtime policy supplies the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connectable: Option<bool>,

    /// Whether graph commands may delete this node. Runtime policy supplies the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletable: Option<bool>,

    /// Optional group container id (subflow / parent frame).
    ///
    /// This is an editor-structure concept (XyFlow `parentId` mental model) and is intentionally
    /// orthogonal to semantic subgraphs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<GroupId>,

    /// Optional adapter-facing movement/resize extent preference.
    ///
    /// Core and runtime preserve this editor-structure fact but do not interpret it as a graph
    /// invariant. A view adapter may resolve it against current native measurements before issuing
    /// an explicit geometry command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<NodeExtent>,

    /// Whether an adapter may offer parent expansion while moving or resizing this node.
    ///
    /// This is a persisted interaction preference, not an implicit side effect of runtime geometry
    /// commands. Atomic parent expansion requires an explicit future group-geometry edit.
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
    /// Projects this node's anchored position into a top-left canvas rectangle for `size`.
    ///
    /// Invalid origin components fall back to zero and finite components are clamped to the
    /// normalized `[0, 1]` range. Invalid positions or non-positive sizes have no bounds.
    pub fn bounds_for_size(&self, size: CanvasSize) -> Option<CanvasRect> {
        if !self.pos.is_finite() || !size.is_positive_finite() {
            return None;
        }

        let (origin_x, origin_y) = self.origin.map(NodeOrigin::as_tuple).unwrap_or((0.0, 0.0));
        let origin_x = normalized_origin_component(origin_x);
        let origin_y = normalized_origin_component(origin_y);
        Some(CanvasRect {
            origin: CanvasPoint {
                x: self.pos.x - origin_x * size.width,
                y: self.pos.y - origin_y * size.height,
            },
            size,
        })
    }

    /// Clears the ordered port list.
    pub fn clear_ports(&mut self) {
        self.ports.clear();
    }

    /// Retains ports that satisfy `f`.
    pub fn retain_ports(&mut self, f: impl FnMut(&PortId) -> bool) {
        self.ports.retain(f);
    }
}

fn normalized_origin_component(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
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

/// Adapter-facing per-node movement/resize extent preference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeExtent {
    /// Constrain to the node's parent container (if any).
    Parent,
    /// Constrain to the given rect in canvas space.
    Rect { rect: CanvasRect },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with_origin(origin: Option<NodeOrigin>) -> Node {
        Node {
            kind: crate::core::NodeKindKey::new("test.node"),
            kind_version: 1,
            pos: CanvasPoint { x: 20.0, y: 10.0 },
            origin,
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
            ports: Vec::new(),
            data: Value::Null,
        }
    }

    #[test]
    fn bounds_for_size_projects_normalized_origin_and_rejects_invalid_geometry() {
        let size = CanvasSize {
            width: 10.0,
            height: 6.0,
        };
        let centered = node_with_origin(Some(NodeOrigin { x: 0.5, y: 0.5 }));
        assert_eq!(
            centered.bounds_for_size(size),
            Some(CanvasRect {
                origin: CanvasPoint { x: 15.0, y: 7.0 },
                size,
            }),
        );

        let normalized = node_with_origin(Some(NodeOrigin {
            x: f32::NAN,
            y: 2.0,
        }));
        assert_eq!(
            normalized.bounds_for_size(size),
            Some(CanvasRect {
                origin: CanvasPoint { x: 20.0, y: 4.0 },
                size,
            }),
        );

        assert_eq!(
            centered.bounds_for_size(CanvasSize {
                width: 0.0,
                height: 6.0,
            }),
            None,
        );

        let mut non_finite_position = centered;
        non_finite_position.pos.x = f32::INFINITY;
        assert_eq!(non_finite_position.bounds_for_size(size), None);
    }
}
