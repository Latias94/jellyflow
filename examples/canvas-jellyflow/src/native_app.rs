use std::fmt;
use std::rc::Rc;

use jellyflow_core::{EdgeKind, Graph};
use jellyflow_open_gpui::host::InitialViewportPolicy;
use jellyflow_open_gpui::navigation::{OpenGpuiInputPreset, OpenGpuiNavigationConfig};
use jellyflow_runtime::graph::{GraphRuntime, GraphRuntimePolicy};
use jellyflow_runtime::view::{ViewSessionOptions, WeakViewSession};
use open_gpui::{
    App, AppContext, Bounds, Context, Entity, ParentElement, Render, Styled, Window, WindowBounds,
    WindowOptions, div, px, size,
};
use open_gpui_canvas_jellyflow::NativeProductTheme;
use open_gpui_canvas_jellyflow::blackboard::NativeBlackboard;
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;
use open_gpui_canvas_jellyflow::nodes::{
    NativeGraphVisualProfile, NativeNodeResources, ShaderFieldStore, WorkflowFieldStore,
    native_node_catalog, native_node_registry, workflow_demo_graph,
};
use open_gpui_canvas_jellyflow::product_domain::blackboard::GraphRuntimeBlackboardSource;
use open_gpui_ui_components::ThemeMode;
use open_gpui_ui_components::theme::set_active_theme_mode;

use crate::demo_fixture::shader_demo_graph;

struct NativeGraphScene {
    _runtime: Rc<GraphRuntime>,
    graph: Entity<NativeGraphSurface>,
    blackboard: Entity<NativeBlackboard>,
    product_theme: NativeProductTheme,
}

impl Render for NativeGraphScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div()
            .size_full()
            .flex()
            .bg(self.product_theme.surface.app)
            .child(div().flex_1().min_w(px(0.0)).child(self.graph.clone()))
            .child(self.blackboard.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeDemo {
    Shader,
    Workflow,
}

impl NativeDemo {
    pub(crate) fn parse<I, S>(arguments: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut demo = Self::Shader;
        for argument in arguments {
            match argument.as_ref() {
                "--workflow" => demo = Self::Workflow,
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }
        Ok(demo)
    }
}

#[derive(Debug)]
pub(crate) struct NativeLaunchError {
    pub(crate) reason: &'static str,
    detail: String,
}

impl NativeLaunchError {
    fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for NativeLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason, self.detail)
    }
}

pub(crate) fn launch_native_graph<ConfigureDocument, OnSurface>(
    cx: &mut App,
    demo: NativeDemo,
    window_size: (f32, f32),
    configure_document: ConfigureDocument,
    on_surface: OnSurface,
) -> Result<(), NativeLaunchError>
where
    ConfigureDocument: FnOnce(&Graph),
    OnSurface: FnOnce(
            &mut Window,
            &mut App,
            Entity<NativeGraphSurface>,
            WeakViewSession,
            NativeProductTheme,
        ) + 'static,
{
    set_active_theme_mode(cx, ThemeMode::Dark)
        .map_err(|error| NativeLaunchError::new("theme_initialization", format!("{error:?}")))?;
    let catalog = native_node_catalog()
        .map_err(|error| NativeLaunchError::new("catalog_initialization", format!("{error:?}")))?;
    let document = match demo {
        NativeDemo::Shader => shader_demo_graph(&catalog),
        NativeDemo::Workflow => workflow_demo_graph().map_err(|error| format!("{error:?}")),
    }
    .map_err(|detail| NativeLaunchError::new("fixture_initialization", detail))?;
    configure_document(&document);
    let runtime = Rc::new(
        match demo {
            NativeDemo::Shader => GraphRuntime::builder(catalog.clone())
                .with_policy(GraphRuntimePolicy::default().with_cycles(EdgeKind::Data, false))
                .build(document),
            NativeDemo::Workflow => GraphRuntime::builder(catalog.clone()).build(document),
        }
        .map_err(|error| NativeLaunchError::new("runtime_initialization", format!("{error:?}")))?,
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .map_err(|error| NativeLaunchError::new("view_attachment", error.to_string()))?;
    let weak_view = view.downgrade();
    let visual_profile = match demo {
        NativeDemo::Shader => NativeGraphVisualProfile::shader(),
        NativeDemo::Workflow => NativeGraphVisualProfile::workflow(),
    };
    let product_theme = visual_profile.product_theme();
    let navigation = OpenGpuiNavigationConfig::for_preset(match demo {
        NativeDemo::Shader => OpenGpuiInputPreset::Blueprint,
        NativeDemo::Workflow => OpenGpuiInputPreset::Dify,
    });
    let resources = NativeNodeResources::ready_default(cx)
        .with_field_stores(WorkflowFieldStore::new(), ShaderFieldStore::new())
        .with_product_theme(product_theme);
    let registry = native_node_registry(resources, &catalog);
    let bounds = Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx);

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        move |window, cx| {
            let graph = cx.new(|cx| {
                NativeGraphSurface::new_with_visual_profile(
                    view,
                    registry,
                    visual_profile,
                    navigation,
                    InitialViewportPolicy::FitOnFirstMeasuredGeometry,
                    cx,
                )
            });
            on_surface(window, cx, graph.clone(), weak_view, product_theme);
            let blackboard_runtime = Rc::clone(&runtime);
            let blackboard = cx.new(|_| {
                NativeBlackboard::new_with_theme(
                    Rc::new(GraphRuntimeBlackboardSource::new(blackboard_runtime)),
                    product_theme,
                )
            });
            cx.new(|_| NativeGraphScene {
                _runtime: runtime,
                graph,
                blackboard,
                product_theme,
            })
        },
    )
    .map_err(|error| NativeLaunchError::new("window_open", format!("{error:?}")))?;
    cx.activate(true);
    Ok(())
}
