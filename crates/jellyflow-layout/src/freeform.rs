use std::cmp::Ordering;
use std::collections::BTreeMap;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use crate::engine::{
    LayoutCoordinateFrame, LayoutDirection, LayoutEngine, LayoutError, LayoutOptions,
    LayoutPlacements, PreparedLayoutEdge, PreparedLayoutInput, checked_canvas_point, checked_f32,
};
use crate::projection::placements_from_centers;

/// Built-in freeform mind-map engine that only resolves overlaps.
#[derive(Debug, Default, Clone, Copy)]
pub struct MindMapFreeformLayoutEngine;

impl LayoutEngine for MindMapFreeformLayoutEngine {
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        run_mind_map_freeform_layout(input)
    }
}

fn run_mind_map_freeform_layout(
    input: &PreparedLayoutInput<'_>,
) -> Result<LayoutPlacements, LayoutError> {
    let mut projection = FreeformProjection::new(input);
    projection.layout(input.options())?;
    projection.into_result()
}

struct FreeformProjection {
    node_infos: BTreeMap<NodeId, FreeformNodeInfo>,
    visible_edges: Vec<PreparedLayoutEdge>,
    placements: BTreeMap<NodeId, CanvasPoint>,
    preserve_canvas_coordinates: bool,
}

#[derive(Debug, Clone, Copy)]
struct FreeformNodeInfo {
    size: CanvasSize,
    current_center: CanvasPoint,
    pinned: bool,
}

impl FreeformProjection {
    fn new(input: &PreparedLayoutInput<'_>) -> Self {
        let mut node_infos = BTreeMap::new();

        for node in input.nodes() {
            node_infos.insert(
                node.id(),
                FreeformNodeInfo {
                    size: node.size(),
                    current_center: node.current_center(),
                    pinned: node.is_pinned(),
                },
            );
        }

        Self {
            node_infos,
            visible_edges: input.edges().to_vec(),
            placements: BTreeMap::new(),
            preserve_canvas_coordinates: input.has_pinned_nodes(),
        }
    }

    fn layout(&mut self, options: LayoutOptions) -> Result<(), LayoutError> {
        if self.node_infos.is_empty() {
            return Ok(());
        }

        let gap = freeform_gap(options);
        let mut node_order = self.node_infos.keys().copied().collect::<Vec<_>>();
        node_order.sort_by(|left, right| self.compare_nodes(*left, *right, options.direction));

        for node in node_order {
            let Some(info) = self.node_infos.get(&node).copied() else {
                continue;
            };

            let center = if info.pinned {
                info.current_center
            } else {
                self.resolve_node_center(node, info, gap, options.direction)?
            };
            self.placements.insert(node, center);
        }

        Ok(())
    }

    fn compare_nodes(&self, left: NodeId, right: NodeId, direction: LayoutDirection) -> Ordering {
        let (Some(left_info), Some(right_info)) =
            (self.node_infos.get(&left), self.node_infos.get(&right))
        else {
            return left.cmp(&right);
        };

        right_info
            .pinned
            .cmp(&left_info.pinned)
            .then_with(|| match direction {
                LayoutDirection::TopToBottom | LayoutDirection::BottomToTop => left_info
                    .current_center
                    .y
                    .total_cmp(&right_info.current_center.y)
                    .then_with(|| {
                        left_info
                            .current_center
                            .x
                            .total_cmp(&right_info.current_center.x)
                    }),
                LayoutDirection::LeftToRight | LayoutDirection::RightToLeft => left_info
                    .current_center
                    .x
                    .total_cmp(&right_info.current_center.x)
                    .then_with(|| {
                        left_info
                            .current_center
                            .y
                            .total_cmp(&right_info.current_center.y)
                    }),
            })
            .then_with(|| left.cmp(&right))
    }

