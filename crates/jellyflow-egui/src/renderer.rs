use std::collections::BTreeMap;

use eframe::egui::{
    Align, Color32, CornerRadius, Id, Label, Layout, Pos2, Rect, Stroke, Ui, UiBuilder, Vec2,
};
use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, Node, NodeId};
use jellyflow::runtime::schema::{
    NodeKindViewDescriptor, NodeSurfaceSlotDescriptor, NodeSurfaceSlotKind,
};

/// Visual style mapped from an adapter-owned renderer key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRendererStyle {
    pub fill: Color32,
    pub stroke: Color32,
    pub accent: Color32,
    pub text: Color32,
}

impl NodeRendererStyle {
    pub const fn new(fill: Color32, stroke: Color32, accent: Color32, text: Color32) -> Self {
        Self {
            fill,
            stroke,
            accent,
            text,
        }
    }

    pub const fn task() -> Self {
        Self::new(
            Color32::from_rgb(252, 253, 255),
            Color32::from_rgb(198, 207, 219),
            Color32::from_rgb(42, 104, 176),
            Color32::from_rgb(31, 41, 55),
        )
    }

    pub const fn decision() -> Self {
        Self::new(
            Color32::from_rgb(255, 252, 246),
            Color32::from_rgb(219, 201, 168),
            Color32::from_rgb(188, 113, 32),
            Color32::from_rgb(61, 46, 28),
        )
    }

    pub const fn data() -> Self {
        Self::new(
            Color32::from_rgb(247, 252, 250),
            Color32::from_rgb(177, 207, 196),
            Color32::from_rgb(22, 128, 96),
            Color32::from_rgb(27, 53, 48),
        )
    }

    pub const fn output() -> Self {
        Self::new(
            Color32::from_rgb(253, 251, 255),
            Color32::from_rgb(206, 195, 222),
            Color32::from_rgb(111, 88, 161),
            Color32::from_rgb(49, 38, 70),
        )
    }

    pub const fn topic() -> Self {
        Self::new(
            Color32::from_rgb(249, 252, 255),
            Color32::from_rgb(186, 205, 225),
            Color32::from_rgb(31, 105, 168),
            Color32::from_rgb(26, 45, 68),
        )
    }

    pub const fn idea() -> Self {
        Self::new(
            Color32::from_rgb(252, 253, 248),
            Color32::from_rgb(195, 207, 163),
            Color32::from_rgb(88, 128, 54),
            Color32::from_rgb(43, 55, 34),
        )
    }

    pub const fn section() -> Self {
        Self::new(
            Color32::from_rgb(252, 252, 255),
            Color32::from_rgb(198, 201, 216),
            Color32::from_rgb(70, 91, 148),
            Color32::from_rgb(42, 44, 68),
        )
    }

    pub const fn source() -> Self {
        Self::new(
            Color32::from_rgb(255, 252, 247),
            Color32::from_rgb(215, 196, 169),
            Color32::from_rgb(145, 94, 46),
            Color32::from_rgb(64, 48, 34),
        )
    }

    pub const fn fallback() -> Self {
        Self::new(
            Color32::from_rgb(252, 252, 251),
            Color32::from_rgb(202, 202, 196),
            Color32::from_rgb(91, 91, 82),
            Color32::from_rgb(36, 36, 32),
        )
    }

    pub fn selected_stroke(self) -> Stroke {
        Stroke::new(2.0, self.accent)
    }
}

/// Adapter-owned renderer state passed to rich node renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeRendererState {
    pub selected: bool,
    pub hovered: bool,
    pub focused: bool,
    pub dragging: bool,
    pub resizing: bool,
    pub connection_preview: bool,
    pub valid_target: bool,
    pub invalid_target: bool,
    pub disabled: bool,
    pub hidden: bool,
    pub diagnostic: bool,
}

/// Renderer-neutral input for adapter-owned rich node renderers.
#[derive(Debug, Clone)]
pub struct NodeRenderInput<'a> {
    pub id: NodeId,
    pub node: &'a Node,
    pub descriptor: &'a NodeKindViewDescriptor,
    pub state: NodeRendererState,
    pub style: NodeRendererStyle,
}

/// Zoom-aware node content level for adapter-owned widget renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeContentLevel {
    Full,
    Compact,
    Shell,
}

impl NodeContentLevel {
    pub fn from_zoom(zoom: f32) -> Self {
        if zoom >= 0.62 {
            Self::Full
        } else if zoom >= 0.18 {
            Self::Compact
        } else {
            Self::Shell
        }
    }

    pub fn from_zoom_and_size(zoom: f32, size: Vec2) -> Self {
        let zoom_level = Self::from_zoom(zoom);
        if size.x < 48.0 || size.y < 24.0 {
            Self::Shell
        } else if size.x < 150.0 || size.y < 90.0 {
            zoom_level.min_detail(Self::Compact)
        } else {
            zoom_level
        }
    }

    pub fn shows_text(self) -> bool {
        matches!(self, Self::Full | Self::Compact)
    }

    pub fn shows_detail(self) -> bool {
        matches!(self, Self::Full)
    }

    fn min_detail(self, cap: Self) -> Self {
        match (self, cap) {
            (Self::Shell, _) | (_, Self::Shell) => Self::Shell,
            (Self::Compact, _) | (_, Self::Compact) => Self::Compact,
            (Self::Full, Self::Full) => Self::Full,
        }
    }

    fn renders_slot_kind(self, slot_kind: Option<NodeSurfaceSlotKind>) -> bool {
        match self {
            Self::Full => true,
            Self::Compact => matches!(
                slot_kind,
                Some(NodeSurfaceSlotKind::FieldRow | NodeSurfaceSlotKind::PortRail)
            ),
            Self::Shell => false,
        }
    }
}

/// egui-specific widget rendering input for rich node internals.
#[derive(Debug)]
pub struct NodeWidgetRenderInput<'a> {
    pub id: NodeId,
    pub node: &'a Node,
    pub descriptor: &'a NodeKindViewDescriptor,
    pub state: NodeRendererState,
    pub style: NodeRendererStyle,
    pub layout: &'a NodeRenderLayout,
    pub node_rect: Rect,
    /// Screen-space clip rect inherited from the canvas viewport.
    ///
    /// Widget renderers should intersect child widget rects with this value before painting.
    pub clip_rect: Rect,
    pub zoom: f32,
    pub content_level: NodeContentLevel,
}

