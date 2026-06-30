//! Renderer-neutral node chrome placement helpers.
//!
//! Runtime owns the semantic facts that make adapter chrome portable: which affordances exist,
//! when they are visible, their node-relative placement, and the resize constraints associated
//! with resize affordances. Adapters still own the concrete widgets, focus handling, popovers, and
//! visual styling.

use serde::{Deserialize, Serialize};

use crate::runtime::resize::NodeResizeConstraints;
use crate::schema::{
    NodeChromeDescriptor, NodeChromeKind, NodeChromePlacement, NodeChromeVisibility,
};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

const DEFAULT_CONTROL_SIZE: f32 = 16.0;
const DEFAULT_BAR_HEIGHT: f32 = 28.0;
const DEFAULT_MARGIN: f32 = 8.0;
const MIN_ZOOM_SCALE: f32 = 0.25;

fn default_zoom() -> f32 {
    1.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Adapter-facing UI state used to resolve node chrome visibility.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeChromeState {
    #[serde(default, skip_serializing_if = "is_false")]
    pub selected: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hovered: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub focused: bool,
}

impl NodeChromeState {
    pub fn selected() -> Self {
        Self {
            selected: true,
            hovered: false,
            focused: false,
        }
    }

    pub fn hovered() -> Self {
        Self {
            selected: false,
            hovered: true,
            focused: false,
        }
    }

    pub fn focused() -> Self {
        Self {
            selected: false,
            hovered: false,
            focused: true,
        }
    }

    pub fn is_visible(self, descriptor: &NodeChromeDescriptor) -> bool {
        descriptor.is_visible_for_state(self.selected, self.hovered, self.focused)
    }
}

/// View-independent sizing policy for semantic chrome placement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeChromeLayoutPolicy {
    pub control_size: f32,
    pub bar_height: f32,
    pub margin: f32,
    #[serde(default = "default_zoom")]
    pub zoom: f32,
}

impl Default for NodeChromeLayoutPolicy {
    fn default() -> Self {
        Self {
            control_size: DEFAULT_CONTROL_SIZE,
            bar_height: DEFAULT_BAR_HEIGHT,
            margin: DEFAULT_MARGIN,
            zoom: 1.0,
        }
    }
}

impl NodeChromeLayoutPolicy {
    pub fn with_control_size(mut self, control_size: f32) -> Self {
        self.control_size = control_size;
        self
    }

    pub fn with_bar_height(mut self, bar_height: f32) -> Self {
        self.bar_height = bar_height;
        self
    }

    pub fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }

    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    fn scale(self) -> f32 {
        if self.zoom.is_finite() && self.zoom > 0.0 {
            (1.0 / self.zoom).max(MIN_ZOOM_SCALE)
        } else {
            1.0
        }
    }

    fn normalized(self) -> Self {
        let scale = self.scale();
        Self {
            control_size: positive_or_default(self.control_size, DEFAULT_CONTROL_SIZE) * scale,
            bar_height: positive_or_default(self.bar_height, DEFAULT_BAR_HEIGHT) * scale,
            margin: positive_or_default(self.margin, DEFAULT_MARGIN) * scale,
            zoom: self.zoom,
        }
    }
}

/// Headless request to resolve adapter-owned chrome around one node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeChromeFactsRequest<'a> {
    pub node: NodeId,
    pub node_rect: CanvasRect,
    pub descriptors: &'a [NodeChromeDescriptor],
    pub state: NodeChromeState,
    pub policy: NodeChromeLayoutPolicy,
    pub resize_constraints: NodeResizeConstraints,
}

impl<'a> NodeChromeFactsRequest<'a> {
    pub fn new(
        node: NodeId,
        node_rect: CanvasRect,
        descriptors: &'a [NodeChromeDescriptor],
    ) -> Self {
        Self {
            node,
            node_rect,
            descriptors,
            state: NodeChromeState::default(),
            policy: NodeChromeLayoutPolicy::default(),
            resize_constraints: NodeResizeConstraints::default(),
        }
    }

    pub fn with_state(mut self, state: NodeChromeState) -> Self {
        self.state = state;
        self
    }

    pub fn with_policy(mut self, policy: NodeChromeLayoutPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_resize_constraints(mut self, constraints: NodeResizeConstraints) -> Self {
        self.resize_constraints = constraints;
        self
    }
}

/// Resolved adapter-owned chrome facts for one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeChromeFacts {
    pub node: NodeId,
    pub chrome: Vec<ResolvedNodeChrome>,
}

impl NodeChromeFacts {
    pub fn is_empty(&self) -> bool {
        self.chrome.is_empty()
    }

    pub fn get(&self, key: impl AsRef<str>) -> Option<&ResolvedNodeChrome> {
        let key = key.as_ref();
        self.chrome.iter().find(|chrome| chrome.key == key)
    }
}

/// One visible semantic chrome affordance resolved in canvas space against node bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedNodeChrome {
    pub key: String,
    pub kind: NodeChromeKind,
    pub placement: NodeChromePlacement,
    pub rect: CanvasRect,
    pub visibility: NodeChromeVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interactive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize_constraints: Option<NodeResizeConstraints>,
}

