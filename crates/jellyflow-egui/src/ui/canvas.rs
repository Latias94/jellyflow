use eframe::egui::{
    Align2, Color32, CornerRadius, CursorIcon, Key, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextStyle, Ui, Vec2,
};
use eframe::epaint::{CubicBezierShape, PathShape, Shape};
use jellyflow::core::{CanvasPoint, CanvasRect, EdgeLabelAnchor, NodeId, PortDirection};
use jellyflow::runtime::runtime::connection::ConnectionHandleRef;
use jellyflow::runtime::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointInput, EdgeHitTestOptions, EdgePath, HandleBounds,
    HandlePosition, bezier_edge_path, edge_path_contains_point, edge_position,
};
use jellyflow::runtime::runtime::resize::NodeResizeDirection;

use crate::bridge::JellyflowEguiBridge;
use crate::renderer::{
    NodeContentLevel, NodeRenderLayout, NodeRendererState, NodeRendererStyle, NodeWidgetRenderInput,
};
use crate::state::{ActiveCanvasInteraction, CanvasTool, HoverTarget, JellyflowEguiState};

const NODE_ROUNDING: f32 = 8.0;
const CANVAS_BG: Color32 = Color32::from_rgb(246, 247, 249);
const GRID_MINOR: Color32 = Color32::from_rgb(232, 235, 240);
const GRID_MAJOR: Color32 = Color32::from_rgb(216, 222, 230);
const EDGE_COLOR: Color32 = Color32::from_rgb(119, 128, 141);
const EDGE_HOVER_COLOR: Color32 = Color32::from_rgb(37, 99, 180);
const EDGE_INVALID_COLOR: Color32 = Color32::from_rgb(220, 76, 76);
const HANDLE_FILL: Color32 = Color32::from_rgb(250, 252, 255);
const RESIZE_HANDLE_SIZE: f32 = 8.0;

pub fn show_canvas(ui: &mut Ui, bridge: &mut JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);
    if state.canvas.fit_view_requested {
        state.canvas.fit_view_requested = false;
        if bridge.fit_view(state.canvas.snapshot.viewport_size) {
            state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);
        }
    }

    if handle_pointer(ui, &response, bridge, state) {
        state.canvas.snapshot = bridge.rebuild_snapshot(&state.canvas.snapshot, rect);
    }

    draw_background(&painter, state);
    draw_edges(&painter, bridge, state);
    draw_nodes(ui, &painter, bridge, state);
    draw_interaction_preview(&painter, bridge, state);
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
    ui: &mut Ui,
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
) {
    for node_id in &state.canvas.snapshot.visible_node_render_order {
        let Some(canvas_rect) = visual_node_canvas_rect(state, *node_id) else {
            continue;
        };
        let Some(descriptor) = bridge.descriptor_for_node(*node_id) else {
            continue;
        };
        let style = bridge.renderers().style_for_descriptor(&descriptor);
        let renderer_state = node_renderer_state(bridge, state, *node_id);
        let Some(layout) =
            visual_node_render_layout(bridge, state, *node_id, canvas_rect, renderer_state)
        else {
            continue;
        };
        let rect = canvas_rect_to_screen(state, layout.body_rect);
        let content_level = NodeContentLevel::from_zoom(state.canvas.snapshot.transform.zoom);
        let has_widget_renderer = bridge
            .renderers()
            .has_widget_renderer(&descriptor.renderer_key);
        draw_node_shell(painter, rect, style, renderer_state);
        draw_node_title(painter, rect, &layout.title, style, content_level);
        if content_level.shows_detail()
            && !has_widget_renderer
            && let Some(summary) = &layout.summary
        {
            draw_node_summary(painter, rect, summary, style);
        }
        let widgets_rendered = draw_node_widgets(
            ui,
            bridge,
            state,
            NodeWidgetDrawRequest {
                node: *node_id,
                descriptor: &descriptor,
                layout: &layout,
                node_rect: rect,
                style,
                renderer_state,
                content_level,
                clip_rect: state.canvas.snapshot.viewport_rect,
            },
        );
        if content_level.shows_detail() && !widgets_rendered {
            draw_port_summary(painter, &descriptor, rect, style);
        }

        draw_handles(painter, bridge, state, *node_id, rect, style);
        if renderer_state.selected {
            draw_resize_handles(painter, rect, style);
        }
    }
}

