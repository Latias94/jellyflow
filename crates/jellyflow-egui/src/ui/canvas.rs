use eframe::egui::{
    Align2, Color32, CornerRadius, Key, Pos2, Rect, Response, Sense, Stroke, StrokeKind, TextStyle,
    Ui, Vec2,
};
use eframe::epaint::{CubicBezierShape, PathShape, Shape};
use jellyflow::core::{CanvasPoint, CanvasRect, NodeId, PortDirection};
use jellyflow::runtime::runtime::connection::ConnectionHandleRef;
use jellyflow::runtime::runtime::geometry::EdgePath;
use jellyflow::runtime::runtime::geometry::{EdgeHitTestOptions, edge_path_contains_point};
use jellyflow::runtime::runtime::resize::{NodeResizeDirection, NodeResizePlan};

use crate::bridge::{JellyflowEguiBridge, NodeRendererStyle};
use crate::state::{ActiveCanvasInteraction, CanvasTool, HoverTarget, JellyflowEguiState};

const NODE_ROUNDING: f32 = 8.0;
const CANVAS_BG: Color32 = Color32::from_rgb(248, 249, 251);
const GRID_MINOR: Color32 = Color32::from_rgb(229, 233, 238);
const GRID_MAJOR: Color32 = Color32::from_rgb(210, 216, 223);
const EDGE_COLOR: Color32 = Color32::from_rgb(107, 114, 128);
const EDGE_HOVER_COLOR: Color32 = Color32::from_rgb(59, 130, 246);
const EDGE_INVALID_COLOR: Color32 = Color32::from_rgb(220, 76, 76);
const HANDLE_COLOR: Color32 = Color32::from_rgb(255, 255, 255);
const RESIZE_HANDLE_SIZE: f32 = 8.0;

pub fn show_canvas(ui: &mut Ui, bridge: &mut JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);

    draw_background(&painter, rect);
    draw_edges(&painter, bridge, state);
    draw_nodes(&painter, bridge, state);
    draw_interaction_preview(&painter, state);
    draw_selection(&painter, state);

    handle_pointer(ui, &response, bridge, state);
}

fn draw_background(painter: &eframe::egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 0.0, CANVAS_BG);
    let step = 40.0;
    let major_step = 200.0;
    let min = rect.min;
    let max = rect.max;
    let mut x = (min.x / step).floor() * step;
    while x <= max.x {
        let is_major = ((x / major_step).round() - x / major_step).abs() < 0.01;
        painter.line_segment(
            [Pos2::new(x, min.y), Pos2::new(x, max.y)],
            Stroke::new(
                if is_major { 1.0 } else { 0.5 },
                if is_major { GRID_MAJOR } else { GRID_MINOR },
            ),
        );
        x += step;
    }
    let mut y = (min.y / step).floor() * step;
    while y <= max.y {
        let is_major = ((y / major_step).round() - y / major_step).abs() < 0.01;
        painter.line_segment(
            [Pos2::new(min.x, y), Pos2::new(max.x, y)],
            Stroke::new(
                if is_major { 1.0 } else { 0.5 },
                if is_major { GRID_MAJOR } else { GRID_MINOR },
            ),
        );
        y += step;
    }
}

fn draw_nodes(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
) {
    for node_id in &state.canvas.snapshot.visible_node_render_order {
        let Some(rect) = state.canvas.snapshot.node_screen_rect(*node_id) else {
            continue;
        };
        let Some(descriptor) = bridge.descriptor_for_node(*node_id) else {
            continue;
        };
        let style = bridge.renderers().style_for_descriptor(&descriptor);
        let is_selected = bridge.store().view_state().selected_nodes.contains(node_id);
        painter.rect_filled(rect, CornerRadius::same(NODE_ROUNDING as u8), style.fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(NODE_ROUNDING as u8),
            if is_selected {
                style.selected_stroke()
            } else {
                Stroke::new(1.0, style.stroke)
            },
            StrokeKind::Outside,
        );
        painter.text(
            rect.center_top() + Vec2::new(0.0, 14.0),
            Align2::CENTER_TOP,
            descriptor.title,
            TextStyle::Button.resolve(&painter.ctx().global_style()),
            style.text,
        );
        if let Some(summary) = node_summary(bridge, *node_id) {
            painter.text(
                rect.center_top() + Vec2::new(0.0, 34.0),
                Align2::CENTER_TOP,
                summary,
                TextStyle::Small.resolve(&painter.ctx().global_style()),
                style.text.gamma_multiply(0.85),
            );
        }

        draw_handles(painter, state, *node_id, style);
        if is_selected {
            draw_resize_handles(painter, rect, style);
        }
    }
}

