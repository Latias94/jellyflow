use eframe::egui::{
    Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui,
    Vec2,
};
use eframe::epaint::{CubicBezierShape, PathShape, Shape};
use jellyflow::core::{CanvasPoint, CanvasRect, NodeId, PortDirection};
use jellyflow::runtime::runtime::geometry::EdgePath;
use jellyflow::runtime::runtime::geometry::{EdgeHitTestOptions, edge_path_contains_point};

use crate::bridge::{JellyflowEguiBridge, NodeRendererStyle};
use crate::state::{ActiveCanvasInteraction, CanvasTool, HoverTarget, JellyflowEguiState};

const NODE_ROUNDING: f32 = 8.0;
const CANVAS_BG: Color32 = Color32::from_rgb(248, 249, 251);
const GRID_MINOR: Color32 = Color32::from_rgb(229, 233, 238);
const GRID_MAJOR: Color32 = Color32::from_rgb(210, 216, 223);
const EDGE_COLOR: Color32 = Color32::from_rgb(107, 114, 128);
const EDGE_HOVER_COLOR: Color32 = Color32::from_rgb(59, 130, 246);
const HANDLE_COLOR: Color32 = Color32::from_rgb(255, 255, 255);

pub fn show_canvas(ui: &mut Ui, bridge: &mut JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);

    draw_background(&painter, rect);
    draw_edges(&painter, bridge, state);
    draw_nodes(&painter, bridge, state);
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

        draw_handles(painter, bridge, *node_id, rect, style);
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
    bridge: &JellyflowEguiBridge,
    node_id: NodeId,
    rect: Rect,
    style: NodeRendererStyle,
) {
    for (handle, bounds) in bridge.default_handle_bounds(node_id) {
        let handle_rect = handle_rect(rect, bounds.rect);
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
        if let Some(node) = hit_node(state, pointer) {
            state.canvas.set_active(ActiveCanvasInteraction::NodeDrag {
                primary: node,
                start_pointer: canvas,
                start_node_pos: bridge
                    .store()
                    .graph()
                    .nodes()
                    .get(&node)
                    .map(|node| node.pos)
                    .unwrap_or_default(),
                preview: bridge.plan_node_drag(node, CanvasPoint::default()),
            });
            state.canvas.hovered = Some(HoverTarget::Node(node));
        } else {
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
                start_node_pos,
                preview,
            } => {
                let delta = CanvasPoint {
                    x: canvas.x - start_pointer.x,
                    y: canvas.y - start_pointer.y,
                };
                let _ = start_node_pos;
                *preview = bridge.plan_node_drag(*primary, delta);
                bridge.select_node(*primary, false);
            }
            ActiveCanvasInteraction::SelectionBox {
                current_pointer, ..
            } => {
                *current_pointer = canvas;
            }
            ActiveCanvasInteraction::Pan {
                current_pointer, ..
            } => {
                *current_pointer = canvas;
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
        } else {
            bridge.clear_selection();
        }
    }

    if response.hovered()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        state.canvas.hovered = hit_target(state, pointer);
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

fn hit_target(state: &JellyflowEguiState, pointer: Pos2) -> Option<HoverTarget> {
    if let Some(node) = hit_node(state, pointer) {
        return Some(HoverTarget::Node(node));
    }
    for (edge_id, path) in state.canvas.snapshot.edge_paths() {
        if edge_path_contains_point(
            path,
            state.canvas.snapshot.screen_point_to_canvas(pointer),
            EdgeHitTestOptions::default(),
        ) {
            return Some(HoverTarget::Edge(*edge_id));
        }
    }
    None
}

fn handle_rect(node_rect: Rect, bounds: CanvasRect) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            node_rect.min.x + bounds.origin.x,
            node_rect.min.y + bounds.origin.y,
        ),
        Vec2::new(bounds.size.width, bounds.size.height),
    )
}

fn to_screen(state: &JellyflowEguiState, point: CanvasPoint) -> Pos2 {
    state.canvas.snapshot.canvas_point_to_screen(point)
}
