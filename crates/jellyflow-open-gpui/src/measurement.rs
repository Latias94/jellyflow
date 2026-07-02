use std::{cell::RefCell, collections::HashSet, rc::Rc};

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};
use jellyflow::runtime::runtime::measurement::{
    MeasuredSurfaceAnchor, MeasuredSurfaceSlot, NodeMeasurement, NodeMeasurementStatus,
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

    pub fn is_positive_finite(self) -> bool {
        self.origin.x.is_finite()
            && self.origin.y.is_finite()
            && self.size.width.is_finite()
            && self.size.height.is_finite()
            && self.size.width > 0.0
            && self.size.height > 0.0
    }
}

/// Semantic kind for a measured region inside a node surface.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OpenGpuiMeasuredRegionKind {
    Slot { key: String },
    Control { key: String, scope: Option<String> },
    RepeatableItem { key: String, item_id: String },
    Anchor { key: String },
}

impl OpenGpuiMeasuredRegionKind {
    pub fn key(&self) -> &str {
        match self {
            Self::Slot { key }
            | Self::Control { key, .. }
            | Self::RepeatableItem { key, .. }
            | Self::Anchor { key } => key,
        }
    }
}

/// Stable semantic identifier attached to a GPUI measured element.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpenGpuiMeasurementId {
    node: NodeId,
    kind: OpenGpuiMeasuredRegionKind,
}

impl OpenGpuiMeasurementId {
    pub fn slot(node: NodeId, key: impl Into<String>) -> Self {
        Self {
            node,
            kind: OpenGpuiMeasuredRegionKind::Slot { key: key.into() },
        }
    }

    pub fn control(node: NodeId, key: impl Into<String>) -> Self {
        Self {
            node,
            kind: OpenGpuiMeasuredRegionKind::Control {
                key: key.into(),
                scope: None,
            },
        }
    }

    pub fn control_in_slot(
        node: NodeId,
        slot_key: impl AsRef<str>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            node,
            kind: OpenGpuiMeasuredRegionKind::Control {
                key: key.into(),
                scope: Some(slot_key.as_ref().to_owned()),
            },
        }
    }

    pub fn repeatable_item(
        node: NodeId,
        key: impl Into<String>,
        item_id: impl Into<String>,
    ) -> Self {
        Self {
            node,
            kind: OpenGpuiMeasuredRegionKind::RepeatableItem {
                key: key.into(),
                item_id: item_id.into(),
            },
        }
    }

    pub fn anchor(node: NodeId, key: impl Into<String>) -> Self {
        Self {
            node,
            kind: OpenGpuiMeasuredRegionKind::Anchor { key: key.into() },
        }
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn kind(&self) -> &OpenGpuiMeasuredRegionKind {
        &self.kind
    }

    pub fn element_id(&self) -> String {
        match &self.kind {
            OpenGpuiMeasuredRegionKind::Slot { key } => {
                format!("jellyflow-node:{}:slot:{key}", self.node.0)
            }
            OpenGpuiMeasuredRegionKind::Control { key, scope } => {
                if let Some(scope) = scope {
                    format!("jellyflow-node:{}:control:{scope}:{key}", self.node.0)
                } else {
                    format!("jellyflow-node:{}:control:{key}", self.node.0)
                }
            }
            OpenGpuiMeasuredRegionKind::RepeatableItem { key, item_id } => {
                format!(
                    "jellyflow-node:{}:repeatable:{key}:item:{item_id}",
                    self.node.0
                )
            }
            OpenGpuiMeasuredRegionKind::Anchor { key } => {
                format!("jellyflow-node:{}:anchor:{key}", self.node.0)
            }
        }
    }

    pub fn into_region_kind(self) -> OpenGpuiMeasuredRegionKind {
        self.kind
    }

    pub fn into_region(self, bounds: OpenGpuiViewBounds) -> OpenGpuiMeasuredRegion {
        let node = self.node;
        let element_id = self.element_id();
        OpenGpuiMeasuredRegion::layout_pass(self.kind, element_id, bounds, None::<String>)
            .for_node(node)
    }
}