fn draw_node_title(
    painter: &eframe::egui::Painter,
    rect: Rect,
    title: &str,
    style: NodeRendererStyle,
    content_level: NodeContentLevel,
) {
    if !content_level.shows_text() || rect.width() < 34.0 || rect.height() < 18.0 {
        return;
    }

    let Some(text_rect) = node_title_text_rect(rect, content_level) else {
        return;
    };

    let font = if content_level.shows_detail() {
        TextStyle::Button.resolve(&painter.ctx().global_style())
    } else {
        TextStyle::Small.resolve(&painter.ctx().global_style())
    };
    let clipped = painter.with_clip_rect(text_rect);
    let galley = clipped.layout_no_wrap(title.to_owned(), font, style.text);
    clipped.galley(text_rect.left_top(), galley, style.text);
}

fn node_title_text_rect(rect: Rect, content_level: NodeContentLevel) -> Option<Rect> {
    let margin = if content_level.shows_detail() {
        18.0
    } else {
        8.0
    };
    let top = if content_level.shows_detail() {
        13.0
    } else {
        6.0
    };
    let text_rect = Rect::from_min_max(
        rect.left_top() + Vec2::new(margin, top),
        rect.right_top() + Vec2::new(-8.0, rect.height().min(28.0)),
    );
    (text_rect.width() >= 18.0 && text_rect.height() >= 8.0).then_some(text_rect)
}

fn draw_node_summary(
    painter: &eframe::egui::Painter,
    rect: Rect,
    summary: &str,
    style: NodeRendererStyle,
) {
    let Some(text_rect) = node_summary_text_rect(rect) else {
        return;
    };

    let color = style.text.gamma_multiply(0.68);
    let clipped = painter.with_clip_rect(text_rect);
    let galley = clipped.layout_no_wrap(
        summary.to_owned(),
        TextStyle::Small.resolve(&painter.ctx().global_style()),
        color,
    );
    clipped.galley(text_rect.left_top(), galley, color);
}

fn node_summary_text_rect(rect: Rect) -> Option<Rect> {
    let text_rect = Rect::from_min_max(
        rect.left_top() + Vec2::new(18.0, 31.0),
        rect.right_top() + Vec2::new(-8.0, rect.height().min(50.0)),
    );
    (text_rect.width() >= 24.0 && text_rect.height() >= 8.0).then_some(text_rect)
}

fn draw_node_shell(
    painter: &eframe::egui::Painter,
    rect: Rect,
    style: NodeRendererStyle,
    state: NodeRendererState,
) {
    let rounding = CornerRadius::same(NODE_ROUNDING as u8);
    let shadow_rect = rect.translate(Vec2::new(0.0, 1.5));
    painter.rect_filled(
        shadow_rect,
        rounding,
        Color32::from_rgba_premultiplied(21, 31, 48, if state.selected { 28 } else { 14 }),
    );
    painter.rect_filled(rect, rounding, style.fill);
    let rail = Rect::from_min_max(
        rect.left_top() + Vec2::new(0.0, 9.0),
        Pos2::new(rect.left() + 3.0, rect.bottom() - 9.0),
    );
    painter.rect_filled(rail, CornerRadius::same(2), style.accent);
    painter.rect_stroke(
        rect,
        rounding,
        if state.selected {
            style.selected_stroke()
        } else if state.hovered {
            Stroke::new(1.25, style.accent.gamma_multiply(0.78))
        } else {
            Stroke::new(1.0, style.stroke)
        },
        StrokeKind::Outside,
    );
}

fn visual_node_render_layout(
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    node: NodeId,
    canvas_rect: CanvasRect,
    renderer_state: NodeRendererState,
) -> Option<NodeRenderLayout> {
    if has_node_geometry_preview(state) {
        bridge.node_render_layout(node, canvas_rect, renderer_state)
    } else {
        state
            .canvas
            .snapshot
            .node_render_layouts
            .get(&node)
            .cloned()
    }
}

