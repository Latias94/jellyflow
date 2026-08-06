use std::process::ExitCode;
use std::rc::Rc;

use open_gpui::{App, QuitMode};
use open_gpui_platform::application;
use open_gpui_ui_components::gpui_adapter::init_text_input;

mod demo_fixture;
mod native_app;
mod real_window_smoke;

use native_app::{NativeDemo, launch_native_graph};
use real_window_smoke::{
    SmokeConfig, configure_smoke_document, fail_smoke, finish_smoke_after_run,
    install_smoke_quit_guard, new_smoke_state, start_smoke_probe, start_smoke_timeout,
};

const USAGE: &str = "usage: open-gpui-real-window-smoke [--workflow]";

fn main() -> ExitCode {
    let demo = match NativeDemo::parse(std::env::args().skip(1)) {
        Ok(demo) => demo,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let config = match SmokeConfig::from_env().and_then(|config| {
        config.prepare_capture_dir()?;
        Ok(config)
    }) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid smoke configuration: {error}");
            return ExitCode::from(2);
        }
    };
    let state = new_smoke_state(demo, config);
    let window_size = state.borrow().window_size();
    let launch_state = Rc::clone(&state);

    application().run(move |cx: &mut App| {
        cx.init_colors();
        init_text_input(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        install_smoke_quit_guard(cx, Rc::clone(&launch_state));

        let document_state = Rc::clone(&launch_state);
        let surface_state = Rc::clone(&launch_state);
        if let Err(error) = launch_native_graph(
            cx,
            demo,
            window_size,
            move |document| configure_smoke_document(&document_state, document),
            move |window, cx, graph, view, product_theme| {
                start_smoke_probe(
                    cx,
                    window.window_handle(),
                    graph,
                    view,
                    product_theme,
                    Rc::clone(&surface_state),
                );
                start_smoke_timeout(cx, surface_state);
                window.refresh();
            },
        ) {
            eprintln!("failed to launch retained native graph: {error}");
            fail_smoke(cx, &launch_state, error.reason);
        }
    });

    finish_smoke_after_run(state)
}
