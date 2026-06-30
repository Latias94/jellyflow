use std::{cell::RefCell, rc::Rc};

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};
use jellyflow::runtime::runtime::measurement::{
    MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeMeasurement,
};

/// A point reported by open-gpui in view-space pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct OpenGpuiViewPoint {
    pub x: f32,
    pub y: f32,
}

impl OpenGpuiViewPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A size reported by open-gpui in view-space pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct OpenGpuiViewSize {
    pub width: f32,
    pub height: f32,
}

impl OpenGpuiViewSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A measured open-gpui region in view-space pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct OpenGpuiViewBounds {
    pub origin: OpenGpuiViewPoint,
    pub size: OpenGpuiViewSize,
}

impl OpenGpuiViewBounds {
    pub fn new(origin: OpenGpuiViewPoint, size: OpenGpuiViewSize) -> Self {
        Self { origin, size }
    }

    pub fn from_tuples(origin: (f32, f32), size: (f32, f32)) -> Self {
        Self::new(
            OpenGpuiViewPoint::new(origin.0, origin.1),
            OpenGpuiViewSize::new(size.0, size.1),
        )
    }
}

/// Semantic kind for a measured region inside a node surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiMeasuredRegionKind {
    Slot { key: String },
    Control { key: String },
    RepeatableItem { key: String, item_id: String },
    Anchor { key: String },
}

impl OpenGpuiMeasuredRegionKind {
    pub fn key(&self) -> &str {
        match self {
            Self::Slot { key }
            | Self::Control { key }
            | Self::RepeatableItem { key, .. }
            | Self::Anchor { key } => key,
        }
    }
}

/// One GPUI layout-pass region reported by the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiMeasuredRegion {
    pub kind: OpenGpuiMeasuredRegionKind,
    pub element_id: String,
    pub global_id: Option<String>,
    pub bounds: OpenGpuiViewBounds,
}

/// Shared collector used by measured elements during a GPUI prepaint pass.
#[derive(Clone, Default)]
pub struct OpenGpuiBoundsCollector {
    regions: Rc<RefCell<Vec<OpenGpuiMeasuredRegion>>>,
}

impl OpenGpuiBoundsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(
        &self,
        kind: OpenGpuiMeasuredRegionKind,
        element_id: impl Into<String>,
        bounds: OpenGpuiViewBounds,
        global_id: Option<impl ToString>,
    ) {
        self.regions.borrow_mut().push(OpenGpuiMeasuredRegion {
            kind,
            element_id: element_id.into(),
            global_id: global_id.map(|id| id.to_string()),
            bounds,
        });
    }

    pub fn regions(&self) -> Vec<OpenGpuiMeasuredRegion> {
        self.regions.borrow().clone()
    }

    pub fn clear(&self) {
        self.regions.borrow_mut().clear();
    }
}

/// Converts GPUI view-space bounds into Jellyflow node-local measurement facts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiMeasurementContext {
    pub node: NodeId,
    pub node_view_origin: OpenGpuiViewPoint,
    pub view_to_document_scale: f32,
    pub node_size: CanvasSize,
    pub revision: u64,
}

impl OpenGpuiMeasurementContext {
    pub fn new(
        node: NodeId,
        node_view_origin: OpenGpuiViewPoint,
        view_to_document_scale: f32,
        node_size: CanvasSize,
    ) -> Self {
        Self {
            node,
            node_view_origin,
            view_to_document_scale,
            node_size,
            revision: 1,
        }
    }

    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    pub fn node_local_rect(&self, bounds: OpenGpuiViewBounds) -> CanvasRect {
        let scale = if self.view_to_document_scale == 0.0 {
            1.0
        } else {
            self.view_to_document_scale
        };
        CanvasRect {
            origin: CanvasPoint {
                x: (bounds.origin.x - self.node_view_origin.x) * scale,
                y: (bounds.origin.y - self.node_view_origin.y) * scale,
            },
            size: CanvasSize {
                width: bounds.size.width * scale,
                height: bounds.size.height * scale,
            },
        }
    }

    pub fn measurement_from_regions(
        &self,
        regions: impl IntoIterator<Item = OpenGpuiMeasuredRegion>,
        anchors: impl IntoIterator<Item = MeasuredSurfaceAnchor>,
    ) -> NodeMeasurement {
        let slots = regions
            .into_iter()
            .filter_map(|region| match region.kind {
                OpenGpuiMeasuredRegionKind::Slot { key }
                | OpenGpuiMeasuredRegionKind::Control { key }
                | OpenGpuiMeasuredRegionKind::RepeatableItem { key, .. } => Some(
                    MeasuredSurfaceSlot::new(key, self.node_local_rect(region.bounds)),
                ),
                OpenGpuiMeasuredRegionKind::Anchor { .. } => None,
            })
            .collect::<Vec<_>>();

        NodeMeasurement::new(self.node)
            .with_revision(self.revision)
            .with_size(Some(self.node_size))
            .with_slots(slots)
            .with_anchors(anchors)
    }
}

impl From<OpenGpuiViewBounds> for OpenGpuiMeasuredRegion {
    fn from(bounds: OpenGpuiViewBounds) -> Self {
        Self {
            kind: OpenGpuiMeasuredRegionKind::Slot {
                key: "region".to_string(),
            },
            element_id: "region".to_string(),
            global_id: None,
            bounds,
        }
    }
}

pub fn gpui_bounds(origin: (f32, f32), size: (f32, f32)) -> OpenGpuiViewBounds {
    OpenGpuiViewBounds::from_tuples(origin, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::runtime::runtime::geometry::HandlePosition;

    #[test]
    fn collector_keeps_nested_gpui_region_bounds() {
        let collector = OpenGpuiBoundsCollector::new();

        collector.record(
            OpenGpuiMeasuredRegionKind::Slot {
                key: "prompt".to_string(),
            },
            "prompt",
            gpui_bounds((12.0, 24.0), (80.0, 20.0)),
            None::<String>,
        );

        let regions = collector.regions();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].kind.key(), "prompt");
        assert_eq!(regions[0].bounds.size.width, 80.0);
    }

    #[test]
    fn measurement_context_converts_view_bounds_to_node_local_slots() {
        let node = NodeId::from_u128(7);
        let context = OpenGpuiMeasurementContext::new(
            node,
            OpenGpuiViewPoint::new(10.0, 20.0),
            2.0,
            CanvasSize {
                width: 200.0,
                height: 120.0,
            },
        )
        .with_revision(3);

        let measurement = context.measurement_from_regions(
            [OpenGpuiMeasuredRegion {
                kind: OpenGpuiMeasuredRegionKind::Slot {
                    key: "prompt".to_string(),
                },
                element_id: "prompt".to_string(),
                global_id: None,
                bounds: gpui_bounds((15.0, 25.0), (40.0, 10.0)),
            }],
            [MeasuredSurfaceAnchor::new(
                "prompt.anchor",
                CanvasRect {
                    origin: CanvasPoint { x: 10.0, y: 10.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 8.0,
                    },
                },
                HandlePosition::Left,
            )],
        );

        assert_eq!(measurement.node, node);
        assert_eq!(measurement.revision, 3);
        assert_eq!(measurement.slots[0].key, "prompt");
        assert_eq!(
            measurement.slots[0].rect.origin,
            CanvasPoint { x: 10.0, y: 10.0 }
        );
        assert_eq!(
            measurement.slots[0].rect.size,
            CanvasSize {
                width: 80.0,
                height: 20.0
            }
        );
        assert_eq!(measurement.anchors.len(), 1);
    }
}