/// Source coverage for a semantic GPUI measurement region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiMeasurementSource {
    LayoutPass,
    ProjectionFallback,
}

/// One GPUI layout-pass region reported by the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiMeasuredRegion {
    pub node: Option<NodeId>,
    pub kind: OpenGpuiMeasuredRegionKind,
    pub element_id: String,
    pub global_id: Option<String>,
    pub bounds: OpenGpuiViewBounds,
    pub source: OpenGpuiMeasurementSource,
}

impl OpenGpuiMeasuredRegion {
    pub fn layout_pass(
        kind: OpenGpuiMeasuredRegionKind,
        element_id: impl Into<String>,
        bounds: OpenGpuiViewBounds,
        global_id: Option<impl ToString>,
    ) -> Self {
        Self {
            node: None,
            kind,
            element_id: element_id.into(),
            global_id: global_id.map(|id| id.to_string()),
            bounds,
            source: OpenGpuiMeasurementSource::LayoutPass,
        }
    }

    pub fn for_node(mut self, node: NodeId) -> Self {
        self.node = Some(node);
        self
    }
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
        self.regions
            .borrow_mut()
            .push(OpenGpuiMeasuredRegion::layout_pass(
                kind, element_id, bounds, global_id,
            ));
    }

    pub fn record_id(
        &self,
        id: OpenGpuiMeasurementId,
        bounds: OpenGpuiViewBounds,
        global_id: Option<impl ToString>,
    ) {
        let node = id.node();
        let element_id = id.element_id();
        self.regions.borrow_mut().push(
            OpenGpuiMeasuredRegion::layout_pass(
                id.into_region_kind(),
                element_id,
                bounds,
                global_id,
            )
            .for_node(node),
        );
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
        let regions = unique_positive_regions(regions).collect::<Vec<_>>();
        let slots = regions
            .iter()
            .filter_map(|region| measured_slot_from_region(self, region))
            .collect::<Vec<_>>();
        let anchors = measured_anchors_from_regions(self, regions.iter(), anchors);

        NodeMeasurement::new(self.node)
            .with_revision(self.revision)
            .with_size(Some(self.node_size))
            .with_slots(slots)
            .with_anchors(anchors)
    }
}

fn measured_slot_from_region(
    context: &OpenGpuiMeasurementContext,
    region: &OpenGpuiMeasuredRegion,
) -> Option<MeasuredSurfaceSlot> {
    match &region.kind {
        OpenGpuiMeasuredRegionKind::Slot { key }
        | OpenGpuiMeasuredRegionKind::Control { key, .. }
        | OpenGpuiMeasuredRegionKind::RepeatableItem { key, .. } => Some(MeasuredSurfaceSlot::new(
            key.clone(),
            context.node_local_rect(region.bounds),
        )),
        OpenGpuiMeasuredRegionKind::Anchor { .. } => None,
    }
}

