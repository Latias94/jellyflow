use std::cell::Cell;
use std::fmt;
use std::process::ExitCode;
use std::rc::Rc;

use jellyflow::runtime::{GraphRuntime, ViewSessionOptions};
use open_gpui::{
    App, AppContext, Bounds, Context, Entity, ParentElement, QuitMode, Render, Styled, Window,
    WindowBounds, WindowOptions, div, px, rgb, size,
};
use open_gpui_canvas_jellyflow::ShaderGraphProfile;
use open_gpui_canvas_jellyflow::blackboard::{GraphRuntimeBlackboardSource, NativeBlackboard};
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;
use open_gpui_canvas_jellyflow::nodes::{
    NativeNodeResources, ShaderFieldStore, WorkflowFieldStore, native_node_catalog,
    native_node_registry, workflow_demo_graph,
};
use open_gpui_platform::application;
use open_gpui_ui_components::ThemeMode;
use open_gpui_ui_components::gpui_adapter::init_text_input;
use open_gpui_ui_components::theme::set_active_theme_mode;

mod demo_fixture;
mod real_window_smoke;

use demo_fixture::shader_demo_graph;
use real_window_smoke::{
    SharedSmokeState, SmokeConfig, configure_smoke_document, fail_smoke, finish_smoke_after_run,
    install_smoke_quit_guard, new_smoke_state, start_smoke_probe, start_smoke_timeout,
};

const WINDOW_WIDTH: f32 = 1_180.0;
const WINDOW_HEIGHT: f32 = 720.0;
const USAGE: &str = "usage: open-gpui-canvas-jellyflow [--workflow] [--smoke]";

