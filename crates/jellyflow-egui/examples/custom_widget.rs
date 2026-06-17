use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Id, Layout, RichText, UiBuilder};
use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId};
use jellyflow::layout::builtin_layout_engine_registry;
use jellyflow::runtime::NodeGraphStore;
use jellyflow::runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow::runtime::schema::{NodeRegistry, NodeSchema, PortDecl};
use jellyflow_egui::{
    EguiNodeWidgetRenderer, JellyflowEguiApp, JellyflowEguiBridge, NodeContentLevel,
    NodeInteractiveRegion, NodeRenderInput, NodeRenderLayout, NodeWidgetRenderInput,
    RendererCatalog, RichNodeRenderer,
};
use serde_json::json;

#[derive(Debug, Clone, Copy)]
struct ReviewCardRenderer;

impl RichNodeRenderer for ReviewCardRenderer {
    fn render(&self, input: &NodeRenderInput<'_>, rect: CanvasRect) -> NodeRenderLayout {
        let mut layout = NodeRenderLayout::fallback(input, rect);
        for (index, key) in ["assignee", "status", "risk"].into_iter().enumerate() {
            layout.interactive_regions.push(NodeInteractiveRegion {
                key: format!("field.{key}"),
                rect: CanvasRect {
                    origin: CanvasPoint {
                        x: 16.0,
                        y: 48.0 + index as f32 * 24.0,
                    },
                    size: CanvasSize {
                        width: (rect.size.width - 32.0).max(80.0),
                        height: 20.0,
                    },
                },
                label: Some(key.to_owned()),
                z_index: 1,
            });
        }
        layout.min_size.height = layout.min_size.height.max(136.0);
        layout
    }
}

impl EguiNodeWidgetRenderer for ReviewCardRenderer {
    fn render_widgets(&self, ui: &mut egui::Ui, input: &NodeWidgetRenderInput<'_>) -> bool {
        if input.content_level == NodeContentLevel::Shell {
            return false;
        }

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
            let value = input
                .node
                .data
                .get(key)
                .and_then(|value| value.as_str())
                .unwrap_or("unset");
            let Some(rect) = input.region_screen_rect(region) else {
                continue;
            };
            let mut child = ui.new_child(
                UiBuilder::new()
                    .id_salt(Id::new(("review-card", input.id, &region.key)))
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            child.set_clip_rect(rect);
            Frame::new()
                .fill(Color32::from_rgb(255, 255, 255).gamma_multiply(0.92))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 1))
                .show(&mut child, |ui| {
                    ui.horizontal_centered(|ui| {
                        if input.content_level.shows_text() {
                            ui.label(
                                RichText::new(key)
                                    .small()
                                    .color(input.style.text.gamma_multiply(0.58)),
                            );
                        }
                        ui.label(
                            RichText::new(value)
                                .small()
                                .strong()
                                .color(input.style.text),
                        );
                    });
                });
        }

        true
    }
}

fn main() -> eframe::Result {
    let app = custom_widget_app().map_err(|err| {
        eframe::Error::AppCreation(Box::new(std::io::Error::other(err.to_string())))
    })?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jellyflow custom widget renderer sample")
            .with_inner_size([1280.0, 780.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Jellyflow custom widget renderer sample",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
}

fn custom_widget_app() -> Result<JellyflowEguiApp, Box<dyn std::error::Error>> {
    let schema = NodeSchema::builder("demo.review_card", "Review card")
        .category(["Workflow"])
        .renderer_key("review-card")
        .default_size(CanvasSize {
            width: 246.0,
            height: 136.0,
        })
        .port(
            PortDecl::data_input("source")
                .with_label("source")
                .on_left()
                .with_view_anchor("field.assignee"),
        )
        .port(
            PortDecl::data_output("approved")
                .with_label("approved")
                .on_right()
                .with_view_anchor("field.status"),
        )
        .default_data(json!({
            "title": "Review request",
            "summary": "Widget-rendered approval card",
            "assignee": "Maya",
            "status": "Waiting",
            "risk": "Medium"
        }))
        .build();

    let mut registry = NodeRegistry::new();
    registry.register(schema.clone());

    let instantiation = schema.instantiate(CanvasPoint {
        x: -120.0,
        y: -60.0,
    });
    let (node_id, node, ports) = instantiation.into_parts();
    let mut builder = GraphBuilder::new(GraphId::new()).with_node(node_id, node);
    for (port, record) in ports {
        builder.insert_port(port, record);
    }
    let graph = builder
        .build()
        .map_err(|errors| std::io::Error::other(format!("{errors:?}")))?;
    let store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let mut renderers = RendererCatalog::default();
    renderers
        .register("review-card", jellyflow_egui::NodeRendererStyle::task())
        .register_rich("review-card", ReviewCardRenderer)
        .register_widgets("review-card", ReviewCardRenderer);
    let bridge =
        JellyflowEguiBridge::new(store, registry, builtin_layout_engine_registry(), renderers);

    Ok(JellyflowEguiApp::new(bridge))
}
