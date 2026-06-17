use std::collections::BTreeMap;

use eframe::egui::{Color32, Stroke};
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
            Color32::from_rgb(245, 248, 252),
            Color32::from_rgb(124, 139, 160),
            Color32::from_rgb(42, 106, 166),
            Color32::from_rgb(31, 41, 55),
        )
    }

    pub const fn decision() -> Self {
        Self::new(
            Color32::from_rgb(255, 248, 235),
            Color32::from_rgb(191, 129, 45),
            Color32::from_rgb(200, 83, 44),
            Color32::from_rgb(61, 46, 28),
        )
    }

    pub const fn data() -> Self {
        Self::new(
            Color32::from_rgb(238, 250, 246),
            Color32::from_rgb(79, 146, 121),
            Color32::from_rgb(18, 128, 96),
            Color32::from_rgb(27, 53, 48),
        )
    }

    pub const fn output() -> Self {
        Self::new(
            Color32::from_rgb(249, 244, 255),
            Color32::from_rgb(135, 107, 177),
            Color32::from_rgb(108, 81, 158),
            Color32::from_rgb(49, 38, 70),
        )
    }

    pub const fn topic() -> Self {
        Self::new(
            Color32::from_rgb(244, 249, 255),
            Color32::from_rgb(82, 127, 172),
            Color32::from_rgb(31, 105, 168),
            Color32::from_rgb(26, 45, 68),
        )
    }

    pub const fn idea() -> Self {
        Self::new(
            Color32::from_rgb(248, 250, 240),
            Color32::from_rgb(134, 152, 86),
            Color32::from_rgb(88, 128, 54),
            Color32::from_rgb(43, 55, 34),
        )
    }

    pub const fn section() -> Self {
        Self::new(
            Color32::from_rgb(246, 246, 252),
            Color32::from_rgb(118, 118, 158),
            Color32::from_rgb(72, 88, 150),
            Color32::from_rgb(42, 44, 68),
        )
    }

    pub const fn source() -> Self {
        Self::new(
            Color32::from_rgb(252, 248, 241),
            Color32::from_rgb(160, 128, 86),
            Color32::from_rgb(150, 94, 46),
            Color32::from_rgb(64, 48, 34),
        )
    }

    pub const fn fallback() -> Self {
        Self::new(
            Color32::from_rgb(247, 247, 246),
            Color32::from_rgb(142, 142, 135),
            Color32::from_rgb(82, 82, 74),
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
#[derive(Debug, Clone, PartialEq)]
pub struct NodeInteractiveRegion {
    pub key: String,
    pub rect: CanvasRect,
    pub z_index: i32,
}

/// Adapter-owned renderer contract. Implementors may measure rich node bodies without mutating graph state.
pub trait RichNodeRenderer: Send + Sync {
    fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout;
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
}

impl std::fmt::Debug for RendererCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererCatalog")
            .field("fallback", &self.fallback)
            .field("style_count", &self.by_renderer_key.len())
            .field("rich_renderer_count", &self.rich_renderers.len())
            .finish()
    }
}

impl Default for RendererCatalog {
    fn default() -> Self {
        Self::with_builtin_styles()
    }
}

impl RendererCatalog {
    pub fn new() -> Self {
        Self {
            fallback: NodeRendererStyle::fallback(),
            by_renderer_key: BTreeMap::new(),
            rich_renderers: BTreeMap::new(),
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
            .register("source-card", NodeRendererStyle::source());
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

    pub fn render_node(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
        self.rich_renderers
            .get(&input.descriptor.renderer_key)
            .map(|renderer| renderer.as_ref())
            .unwrap_or(&FallbackRichNodeRenderer)
            .render(input, rect)
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
                rect,
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
}
