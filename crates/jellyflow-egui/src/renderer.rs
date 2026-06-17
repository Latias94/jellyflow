use std::collections::BTreeMap;

use eframe::egui::{
    Align, Color32, CornerRadius, Frame, Id, Layout, Rect, Stroke, Ui, UiBuilder, Vec2,
};
use jellyflow::core::{CanvasRect, CanvasSize, Node, NodeId};
use jellyflow::runtime::schema::NodeKindViewDescriptor;

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
        if zoom >= 0.72 {
            Self::Full
        } else if zoom >= 0.38 {
            Self::Compact
        } else {
            Self::Shell
        }
    }

    pub fn shows_text(self) -> bool {
        matches!(self, Self::Full)
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
    pub zoom: f32,
    pub content_level: NodeContentLevel,
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
            .register_rich("table-card", FieldListNodeRenderer)
            .register_widgets("table-card", FieldListNodeRenderer);
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
        let Some(fields) = input
            .node
            .data
            .get("fields")
            .and_then(|value| value.as_object())
        else {
            return layout;
        };
        let field_order = input
            .node
            .data
            .get("field_order")
            .and_then(|value| value.as_array())
            .map(|order| {
                order
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut keys = field_order;
        for key in fields.keys() {
            if !keys.iter().any(|existing| existing == key) {
                keys.push(key.clone());
            }
        }
        let base_top = 46.0;
        let row_height = 20.0;
        let row_width = (rect.size.width - 28.0).max(80.0);
        let field_count = keys.len();
        for (index, key) in keys.into_iter().enumerate() {
            let row_top = base_top + index as f32 * row_height;
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: format!("field.{key}"),
                rect: CanvasRect {
                    origin: jellyflow::core::CanvasPoint {
                        x: 14.0,
                        y: row_top,
                    },
                    size: CanvasSize {
                        width: row_width,
                        height: row_height - 2.0,
                    },
                },
                label: fields.get(&key).map(field_value_label),
                z_index: 1,
            });
        }
        let desired_height = base_top + field_count as f32 * row_height + 14.0;
        layout.min_size.height = layout.min_size.height.max(desired_height);
        layout
    }
}

impl EguiNodeWidgetRenderer for FieldListNodeRenderer {
    fn render_widgets(&self, ui: &mut Ui, input: &NodeWidgetRenderInput<'_>) -> bool {
        if input.content_level == NodeContentLevel::Shell {
            return false;
        }

        let Some(fields) = input
            .node
            .data
            .get("fields")
            .and_then(|value| value.as_object())
        else {
            return false;
        };

        let show_badges = matches!(input.content_level, NodeContentLevel::Full);
        let row_padding = if show_badges { 8.0 } else { 6.0 };
        let row_height = if show_badges { 20.0 } else { 18.0 };

        for region in input
            .layout
            .interactive_regions
            .iter()
            .filter(|region| region.key.starts_with("field."))
        {
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
            let rect = node_local_rect_to_screen(input.node_rect, region.rect, input.zoom);
            let mut child_ui = ui.new_child(
                UiBuilder::new()
                    .id_salt(Id::new(("field-region", input.id, &region.key)))
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            child_ui.set_clip_rect(rect);
            child_ui.set_min_size(rect.size());
            child_ui.spacing_mut().item_spacing =
                Vec2::new(if show_badges { 6.0 } else { 4.0 }, 0.0);
            Frame::new()
                .fill(Color32::from_rgb(255, 255, 255).gamma_multiply(0.9))
                .stroke(Stroke::new(
                    0.75,
                    input
                        .style
                        .stroke
                        .gamma_multiply(if show_badges { 0.55 } else { 0.4 }),
                ))
                .corner_radius(CornerRadius::same(if show_badges { 4 } else { 3 }))
                .inner_margin(eframe::egui::Margin::symmetric(row_padding as i8, 2))
                .show(&mut child_ui, |ui| {
                    ui.set_min_size(Vec2::new(rect.width(), row_height));
                    ui.horizontal_centered(|ui| {
                        if show_badges && let Some(badge) = field_badge(key) {
                            let badge_color = input.style.accent;
                            Frame::new()
                                .fill(badge_color.gamma_multiply(0.12))
                                .corner_radius(CornerRadius::same(3))
                                .inner_margin(eframe::egui::Margin::symmetric(4, 0))
                                .show(ui, |ui| {
                                    ui.label(
                                        eframe::egui::RichText::new(badge)
                                            .small()
                                            .strong()
                                            .color(badge_color),
                                    );
                                });
                        }
                        ui.label(
                            eframe::egui::RichText::new(label)
                                .small()
                                .color(input.style.text.gamma_multiply(0.82)),
                        );
                    });
                });
        }

        true
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

    #[test]
    fn renderer_catalog_falls_back_and_routes_rich_renderers() {
        let descriptor = NodeKindViewDescriptor {
            kind: NodeKindKey::new("demo.rich"),
            renderer_key: "demo.rich".to_owned(),
            title: "Rich".to_owned(),
            category: Vec::new(),
            keywords: Vec::new(),
            default_size: None,
            ports: Vec::new(),
            default_data: serde_json::Value::Null,
        };
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
            data: serde_json::json!({ "title": "Node" }),
        };
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
    fn node_content_level_derives_from_zoom() {
        assert_eq!(NodeContentLevel::from_zoom(1.0), NodeContentLevel::Full);
        assert_eq!(NodeContentLevel::from_zoom(0.5), NodeContentLevel::Compact);
        assert_eq!(NodeContentLevel::from_zoom(0.2), NodeContentLevel::Shell);
        assert!(NodeContentLevel::Full.shows_text());
        assert!(!NodeContentLevel::Compact.shows_text());
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
        assert_eq!(primary.label.as_deref(), Some("id"));
        assert!(foreign.rect.origin.y > primary.rect.origin.y);
        assert_eq!(foreign.label.as_deref(), Some("customer_id"));
    }
}
