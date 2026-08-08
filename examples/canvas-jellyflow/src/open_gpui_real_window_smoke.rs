use open_gpui::{App, QuitMode};
use open_gpui_platform::application;
use open_gpui_ui_components::gpui_adapter::init_text_input;
use std::process::ExitCode;

mod demo_fixture;
mod native_app;
mod real_window_smoke;

use native_app::{NativeDemo, launch_native_graph};
use real_window_smoke::{
    SMOKE_WINDOW_SIZE, fail_smoke, finish_smoke_after_run, new_smoke_state, start_smoke_probe,
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
    let state = new_smoke_state(demo);
    let launch_state = state.clone();

    application().run(move |cx: &mut App| {
        cx.init_colors();
        init_text_input(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);

        let surface_state = launch_state.clone();
        if let Err(error) = launch_native_graph(
            cx,
            demo,
            SMOKE_WINDOW_SIZE,
            |_| {},
            move |window, cx, graph, view, _| {
                start_smoke_probe(cx, window, graph, view, surface_state);
                window.refresh();
            },
        ) {
            eprintln!("failed to launch retained native graph: {error}");
            fail_smoke(cx, &launch_state, error.to_string());
        }
    });

    finish_smoke_after_run(state)
}