struct NodeWidgetDrawRequest<'a> {
    node: NodeId,
    descriptor: &'a jellyflow::runtime::schema::NodeKindViewDescriptor,
    layout: &'a NodeRenderLayout,
    node_rect: Rect,
    style: NodeRendererStyle,
    renderer_state: NodeRendererState,
    content_level: NodeContentLevel,
    clip_rect: Rect,
}

fn draw_node_widgets(
    ui: &mut Ui,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    request: NodeWidgetDrawRequest<'_>,
) -> bool {
    let Some(node_record) = bridge.store().graph().nodes().get(&request.node) else {
        return false;
    };
    bridge.renderers().render_widgets(
        ui,
        &NodeWidgetRenderInput {
            id: request.node,
            node: node_record,
            descriptor: request.descriptor,
            state: request.renderer_state,
            style: request.style,
            layout: request.layout,
            node_rect: request.node_rect,
            clip_rect: request.clip_rect,
            zoom: state.canvas.snapshot.transform.zoom,
            content_level: request.content_level,
        },
    )
}

fn draw_handles(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    node_id: NodeId,
    node_rect: Rect,
    style: NodeRendererStyle,
) {
    for (handle, bounds) in visual_handle_bounds_for_node(bridge, state, node_id) {
        let handle_rect = handle_screen_rect_for_node(state, node_rect, bounds.rect);
        draw_handle(
            painter,
            handle_rect,
            bounds.position,
            handle.direction,
            style,
        );
    }
}

fn draw_handle(
    painter: &eframe::egui::Painter,
    rect: Rect,
    position: HandlePosition,
    direction: PortDirection,
    style: NodeRendererStyle,
) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.42;
    match direction {
        PortDirection::In => {
            painter.circle_filled(center, radius, HANDLE_FILL);
            painter.circle_stroke(center, radius, Stroke::new(1.25, style.accent));
        }
        PortDirection::Out => {
            let points = diamond_points(center, radius, position);
            painter.add(Shape::convex_polygon(
                points.to_vec(),
                HANDLE_FILL,
                Stroke::new(1.25, style.accent),
            ));
        }
    }
}

fn diamond_points(center: Pos2, radius: f32, position: HandlePosition) -> [Pos2; 4] {
    let horizontal_bias = match position {
        HandlePosition::Left => -0.8,
        HandlePosition::Right => 0.8,
        HandlePosition::Top | HandlePosition::Bottom => 0.0,
    };
    let vertical_bias = match position {
        HandlePosition::Top => -0.8,
        HandlePosition::Bottom => 0.8,
        HandlePosition::Left | HandlePosition::Right => 0.0,
    };
    let center = center + Vec2::new(horizontal_bias, vertical_bias);
    [
        center + Vec2::new(0.0, -radius),
        center + Vec2::new(radius, 0.0),
        center + Vec2::new(0.0, radius),
        center + Vec2::new(-radius, 0.0),
    ]
}

fn draw_port_summary(
    painter: &eframe::egui::Painter,
    descriptor: &jellyflow::runtime::schema::NodeKindViewDescriptor,
    rect: Rect,
    style: NodeRendererStyle,
) {
    if descriptor.ports.is_empty() {
        return;
    }
    let mut summary = String::new();
    for decl in &descriptor.ports {
        let label = decl.label.as_deref().unwrap_or(decl.key.0.as_str());
        if !summary.is_empty() {
            summary.push_str(" · ");
        }
        summary.push_str(label);
    }
    painter.text(
        rect.center_bottom() - Vec2::new(0.0, 14.0),
        Align2::CENTER_BOTTOM,
        summary,
        TextStyle::Small.resolve(&painter.ctx().global_style()),
        style.text.gamma_multiply(0.75),
    );
}