    fn resolve_node_center(
        &self,
        node: NodeId,
        info: FreeformNodeInfo,
        gap: f32,
        direction: LayoutDirection,
    ) -> Result<CanvasPoint, LayoutError> {
        let mut center = info.current_center;
        let max_shifts = self.placements.len();

        for attempt in 0..=max_shifts {
            let rect = rect_from_center(center, info.size)
                .ok_or(LayoutError::InvalidResultNodeGeometry(node))?;
            let mut collision = None;
            for (placed_node, placed_center) in &self.placements {
                let placed_size = self
                    .node_infos
                    .get(placed_node)
                    .map(|info| info.size)
                    .ok_or(LayoutError::InvalidResultNodeGeometry(*placed_node))?;
                let other = rect_from_center(*placed_center, placed_size)
                    .ok_or(LayoutError::InvalidResultNodeGeometry(*placed_node))?;
                if rects_overlap_with_gap(rect, other, gap)
                    .ok_or(LayoutError::InvalidResultNodeGeometry(node))?
                {
                    collision = Some(other);
                    break;
                }
            }
            let Some(other) = collision else {
                return Ok(center);
            };
            if attempt == max_shifts {
                return Err(LayoutError::UnresolvedOverlap(node));
            }

            center = shift_center_out_of_rect(center, info.size, other, direction, gap)
                .ok_or(LayoutError::InvalidResultNodeGeometry(node))?;
        }

        Err(LayoutError::UnresolvedOverlap(node))
    }

    fn into_result(self) -> Result<LayoutPlacements, LayoutError> {
        let coordinate_frame = if self.preserve_canvas_coordinates {
            LayoutCoordinateFrame::PreserveCanvasCoordinates
        } else {
            LayoutCoordinateFrame::NormalizeToMargin
        };
        placements_from_centers(self.placements, &self.visible_edges, coordinate_frame)
    }
}

fn freeform_gap(options: LayoutOptions) -> f32 {
    options.spacing.nodesep.max(24.0)
}

fn rect_from_center(center: CanvasPoint, size: CanvasSize) -> Option<CanvasRect> {
    let rect = CanvasRect {
        origin: checked_canvas_point(
            f64::from(center.x) - f64::from(size.width) * 0.5,
            f64::from(center.y) - f64::from(size.height) * 0.5,
        )?,
        size,
    };
    rect.is_positive_finite().then_some(rect)
}

fn shift_center_out_of_rect(
    center: CanvasPoint,
    size: CanvasSize,
    other: CanvasRect,
    direction: LayoutDirection,
    gap: f32,
) -> Option<CanvasPoint> {
    let rect = rect_from_center(center, size)?;
    let expanded = expand_rect(other, gap)?;
    let rect_max = rect.checked_max()?;
    let expanded_max = expanded.checked_max()?;

    match direction {
        LayoutDirection::TopToBottom => checked_canvas_point(
            f64::from(center.x),
            f64::from(center.y) + f64::from(expanded_max.y) - f64::from(rect.origin.y),
        ),
        LayoutDirection::BottomToTop => checked_canvas_point(
            f64::from(center.x),
            f64::from(center.y) - (f64::from(rect_max.y) - f64::from(expanded.origin.y)),
        ),
        LayoutDirection::LeftToRight => checked_canvas_point(
            f64::from(center.x) + f64::from(expanded_max.x) - f64::from(rect.origin.x),
            f64::from(center.y),
        ),
        LayoutDirection::RightToLeft => checked_canvas_point(
            f64::from(center.x) - (f64::from(rect_max.x) - f64::from(expanded.origin.x)),
            f64::from(center.y),
        ),
    }
}

fn rects_overlap_with_gap(candidate: CanvasRect, other: CanvasRect, gap: f32) -> Option<bool> {
    let other = expand_rect(other, gap)?;
    let candidate_max = candidate.checked_max()?;
    let other_max = other.checked_max()?;

    Some(
        candidate.origin.x < other_max.x
            && candidate_max.x > other.origin.x
            && candidate.origin.y < other_max.y
            && candidate_max.y > other.origin.y,
    )
}

fn expand_rect(rect: CanvasRect, gap: f32) -> Option<CanvasRect> {
    if !gap.is_finite() || gap < 0.0 {
        return None;
    }
    let expanded = CanvasRect {
        origin: checked_canvas_point(
            f64::from(rect.origin.x) - f64::from(gap),
            f64::from(rect.origin.y) - f64::from(gap),
        )?,
        size: CanvasSize {
            width: checked_f32(f64::from(rect.size.width) + f64::from(gap) * 2.0)?,
            height: checked_f32(f64::from(rect.size.height) + f64::from(gap) * 2.0)?,
        },
    };
    expanded.is_positive_finite().then_some(expanded)
}