impl NodeWidgetRenderInput<'_> {
    pub fn region_screen_rect(&self, region: &NodeInteractiveRegion) -> Option<Rect> {
        self.node_local_screen_rect(region.rect)
    }

    pub fn node_local_screen_rect(&self, rect: CanvasRect) -> Option<Rect> {
        let rect = node_local_rect_to_screen(self.node_rect, rect, self.zoom)
            .intersect(self.node_rect)
            .intersect(self.clip_rect);
        rect.is_positive().then_some(rect)
    }
}

/// Renderer output consumed by the egui canvas fallback painter.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRenderLayout {
    pub title: String,
    pub summary: Option<String>,
    pub min_size: CanvasSize,
    pub body_rect: CanvasRect,
    pub interactive_regions: Vec<NodeInteractiveRegion>,
}

impl NodeRenderLayout {
    pub fn fallback(input: &NodeRenderInput<'_>, rect: CanvasRect) -> Self {
        Self {
            title: node_title(input).unwrap_or_else(|| input.descriptor.title.clone()),
            summary: node_summary(input),
            min_size: input.descriptor.default_size.unwrap_or(CanvasSize {
                width: 160.0,
                height: 80.0,
            }),
            body_rect: rect,
            interactive_regions: Vec::new(),
        }
    }
}

/// Named hit-test or event region produced by a rich renderer.
///
/// The rect is node-local and relative to the node's top-left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeInteractiveRegion {
    pub key: String,
    pub slot_kind: Option<NodeSurfaceSlotKind>,
    pub rect: CanvasRect,
    pub label: Option<String>,
    pub z_index: i32,
}

/// Adapter-owned renderer contract. Implementors may measure rich node bodies without mutating graph state.
pub trait RichNodeRenderer: Send + Sync {
    fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout;
}

/// Adapter-owned egui widget renderer for drawing controls inside a node.
pub trait EguiNodeWidgetRenderer: Send + Sync {
    fn render_widgets(&self, ui: &mut Ui, input: &NodeWidgetRenderInput<'_>) -> bool;
}

#[derive(Debug, Clone, Copy)]
struct FallbackRichNodeRenderer;

impl RichNodeRenderer for FallbackRichNodeRenderer {
    fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
        NodeRenderLayout::fallback(input, rect)
    }
}

/// Adapter-owned renderer catalog keyed by `NodeKindViewDescriptor::renderer_key`.
pub struct RendererCatalog {
    fallback: NodeRendererStyle,
    by_renderer_key: BTreeMap<String, NodeRendererStyle>,
    rich_renderers: BTreeMap<String, Box<dyn RichNodeRenderer>>,
    widget_renderers: BTreeMap<String, Box<dyn EguiNodeWidgetRenderer>>,
}

impl std::fmt::Debug for RendererCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererCatalog")
            .field("fallback", &self.fallback)
            .field("style_count", &self.by_renderer_key.len())
            .field("rich_renderer_count", &self.rich_renderers.len())
            .field("widget_renderer_count", &self.widget_renderers.len())
            .finish()
    }
}

impl Default for RendererCatalog {
    fn default() -> Self {
        Self::with_builtin_styles()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FieldListNodeRenderer;

impl RendererCatalog {
    pub fn new() -> Self {
        Self {
            fallback: NodeRendererStyle::fallback(),
            by_renderer_key: BTreeMap::new(),
            rich_renderers: BTreeMap::new(),
            widget_renderers: BTreeMap::new(),
        }
    }

    pub fn with_builtin_styles() -> Self {
        let mut catalog = Self::new();
        catalog
            .register("task-card", NodeRendererStyle::task())
            .register("decision-card", NodeRendererStyle::decision())
            .register("data-card", NodeRendererStyle::data())
            .register("output-card", NodeRendererStyle::output())
            .register("topic-card", NodeRendererStyle::topic())
            .register("idea-card", NodeRendererStyle::idea())
            .register("section-card", NodeRendererStyle::section())
            .register("source-card", NodeRendererStyle::source())
            .register("table-card", NodeRendererStyle::section())
            .register("shader-card", NodeRendererStyle::data())
            .register_rich("decision-card", FieldListNodeRenderer)
            .register_widgets("decision-card", FieldListNodeRenderer)
            .register_rich("table-card", FieldListNodeRenderer)
            .register_widgets("table-card", FieldListNodeRenderer)
            .register_rich("shader-card", FieldListNodeRenderer)
            .register_widgets("shader-card", FieldListNodeRenderer);
        catalog
    }

    pub fn register(
        &mut self,
        renderer_key: impl Into<String>,
        style: NodeRendererStyle,
    ) -> &mut Self {
        self.by_renderer_key.insert(renderer_key.into(), style);
        self
    }

    pub fn register_rich(
        &mut self,
        renderer_key: impl Into<String>,
        renderer: impl RichNodeRenderer + 'static,
    ) -> &mut Self {
        self.rich_renderers
            .insert(renderer_key.into(), Box::new(renderer));
        self
    }

    pub fn register_widgets(
        &mut self,
        renderer_key: impl Into<String>,
        renderer: impl EguiNodeWidgetRenderer + 'static,
    ) -> &mut Self {
        self.widget_renderers
            .insert(renderer_key.into(), Box::new(renderer));
        self
    }

    pub fn render_node(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
        self.rich_renderers
            .get(&input.descriptor.renderer_key)
            .map(|renderer| renderer.as_ref())
            .unwrap_or(&FallbackRichNodeRenderer)
            .render(input, rect)
    }

    pub fn render_widgets(&self, ui: &mut Ui, input: &NodeWidgetRenderInput<'_>) -> bool {
        self.widget_renderers
            .get(&input.descriptor.renderer_key)
            .is_some_and(|renderer| renderer.render_widgets(ui, input))
    }

    pub fn has_widget_renderer(&self, renderer_key: &str) -> bool {
        self.widget_renderers.contains_key(renderer_key)
    }

    pub fn style_for_descriptor(&self, descriptor: &NodeKindViewDescriptor) -> NodeRendererStyle {
        self.style_for_key(&descriptor.renderer_key)
    }

    pub fn style_for_key(&self, renderer_key: &str) -> NodeRendererStyle {
        self.by_renderer_key
            .get(renderer_key)
            .copied()
            .unwrap_or(self.fallback)
    }
}

fn node_summary(input: &NodeRenderInput<'_>) -> Option<String> {
    let summary = input
        .node
        .data
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    (!summary.is_empty()).then(|| summary.to_owned())
}

fn node_title(input: &NodeRenderInput<'_>) -> Option<String> {
    let title = input
        .node
        .data
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    (!title.is_empty()).then(|| title.to_owned())
}

impl RichNodeRenderer for FieldListNodeRenderer {
    fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
        let mut layout = NodeRenderLayout::fallback(input, rect);
        let fields = input
            .node
            .data
            .get("fields")
            .and_then(|value| value.as_object());
        let field_slots = semantic_slots(input, NodeSurfaceSlotKind::FieldRow);
        let badge_slots = semantic_slots(input, NodeSurfaceSlotKind::Badge);
        let metric_slots = semantic_slots(input, NodeSurfaceSlotKind::MetricBadge);
        let status_slots = semantic_slots(input, NodeSurfaceSlotKind::StatusBanner);
        let rail_slots = semantic_slots(input, NodeSurfaceSlotKind::PortRail);
        let config_slots = semantic_slots(input, NodeSurfaceSlotKind::ConfigGroup);
        let preview_slots = semantic_slots(input, NodeSurfaceSlotKind::Preview);
        let nested_slots = semantic_slots(input, NodeSurfaceSlotKind::NestedRegion);
        let action_slots = semantic_slots(input, NodeSurfaceSlotKind::ActionRow);
        let keys = fields
            .map(|fields| field_keys(input, fields, &field_slots))
            .unwrap_or_default();
        let metrics = FieldListMetrics::new(rect.size);
        let field_count = keys.len();

        for slot in badge_slots.into_iter().chain(metric_slots) {
            let text = semantic_slot_chip_text(&input.node.data, slot, fields);
            let width = semantic_chip_width(&text, 32.0, rect.size.width * 0.42);
            let chip_rect = CanvasRect {
                origin: CanvasPoint {
                    x: (rect.size.width - 14.0 - width).max(14.0),
                    y: 12.0,
                },
                size: CanvasSize {
                    width,
                    height: 18.0,
                },
            };
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: slot.key.clone(),
                slot_kind: Some(slot.kind),
                rect: chip_rect,
                label: Some(text),
                z_index: 2,
            });
        }