fn node_renderer_state(
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    node: NodeId,
) -> NodeRendererState {
    NodeRendererState {
        selected: bridge.store().view_state().selected_nodes.contains(&node),
        hovered: matches!(state.canvas.hovered, Some(HoverTarget::Node(found)) if found == node),
        focused: false,
        dragging: matches!(state.canvas.active, ActiveCanvasInteraction::NodeDrag { primary, .. } if primary == node),
        resizing: matches!(state.canvas.active, ActiveCanvasInteraction::NodeResize { node: found, .. } if found == node),
        connection_preview: matches!(state.canvas.active, ActiveCanvasInteraction::Connect { .. }),
        valid_target: false,
        invalid_target: false,
        disabled: false,
        hidden: bridge
            .store()
            .graph()
            .nodes()
            .get(&node)
            .is_none_or(|record| record.hidden),
        diagnostic: false,
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
            if should_draw_edge_labels(state) {
                draw_edge_label(painter, bridge, state, *edge_id, &path, color);
            }
        } else if let Some(path) = state.canvas.snapshot.edge_paths.get(edge_id) {
            draw_edge_path(painter, state, path, color);
            if should_draw_edge_labels(state) {
                draw_edge_label(painter, bridge, state, *edge_id, path, color);
            }
        }
    }
}

fn should_draw_edge_labels(state: &JellyflowEguiState) -> bool {
    NodeContentLevel::from_zoom(state.canvas.snapshot.transform.zoom).shows_detail()
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

fn draw_edge_label(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    edge_id: jellyflow::core::EdgeId,
    path: &EdgePath,
    color: Color32,
) {
    let Some(label) = edge_label(bridge, edge_id) else {
        return;
    };
    let label_anchor = bridge
        .store()
        .graph()
        .edges()
        .get(&edge_id)
        .and_then(|edge| edge.view.label_anchor)
        .unwrap_or(EdgeLabelAnchor::Center);
    let label_pos = to_screen(state, edge_label_point(path, label_anchor));
    let font = TextStyle::Small.resolve(&painter.ctx().global_style());
    let galley = painter.layout_no_wrap(label.to_owned(), font, color);
    let padding = Vec2::new(8.0, 4.0);
    let rect = Rect::from_center_size(label_pos, galley.size() + padding * 2.0);
    painter.rect_filled(rect, CornerRadius::same(4), Color32::WHITE);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, color.gamma_multiply(0.65)),
        StrokeKind::Outside,
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, color);
}

pub(crate) fn edge_label(
    bridge: &JellyflowEguiBridge,
    edge_id: jellyflow::core::EdgeId,
) -> Option<&str> {
    let edge = bridge.store().graph().edges().get(&edge_id)?;
    if let Some(label) = edge.view.label.as_ref().filter(|label| !label.is_empty()) {
        return Some(label);
    }
    for key in ["label", "condition", "cardinality", "branch"] {
        if let Some(label) = edge.data.get(key).and_then(|value| value.as_str())
            && !label.is_empty()
        {
            return Some(label);
        }
    }
    None
}

fn edge_label_point(path: &EdgePath, anchor: EdgeLabelAnchor) -> CanvasPoint {
    match anchor {
        EdgeLabelAnchor::Source => path
            .commands
            .first()
            .map(path_command_point)
            .unwrap_or(path.label.point),
        EdgeLabelAnchor::Center => path.label.point,
        EdgeLabelAnchor::Target => path
            .commands
            .last()
            .map(path_command_point)
            .unwrap_or(path.label.point),
    }
}

