use serde::{Deserialize, Deserializer, Serialize};

/// Behavior for selecting edges during marquee (box) selection.
///
/// XyFlow selects edges connected to the selected nodes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphBoxSelectEdges {
    /// Do not select edges from a marquee selection.
    None,
    /// Select edges connected to any selected node (XyFlow default).
    #[default]
    Connected,
    /// Select edges only when both endpoints are within the marquee-selected node set.
    BothEndpoints,
}

impl<'de> Deserialize<'de> for NodeGraphBoxSelectEdges {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NodeGraphBoxSelectEdges;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a bool or one of: none, connected, both_endpoints")
            }

            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(if v {
                    NodeGraphBoxSelectEdges::Connected
                } else {
                    NodeGraphBoxSelectEdges::None
                })
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v {
                    "none" => Ok(NodeGraphBoxSelectEdges::None),
                    "connected" => Ok(NodeGraphBoxSelectEdges::Connected),
                    "both_endpoints" => Ok(NodeGraphBoxSelectEdges::BothEndpoints),
                    other => Err(E::custom(format!(
                        "unrecognized box select edges mode: {other}"
                    ))),
                }
            }

            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Behavior for selecting nodes during marquee (box) selection.
///
/// This matches XyFlow's `selectionMode`:
/// - `full`: select nodes only when their rect is fully contained in the marquee.
/// - `partial`: select nodes when they intersect the marquee.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphSelectionMode {
    /// Select nodes only when fully contained by the marquee (XyFlow default).
    #[default]
    Full,
    /// Select nodes when partially intersecting the marquee.
    Partial,
}

/// Node origin (anchor) used to interpret `Node.pos` (XyFlow `nodeOrigin`).
///
/// This is expressed as a normalized fraction of the node rect:
/// - `(0.0, 0.0)` means `Node.pos` is the node's top-left.
/// - `(0.5, 0.5)` means `Node.pos` is the node's center.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphNodeOrigin {
    pub x: f32,
    pub y: f32,
}

impl NodeGraphNodeOrigin {
    pub fn normalized(self) -> Self {
        let mut out = self;
        if !out.x.is_finite() {
            out.x = 0.0;
        }
        if !out.y.is_finite() {
            out.y = 0.0;
        }
        out.x = out.x.clamp(0.0, 1.0);
        out.y = out.y.clamp(0.0, 1.0);
        out
    }
}

/// Nudge step semantics for keyboard-driven movement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphNudgeStepMode {
    /// Interprets the step as screen-space pixels (converted to canvas units by dividing by zoom).
    #[default]
    ScreenPx,
    /// Uses the editor snap grid (`snap_grid`) as the step (canvas-space).
    Grid,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeGraphPanOnDragButtons {
    /// Pan the canvas by dragging on empty background with the left mouse button.
    #[serde(default)]
    pub left: bool,
    /// Pan the canvas by dragging with the middle mouse button.
    #[serde(default)]
    pub middle: bool,
    /// Pan the canvas by dragging with the right mouse button.
    ///
    /// When enabled, apps should provide an alternate way to open context menus (or make context
    /// menus conditional on "click without pan"), matching XyFlow's `panOnDrag={[2]}` patterns.
    #[serde(default)]
    pub right: bool,
}

pub(super) fn default_pan_on_drag_buttons() -> NodeGraphPanOnDragButtons {
    NodeGraphPanOnDragButtons {
        left: true,
        middle: true,
        right: false,
    }
}

pub(super) fn default_box_select_edges() -> NodeGraphBoxSelectEdges {
    NodeGraphBoxSelectEdges::Connected
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphPanOnScrollMode {
    #[default]
    Free,
    Horizontal,
    Vertical,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphViewportInterpolate {
    Linear,
    #[default]
    Smooth,
}

/// Easing curve for animated viewport changes (XyFlow `fitViewOptions.ease`).
///
/// Note: this is an optional override. When unset, the legacy behavior is derived from
/// `frame_view_interpolate` for backward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphViewportEase {
    Linear,
    /// Smoothstep `t*t*(3-2*t)` (close to common editor defaults).
    Smoothstep,
    /// Cubic ease-in-out.
    CubicInOut,
}