fn node_summary(bridge: &JellyflowEguiBridge, node_id: NodeId) -> Option<String> {
    let node = bridge.store().graph().nodes().get(&node_id)?;
    let summary = node
        .data
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    (!summary.is_empty()).then(|| summary.to_owned())
}

fn draw_handles(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    node_id: NodeId,
    style: NodeRendererStyle,
) {
    let handles = state
        .canvas
        .snapshot
        .handle_bounds
        .iter()
        .filter(|(handle, _)| handle.node == node_id);
    for (handle, _) in handles {
        let Some(handle_rect) = state.canvas.snapshot.handle_screen_rect(*handle) else {
            continue;
        };
        painter.rect_filled(handle_rect, CornerRadius::same(5), HANDLE_COLOR);
        painter.rect_stroke(
            handle_rect,
            CornerRadius::same(5),
            Stroke::new(1.0, style.accent),
            StrokeKind::Outside,
        );
        let label = match handle.direction {
            PortDirection::In => "<",
            PortDirection::Out => ">",
        };
        painter.text(
            handle_rect.center(),
            Align2::CENTER_CENTER,
            label,
            TextStyle::Body.resolve(&painter.ctx().global_style()),
            style.accent,
        );
    }
}

fn draw_resize_handles(painter: &eframe::egui::Painter, rect: Rect, style: NodeRendererStyle) {
    for direction in resize_directions() {
        let handle = resize_handle_rect(rect, direction);
        painter.rect_filled(handle, CornerRadius::same(2), style.accent);
        painter.rect_stroke(
            handle,
            CornerRadius::same(2),
            Stroke::new(1.0, Color32::WHITE),
            StrokeKind::Outside,
        );
    }
}

fn draw_edges(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
) {
    for edge_id in &state.canvas.snapshot.visible_edge_render_order {
        let Some(path) = state.canvas.snapshot.edge_paths.get(edge_id) else {
            continue;
        };
        let color = if bridge.store().view_state().selected_edges.contains(edge_id) {
            EDGE_HOVER_COLOR
        } else {
            EDGE_COLOR
        };
        draw_edge_path(painter, state, path, color);
    }
}

fn draw_edge_path(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    path: &EdgePath,
    color: Color32,
) {
    let stroke = Stroke::new(2.0, color);
    match path.commands.as_slice() {
        [
            jellyflow::runtime::runtime::geometry::PathCommand::MoveTo(a),
            jellyflow::runtime::runtime::geometry::PathCommand::LineTo(b),
        ] => {
            painter.line_segment([to_screen(state, *a), to_screen(state, *b)], stroke);
        }
        [
            jellyflow::runtime::runtime::geometry::PathCommand::MoveTo(a),
            jellyflow::runtime::runtime::geometry::PathCommand::CubicTo {
                control1,
                control2,
                to,
            },
        ] => {
            painter.add(Shape::CubicBezier(CubicBezierShape::from_points_stroke(
                [
                    to_screen(state, *a),
                    to_screen(state, *control1),
                    to_screen(state, *control2),
                    to_screen(state, *to),
                ],
                false,
                Color32::TRANSPARENT,
                stroke,
            )));
        }
        _ => {
            let mut points = Vec::new();
            for command in &path.commands {
                match *command {
                    jellyflow::runtime::runtime::geometry::PathCommand::MoveTo(p)
                    | jellyflow::runtime::runtime::geometry::PathCommand::LineTo(p) => {
                        points.push(to_screen(state, p));
                    }
                    jellyflow::runtime::runtime::geometry::PathCommand::CubicTo {
                        control1,
                        control2,
                        to,
                    } => {
                        painter.add(Shape::CubicBezier(CubicBezierShape::from_points_stroke(
                            [
                                *points.last().unwrap_or(&Pos2::ZERO),
                                to_screen(state, control1),
                                to_screen(state, control2),
                                to_screen(state, to),
                            ],
                            false,
                            Color32::TRANSPARENT,
                            stroke,
                        )));
                    }
                }
            }
            if points.len() > 1 {
                painter.add(Shape::Path(PathShape::line(points, stroke)));
            }
        }
    }
}

