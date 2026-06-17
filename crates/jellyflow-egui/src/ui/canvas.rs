use eframe::egui::{
    Align2, Color32, CornerRadius, CursorIcon, Key, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextStyle, Ui, Vec2,
};
use eframe::epaint::{CubicBezierShape, PathShape, Shape};
use jellyflow::core::{CanvasPoint, CanvasRect, NodeId, PortDirection};
use jellyflow::runtime::runtime::connection::ConnectionHandleRef;
use jellyflow::runtime::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointInput, EdgeHitTestOptions, EdgePath, HandlePosition,
    bezier_edge_path, edge_path_contains_point, edge_position,
};
use jellyflow::runtime::runtime::resize::NodeResizeDirection;

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

    if handle_pointer(ui, &response, bridge, state) {
        state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);
    }

    draw_background(&painter, state);
    draw_edges(&painter, bridge, state);
    draw_nodes(&painter, bridge, state);
    draw_interaction_preview(&painter, state);
    draw_selection(&painter, state);
    update_cursor(ui, &response, state);
}

fn draw_background(painter: &eframe::egui::Painter, state: &JellyflowEguiState) {
    let rect = state.canvas.snapshot.viewport_rect;
    painter.rect_filled(rect, 0.0, CANVAS_BG);
    let step = 40.0;
    let major_step = 200.0;
    let canvas_min = state.canvas.snapshot.screen_point_to_canvas(rect.min);
    let canvas_max = state.canvas.snapshot.screen_point_to_canvas(rect.max);
    let mut x = (canvas_min.x / step).floor() * step;
    while x <= canvas_max.x {
        let is_major = ((x / major_step).round() - x / major_step).abs() < 0.01;
        let screen_x = state
            .canvas
            .snapshot
            .canvas_point_to_screen(CanvasPoint { x, y: 0.0 })
            .x;
        painter.line_segment(
            [
                Pos2::new(screen_x, rect.min.y),
                Pos2::new(screen_x, rect.max.y),
            ],
            Stroke::new(
                if is_major { 1.0 } else { 0.5 },
                if is_major { GRID_MAJOR } else { GRID_MINOR },
            ),
        );
        x += step;
    }
    let mut y = (canvas_min.y / step).floor() * step;
    while y <= canvas_max.y {
        let is_major = ((y / major_step).round() - y / major_step).abs() < 0.01;
        let screen_y = state
            .canvas
            .snapshot
            .canvas_point_to_screen(CanvasPoint { x: 0.0, y })
            .y;
        painter.line_segment(
            [
                Pos2::new(rect.min.x, screen_y),
                Pos2::new(rect.max.x, screen_y),
            ],
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
        let Some(canvas_rect) = visual_node_canvas_rect(state, *node_id) else {
            continue;
        };
        let rect = canvas_rect_to_screen(state, canvas_rect);
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

        draw_handles(painter, state, *node_id, rect, style);
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
    node_rect: Rect,
    style: NodeRendererStyle,
) {
    let handles = state
        .canvas
        .snapshot
        .handle_bounds
        .iter()
        .filter(|(handle, _)| handle.node == node_id);
    for (handle, bounds) in handles {
        let handle_rect = handle_screen_rect_for_node(state, node_rect, bounds.rect);
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
        let color = if bridge.store().view_state().selected_edges.contains(edge_id) {
            EDGE_HOVER_COLOR
        } else {
            EDGE_COLOR
        };
        if let Some(path) = preview_edge_path(state, bridge, *edge_id) {
            draw_edge_path(painter, state, &path, color);
        } else if let Some(path) = state.canvas.snapshot.edge_paths.get(edge_id) {
            draw_edge_path(painter, state, path, color);
        }
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
        ActiveCanvasInteraction::NodeDrag { .. } => {}
        ActiveCanvasInteraction::NodeResize { .. } => {}
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

fn draw_connection_preview(
    painter: &eframe::egui::Painter,
    state: &JellyflowEguiState,
    from: ConnectionHandleRef,
    current_pointer: CanvasPoint,
    target: Option<jellyflow::runtime::runtime::connection::ResolvedConnectionTarget>,
) {
    let Some(start_rect) = visual_handle_screen_rect(state, from) else {
        return;
    };
    let start = start_rect.center();
    let end = target
        .and_then(|target| target.target)
        .and_then(|target| visual_handle_screen_rect(state, target.handle))
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
) -> bool {
    let mut needs_rebuild = false;

    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let canvas = state.canvas.snapshot.screen_point_to_canvas(pointer);
        if matches!(state.canvas_tool, CanvasTool::Pan) || ui.input(|i| i.key_down(Key::Space)) {
            state.canvas.set_active(ActiveCanvasInteraction::Pan {
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
                let delta = CanvasPoint {
                    x: pointer.x - current_pointer.x,
                    y: pointer.y - current_pointer.y,
                };
                needs_rebuild |= bridge.pan_by_screen_delta(delta);
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
            Ok(Some(_)) => {
                state.set_status("Committed");
                needs_rebuild = true;
            }
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
                    Ok(_) => {
                        state.set_status("Node created");
                        needs_rebuild = true;
                    }
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
    } else if !state.canvas.is_busy() {
        state.canvas.hovered = None;
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll != Vec2::ZERO {
            if ui.input(|i| i.modifiers.ctrl) {
                let factor = if scroll.y > 0.0 { 1.1 } else { 0.9 };
                if let Some(pointer) = response.hover_pos() {
                    needs_rebuild |= bridge.zoom_at_screen(
                        CanvasPoint {
                            x: pointer.x - state.canvas.snapshot.viewport_rect.min.x,
                            y: pointer.y - state.canvas.snapshot.viewport_rect.min.y,
                        },
                        factor,
                    );
                }
            } else {
                needs_rebuild |= bridge.pan_by_screen_delta(CanvasPoint {
                    x: scroll.x,
                    y: scroll.y,
                });
            }
        }
    }

    ui.ctx().request_repaint();
    needs_rebuild
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
            visual_node_screen_rect(state, *node).is_some_and(|rect| rect.contains(pointer))
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
            visual_handle_screen_rect(state, *handle)
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
        .filter(|node| visual_node_screen_rect(state, *node).is_some())
        .find_map(|node| {
            let rect = visual_node_screen_rect(state, node)?;
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

fn visual_node_screen_rect(state: &JellyflowEguiState, node: NodeId) -> Option<Rect> {
    visual_node_canvas_rect(state, node).map(|rect| canvas_rect_to_screen(state, rect))
}

fn visual_node_canvas_rect(state: &JellyflowEguiState, node: NodeId) -> Option<CanvasRect> {
    match &state.canvas.active {
        ActiveCanvasInteraction::NodeDrag { preview, .. } => {
            let preview = preview.as_ref()?;
            let rect = state.canvas.snapshot.node_rects.get(&node)?;
            preview.items().iter().find_map(|item| {
                (item.node == node).then_some(CanvasRect {
                    origin: item.to,
                    size: rect.size,
                })
            })
        }
        ActiveCanvasInteraction::NodeResize { preview, .. } => {
            let preview = preview.as_ref()?;
            (preview.node == node).then_some(CanvasRect {
                origin: preview.to_pos,
                size: preview.to,
            })
        }
        ActiveCanvasInteraction::None
        | ActiveCanvasInteraction::Connect { .. }
        | ActiveCanvasInteraction::SelectionBox { .. }
        | ActiveCanvasInteraction::Pan { .. } => None,
    }
    .or_else(|| state.canvas.snapshot.node_rects.get(&node).copied())
}

fn preview_edge_path(
    state: &JellyflowEguiState,
    bridge: &JellyflowEguiBridge,
    edge: jellyflow::core::EdgeId,
) -> Option<EdgePath> {
    if !has_node_geometry_preview(state) {
        return None;
    }

    let edge_record = bridge.store().graph().edges().get(&edge)?;
    let source_port = bridge.store().graph().ports().get(&edge_record.from)?;
    let target_port = bridge.store().graph().ports().get(&edge_record.to)?;
    let source_handle =
        ConnectionHandleRef::new(source_port.node, edge_record.from, source_port.dir);
    let target_handle = ConnectionHandleRef::new(target_port.node, edge_record.to, target_port.dir);
    let position = edge_position(
        EdgeEndpointInput {
            node_rect: visual_node_canvas_rect(state, source_port.node)?,
            handle: state
                .canvas
                .snapshot
                .handle_bounds
                .get(&source_handle)
                .copied(),
            fallback_position: handle_fallback_position(source_port.dir),
        },
        EdgeEndpointInput {
            node_rect: visual_node_canvas_rect(state, target_port.node)?,
            handle: state
                .canvas
                .snapshot
                .handle_bounds
                .get(&target_handle)
                .copied(),
            fallback_position: handle_fallback_position(target_port.dir),
        },
    )?;
    bezier_edge_path(
        position.source,
        position.target,
        BezierEdgeOptions::default(),
    )
}

fn has_node_geometry_preview(state: &JellyflowEguiState) -> bool {
    match &state.canvas.active {
        ActiveCanvasInteraction::NodeDrag { preview, .. } => preview.is_some(),
        ActiveCanvasInteraction::NodeResize { preview, .. } => preview.is_some(),
        ActiveCanvasInteraction::None
        | ActiveCanvasInteraction::Connect { .. }
        | ActiveCanvasInteraction::SelectionBox { .. }
        | ActiveCanvasInteraction::Pan { .. } => false,
    }
}

fn handle_fallback_position(direction: PortDirection) -> HandlePosition {
    match direction {
        PortDirection::In => HandlePosition::Left,
        PortDirection::Out => HandlePosition::Right,
    }
}

fn canvas_rect_to_screen(state: &JellyflowEguiState, rect: CanvasRect) -> Rect {
    let min = state.canvas.snapshot.canvas_point_to_screen(rect.origin);
    let max = state.canvas.snapshot.canvas_point_to_screen(CanvasPoint {
        x: rect.origin.x + rect.size.width,
        y: rect.origin.y + rect.size.height,
    });
    Rect::from_min_max(min, max)
}

fn visual_handle_screen_rect(
    state: &JellyflowEguiState,
    handle: ConnectionHandleRef,
) -> Option<Rect> {
    let node_rect = visual_node_screen_rect(state, handle.node)?;
    let bounds = state.canvas.snapshot.handle_bounds.get(&handle)?;
    Some(handle_screen_rect_for_node(state, node_rect, bounds.rect))
}

fn handle_screen_rect_for_node(
    state: &JellyflowEguiState,
    node_rect: Rect,
    handle_rect: CanvasRect,
) -> Rect {
    let zoom = state.canvas.snapshot.transform.zoom;
    Rect::from_min_size(
        Pos2::new(
            node_rect.min.x + handle_rect.origin.x * zoom,
            node_rect.min.y + handle_rect.origin.y * zoom,
        ),
        Vec2::new(
            handle_rect.size.width * zoom,
            handle_rect.size.height * zoom,
        ),
    )
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

fn update_cursor(ui: &mut Ui, response: &Response, state: &JellyflowEguiState) {
    if !response.hovered() && !state.canvas.is_busy() {
        return;
    }

    let cursor = match state.canvas.active {
        ActiveCanvasInteraction::Pan { .. } => CursorIcon::Grabbing,
        ActiveCanvasInteraction::Connect { .. } => CursorIcon::Crosshair,
        ActiveCanvasInteraction::NodeResize { direction, .. } => resize_cursor(direction),
        ActiveCanvasInteraction::NodeDrag { .. } => CursorIcon::Grabbing,
        ActiveCanvasInteraction::SelectionBox { .. } => CursorIcon::Crosshair,
        ActiveCanvasInteraction::None => match state.canvas.hovered {
            Some(HoverTarget::Handle(_)) => CursorIcon::Crosshair,
            Some(HoverTarget::ResizeHandle { direction, .. }) => resize_cursor(direction),
            Some(HoverTarget::Node(_)) => CursorIcon::Grab,
            Some(HoverTarget::Edge(_)) => CursorIcon::PointingHand,
            None => CursorIcon::Default,
        },
    };
    ui.ctx().set_cursor_icon(cursor);
}

fn resize_cursor(direction: NodeResizeDirection) -> CursorIcon {
    match direction {
        NodeResizeDirection::Top | NodeResizeDirection::Bottom => CursorIcon::ResizeVertical,
        NodeResizeDirection::Left | NodeResizeDirection::Right => CursorIcon::ResizeHorizontal,
        NodeResizeDirection::TopRight | NodeResizeDirection::BottomLeft => CursorIcon::ResizeNeSw,
        NodeResizeDirection::TopLeft | NodeResizeDirection::BottomRight => CursorIcon::ResizeNwSe,
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Pos2, Rect, Vec2};
    use jellyflow::core::{CanvasPoint, NodeId};
    use jellyflow::runtime::runtime::geometry::PathCommand;

    use super::{preview_edge_path, visual_node_canvas_rect};
    use crate::bridge::JellyflowEguiBridge;
    use crate::state::{ActiveCanvasInteraction, CanvasSnapshot, JellyflowEguiState};

    #[test]
    fn node_drag_preview_moves_the_visual_node_rect() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let mut state = state_with_snapshot(&mut bridge);
        let node = first_visible_node(&state);
        let original = visual_node_canvas_rect(&state, node).expect("node rect exists");
        state.canvas.active = ActiveCanvasInteraction::NodeDrag {
            primary: node,
            start_pointer: original.origin,
            preview: bridge.plan_node_drag(node, CanvasPoint { x: 48.0, y: -24.0 }),
        };

        let visual = visual_node_canvas_rect(&state, node).expect("preview rect exists");

        assert_eq!(visual.size, original.size);
        assert_eq!(
            visual.origin,
            CanvasPoint {
                x: original.origin.x + 48.0,
                y: original.origin.y - 24.0,
            }
        );
    }

    #[test]
    fn edge_preview_follows_dragged_node_geometry() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let mut state = state_with_snapshot(&mut bridge);
        let edge = *state
            .canvas
            .snapshot
            .visible_edge_render_order
            .first()
            .expect("demo edge exists");
        let edge_record = bridge.store().graph().edges().get(&edge).expect("edge");
        let source_node = bridge
            .store()
            .graph()
            .ports()
            .get(&edge_record.from)
            .expect("source port")
            .node;
        let original_path = preview_or_snapshot_edge_path(&state, &bridge, edge);
        state.canvas.active = ActiveCanvasInteraction::NodeDrag {
            primary: source_node,
            start_pointer: CanvasPoint::default(),
            preview: bridge.plan_node_drag(source_node, CanvasPoint { x: 80.0, y: 0.0 }),
        };

        let preview_path = preview_edge_path(&state, &bridge, edge).expect("preview path");

        assert_ne!(
            first_path_point(&preview_path),
            first_path_point(&original_path),
            "edge source endpoint should follow the dragged node preview"
        );
    }

    fn state_with_snapshot(bridge: &mut JellyflowEguiBridge) -> JellyflowEguiState {
        let mut state = JellyflowEguiState::default();
        state.canvas.snapshot = bridge.rebuild_snapshot(
            &CanvasSnapshot::empty(),
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0)),
        );
        state
    }

    fn first_visible_node(state: &JellyflowEguiState) -> NodeId {
        *state
            .canvas
            .snapshot
            .visible_node_render_order
            .first()
            .expect("demo node exists")
    }

    fn preview_or_snapshot_edge_path(
        state: &JellyflowEguiState,
        bridge: &JellyflowEguiBridge,
        edge: jellyflow::core::EdgeId,
    ) -> jellyflow::runtime::runtime::geometry::EdgePath {
        preview_edge_path(state, bridge, edge)
            .or_else(|| state.canvas.snapshot.edge_paths.get(&edge).cloned())
            .expect("edge path exists")
    }

    fn first_path_point(path: &jellyflow::runtime::runtime::geometry::EdgePath) -> CanvasPoint {
        match path.commands.first().expect("path has a move command") {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => *point,
            PathCommand::CubicTo { to, .. } => *to,
        }
    }
}