        for (index, key) in keys.into_iter().enumerate() {
            let row_rect = metrics.row_rect(index);
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: format!("field.{key}"),
                slot_kind: Some(NodeSurfaceSlotKind::FieldRow),
                rect: row_rect,
                label: fields
                    .and_then(|fields| fields.get(&key))
                    .map(field_value_label),
                z_index: 1,
            });
        }
        let mut cursor_y = metrics.bottom_after(field_count);

        for slot in rail_slots
            .into_iter()
            .chain(config_slots)
            .chain(preview_slots)
            .chain(nested_slots)
        {
            let title = semantic_slot_title(slot);
            let lines = semantic_slot_lines(&input.node.data, slot, fields);
            let height = semantic_nested_region_height(title.as_deref(), &lines);
            let block_rect = CanvasRect {
                origin: CanvasPoint {
                    x: 14.0,
                    y: cursor_y,
                },
                size: CanvasSize {
                    width: (rect.size.width - 28.0).max(96.0),
                    height,
                },
            };
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: slot.key.clone(),
                slot_kind: Some(slot.kind),
                rect: block_rect,
                label: title,
                z_index: 1,
            });
            cursor_y = block_rect.origin.y + block_rect.size.height + 8.0;
        }

        for slot in status_slots {
            let title = semantic_slot_title(slot);
            let lines = semantic_slot_lines(&input.node.data, slot, fields);
            let height = semantic_nested_region_height(title.as_deref(), &lines).min(42.0);
            let block_rect = CanvasRect {
                origin: CanvasPoint {
                    x: 14.0,
                    y: cursor_y,
                },
                size: CanvasSize {
                    width: (rect.size.width - 28.0).max(96.0),
                    height,
                },
            };
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: slot.key.clone(),
                slot_kind: Some(NodeSurfaceSlotKind::StatusBanner),
                rect: block_rect,
                label: title,
                z_index: 1,
            });
            cursor_y = block_rect.origin.y + block_rect.size.height + 8.0;
        }

        for slot in action_slots {
            let items = semantic_slot_lines(&input.node.data, slot, fields);
            let height = semantic_action_region_height(&items);
            let block_rect = CanvasRect {
                origin: CanvasPoint {
                    x: 14.0,
                    y: cursor_y,
                },
                size: CanvasSize {
                    width: (rect.size.width - 28.0).max(96.0),
                    height,
                },
            };
            if items.is_empty() {
                layout.interactive_regions.push(NodeInteractiveRegion {
                    key: slot.key.clone(),
                    slot_kind: Some(NodeSurfaceSlotKind::ActionRow),
                    rect: block_rect,
                    label: semantic_slot_title(slot),
                    z_index: 1,
                });
            } else {
                let mut chip_y = block_rect.origin.y;
                for (index, item) in items.into_iter().enumerate() {
                    let chip_rect = CanvasRect {
                        origin: CanvasPoint {
                            x: block_rect.origin.x,
                            y: chip_y,
                        },
                        size: CanvasSize {
                            width: block_rect.size.width,
                            height: 18.0,
                        },
                    };
                    layout.interactive_regions.push(NodeInteractiveRegion {
                        key: format!("{}.{index}", slot.key),
                        slot_kind: Some(NodeSurfaceSlotKind::ActionRow),
                        rect: chip_rect,
                        label: Some(item),
                        z_index: 1,
                    });
                    chip_y += 20.0;
                }
            }
            cursor_y = block_rect.origin.y + block_rect.size.height + 8.0;
        }

        layout.min_size = metrics.min_size(layout.min_size, field_count);
        layout.min_size.height = layout.min_size.height.max(cursor_y + 8.0);
        layout
    }
}

