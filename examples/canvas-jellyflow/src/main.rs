use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;

use open_gpui::{App, QuitMode};
use open_gpui_platform::application;
use open_gpui_ui_components::gpui_adapter::init_text_input;

mod demo_fixture;
mod native_app;

use native_app::{NativeDemo, launch_native_graph};

const USAGE: &str = "usage: open-gpui-canvas-jellyflow [--workflow]";
const WINDOW_SIZE: (f32, f32) = (1_180.0, 720.0);

fn main() -> ExitCode {
    let demo = match NativeDemo::parse(std::env::args().skip(1)) {
        Ok(demo) => demo,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let launch_failed = Rc::new(Cell::new(false));
    let app_launch_failed = Rc::clone(&launch_failed);

    application().run(move |cx: &mut App| {
        cx.init_colors();
        init_text_input(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);

        if let Err(error) = launch_native_graph(cx, demo, WINDOW_SIZE, |_| {}, |_, _, _, _, _| {}) {
            eprintln!("failed to launch retained native graph: {error}");
            app_launch_failed.set(true);
            cx.quit();
        }
    });

    if launch_failed.get() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