fn draw_selection(painter: &eframe::egui::Painter, state: &JellyflowEguiState) {
    if let ActiveCanvasInteraction::SelectionBox {
        start_pointer,
        current_pointer,
        ..
    } = state.canvas.active
    {
        let rect = Rect::from_two_pos(
            state.canvas.snapshot.canvas_point_to_screen(start_pointer),
            state
                .canvas
                .snapshot
                .canvas_point_to_screen(current_pointer),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, EDGE_HOVER_COLOR),
            StrokeKind::Outside,
        );
    }
}

fn draw_interaction_preview(painter: &eframe::egui::Painter, state: &JellyflowEguiState) {
    match &state.canvas.active {
        ActiveCanvasInteraction::NodeDrag { preview, .. } => {
            let Some(preview) = preview else {
                return;
            };
            for item in preview.items() {
                let Some(rect) = state.canvas.snapshot.node_rects.get(&item.node) else {
                    continue;
                };
                draw_preview_rect(
                    painter,
                    state,
                    CanvasRect {
                        origin: item.to,
                        size: rect.size,
                    },
                );
            }
        }
        ActiveCanvasInteraction::NodeResize { preview, .. } => {
            let Some(preview) = preview else {
                return;
            };
            draw_resize_preview(painter, state, preview);
        }
        ActiveCanvasInteraction::Connect {
            from,
            current_pointer,
            target,
            ..
        } => {
            draw_connection_preview(painter, state, *from, *current_pointer, *target);
        }
        ActiveCanvasInteraction::SelectionBox { .. }
        | ActiveCanvasInteraction::Pan { .. }
        | ActiveCanvasInteraction::None => {}
    }
}

fn draw_resize_preview(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    preview: &NodeResizePlan,
) {
    draw_preview_rect(
        painter,
        state,
        CanvasRect {
            origin: preview.to_pos,
            size: preview.to,
        },
    );
}

fn draw_preview_rect(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    rect: CanvasRect,
) {
    let min = state.canvas.snapshot.canvas_point_to_screen(rect.origin);
    let max = state.canvas.snapshot.canvas_point_to_screen(CanvasPoint {
        x: rect.origin.x + rect.size.width,
        y: rect.origin.y + rect.size.height,
    });
    painter.rect_stroke(
        Rect::from_min_max(min, max),
        CornerRadius::same(NODE_ROUNDING as u8),
        Stroke::new(1.5, EDGE_HOVER_COLOR),
        StrokeKind::Outside,
    );
}

fn draw_connection_preview(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    from: ConnectionHandleRef,
    current_pointer: CanvasPoint,
    target: Option<jellyflow::runtime::runtime::connection::ResolvedConnectionTarget>,
) {
    let Some(start_rect) = state.canvas.snapshot.handle_screen_rect(from) else {
        return;
    };
    let start = start_rect.center();
    let end = target
        .and_then(|target| target.target)
        .and_then(|target| state.canvas.snapshot.handle_screen_rect(target.handle))
        .map(|rect| rect.center())
        .unwrap_or_else(|| {
            state
                .canvas
                .snapshot
                .canvas_point_to_screen(current_pointer)
        });
    let valid = target.is_some_and(|target| target.is_handle_valid);
    painter.line_segment(
        [start, end],
        Stroke::new(
            2.0,
            if valid {
                EDGE_HOVER_COLOR
            } else {
                EDGE_INVALID_COLOR
            },
        ),
    );
}