impl EguiNodeWidgetRenderer for FieldListNodeRenderer {
    fn render_widgets(&self, ui: &mut Ui, input: &NodeWidgetRenderInput<'_>) -> bool {
        if input.content_level == NodeContentLevel::Shell {
            return false;
        }

        let fields = input
            .node
            .data
            .get("fields")
            .and_then(|value| value.as_object());

        let show_detail = input.content_level.shows_detail();
        let mut rendered_any = false;

        if let Some(fields) = fields {
            for region in input.layout.interactive_regions.iter().filter(|region| {
                region.slot_kind == Some(NodeSurfaceSlotKind::FieldRow)
                    || region.key.starts_with("field.")
            }) {
                let key = region
                    .key
                    .strip_prefix("field.")
                    .unwrap_or(region.key.as_str());
                let Some(label) = fields
                    .get(key)
                    .map(field_value_label)
                    .filter(|label| !label.is_empty())
                else {
                    continue;
                };
                let Some(rect) = input.region_screen_rect(region) else {
                    continue;
                };
                rendered_any = true;
                let mut child_ui = ui.new_child(
                    UiBuilder::new()
                        .id_salt(Id::new(("field-region", input.id, &region.key)))
                        .max_rect(rect)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                child_ui.set_clip_rect(rect);
                child_ui.set_min_size(rect.size());
                child_ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    Color32::from_rgb(255, 255, 255),
                );
                child_ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(4),
                    Stroke::new(0.75, input.style.stroke.gamma_multiply(0.55)),
                    eframe::egui::StrokeKind::Inside,
                );

                let mut content_rect = rect.shrink2(Vec2::new(7.0, 2.0));
                if content_rect.width() <= 6.0 || content_rect.height() <= 6.0 {
                    continue;
                }
                child_ui.scope_builder(UiBuilder::new().max_rect(content_rect), |ui| {
                    ui.set_clip_rect(content_rect);
                    ui.set_min_size(content_rect.size());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                        if show_detail && let Some(badge) = field_badge(key) {
                            let badge_width = 24.0f32.min(content_rect.width() * 0.38);
                            let badge_rect = Rect::from_min_size(
                                content_rect.min,
                                Vec2::new(badge_width, content_rect.height()),
                            );
                            draw_field_badge(ui, badge_rect, badge, input.style.accent);
                            content_rect.min.x = badge_rect.max.x + 6.0;
                        }
                        ui.scope_builder(UiBuilder::new().max_rect(content_rect), |ui| {
                            ui.set_clip_rect(content_rect);
                            ui.add(
                                Label::new(
                                    eframe::egui::RichText::new(label)
                                        .small()
                                        .color(input.style.text.gamma_multiply(0.84)),
                                )
                                .truncate(),
                            );
                        });
                    });
                });
            }
        }

        for region in input.layout.interactive_regions.iter().filter(|region| {
            input.content_level.renders_slot_kind(region.slot_kind) && region_is_badge(region)
        }) {
            let Some(rect) = input.region_screen_rect(region) else {
                continue;
            };
            rendered_any = true;
            draw_semantic_chip(
                ui,
                rect,
                region.label.as_deref().unwrap_or(region.key.as_str()),
                input.style.accent.gamma_multiply(0.16),
                input.style.accent.gamma_multiply(0.52),
                input.style.text,
            );
        }

        for region in input.layout.interactive_regions.iter().filter(|region| {
            input.content_level.renders_slot_kind(region.slot_kind)
                && region_is_detail_block(region)
        }) {
            let Some(rect) = input.region_screen_rect(region) else {
                continue;
            };
            rendered_any = true;
            let slot = input
                .descriptor
                .surface_slots
                .iter()
                .find(|slot| slot.key == region.key);
            let title = region
                .label
                .as_deref()
                .or_else(|| slot.and_then(|slot| slot.display_label()));
            let lines = slot
                .filter(|_| show_detail)
                .map(|slot| semantic_slot_lines(&input.node.data, slot, fields))
                .unwrap_or_default();
            draw_semantic_nested_region(ui, rect, title, &lines, input.style);
        }

        for region in input.layout.interactive_regions.iter().filter(|region| {
            input.content_level.renders_slot_kind(region.slot_kind) && region_is_action(region)
        }) {
            let Some(rect) = input.region_screen_rect(region) else {
                continue;
            };
            rendered_any = true;
            let text = region
                .label
                .as_deref()
                .unwrap_or_else(|| region.key.as_str());
            draw_semantic_chip(
                ui,
                rect,
                text,
                input.style.accent.gamma_multiply(0.12),
                input.style.stroke.gamma_multiply(0.62),
                input.style.text,
            );
        }

        rendered_any
    }
}

fn region_is_detail_block(region: &NodeInteractiveRegion) -> bool {
    region.slot_kind == Some(NodeSurfaceSlotKind::NestedRegion)
        || region.slot_kind == Some(NodeSurfaceSlotKind::ConfigGroup)
        || region.slot_kind == Some(NodeSurfaceSlotKind::PortRail)
        || region.slot_kind == Some(NodeSurfaceSlotKind::Preview)
        || region.slot_kind == Some(NodeSurfaceSlotKind::StatusBanner)
        || region.key.starts_with("nested.")
        || region.key.starts_with("config.")
        || region.key.starts_with("rail.")
        || region.key.starts_with("preview.")
        || region.key.starts_with("status.")
}

fn region_is_action(region: &NodeInteractiveRegion) -> bool {
    region.slot_kind == Some(NodeSurfaceSlotKind::ActionRow) || region.key.starts_with("actions.")
}

fn region_is_badge(region: &NodeInteractiveRegion) -> bool {
    region.slot_kind == Some(NodeSurfaceSlotKind::Badge)
        || region.key.starts_with("badge.")
        || region.slot_kind == Some(NodeSurfaceSlotKind::MetricBadge)
        || region.key.starts_with("metric.")
}