struct NativeGraphScene {
    _runtime: Rc<GraphRuntime>,
    graph: Entity<NativeGraphSurface>,
    blackboard: Entity<NativeBlackboard>,
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
            .bg(rgb(0x111318))
            .child(div().flex_1().min_w(px(0.0)).child(self.graph.clone()))
            .child(self.blackboard.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeDemo {
    Shader,
    Workflow,
}

impl NativeDemo {
    const fn label(self) -> &'static str {
        match self {
            Self::Shader => "shader",
            Self::Workflow => "workflow",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LaunchOptions {
    demo: NativeDemo,
    smoke: bool,
}

impl LaunchOptions {
    fn parse<I, S>(arguments: I) -> Result<Self, LaunchOptionsError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut options = Self {
            demo: NativeDemo::Shader,
            smoke: false,
        };
        for argument in arguments {
            match argument.as_ref() {
                "--workflow" => options.demo = NativeDemo::Workflow,
                "--smoke" => options.smoke = true,
                unknown => return Err(LaunchOptionsError::UnknownArgument(unknown.to_owned())),
            }
        }
        Ok(options)
    }

    fn require_smoke_support(
        self,
        real_window_smoke_enabled: bool,
    ) -> Result<Self, LaunchOptionsError> {
        if self.smoke && !real_window_smoke_enabled {
            return Err(LaunchOptionsError::RealWindowSmokeFeatureDisabled);
        }
        Ok(self)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LaunchOptionsError {
    UnknownArgument(String),
    RealWindowSmokeFeatureDisabled,
}

impl fmt::Display for LaunchOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownArgument(argument) => write!(formatter, "unknown argument: {argument}"),
            Self::RealWindowSmokeFeatureDisabled => write!(
                formatter,
                "--smoke requires the non-default `real-window-smoke` feature; rerun with \
                 `--features real-window-smoke`"
            ),
        }
    }
}

#[derive(Debug)]
struct NativeLaunchError {
    reason: &'static str,
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

fn main() -> ExitCode {
    let options = match LaunchOptions::parse(std::env::args().skip(1))
        .and_then(|options| options.require_smoke_support(cfg!(feature = "real-window-smoke")))
    {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let smoke_config = if options.smoke {
        match SmokeConfig::from_env().and_then(|config| {
            config.prepare_capture_dir()?;
            Ok(config)
        }) {
            Ok(config) => Some(config),
            Err(error) => {
                eprintln!("invalid smoke configuration: {error}");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let smoke_state = smoke_config
        .clone()
        .map(|config| new_smoke_state(options.demo.label(), config));
    let launch_smoke_state = smoke_state.clone();
    let launch_failed = Rc::new(Cell::new(false));
    let app_launch_failed = Rc::clone(&launch_failed);

    application().run(move |cx: &mut App| {
        cx.init_colors();
        init_text_input(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        if let Some(state) = launch_smoke_state.as_ref() {
            install_smoke_quit_guard(cx, Rc::clone(state));
        }

        if let Err(error) = launch_native_graph(cx, options, launch_smoke_state.clone()) {
            eprintln!("failed to launch retained native graph: {error}");
            app_launch_failed.set(true);
            if let Some(state) = launch_smoke_state.as_ref() {
                fail_smoke(state, error.reason);
            }
            cx.quit();
        }
    });

    if let Some(state) = smoke_state {
        finish_smoke_after_run(state)
    } else if launch_failed.get() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn launch_native_graph(
    cx: &mut App,
    options: LaunchOptions,
    smoke_state: Option<SharedSmokeState>,
) -> Result<(), NativeLaunchError> {
    set_active_theme_mode(cx, ThemeMode::Dark)
        .map_err(|error| NativeLaunchError::new("theme_initialization", format!("{error:?}")))?;
    let catalog = native_node_catalog()
        .map_err(|error| NativeLaunchError::new("catalog_initialization", format!("{error:?}")))?;
    let document = match options.demo {
        NativeDemo::Shader => shader_demo_graph(&catalog),
        NativeDemo::Workflow => workflow_demo_graph().map_err(|error| format!("{error:?}")),
    }
    .map_err(|detail| NativeLaunchError::new("fixture_initialization", detail))?;
    if let Some(state) = smoke_state.as_ref() {
        configure_smoke_document(state, &document);
    }
    let runtime = Rc::new(
        match options.demo {
            NativeDemo::Shader => GraphRuntime::builder(catalog)
                .with_profile(Box::new(ShaderGraphProfile))
                .build(document),
            NativeDemo::Workflow => GraphRuntime::builder(catalog).build(document),
        }
        .map_err(|error| NativeLaunchError::new("runtime_initialization", format!("{error:?}")))?,
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .map_err(|error| NativeLaunchError::new("view_attachment", error.to_string()))?;
    let smoke_view = options.smoke.then(|| view.downgrade());
    let workflow_fields = WorkflowFieldStore::new();
    let shader_fields = ShaderFieldStore::new();
    let resources =
        NativeNodeResources::ready_default(cx).with_field_stores(workflow_fields, shader_fields);
    let registry = native_node_registry(resources);
    let (window_width, window_height) = smoke_state
        .as_ref()
        .map_or((WINDOW_WIDTH, WINDOW_HEIGHT), |state| {
            state.borrow().window_size()
        });
    let bounds = Bounds::centered(None, size(px(window_width), px(window_height)), cx);

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        move |window, cx| {
            let graph = cx.new(|cx| NativeGraphSurface::new(view, registry, cx));
            if let Some(state) = smoke_state {
                if let Some(smoke_view) = smoke_view {
                    start_smoke_probe(
                        cx,
                        window.window_handle(),
                        graph.clone(),
                        smoke_view,
                        Rc::clone(&state),
                    );
                } else {
                    fail_smoke(&state, "smoke_view_unavailable");
                    cx.quit();
                }
                start_smoke_timeout(cx, state);
                window.refresh();
            }
            let blackboard_runtime = Rc::clone(&runtime);
            let blackboard = cx.new(|_| {
                NativeBlackboard::new(Rc::new(GraphRuntimeBlackboardSource::new(
                    blackboard_runtime,
                )))
            });
            cx.new(|_| NativeGraphScene {
                _runtime: runtime,
                graph,
                blackboard,
            })
        },
    )
    .map_err(|error| NativeLaunchError::new("window_open", format!("{error:?}")))?;
    cx.activate(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_options_keep_smoke_orthogonal_to_the_demo() {
        assert_eq!(
            LaunchOptions::parse(std::iter::empty::<&str>()),
            Ok(LaunchOptions {
                demo: NativeDemo::Shader,
                smoke: false,
            })
        );
        assert_eq!(
            LaunchOptions::parse(["--smoke", "--workflow"]),
            Ok(LaunchOptions {
                demo: NativeDemo::Workflow,
                smoke: true,
            })
        );
        assert_eq!(
            LaunchOptions::parse(["--unknown"]),
            Err(LaunchOptionsError::UnknownArgument("--unknown".to_owned()))
        );
    }

    #[test]
    fn smoke_requires_the_non_default_real_window_feature() {
        let options = LaunchOptions {
            demo: NativeDemo::Shader,
            smoke: true,
        };

        assert_eq!(
            options.require_smoke_support(false),
            Err(LaunchOptionsError::RealWindowSmokeFeatureDisabled)
        );
        assert_eq!(options.require_smoke_support(true), Ok(options));
    }
}