impl ResolvedNodeChrome {
    pub fn contains_point(&self, point: CanvasPoint) -> bool {
        point.x >= self.rect.origin.x
            && point.x <= self.rect.origin.x + self.rect.size.width
            && point.y >= self.rect.origin.y
            && point.y <= self.rect.origin.y + self.rect.size.height
    }
}

/// Resolves semantic node chrome into canvas-space geometry facts.
pub fn resolve_node_chrome_facts(request: NodeChromeFactsRequest<'_>) -> Option<NodeChromeFacts> {
    if !request.node_rect.is_positive_finite() {
        return None;
    }

    let policy = request.policy.normalized();
    let mut chrome: Vec<_> = request
        .descriptors
        .iter()
        .filter(|descriptor| request.state.is_visible(descriptor))
        .filter_map(|descriptor| {
            let rect = chrome_rect(
                request.node_rect,
                descriptor.kind,
                descriptor.placement,
                policy,
            )?;
            Some(ResolvedNodeChrome {
                key: descriptor.key.clone(),
                kind: descriptor.kind,
                placement: descriptor.placement,
                rect,
                visibility: descriptor.effective_visibility(),
                label: descriptor.label.clone(),
                renderer_key: descriptor.renderer_key.clone(),
                icon_key: descriptor.icon_key.clone(),
                order: descriptor.order,
                interactive: descriptor.interactive,
                resize_constraints: (descriptor.kind == NodeChromeKind::Resizer)
                    .then_some(request.resize_constraints),
            })
        })
        .collect();

    chrome.sort_by(|left, right| {
        left.order
            .unwrap_or(0)
            .cmp(&right.order.unwrap_or(0))
            .then_with(|| left.key.cmp(&right.key))
    });

    Some(NodeChromeFacts {
        node: request.node,
        chrome,
    })
}

fn chrome_rect(
    node_rect: CanvasRect,
    kind: NodeChromeKind,
    placement: NodeChromePlacement,
    policy: NodeChromeLayoutPolicy,
) -> Option<CanvasRect> {
    let size = chrome_size(node_rect, kind, placement, policy)?;
    let margin = policy.margin;
    let left = node_rect.origin.x;
    let top = node_rect.origin.y;
    let right = left + node_rect.size.width;
    let bottom = top + node_rect.size.height;

    let origin = match placement {
        NodeChromePlacement::Top => CanvasPoint {
            x: left + (node_rect.size.width - size.width) / 2.0,
            y: top - margin - size.height,
        },
        NodeChromePlacement::TopRight => CanvasPoint {
            x: right - size.width,
            y: top - margin - size.height,
        },
        NodeChromePlacement::Right => CanvasPoint {
            x: right + margin,
            y: top + (node_rect.size.height - size.height) / 2.0,
        },
        NodeChromePlacement::BottomRight => CanvasPoint {
            x: right - size.width,
            y: bottom + margin,
        },
        NodeChromePlacement::Bottom => CanvasPoint {
            x: left + (node_rect.size.width - size.width) / 2.0,
            y: bottom + margin,
        },
        NodeChromePlacement::BottomLeft => CanvasPoint {
            x: left,
            y: bottom + margin,
        },
        NodeChromePlacement::Left => CanvasPoint {
            x: left - margin - size.width,
            y: top + (node_rect.size.height - size.height) / 2.0,
        },
        NodeChromePlacement::TopLeft => CanvasPoint {
            x: left,
            y: top - margin - size.height,
        },
        NodeChromePlacement::InsideHeader => CanvasPoint {
            x: left + margin,
            y: top + margin,
        },
        NodeChromePlacement::InsideFooter => CanvasPoint {
            x: left + margin,
            y: bottom - margin - size.height,
        },
    };

    let rect = CanvasRect { origin, size };
    rect.is_positive_finite().then_some(rect)
}

fn chrome_size(
    node_rect: CanvasRect,
    kind: NodeChromeKind,
    placement: NodeChromePlacement,
    policy: NodeChromeLayoutPolicy,
) -> Option<CanvasSize> {
    let inline_width = (node_rect.size.width - policy.margin * 2.0).max(policy.control_size);
    let size = match kind {
        NodeChromeKind::Resizer | NodeChromeKind::InspectorAnchor => CanvasSize {
            width: policy.control_size,
            height: policy.control_size,
        },
        NodeChromeKind::Toolbar => {
            let width = match placement {
                NodeChromePlacement::Left | NodeChromePlacement::Right => policy.control_size,
                _ => node_rect
                    .size
                    .width
                    .min(160.0 * policy.scale())
                    .max(64.0 * policy.scale()),
            };
            CanvasSize {
                width,
                height: policy.bar_height,
            }
        }
        NodeChromeKind::StatusStrip
        | NodeChromeKind::ValidationBanner
        | NodeChromeKind::RunActionStrip => CanvasSize {
            width: inline_width,
            height: policy.bar_height,
        },
    };
    size.is_positive_finite().then_some(size)
}

fn positive_or_default(value: f32, default: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        default
    }
}