fn field_keys(
    input: &NodeRenderInput<'_>,
    fields: &serde_json::Map<String, serde_json::Value>,
    field_slots: &[&NodeSurfaceSlotDescriptor],
) -> Vec<String> {
    let mut keys = Vec::new();
    for slot in field_slots {
        if let Some(key) = slot.data_key()
            && fields.contains_key(key)
            && !keys.iter().any(|existing| existing == key)
        {
            keys.push(key.to_owned());
        }
    }

    if let Some(order) = input
        .node
        .data
        .get("field_order")
        .and_then(|value| value.as_array())
    {
        for key in order.iter().filter_map(|value| value.as_str()) {
            if fields.contains_key(key) && !keys.iter().any(|existing| existing == key) {
                keys.push(key.to_owned());
            }
        }
    }

    for key in fields.keys() {
        if !keys.iter().any(|existing| existing == key) {
            keys.push(key.clone());
        }
    }
    keys
}

fn semantic_slots<'a>(
    input: &'a NodeRenderInput<'_>,
    kind: NodeSurfaceSlotKind,
) -> Vec<&'a NodeSurfaceSlotDescriptor> {
    input.descriptor.surface_slots_of_kind(kind)
}

fn semantic_slot_title(slot: &NodeSurfaceSlotDescriptor) -> Option<String> {
    slot.display_label().map(ToOwned::to_owned)
}

fn semantic_slot_chip_text(
    node_data: &serde_json::Value,
    slot: &NodeSurfaceSlotDescriptor,
    fields: Option<&serde_json::Map<String, serde_json::Value>>,
) -> String {
    semantic_slot_value(node_data, slot, fields)
        .map(semantic_value_preview)
        .filter(|text| !text.is_empty())
        .or_else(|| semantic_slot_title(slot))
        .unwrap_or_else(|| slot.key.clone())
}

fn semantic_slot_lines(
    node_data: &serde_json::Value,
    slot: &NodeSurfaceSlotDescriptor,
    fields: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Vec<String> {
    let Some(value) = semantic_slot_value(node_data, slot, fields) else {
        return semantic_slot_title(slot).into_iter().collect();
    };

    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .map(semantic_value_preview)
            .filter(|text| !text.is_empty())
            .collect(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| format!("{key}: {}", semantic_value_preview(value)))
            .collect(),
        other => {
            let text = semantic_value_preview(other);
            if text.is_empty() {
                semantic_slot_title(slot).into_iter().collect()
            } else {
                text.lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            }
        }
    }
}

fn semantic_slot_value<'a>(
    node_data: &'a serde_json::Value,
    slot: &NodeSurfaceSlotDescriptor,
    fields: Option<&'a serde_json::Map<String, serde_json::Value>>,
) -> Option<&'a serde_json::Value> {
    let key = slot.data_key()?;
    if slot.kind == NodeSurfaceSlotKind::FieldRow
        && let Some(fields) = fields
        && let Some(value) = fields.get(key)
    {
        return Some(value);
    }
    semantic_json_lookup(node_data, key)
}

fn semantic_json_lookup<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn semantic_value_preview(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(items) => {
            let preview = items
                .iter()
                .take(2)
                .map(semantic_value_preview)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                format!("{} items", items.len())
            } else if items.len() > 2 {
                format!("{preview} …")
            } else {
                preview
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("label").and_then(serde_json::Value::as_str) {
                return text.to_owned();
            }
            if let Some(text) = map.get("title").and_then(serde_json::Value::as_str) {
                return text.to_owned();
            }
            let preview = map
                .iter()
                .take(2)
                .map(|(key, value)| format!("{key}: {}", semantic_value_preview(value)))
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                "{}".to_owned()
            } else {
                preview
            }
        }
        serde_json::Value::Null => String::new(),
    }
}

fn semantic_nested_region_height(title: Option<&str>, lines: &[String]) -> f32 {
    let mut height = 18.0;
    if title.is_some() {
        height += 14.0;
    }
    if !lines.is_empty() {
        height += (lines.len() as f32 * 13.0).min(39.0);
    }
    height.max(34.0)
}

fn semantic_action_region_height(items: &[String]) -> f32 {
    18.0 + items.len().max(1) as f32 * 20.0
}

fn semantic_chip_width(text: &str, min_width: f32, max_width: f32) -> f32 {
    let estimated = text.chars().count() as f32 * 6.5 + 16.0;
    estimated.clamp(min_width, max_width)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FieldListMetrics {
    left: f32,
    top: f32,
    row_width: f32,
    row_height: f32,
    row_gap: f32,
    bottom_padding: f32,
}

impl FieldListMetrics {
    fn new(size: CanvasSize) -> Self {
        Self {
            left: 14.0,
            top: 46.0,
            row_width: (size.width - 28.0).max(80.0),
            row_height: 22.0,
            row_gap: 4.0,
            bottom_padding: 14.0,
        }
    }

    fn row_rect(self, index: usize) -> CanvasRect {
        CanvasRect {
            origin: jellyflow::core::CanvasPoint {
                x: self.left,
                y: self.top + index as f32 * (self.row_height + self.row_gap),
            },
            size: CanvasSize {
                width: self.row_width,
                height: self.row_height,
            },
        }
    }

    fn bottom_after(self, row_count: usize) -> f32 {
        self.top
            + row_count as f32 * self.row_height
            + row_count.saturating_sub(1) as f32 * self.row_gap
            + self.bottom_padding
    }

    fn min_size(self, current: CanvasSize, row_count: usize) -> CanvasSize {
        CanvasSize {
            width: current.width.max(self.row_width + self.left * 2.0),
            height: current.height.max(self.bottom_after(row_count)),
        }
    }
}

fn draw_field_badge(ui: &mut Ui, rect: Rect, badge: &str, color: Color32) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), color.gamma_multiply(0.12));
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        badge,
        eframe::egui::TextStyle::Small.resolve(ui.style()),
        color,
    );
}

fn draw_semantic_chip(
    ui: &mut Ui,
    rect: Rect,
    text: &str,
    fill: Color32,
    stroke: Color32,
    text_color: Color32,
) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(0.8, stroke),
        eframe::egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        text,
        eframe::egui::TextStyle::Small.resolve(ui.style()),
        text_color,
    );
}