fn handle_pointer(
    ui: &mut Ui,
    response: &Response,
    bridge: &mut JellyflowEguiBridge,
    state: &mut JellyflowEguiState,
) {
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let canvas = state.canvas.snapshot.screen_point_to_canvas(pointer);
        if matches!(state.canvas_tool, CanvasTool::Pan) || ui.input(|i| i.key_down(Key::Space)) {
            state.canvas.set_active(ActiveCanvasInteraction::Pan {
                start_pointer: CanvasPoint {
                    x: pointer.x,
                    y: pointer.y,
                },
                current_pointer: CanvasPoint {
                    x: pointer.x,
                    y: pointer.y,
                },
            });
        } else if let Some(handle) = hit_handle(state, pointer)
            && matches!(state.canvas_tool, CanvasTool::Connect | CanvasTool::Select)
        {
            state.canvas.set_active(ActiveCanvasInteraction::Connect {
                from: handle,
                start_pointer: canvas,
                current_pointer: canvas,
                target: Some(bridge.resolve_connection_target(canvas, handle)),
            });
            state.canvas.hovered = Some(HoverTarget::Handle(handle));
        } else if let Some((node, direction)) = hit_resize_handle(state, bridge, pointer)
            && matches!(state.canvas_tool, CanvasTool::Resize | CanvasTool::Select)
        {
            state
                .canvas
                .set_active(ActiveCanvasInteraction::NodeResize {
                    node,
                    direction,
                    start_pointer: canvas,
                    current_pointer: canvas,
                    preview: bridge.plan_pointer_resize(node, canvas, canvas, direction),
                });
            state.canvas.hovered = Some(HoverTarget::ResizeHandle { node, direction });
        } else if let Some(node) = hit_node(state, pointer)
            && matches!(state.canvas_tool, CanvasTool::Select | CanvasTool::Drag)
        {
            bridge.start_node_drag(node, ui.input(|i| i.modifiers.shift));
            state.canvas.set_active(ActiveCanvasInteraction::NodeDrag {
                primary: node,
                start_pointer: canvas,
                preview: bridge.plan_node_drag(node, CanvasPoint::default()),
            });
            state.canvas.hovered = Some(HoverTarget::Node(node));
        } else if matches!(state.canvas_tool, CanvasTool::Select) {
            state
                .canvas
                .set_active(ActiveCanvasInteraction::SelectionBox {
                    start_pointer: canvas,
                    current_pointer: canvas,
                    additive: false,
                });
        }
    }

    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let canvas = state.canvas.snapshot.screen_point_to_canvas(pointer);
        match &mut state.canvas.active {
            ActiveCanvasInteraction::NodeDrag {
                primary,
                start_pointer,
                preview,
            } => {
                let delta = CanvasPoint {
                    x: canvas.x - start_pointer.x,
                    y: canvas.y - start_pointer.y,
                };
                *preview = bridge.plan_node_drag(*primary, delta);
            }
            ActiveCanvasInteraction::SelectionBox {
                current_pointer, ..
            } => {
                *current_pointer = canvas;
            }
            ActiveCanvasInteraction::Pan {
                current_pointer, ..
            } => {
                *current_pointer = CanvasPoint {
                    x: pointer.x,
                    y: pointer.y,
                };
            }
            ActiveCanvasInteraction::NodeResize {
                current_pointer,
                preview,
                node,
                direction,
                start_pointer,
            } => {
                *current_pointer = canvas;
                *preview = bridge.plan_pointer_resize(*node, *start_pointer, canvas, *direction);
            }
            ActiveCanvasInteraction::Connect {
                current_pointer,
                target,
                from,
                ..
            } => {
                *current_pointer = canvas;
                *target = Some(bridge.resolve_connection_target(canvas, *from));
            }
            ActiveCanvasInteraction::None => {}
        }
    }

    if response.drag_stopped() {
        let interaction =
            std::mem::replace(&mut state.canvas.active, ActiveCanvasInteraction::None);
        match bridge.commit_interaction(interaction) {
            Ok(Some(_)) => state.set_status("Committed"),
            Ok(None) => {}
            Err(err) => state.set_status(err),
        }
    }

    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        if state.canvas_tool == CanvasTool::CreateNode {
            if let Some(kind) = state.pending_create_kind.take() {
                let canvas = state.canvas.snapshot.screen_point_to_canvas(pointer);
                match bridge.create_node(kind, canvas) {
                    Ok(_) => state.set_status("Node created"),
                    Err(err) => state.set_status(err),
                }
                state.canvas_tool = CanvasTool::Select;
            }
        } else if let Some(node) = hit_node(state, pointer) {
            bridge.select_node(node, ui.input(|i| i.modifiers.shift));
        } else if let Some(edge) = hit_edge(state, pointer) {
            bridge.select_edge(edge, ui.input(|i| i.modifiers.shift));
        } else {
            bridge.clear_selection();
        }
    }

    if response.hovered()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        state.canvas.hovered = hit_target(state, bridge, pointer);
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll != Vec2::ZERO {
            if ui.input(|i| i.modifiers.ctrl) {
                let factor = if scroll.y > 0.0 { 1.1 } else { 0.9 };
                if let Some(pointer) = response.hover_pos() {
                    let _ = bridge.zoom_at_screen(
                        CanvasPoint {
                            x: pointer.x - state.canvas.snapshot.viewport_rect.min.x,
                            y: pointer.y - state.canvas.snapshot.viewport_rect.min.y,
                        },
                        factor,
                    );
                }
            } else {
                let _ = bridge.pan_by_screen_delta(CanvasPoint {
                    x: scroll.x,
                    y: scroll.y,
                });
            }
        }
    }

    ui.ctx().request_repaint();
}