fn measured_anchors_from_regions<'a>(
    context: &OpenGpuiMeasurementContext,
    regions: impl IntoIterator<Item = &'a OpenGpuiMeasuredRegion>,
    fallback_anchors: impl IntoIterator<Item = MeasuredSurfaceAnchor>,
) -> Vec<MeasuredSurfaceAnchor> {
    let anchor_regions = regions
        .into_iter()
        .filter_map(|region| match &region.kind {
            OpenGpuiMeasuredRegionKind::Anchor { key } => {
                Some((key.clone(), context.node_local_rect(region.bounds)))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    fallback_anchors
        .into_iter()
        .map(|anchor| {
            anchor_regions
                .iter()
                .find(|(key, _)| {
                    key == &anchor.anchor
                        || anchor
                            .port_key
                            .as_ref()
                            .is_some_and(|port_key| key == &port_key.0)
                })
                .map(|(_, rect)| {
                    MeasuredSurfaceAnchor::new(anchor.anchor.clone(), *rect, anchor.position)
                        .with_visibility(anchor.visibility)
                })
                .map(|mut measured| {
                    if let Some(port) = anchor.port {
                        measured = measured.with_port(port);
                    }
                    if let Some(port_key) = anchor.port_key.clone() {
                        measured = measured.with_port_key(port_key);
                    }
                    measured
                })
                .unwrap_or(anchor)
        })
        .collect()
}

/// Adapter-level coverage facts for one measurement publish.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenGpuiMeasurementCoverage {
    pub layout_pass_regions: usize,
    pub projection_fallback_regions: usize,
    pub missing_regions: usize,
    pub stale_regions: usize,
    pub partial_regions: usize,
    pub duplicate_regions: usize,
    pub measured_slots: usize,
    pub measured_anchors: usize,
}

impl OpenGpuiMeasurementCoverage {
    pub fn from_regions(
        regions: impl IntoIterator<Item = OpenGpuiMeasuredRegion>,
        measurement: &NodeMeasurement,
    ) -> Self {
        let mut layout_pass_regions = 0;
        let mut projection_fallback_regions = 0;
        let mut missing_regions = 0;
        let mut duplicate_regions = 0;
        let mut seen = HashSet::new();
        for region in regions {
            if !region.bounds.is_positive_finite() {
                missing_regions += 1;
                continue;
            }
            if !seen.insert(region.element_id.clone()) {
                duplicate_regions += 1;
                continue;
            }
            match region.source {
                OpenGpuiMeasurementSource::LayoutPass => layout_pass_regions += 1,
                OpenGpuiMeasurementSource::ProjectionFallback => projection_fallback_regions += 1,
            }
        }

        Self {
            layout_pass_regions,
            projection_fallback_regions,
            missing_regions,
            stale_regions: 0,
            partial_regions: 0,
            duplicate_regions,
            measured_slots: measurement.slots.len(),
            measured_anchors: measurement.anchors.len(),
        }
    }

    pub fn is_full_layout_pass(&self) -> bool {
        self.layout_pass_regions > 0
            && self.projection_fallback_regions == 0
            && self.missing_regions == 0
            && self.stale_regions == 0
            && self.partial_regions == 0
            && self.duplicate_regions == 0
    }
}

fn unique_positive_regions(
    regions: impl IntoIterator<Item = OpenGpuiMeasuredRegion>,
) -> impl Iterator<Item = OpenGpuiMeasuredRegion> {
    let mut seen = HashSet::new();
    regions.into_iter().filter(move |region| {
        region.bounds.is_positive_finite() && seen.insert(region.element_id.clone())
    })
}

/// Build a GPUI layout-pass measurement and coverage report from collected regions.
pub fn layout_pass_measurement_from_regions(
    context: OpenGpuiMeasurementContext,
    regions: impl IntoIterator<Item = OpenGpuiMeasuredRegion>,
    fallback_anchors: impl IntoIterator<Item = MeasuredSurfaceAnchor>,
) -> (NodeMeasurement, OpenGpuiMeasurementCoverage) {
    let regions = regions.into_iter().collect::<Vec<_>>();
    let fallback_anchors = fallback_anchors.into_iter().collect::<Vec<_>>();
    let measurement = context.measurement_from_regions(regions.clone(), fallback_anchors.clone());
    let mut coverage = OpenGpuiMeasurementCoverage::from_regions(regions.clone(), &measurement);
    coverage.projection_fallback_regions += fallback_anchors
        .iter()
        .filter(|anchor| !has_live_anchor_region(&regions, anchor))
        .count();
    (measurement, coverage)
}

/// Result of assigning a stable layout-pass measurement revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiMeasurementRevisionDecision {
    Reused { revision: u64 },
    Advanced { revision: u64 },
}

/// Reuses a fresh revision for identical node internals or advances `next_revision`.
///
/// Hosts call this after converting GPUI layout-pass bounds into a [`NodeMeasurement`]. Keeping
/// this policy here prevents every Open GPUI surface from reimplementing stale/fresh comparison
/// rules and avoids unnecessary handle invalidation when layout bounds did not change.
pub fn assign_layout_pass_measurement_revision(
    status: NodeMeasurementStatus,
    existing: Option<&NodeMeasurement>,
    measurement: &mut NodeMeasurement,
    next_revision: &mut u64,
) -> OpenGpuiMeasurementRevisionDecision {
    if status.is_fresh()
        && let Some(existing) = existing
        && open_gpui_measurement_regions_match(existing, measurement)
    {
        measurement.revision = existing.revision;
        return OpenGpuiMeasurementRevisionDecision::Reused {
            revision: existing.revision,
        };
    }

    let floor = existing
        .map(|measurement| measurement.revision)
        .unwrap_or(0);
    *next_revision = (*next_revision).max(floor).saturating_add(1);
    measurement.revision = *next_revision;
    OpenGpuiMeasurementRevisionDecision::Advanced {
        revision: *next_revision,
    }
}

/// Compares the region-bearing parts of two node-internal measurements.
pub fn open_gpui_measurement_regions_match(
    left: &NodeMeasurement,
    right: &NodeMeasurement,
) -> bool {
    left.node == right.node
        && left.density == right.density
        && left.size == right.size
        && left.handles == right.handles
        && left.slots == right.slots
        && left.anchors == right.anchors
}

fn has_live_anchor_region(
    regions: &[OpenGpuiMeasuredRegion],
    anchor: &MeasuredSurfaceAnchor,
) -> bool {
    regions.iter().any(|region| {
        region.source == OpenGpuiMeasurementSource::LayoutPass
            && region.bounds.is_positive_finite()
            && matches!(
                &region.kind,
                OpenGpuiMeasuredRegionKind::Anchor { key }
                    if key == &anchor.anchor
                        || anchor
                            .port_key
                            .as_ref()
                            .is_some_and(|port_key| key == &port_key.0)
            )
    })
}

impl From<OpenGpuiViewBounds> for OpenGpuiMeasuredRegion {
    fn from(bounds: OpenGpuiViewBounds) -> Self {
        Self {
            node: None,
            kind: OpenGpuiMeasuredRegionKind::Slot {
                key: "region".to_string(),
            },
            element_id: "region".to_string(),
            global_id: None,
            bounds,
            source: OpenGpuiMeasurementSource::LayoutPass,
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
                node: None,
                kind: OpenGpuiMeasuredRegionKind::Slot {
                    key: "prompt".to_string(),
                },
                element_id: "prompt".to_string(),
                global_id: None,
                bounds: gpui_bounds((15.0, 25.0), (40.0, 10.0)),
                source: OpenGpuiMeasurementSource::LayoutPass,
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

    #[test]
    fn semantic_measurement_ids_are_stable_and_descriptor_backed() {
        let node = NodeId::from_u128(42);
        let slot = OpenGpuiMeasurementId::slot(node, "field.prompt");
        let control = OpenGpuiMeasurementId::control(node, "control.model");
        let scoped_control =
            OpenGpuiMeasurementId::control_in_slot(node, "field.prompt", "control.model");
        let item = OpenGpuiMeasurementId::repeatable_item(node, "shader.inputs", "factor");
        let anchor = OpenGpuiMeasurementId::anchor(node, "field.completion");

        assert_eq!(slot.node(), node);
        assert_eq!(
            slot.element_id(),
            format!("jellyflow-node:{}:slot:field.prompt", node.0)
        );
        assert_eq!(
            control.element_id(),
            format!("jellyflow-node:{}:control:control.model", node.0)
        );
        assert_eq!(
            scoped_control.element_id(),
            format!(
                "jellyflow-node:{}:control:field.prompt:control.model",
                node.0
            )
        );
        assert!(matches!(
            scoped_control.kind(),
            OpenGpuiMeasuredRegionKind::Control { key, scope }
                if key == "control.model" && scope.as_deref() == Some("field.prompt")
        ));
        assert_eq!(
            item.element_id(),
            format!(
                "jellyflow-node:{}:repeatable:shader.inputs:item:factor",
                node.0
            )
        );
        assert_eq!(
            anchor.element_id(),
            format!("jellyflow-node:{}:anchor:field.completion", node.0)
        );
        assert!(matches!(
            item.kind(),
            OpenGpuiMeasuredRegionKind::RepeatableItem { key, item_id }
                if key == "shader.inputs" && item_id == "factor"
        ));
    }

    #[test]
    fn collector_records_semantic_ids_as_layout_pass_regions() {
        let collector = OpenGpuiBoundsCollector::new();
        let id = OpenGpuiMeasurementId::slot(NodeId::from_u128(9), "field.prompt");

        collector.record_id(id, gpui_bounds((4.0, 8.0), (32.0, 16.0)), None::<String>);

        let regions = collector.regions();
        assert_eq!(regions.len(), 1);
        assert_eq!(
            regions[0].element_id,
            format!(
                "jellyflow-node:{}:slot:field.prompt",
                NodeId::from_u128(9).0
            )
        );
        assert_eq!(regions[0].source, OpenGpuiMeasurementSource::LayoutPass);
        assert_eq!(regions[0].node, Some(NodeId::from_u128(9)));
    }

    #[test]
    fn layout_pass_anchor_regions_override_projection_fallback_rects() {
        let node = NodeId::from_u128(11);
        let context = OpenGpuiMeasurementContext::new(
            node,
            OpenGpuiViewPoint::new(100.0, 50.0),
            1.0,
            CanvasSize {
                width: 240.0,
                height: 160.0,
            },
        )
        .with_revision(8);
        let fallback_anchor = MeasuredSurfaceAnchor::new(
            "field.prompt",
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 20.0 },
                size: CanvasSize {
                    width: 240.0,
                    height: 20.0,
                },
            },
            HandlePosition::Left,
        )
        .with_port_key("prompt");
        let (measurement, coverage) = layout_pass_measurement_from_regions(
            context,
            [
                OpenGpuiMeasuredRegion::layout_pass(
                    OpenGpuiMeasuredRegionKind::Slot {
                        key: "field.prompt".to_string(),
                    },
                    "slot",
                    gpui_bounds((112.0, 70.0), (80.0, 24.0)),
                    None::<String>,
                ),
                OpenGpuiMeasuredRegion::layout_pass(
                    OpenGpuiMeasuredRegionKind::Anchor {
                        key: "field.prompt".to_string(),
                    },
                    "anchor",
                    gpui_bounds((100.0, 72.0), (240.0, 18.0)),
                    None::<String>,
                ),
            ],
            [fallback_anchor],
        );

        assert_eq!(measurement.revision, 8);
        assert_eq!(measurement.slots[0].key, "field.prompt");
        assert_eq!(
            measurement.slots[0].rect,
            CanvasRect {
                origin: CanvasPoint { x: 12.0, y: 20.0 },
                size: CanvasSize {
                    width: 80.0,
                    height: 24.0,
                },
            }
        );
        assert_eq!(
            measurement.anchors[0].rect,
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 22.0 },
                size: CanvasSize {
                    width: 240.0,
                    height: 18.0,
                },
            }
        );
        assert_eq!(
            measurement.anchors[0].port_key.as_ref().unwrap().0,
            "prompt"
        );
        assert_eq!(coverage.layout_pass_regions, 2);
        assert_eq!(coverage.projection_fallback_regions, 0);
        assert_eq!(coverage.measured_slots, 1);
        assert_eq!(coverage.measured_anchors, 1);
        assert!(coverage.is_full_layout_pass());
    }

    #[test]
    fn layout_pass_coverage_counts_unmeasured_fallback_anchors() {
        let node = NodeId::from_u128(12);
        let context = OpenGpuiMeasurementContext::new(
            node,
            OpenGpuiViewPoint::new(100.0, 50.0),
            1.0,
            CanvasSize {
                width: 240.0,
                height: 160.0,
            },
        );
        let (_, coverage) = layout_pass_measurement_from_regions(
            context,
            [OpenGpuiMeasurementId::slot(node, "prompt")
                .into_region(gpui_bounds((120.0, 80.0), (90.0, 24.0)))],
            [MeasuredSurfaceAnchor::new(
                "prompt.anchor",
                CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 40.0 },
                    size: CanvasSize {
                        width: 16.0,
                        height: 16.0,
                    },
                },
                HandlePosition::Left,
            )],
        );

        assert_eq!(coverage.layout_pass_regions, 1);
        assert_eq!(coverage.projection_fallback_regions, 1);
        assert!(!coverage.is_full_layout_pass());
    }

    #[test]
    fn layout_pass_measurement_filters_zero_size_and_duplicate_regions() {
        let node = NodeId::from_u128(12);
        let context = OpenGpuiMeasurementContext::new(
            node,
            OpenGpuiViewPoint::new(0.0, 0.0),
            1.0,
            CanvasSize {
                width: 200.0,
                height: 120.0,
            },
        );
        let (measurement, coverage) = layout_pass_measurement_from_regions(
            context,
            [
                OpenGpuiMeasuredRegion::layout_pass(
                    OpenGpuiMeasuredRegionKind::Slot {
                        key: "field.visible".to_string(),
                    },
                    "slot.visible",
                    gpui_bounds((8.0, 10.0), (80.0, 24.0)),
                    None::<String>,
                ),
                OpenGpuiMeasuredRegion::layout_pass(
                    OpenGpuiMeasuredRegionKind::Slot {
                        key: "field.zero".to_string(),
                    },
                    "slot.zero",
                    gpui_bounds((8.0, 40.0), (0.0, 24.0)),
                    None::<String>,
                ),
                OpenGpuiMeasuredRegion::layout_pass(
                    OpenGpuiMeasuredRegionKind::Control {
                        key: "control.duplicate".to_string(),
                        scope: None,
                    },
                    "slot.visible",
                    gpui_bounds((12.0, 12.0), (40.0, 16.0)),
                    None::<String>,
                ),
            ],
            [],
        );

        assert_eq!(measurement.slots.len(), 1);
        assert_eq!(measurement.slots[0].key, "field.visible");
        assert_eq!(coverage.layout_pass_regions, 1);
        assert_eq!(coverage.missing_regions, 1);
        assert_eq!(coverage.duplicate_regions, 1);
        assert!(!coverage.is_full_layout_pass());
    }

    #[test]
    fn identical_fresh_layout_pass_measurement_reuses_revision() {
        let node = NodeId::from_u128(77);
        let mut next_revision = 12;
        let mut measurement = NodeMeasurement::new(node)
            .with_revision(0)
            .with_size(Some(CanvasSize {
                width: 220.0,
                height: 160.0,
            }))
            .with_anchors([MeasuredSurfaceAnchor::new(
                "field.prompt",
                CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 24.0 },
                    size: CanvasSize {
                        width: 16.0,
                        height: 18.0,
                    },
                },
                HandlePosition::Left,
            )]);
        let existing = measurement.clone().with_revision(12);

        let decision = assign_layout_pass_measurement_revision(
            NodeMeasurementStatus::Fresh { revision: 12 },
            Some(&existing),
            &mut measurement,
            &mut next_revision,
        );

        assert_eq!(
            decision,
            OpenGpuiMeasurementRevisionDecision::Reused { revision: 12 }
        );
        assert_eq!(measurement.revision, 12);
        assert_eq!(next_revision, 12);
    }

    #[test]
    fn dirty_layout_pass_measurement_advances_revision_even_when_regions_match() {
        let node = NodeId::from_u128(78);
        let mut next_revision = 12;
        let mut measurement = NodeMeasurement::new(node).with_revision(0);
        let existing = measurement.clone().with_revision(12);

        let decision = assign_layout_pass_measurement_revision(
            NodeMeasurementStatus::Dirty {
                revision: 12,
                reason: jellyflow::runtime::runtime::measurement::NodeInternalsInvalidationReason::DataChanged,
            },
            Some(&existing),
            &mut measurement,
            &mut next_revision,
        );

        assert_eq!(
            decision,
            OpenGpuiMeasurementRevisionDecision::Advanced { revision: 13 }
        );
        assert_eq!(measurement.revision, 13);
        assert_eq!(next_revision, 13);
    }
}