fn draw_semantic_nested_region(
    ui: &mut Ui,
    rect: Rect,
    title: Option<&str>,
    lines: &[String],
    style: NodeRendererStyle,
) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::same(4), style.fill.gamma_multiply(0.9));
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(0.8, style.stroke.gamma_multiply(0.72)),
        eframe::egui::StrokeKind::Inside,
    );

    let mut y = rect.top() + 5.0;
    if let Some(title) = title {
        painter.text(
            Pos2::new(rect.left() + 7.0, y),
            eframe::egui::Align2::LEFT_TOP,
            title,
            eframe::egui::TextStyle::Small.resolve(ui.style()),
            style.text,
        );
        y += 13.0;
    }

    for line in lines.iter().take(3) {
        painter.text(
            Pos2::new(rect.left() + 7.0, y),
            eframe::egui::Align2::LEFT_TOP,
            line,
            eframe::egui::TextStyle::Small.resolve(ui.style()),
            style.text.gamma_multiply(0.82),
        );
        y += 12.0;
    }
}

fn field_value_label(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn field_badge(key: &str) -> Option<&'static str> {
    match key {
        "primary_key" | "pk" => Some("PK"),
        "foreign_key" | "fk" => Some("FK"),
        _ => None,
    }
}

fn node_local_rect_to_screen(node_rect: Rect, local_rect: CanvasRect, zoom: f32) -> Rect {
    Rect::from_min_size(
        node_rect.min + Vec2::new(local_rect.origin.x * zoom, local_rect.origin.y * zoom),
        Vec2::new(local_rect.size.width * zoom, local_rect.size.height * zoom),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::core::{CanvasPoint, NodeKindKey};

    #[derive(Debug, Clone, Copy)]
    struct TestRenderer;

    impl RichNodeRenderer for TestRenderer {
        fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
            let mut layout = NodeRenderLayout::fallback(input, rect);
            layout.title = format!("rich:{}", layout.title);
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: "body".to_owned(),
                slot_kind: Some(NodeSurfaceSlotKind::Body),
                rect: CanvasRect {
                    origin: CanvasPoint::default(),
                    size: rect.size,
                },
                label: None,
                z_index: 1,
            });
            layout
        }
    }

    fn test_descriptor(renderer_key: &str) -> NodeKindViewDescriptor {
        NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.rich"),
            renderer_key: renderer_key.to_owned(),
            title: "Rich".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            surface_slots: Vec::new(),
            chrome: Vec::new(),
            default_data: serde_json::Value::Null,
        }
    }

    fn test_node() -> Node {
        Node {
            kind: NodeKindKey::new("demo.rich"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: serde_json::json!({ "title": "Node" }),
        }
    }

    #[test]
    fn renderer_catalog_falls_back_and_routes_rich_renderers() {
        let descriptor = test_descriptor("demo.rich");
        let node = test_node();
        let input = NodeRenderInput {
            id: NodeId::from_u128(1),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::fallback(),
        };
        let rect = CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 120.0,
                height: 80.0,
            },
        };
        let mut catalog = RendererCatalog::new();

        assert_eq!(catalog.render_node(&input, rect).title, "Node");
        catalog.register_rich("demo.rich", TestRenderer);
        let layout = catalog.render_node(&input, rect);
        assert_eq!(layout.title, "rich:Node");
        assert_eq!(layout.interactive_regions[0].key, "body");
    }

    #[test]
    fn renderer_catalog_tracks_widget_renderers_separately() {
        let mut catalog = RendererCatalog::new();

        assert!(!catalog.has_widget_renderer("table-card"));
        catalog.register_widgets("table-card", FieldListNodeRenderer);

        assert!(catalog.has_widget_renderer("table-card"));
        assert!(!catalog.has_widget_renderer("unknown"));
    }

    #[test]
    fn semantic_slots_sort_and_resolve_renderer_neutral_paths() {
        let node = Node {
            kind: NodeKindKey::new("demo.rich"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: serde_json::json!({
                "meta": { "model": "gpt-4.1-mini" },
                "nested": { "policy": { "guardrails": "Block PII" } },
                "actions": { "primary": ["Test prompt", "Open trace"] }
            }),
        };
        let descriptor = NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.rich"),
            renderer_key: "demo.rich".to_owned(),
            title: "Rich".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            surface_slots: vec![
                NodeSurfaceSlotDescriptor::badge("badge.model")
                    .with_label("Model")
                    .with_anchor("meta.model")
                    .with_order(2),
                NodeSurfaceSlotDescriptor::nested_region("nested.policy")
                    .with_label("Policy")
                    .with_slot("nested.policy")
                    .with_anchor("nested.policy")
                    .with_order(0),
                NodeSurfaceSlotDescriptor::action_row("actions.primary")
                    .with_label("Actions")
                    .with_slot("actions.primary")
                    .with_anchor("actions.primary")
                    .with_order(1),
            ],
            chrome: Vec::new(),
            default_data: serde_json::Value::Null,
        };
        let input = NodeRenderInput {
            id: NodeId::from_u128(9),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::fallback(),
        };

        let slots = semantic_slots(&input, NodeSurfaceSlotKind::NestedRegion);
        assert_eq!(slots[0].key, "nested.policy");
        assert_eq!(
            semantic_slot_lines(&input.node.data, slots[0], None),
            vec!["guardrails: Block PII".to_owned()]
        );
        assert_eq!(
            semantic_slot_chip_text(&input.node.data, &descriptor.surface_slots[2], None),
            "Test prompt · Open trace"
        );
    }

    #[test]
    fn semantic_slot_data_key_resolves_explicit_slot_and_keeps_field_compatibility() {
        let field =
            NodeSurfaceSlotDescriptor::field_row("field.source").with_anchor("field.source");
        assert_eq!(field.data_key(), Some("source"));

        let badge = NodeSurfaceSlotDescriptor::badge("badge.model").with_anchor("meta.model");
        assert_eq!(badge.data_key(), None);

        let nested = NodeSurfaceSlotDescriptor::nested_region("nested.policy")
            .with_slot("nested.policy")
            .with_anchor("nested.policy");
        assert_eq!(nested.data_key(), Some("nested.policy"));
    }

    #[test]
    fn node_content_level_derives_from_zoom() {
        assert_eq!(NodeContentLevel::from_zoom(1.0), NodeContentLevel::Full);
        assert_eq!(NodeContentLevel::from_zoom(0.5), NodeContentLevel::Compact);
        assert_eq!(NodeContentLevel::from_zoom(0.12), NodeContentLevel::Shell);
        assert_eq!(
            NodeContentLevel::from_zoom_and_size(1.0, Vec2::new(220.0, 120.0)),
            NodeContentLevel::Full
        );
        assert_eq!(
            NodeContentLevel::from_zoom_and_size(1.0, Vec2::new(120.0, 70.0)),
            NodeContentLevel::Compact
        );
        assert_eq!(
            NodeContentLevel::from_zoom_and_size(1.0, Vec2::new(40.0, 20.0)),
            NodeContentLevel::Shell
        );
        assert!(NodeContentLevel::Full.shows_text());
        assert!(NodeContentLevel::Compact.shows_text());
        assert!(NodeContentLevel::Full.shows_detail());
        assert!(!NodeContentLevel::Compact.shows_detail());
        assert!(NodeContentLevel::Full.renders_slot_kind(Some(NodeSurfaceSlotKind::Badge)));
        assert!(NodeContentLevel::Compact.renders_slot_kind(Some(NodeSurfaceSlotKind::FieldRow)));
        assert!(NodeContentLevel::Compact.renders_slot_kind(Some(NodeSurfaceSlotKind::PortRail)));
        assert!(!NodeContentLevel::Compact.renders_slot_kind(Some(NodeSurfaceSlotKind::Badge)));
        assert!(
            !NodeContentLevel::Compact.renders_slot_kind(Some(NodeSurfaceSlotKind::StatusBanner))
        );
        assert!(!NodeContentLevel::Shell.renders_slot_kind(Some(NodeSurfaceSlotKind::FieldRow)));
    }

    #[test]
    fn widget_clip_rect_can_clip_node_local_regions() {
        let node_rect =
            Rect::from_min_size(eframe::egui::pos2(100.0, 20.0), Vec2::new(120.0, 80.0));
        let region = CanvasRect {
            origin: CanvasPoint { x: 8.0, y: 16.0 },
            size: CanvasSize {
                width: 100.0,
                height: 20.0,
            },
        };
        let clip = Rect::from_min_max(
            eframe::egui::pos2(130.0, 0.0),
            eframe::egui::pos2(260.0, 100.0),
        );
        let descriptor = test_descriptor("demo.rich");
        let node = test_node();
        let layout = NodeRenderLayout::fallback(
            &NodeRenderInput {
                id: NodeId::from_u128(1),
                node: &node,
                descriptor: &descriptor,
                state: NodeRendererState::default(),
                style: NodeRendererStyle::fallback(),
            },
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 120.0,
                    height: 80.0,
                },
            },
        );
        let input = NodeWidgetRenderInput {
            id: NodeId::from_u128(1),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::fallback(),
            layout: &layout,
            node_rect,
            clip_rect: clip,
            zoom: 1.0,
            content_level: NodeContentLevel::Full,
        };

        let clipped = input
            .node_local_screen_rect(region)
            .expect("region intersects the widget clip rect");

        assert_eq!(clipped.left(), 130.0);
        assert_eq!(clipped.right(), 208.0);
    }

    #[test]
    fn widget_region_rect_is_clipped_to_node_bounds() {
        let node_rect =
            Rect::from_min_size(eframe::egui::pos2(100.0, 20.0), Vec2::new(120.0, 80.0));
        let descriptor = test_descriptor("demo.rich");
        let node = test_node();
        let layout = NodeRenderLayout::fallback(
            &NodeRenderInput {
                id: NodeId::from_u128(1),
                node: &node,
                descriptor: &descriptor,
                state: NodeRendererState::default(),
                style: NodeRendererStyle::fallback(),
            },
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 120.0,
                    height: 80.0,
                },
            },
        );
        let input = NodeWidgetRenderInput {
            id: NodeId::from_u128(1),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::fallback(),
            layout: &layout,
            node_rect,
            clip_rect: Rect::from_min_max(
                eframe::egui::pos2(0.0, 0.0),
                eframe::egui::pos2(300.0, 300.0),
            ),
            zoom: 1.0,
            content_level: NodeContentLevel::Full,
        };

        let clipped = input
            .node_local_screen_rect(CanvasRect {
                origin: CanvasPoint { x: 80.0, y: 62.0 },
                size: CanvasSize {
                    width: 72.0,
                    height: 40.0,
                },
            })
            .expect("oversized local rect should still intersect the node");

        assert_eq!(clipped.right(), node_rect.right());
        assert_eq!(clipped.bottom(), node_rect.bottom());
    }

    #[test]
    fn field_list_metrics_space_rows_and_grow_min_height() {
        let metrics = FieldListMetrics::new(CanvasSize {
            width: 226.0,
            height: 80.0,
        });
        let first = metrics.row_rect(0);
        let second = metrics.row_rect(1);
        let min_size = metrics.min_size(
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
            3,
        );

        assert_eq!(first.size.height, 22.0);
        assert_eq!(second.origin.y - first.origin.y, 26.0);
        assert!(min_size.width >= 226.0);
        assert!(min_size.height >= 132.0);
    }

    #[test]
    fn builtin_table_renderer_emits_field_regions_in_node_local_coordinates() {
        let descriptor = NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.table"),
            renderer_key: "table-card".to_owned(),
            title: "Table".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            surface_slots: Vec::new(),
            chrome: Vec::new(),
            default_data: serde_json::Value::Null,
        };
        let node = Node {
            kind: NodeKindKey::new("demo.table"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: serde_json::json!({
                "title": "orders",
                "field_order": ["primary_key", "foreign_key"],
                "fields": {
                    "primary_key": "id",
                    "foreign_key": "customer_id"
                }
            }),
        };
        let input = NodeRenderInput {
            id: NodeId::from_u128(2),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::section(),
        };
        let layout = RendererCatalog::default().render_node(
            &input,
            CanvasRect {
                origin: CanvasPoint { x: 200.0, y: 100.0 },
                size: CanvasSize {
                    width: 226.0,
                    height: 150.0,
                },
            },
        );

        let primary = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "field.primary_key")
            .expect("primary key region exists");
        let foreign = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "field.foreign_key")
            .expect("foreign key region exists");
        assert_eq!(primary.rect.origin.x, 14.0);
        assert_eq!(primary.rect.origin.y, 46.0);
        assert_eq!(primary.rect.size.height, 22.0);
        assert_eq!(primary.label.as_deref(), Some("id"));
        assert!(foreign.rect.origin.y > primary.rect.origin.y);
        assert_eq!(foreign.label.as_deref(), Some("customer_id"));
    }

    #[test]
    fn dify_style_regions_keep_field_anchors_before_status_and_actions() {
        let descriptor = NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.llm"),
            renderer_key: "decision-card".to_owned(),
            title: "LLM".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            surface_slots: vec![
                NodeSurfaceSlotDescriptor::field_row("field.prompt")
                    .with_label("Prompt")
                    .with_slot("prompt")
                    .with_anchor("field.prompt")
                    .with_order(0),
                NodeSurfaceSlotDescriptor::field_row("field.completion")
                    .with_label("Completion")
                    .with_slot("completion")
                    .with_anchor("field.completion")
                    .with_order(1),
                NodeSurfaceSlotDescriptor::metric_badge("metric.latency")
                    .with_label("Latency")
                    .with_slot("metrics.latency"),
                NodeSurfaceSlotDescriptor::config_group("config.model")
                    .with_label("Config")
                    .with_slot("config.model"),
                NodeSurfaceSlotDescriptor::status_banner("status.validation")
                    .with_label("Status")
                    .with_slot("status.validation"),
                NodeSurfaceSlotDescriptor::action_row("actions.primary")
                    .with_label("Actions")
                    .with_slot("actions.primary"),
            ],
            chrome: Vec::new(),
            default_data: serde_json::Value::Null,
        };
        let node = Node {
            kind: NodeKindKey::new("demo.llm"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: serde_json::json!({
                "fields": {
                    "prompt": "Customer intake + policy",
                    "completion": "Priority and route"
                },
                "metrics": { "latency": "420ms" },
                "config": { "model": { "temperature": 0.2 } },
                "status": { "validation": "Ready" },
                "actions": { "primary": ["Test prompt", "Open trace"] }
            }),
        };
        let input = NodeRenderInput {
            id: NodeId::from_u128(3),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::decision(),
        };
        let layout = RendererCatalog::default().render_node(
            &input,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 268.0,
                    height: 228.0,
                },
            },
        );

        let prompt = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "field.prompt")
            .expect("prompt field anchor exists");
        let completion = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "field.completion")
            .expect("completion field anchor exists");
        let status = layout
            .interactive_regions
            .iter()
            .find(|region| region.key == "status.validation")
            .expect("status region exists");
        let last_action = layout
            .interactive_regions
            .iter()
            .filter(|region| region.slot_kind == Some(NodeSurfaceSlotKind::ActionRow))
            .max_by(|a, b| a.rect.origin.y.total_cmp(&b.rect.origin.y))
            .expect("action chip exists");

        assert_eq!(prompt.rect.origin.y, 46.0);
        assert_eq!(completion.rect.origin.y - prompt.rect.origin.y, 26.0);
        assert!(
            status.rect.origin.y > completion.rect.origin.y,
            "decorative/status regions should not push port-bearing field anchors down"
        );
        assert!(
            last_action.rect.origin.y + last_action.rect.size.height <= layout.min_size.height,
            "layout minimum height should cover adapter-owned Dify controls"
        );
    }

    #[test]
    fn shader_card_renderer_measures_port_rail_config_and_preview_without_fields() {
        let descriptor = NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.shader.mix"),
            renderer_key: "shader-card".to_owned(),
            title: "Mix".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            surface_slots: vec![
                NodeSurfaceSlotDescriptor::port_rail("rail.inputs")
                    .with_label("Inputs")
                    .with_slot("ports.inputs")
                    .with_anchor("rail.inputs"),
                NodeSurfaceSlotDescriptor::config_group("config.factor")
                    .with_label("Factor")
                    .with_slot("config.factor")
                    .with_anchor("config.factor"),
                NodeSurfaceSlotDescriptor::preview("preview.result")
                    .with_label("Preview")
                    .with_slot("preview.result")
                    .with_anchor("preview.result"),
                NodeSurfaceSlotDescriptor::port_rail("rail.outputs")
                    .with_label("Outputs")
                    .with_slot("ports.outputs")
                    .with_anchor("rail.outputs"),
            ],
            chrome: Vec::new(),
            default_data: serde_json::Value::Null,
        };
        let node = Node {
            kind: NodeKindKey::new("demo.shader.mix"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: serde_json::json!({
                "ports": {
                    "inputs": ["vec4 albedo", "float factor"],
                    "outputs": ["vec4 result"]
                },
                "config": {
                    "factor": { "type": "float", "default": 0.5 }
                },
                "preview": {
                    "result": "gradient"
                }
            }),
        };
        let input = NodeRenderInput {
            id: NodeId::from_u128(4),
            node: &node,
            descriptor: &descriptor,
            state: NodeRendererState::default(),
            style: NodeRendererStyle::data(),
        };
        let layout = RendererCatalog::default().render_node(
            &input,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 236.0,
                    height: 190.0,
                },
            },
        );

        let keys = layout
            .interactive_regions
            .iter()
            .map(|region| region.key.as_str())
            .collect::<Vec<_>>();
        assert!(keys.contains(&"rail.inputs"));
        assert!(keys.contains(&"config.factor"));
        assert!(keys.contains(&"preview.result"));
        assert!(keys.contains(&"rail.outputs"));
        assert!(
            layout
                .interactive_regions
                .iter()
                .any(|region| region.slot_kind == Some(NodeSurfaceSlotKind::PortRail))
        );
        assert!(
            layout
                .interactive_regions
                .iter()
                .any(|region| region.slot_kind == Some(NodeSurfaceSlotKind::Preview))
        );
    }
}