fn path_command_point(command: &jellyflow::runtime::runtime::geometry::PathCommand) -> CanvasPoint {
    match *command {
        jellyflow::runtime::runtime::geometry::PathCommand::MoveTo(point)
        | jellyflow::runtime::runtime::geometry::PathCommand::LineTo(point) => point,
        jellyflow::runtime::runtime::geometry::PathCommand::CubicTo { to, .. } => to,
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

fn draw_interaction_preview(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
) {
    match &state.canvas.active {
        ActiveCanvasInteraction::NodeDrag { .. } => {}
        ActiveCanvasInteraction::NodeResize { .. } => {}
        ActiveCanvasInteraction::Connect {
            from,
            current_pointer,
            target,
            ..
        } => {
            draw_connection_preview(painter, bridge, state, *from, *current_pointer, *target);
        }
        ActiveCanvasInteraction::SelectionBox { .. }
        | ActiveCanvasInteraction::Pan { .. }
        | ActiveCanvasInteraction::None => {}
    }
}

fn draw_connection_preview(
    painter: &eframe::egui::Painter,
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    from: ConnectionHandleRef,
    current_pointer: CanvasPoint,
    target: Option<jellyflow::runtime::runtime::connection::ResolvedConnectionTarget>,
) {
    let Some(start_rect) = visual_handle_screen_rect(bridge, state, from) else {
        return;
    };
    let start = start_rect.center();
    let end = target
        .and_then(|target| target.target)
        .and_then(|target| visual_handle_screen_rect(bridge, state, target.handle))
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
        } else if let Some(handle) = hit_handle(bridge, state, pointer)
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
        } else if let Some(edge) = hit_edge(state, bridge, pointer) {
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
    match state.canvas_tool {
        CanvasTool::Select => {
            if let Some(handle) = hit_handle(bridge, state, pointer) {
                return Some(HoverTarget::Handle(handle));
            }
            if let Some((node, direction)) = hit_resize_handle(state, bridge, pointer) {
                return Some(HoverTarget::ResizeHandle { node, direction });
            }
            if let Some(node) = hit_node(state, pointer) {
                return Some(HoverTarget::Node(node));
            }
            hit_edge(state, bridge, pointer).map(HoverTarget::Edge)
        }
        CanvasTool::Connect => hit_handle(bridge, state, pointer).map(HoverTarget::Handle),
        CanvasTool::Resize => hit_resize_handle(state, bridge, pointer)
            .map(|(node, direction)| HoverTarget::ResizeHandle { node, direction }),
        CanvasTool::Drag => hit_node(state, pointer).map(HoverTarget::Node),
        CanvasTool::CreateNode | CanvasTool::Pan => None,
    }
}

fn hit_edge(
    state: &JellyflowEguiState,
    bridge: &JellyflowEguiBridge,
    pointer: Pos2,
) -> Option<jellyflow::core::EdgeId> {
    for (edge_id, path) in state.canvas.snapshot.edge_paths() {
        let Some(edge) = bridge.store().graph().edges().get(edge_id) else {
            continue;
        };
        let hit_target_width = edge.view.hit_target_width;
        let base_options = bridge
            .store()
            .resolved_interaction_state()
            .edge_hit_test_options_for(edge);
        if edge_path_contains_point(
            path,
            state.canvas.snapshot.screen_point_to_canvas(pointer),
            edge_hit_test_options(base_options, hit_target_width),
        ) {
            return Some(*edge_id);
        }
    }
    None
}

fn edge_hit_test_options(
    base_options: EdgeHitTestOptions,
    hit_target_width: Option<f32>,
) -> EdgeHitTestOptions {
    hit_target_width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(|width| EdgeHitTestOptions {
            interaction_width: width,
            ..base_options
        })
        .unwrap_or(base_options)
}

fn hit_handle(
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    pointer: Pos2,
) -> Option<ConnectionHandleRef> {
    state
        .canvas
        .snapshot
        .handle_bounds
        .keys()
        .copied()
        .find(|handle| {
            visual_handle_screen_rect(bridge, state, *handle)
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
            handle: visual_handle_bounds(bridge, state, source_handle),
            fallback_position: handle_fallback_position(source_port.dir),
        },
        EdgeEndpointInput {
            node_rect: visual_node_canvas_rect(state, target_port.node)?,
            handle: visual_handle_bounds(bridge, state, target_handle),
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
    HandlePosition::fallback_for_direction(direction)
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
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    handle: ConnectionHandleRef,
) -> Option<Rect> {
    let node_rect = visual_node_screen_rect(state, handle.node)?;
    let bounds = visual_handle_bounds(bridge, state, handle)?;
    Some(handle_screen_rect_for_node(state, node_rect, bounds.rect))
}

fn visual_handle_bounds(
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    handle: ConnectionHandleRef,
) -> Option<HandleBounds> {
    visual_handle_bounds_for_node(bridge, state, handle.node)
        .into_iter()
        .find_map(|(found, bounds)| (found == handle).then_some(bounds))
}

fn visual_handle_bounds_for_node(
    bridge: &JellyflowEguiBridge,
    state: &JellyflowEguiState,
    node: NodeId,
) -> Vec<(ConnectionHandleRef, HandleBounds)> {
    if needs_preview_handle_bounds(state, node) {
        let Some(canvas_rect) = visual_node_canvas_rect(state, node) else {
            return Vec::new();
        };
        let Some(layout) = visual_node_render_layout(
            bridge,
            state,
            node,
            canvas_rect,
            node_renderer_state(bridge, state, node),
        ) else {
            return Vec::new();
        };
        return bridge.handle_bounds_for_size(node, canvas_rect.size, &layout.interactive_regions);
    }

    state
        .canvas
        .snapshot
        .handle_bounds
        .iter()
        .filter_map(|(handle, bounds)| (handle.node == node).then_some((*handle, *bounds)))
        .collect()
}

fn needs_preview_handle_bounds(state: &JellyflowEguiState, node: NodeId) -> bool {
    matches!(
        &state.canvas.active,
        ActiveCanvasInteraction::NodeResize {
            node: resizing_node,
            preview: Some(_),
            ..
        } if *resizing_node == node
    )
}

fn handle_screen_rect_for_node(
    state: &JellyflowEguiState,
    node_rect: Rect,
    handle_rect: CanvasRect,
) -> Rect {
    let zoom = state.canvas.snapshot.transform.zoom;
    node_local_rect_to_screen(node_rect, handle_rect, zoom)
}

fn node_local_rect_to_screen(node_rect: Rect, local_rect: CanvasRect, zoom: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            node_rect.min.x + local_rect.origin.x * zoom,
            node_rect.min.y + local_rect.origin.y * zoom,
        ),
        Vec2::new(local_rect.size.width * zoom, local_rect.size.height * zoom),
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
            None => match state.canvas_tool {
                CanvasTool::Pan => CursorIcon::Grab,
                CanvasTool::Connect | CanvasTool::CreateNode => CursorIcon::Crosshair,
                CanvasTool::Resize => CursorIcon::Default,
                CanvasTool::Select | CanvasTool::Drag => CursorIcon::Default,
            },
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
    use jellyflow::core::{CanvasPoint, EdgeId, EdgeLabelAnchor, NodeId};
    use jellyflow::runtime::runtime::connection::ConnectionHandleRef;
    use jellyflow::runtime::runtime::geometry::{EdgeHitTestOptions, PathCommand};
    use jellyflow::runtime::runtime::resize::NodeResizeDirection;

    use super::{
        edge_hit_test_options, edge_label, edge_label_point, hit_target, node_local_rect_to_screen,
        node_title_text_rect, preview_edge_path, resize_handle_rect, should_draw_edge_labels,
        visual_handle_screen_rect, visual_node_canvas_rect, visual_node_screen_rect,
    };
    use crate::bridge::JellyflowEguiBridge;
    use crate::samples::SampleGraphKind;
    use crate::state::{
        ActiveCanvasInteraction, CanvasSnapshot, CanvasTool, HoverTarget, JellyflowEguiState,
    };

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

    #[test]
    fn edge_preview_follows_resized_source_handle_geometry() {
        let (mut bridge, _layout) =
            JellyflowEguiBridge::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let mut state = state_with_snapshot(&mut bridge);
        let (edge, source_node, source_handle) = first_edge_with_table_source(&bridge);
        let original_path = preview_or_snapshot_edge_path(&state, &bridge, edge);
        let original_handle_rect =
            visual_handle_screen_rect(&bridge, &state, source_handle).expect("handle is visible");
        let source_rect =
            visual_node_canvas_rect(&state, source_node).expect("source node rect exists");
        let start = CanvasPoint {
            x: source_rect.origin.x + source_rect.size.width,
            y: source_rect.origin.y + source_rect.size.height,
        };
        let current = CanvasPoint {
            x: start.x + 96.0,
            y: start.y,
        };
        state.canvas.active = ActiveCanvasInteraction::NodeResize {
            node: source_node,
            direction: NodeResizeDirection::BottomRight,
            start_pointer: start,
            current_pointer: current,
            preview: bridge.plan_pointer_resize(
                source_node,
                start,
                current,
                NodeResizeDirection::BottomRight,
            ),
        };

        let preview_handle_rect =
            visual_handle_screen_rect(&bridge, &state, source_handle).expect("preview handle");
        let preview_path = preview_edge_path(&state, &bridge, edge).expect("preview path");

        assert!(
            preview_handle_rect.center().x > original_handle_rect.center().x + 40.0,
            "source handle should follow the resized visual width"
        );
        assert!(
            first_path_point(&preview_path).x > first_path_point(&original_path).x + 40.0,
            "edge source endpoint should follow the resized source handle"
        );
    }

    #[test]
    fn field_hover_does_not_promote_to_resize_target() {
        let (mut bridge, _layout) =
            JellyflowEguiBridge::sample(SampleGraphKind::Erd).expect("erd sample builds");
        let state = state_with_snapshot(&mut bridge);
        let table = first_table_node(&bridge);
        bridge.select_node(table, false);
        let node_rect = visual_node_screen_rect(&state, table).expect("table is visible");
        let layout = state
            .canvas
            .snapshot
            .node_render_layouts
            .get(&table)
            .expect("table layout exists");
        let field = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "field.primary_key")
            .expect("primary key field exists");
        let pointer =
            node_local_rect_to_screen(node_rect, field.rect, state.canvas.snapshot.transform.zoom)
                .center();

        assert_eq!(
            hit_target(&state, &bridge, pointer),
            Some(HoverTarget::Node(table))
        );
    }

    #[test]
    fn hover_target_respects_active_canvas_tool() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let mut state = state_with_snapshot(&mut bridge);
        let node = first_visible_node(&state);
        let node_rect = visual_node_screen_rect(&state, node).expect("node is visible");
        let node_center = node_rect.center();

        state.canvas_tool = CanvasTool::Select;
        assert_eq!(
            hit_target(&state, &bridge, node_center),
            Some(HoverTarget::Node(node))
        );

        state.canvas_tool = CanvasTool::Pan;
        assert_eq!(hit_target(&state, &bridge, node_center), None);

        state.canvas_tool = CanvasTool::Connect;
        assert_eq!(hit_target(&state, &bridge, node_center), None);

        state.canvas_tool = CanvasTool::Drag;
        assert_eq!(
            hit_target(&state, &bridge, node_center),
            Some(HoverTarget::Node(node))
        );

        bridge.select_node(node, false);
        let resize_pointer =
            resize_handle_rect(node_rect, NodeResizeDirection::BottomRight).center();
        state.canvas_tool = CanvasTool::Select;
        assert_eq!(
            hit_target(&state, &bridge, resize_pointer),
            Some(HoverTarget::ResizeHandle {
                node,
                direction: NodeResizeDirection::BottomRight
            })
        );
        state.canvas_tool = CanvasTool::Pan;
        assert_eq!(hit_target(&state, &bridge, resize_pointer), None);
        state.canvas_tool = CanvasTool::Resize;
        assert_eq!(
            hit_target(&state, &bridge, resize_pointer),
            Some(HoverTarget::ResizeHandle {
                node,
                direction: NodeResizeDirection::BottomRight
            })
        );
    }

    #[test]
    fn edge_label_prefers_view_label_and_falls_back_to_edge_data() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let edge = *bridge
            .store()
            .graph()
            .edges()
            .keys()
            .next()
            .expect("sample edge exists");

        assert!(edge_label(&bridge, edge).is_some());

        let from_data = bridge.store().graph().edges()[&edge].data.clone();
        let from_view = bridge.store().graph().edges()[&edge].view.clone();
        let mut to_view = from_view.clone();
        to_view.label = None;
        bridge
            .store_mut()
            .dispatch_transaction(&jellyflow::core::GraphTransaction::from_ops([
                jellyflow::core::GraphOp::SetEdgeData {
                    id: edge,
                    from: from_data,
                    to: serde_json::json!({ "condition": "approved" }),
                },
                jellyflow::core::GraphOp::SetEdgeView {
                    id: edge,
                    from: from_view,
                    to: to_view,
                },
            ]))
            .expect("edge metadata dispatches");

        assert_eq!(edge_label(&bridge, edge), Some("approved"));
    }

    #[test]
    fn edge_label_point_honors_edge_view_anchor() {
        let mut bridge = JellyflowEguiBridge::demo().expect("demo bridge builds");
        let state = state_with_snapshot(&mut bridge);
        let edge = *state
            .canvas
            .snapshot
            .visible_edge_render_order
            .first()
            .expect("demo edge exists");
        let path = preview_or_snapshot_edge_path(&state, &bridge, edge);

        assert_eq!(
            edge_label_point(&path, EdgeLabelAnchor::Source),
            first_path_point(&path)
        );
        assert_eq!(
            edge_label_point(&path, EdgeLabelAnchor::Target),
            last_path_point(&path)
        );
        assert_eq!(
            edge_label_point(&path, EdgeLabelAnchor::Center),
            path.label.point
        );
    }

    #[test]
    fn edge_hit_test_options_use_positive_hit_target_width() {
        let base_options = EdgeHitTestOptions {
            interaction_width: 17.0,
            ..EdgeHitTestOptions::default()
        };
        assert_eq!(
            edge_hit_test_options(base_options, Some(30.0)).interaction_width,
            30.0
        );
        assert_eq!(
            edge_hit_test_options(base_options, Some(f32::NAN)).interaction_width,
            base_options.interaction_width
        );
        assert_eq!(
            edge_hit_test_options(base_options, Some(0.0)).interaction_width,
            base_options.interaction_width
        );
    }

    #[test]
    fn compact_title_rect_stays_inside_node_bounds() {
        let node_rect = Rect::from_min_size(Pos2::new(100.0, 20.0), Vec2::new(64.0, 28.0));
        let text_rect = node_title_text_rect(node_rect, crate::renderer::NodeContentLevel::Compact)
            .expect("compact node should still expose a title rect");

        assert!(node_rect.contains_rect(text_rect));
        assert!(text_rect.width() < node_rect.width());
    }

    #[test]
    fn edge_labels_are_hidden_in_compact_zoom() {
        let mut state = JellyflowEguiState::default();
        state.canvas.snapshot.transform.zoom = 0.5;
        assert!(!should_draw_edge_labels(&state));

        state.canvas.snapshot.transform.zoom = 1.0;
        assert!(should_draw_edge_labels(&state));
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

    fn first_table_node(bridge: &JellyflowEguiBridge) -> NodeId {
        bridge
            .store()
            .graph()
            .nodes()
            .iter()
            .find_map(|(node, record)| (record.kind.0 == "demo.table").then_some(*node))
            .expect("table node exists")
    }

    fn first_edge_with_table_source(
        bridge: &JellyflowEguiBridge,
    ) -> (EdgeId, NodeId, ConnectionHandleRef) {
        let graph = bridge.store().graph();
        graph
            .edges()
            .iter()
            .find_map(|(edge, record)| {
                let source_port = graph.ports().get(&record.from)?;
                let source_node = graph.nodes().get(&source_port.node)?;
                (source_node.kind.0 == "demo.table").then_some((
                    *edge,
                    source_port.node,
                    ConnectionHandleRef::new(source_port.node, record.from, source_port.dir),
                ))
            })
            .expect("table source edge exists")
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

    fn last_path_point(path: &jellyflow::runtime::runtime::geometry::EdgePath) -> CanvasPoint {
        match path.commands.last().expect("path has a last command") {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => *point,
            PathCommand::CubicTo { to, .. } => *to,
        }
    }
}