fn hit_node(state: &JellyflowEguiState, pointer: Pos2) -> Option<NodeId> {
    state
        .canvas
        .snapshot
        .visible_node_render_order
        .iter()
        .rev()
        .copied()
        .find(|node| {
            state
                .canvas
                .snapshot
                .node_screen_rect(*node)
                .is_some_and(|rect| rect.contains(pointer))
        })
}

fn hit_target(
    state: &JellyflowEguiState,
    bridge: &JellyflowEguiBridge,
    pointer: Pos2,
) -> Option<HoverTarget> {
    if let Some(handle) = hit_handle(state, pointer) {
        return Some(HoverTarget::Handle(handle));
    }
    if let Some((node, direction)) = hit_resize_handle(state, bridge, pointer) {
        return Some(HoverTarget::ResizeHandle { node, direction });
    }
    if let Some(node) = hit_node(state, pointer) {
        return Some(HoverTarget::Node(node));
    }
    hit_edge(state, pointer).map(HoverTarget::Edge)
}

fn hit_edge(state: &JellyflowEguiState, pointer: Pos2) -> Option<jellyflow::core::EdgeId> {
    for (edge_id, path) in state.canvas.snapshot.edge_paths() {
        if edge_path_contains_point(
            path,
            state.canvas.snapshot.screen_point_to_canvas(pointer),
            EdgeHitTestOptions::default(),
        ) {
            return Some(*edge_id);
        }
    }
    None
}

fn hit_handle(state: &JellyflowEguiState, pointer: Pos2) -> Option<ConnectionHandleRef> {
    state
        .canvas
        .snapshot
        .handle_bounds
        .keys()
        .copied()
        .find(|handle| {
            state
                .canvas
                .snapshot
                .handle_screen_rect(*handle)
                .is_some_and(|rect| rect.expand(3.0).contains(pointer))
        })
}

fn hit_resize_handle(
    state: &JellyflowEguiState,
    bridge: &JellyflowEguiBridge,
    pointer: Pos2,
) -> Option<(NodeId, NodeResizeDirection)> {
    let selected_nodes = &bridge.store().view_state().selected_nodes;
    state
        .canvas
        .snapshot
        .visible_node_render_order
        .iter()
        .rev()
        .copied()
        .filter(|node| selected_nodes.contains(node))
        .filter(|node| state.canvas.snapshot.node_screen_rect(*node).is_some())
        .find_map(|node| {
            let rect = state.canvas.snapshot.node_screen_rect(node)?;
            resize_directions()
                .into_iter()
                .find(|direction| {
                    resize_handle_rect(rect, *direction)
                        .expand(3.0)
                        .contains(pointer)
                })
                .map(|direction| (node, direction))
        })
}

fn to_screen(state: &JellyflowEguiState, point: CanvasPoint) -> Pos2 {
    state.canvas.snapshot.canvas_point_to_screen(point)
}

fn resize_directions() -> [NodeResizeDirection; 8] {
    [
        NodeResizeDirection::Top,
        NodeResizeDirection::TopRight,
        NodeResizeDirection::Right,
        NodeResizeDirection::BottomRight,
        NodeResizeDirection::Bottom,
        NodeResizeDirection::BottomLeft,
        NodeResizeDirection::Left,
        NodeResizeDirection::TopLeft,
    ]
}

fn resize_handle_rect(node_rect: Rect, direction: NodeResizeDirection) -> Rect {
    let center = match direction {
        NodeResizeDirection::Top => Pos2::new(node_rect.center().x, node_rect.top()),
        NodeResizeDirection::TopRight => node_rect.right_top(),
        NodeResizeDirection::Right => Pos2::new(node_rect.right(), node_rect.center().y),
        NodeResizeDirection::BottomRight => node_rect.right_bottom(),
        NodeResizeDirection::Bottom => Pos2::new(node_rect.center().x, node_rect.bottom()),
        NodeResizeDirection::BottomLeft => node_rect.left_bottom(),
        NodeResizeDirection::Left => Pos2::new(node_rect.left(), node_rect.center().y),
        NodeResizeDirection::TopLeft => node_rect.left_top(),
    };
    Rect::from_center_size(center, Vec2::new(RESIZE_HANDLE_SIZE, RESIZE_HANDLE_SIZE))
}
