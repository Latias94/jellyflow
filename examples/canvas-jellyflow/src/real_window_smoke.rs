use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use jellyflow_runtime::view::WeakViewSession;
use open_gpui::{App, Entity, Window};
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

use crate::native_app::NativeDemo;

pub(super) const SMOKE_WINDOW_SIZE: (f32, f32) = (1_180.0, 720.0);

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_PROBE_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
enum SmokeOutcome {
    Pending,
    Passed(SmokeSummary),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SmokeSummary {
    graph_nodes: usize,
    active_nodes: usize,
    published_nodes: usize,
    published_ports: usize,
    published_routes: usize,
}

#[derive(Debug)]
pub(super) struct SmokeState {
    demo: NativeDemo,
    outcome: SmokeOutcome,
}

pub(super) type SharedSmokeState = Rc<RefCell<SmokeState>>;

pub(super) fn new_smoke_state(demo: NativeDemo) -> SharedSmokeState {
    Rc::new(RefCell::new(SmokeState {
        demo,
        outcome: SmokeOutcome::Pending,
    }))
}

pub(super) fn start_smoke_probe(
    cx: &mut App,
    window: &Window,
    graph: Entity<NativeGraphSurface>,
    view: WeakViewSession,
    state: SharedSmokeState,
) {
    let window_handle = window.window_handle();
    cx.spawn(async move |cx| {
        let started = Instant::now();
        loop {
            cx.background_executor().timer(SMOKE_PROBE_INTERVAL).await;

            match cx.update(|cx| observe_surface(&graph, &view, cx)) {
                Ok(Some(summary)) => {
                    state.borrow_mut().outcome = SmokeOutcome::Passed(summary);
                    cx.update(|cx| cx.quit());
                    return;
                }
                Ok(None) if started.elapsed() < SMOKE_TIMEOUT => {
                    let window_available = cx.update(|cx| {
                        window_handle
                            .update(cx, |_, window, _| window.refresh())
                            .is_ok()
                    });
                    if !window_available {
                        let state = state.clone();
                        cx.update(|cx| {
                            fail_smoke(
                                cx,
                                &state,
                                "native window closed before the first valid frame",
                            )
                        });
                        return;
                    }
                }
                Ok(None) => {
                    let state = state.clone();
                    cx.update(|cx| {
                        fail_smoke(
                            cx,
                            &state,
                            "timed out waiting for the first valid native frame",
                        )
                    });
                    return;
                }
                Err(error) => {
                    let state = state.clone();
                    cx.update(|cx| fail_smoke(cx, &state, error));
                    return;
                }
            }
        }
    })
    .detach();
}

fn observe_surface(
    graph: &Entity<NativeGraphSurface>,
    view: &WeakViewSession,
    cx: &App,
) -> Result<Option<SmokeSummary>, String> {
    if !view.is_open() {
        return Err("native view closed before the first valid frame".to_owned());
    }

    let (surface_size, resources, diagnostic) = {
        let surface = graph.read(cx);
        let host = surface.host().read(cx);
        (
            host.surface_size(),
            host.resource_snapshot(),
            host.last_diagnostic()
                .map(|diagnostic| format!("{diagnostic:?}")),
        )
    };
    if let Some(diagnostic) = diagnostic {
        return Err(format!("native host reported a diagnostic: {diagnostic}"));
    }
    let Some(surface_size) = surface_size else {
        return Ok(None);
    };
    if !surface_size.width.is_finite()
        || !surface_size.height.is_finite()
        || surface_size.width <= 0.0
        || surface_size.height <= 0.0
    {
        return Err(format!(
            "native host published an invalid surface: {surface_size:?}"
        ));
    }

    let Ok(snapshot) = view.graph_snapshot() else {
        return Ok(None);
    };
    let Ok(geometry) = view.geometry_snapshot() else {
        return Ok(None);
    };
    let graph_nodes = snapshot.graph().nodes().len();
    let graph_ports = snapshot.graph().ports().len();
    let graph_edges = snapshot.graph().edges().len();
    if graph_nodes == 0 {
        return Err("native smoke fixture has no nodes".to_owned());
    }

    let active_nodes = resources.active_nodes();
    let published_nodes = geometry.nodes().len();
    let published_ports = geometry.ports().len();
    let published_routes = geometry.routes().len();
    let frame_is_ready = active_nodes > 0
        && published_nodes > 0
        && (graph_ports == 0 || published_ports > 0)
        && (graph_edges == 0 || published_routes > 0);
    if !frame_is_ready {
        return Ok(None);
    }

    Ok(Some(SmokeSummary {
        graph_nodes,
        active_nodes,
        published_nodes,
        published_ports,
        published_routes,
    }))
}

pub(super) fn fail_smoke(cx: &mut App, state: &SharedSmokeState, error: impl Into<String>) {
    let mut state = state.borrow_mut();
    if matches!(state.outcome, SmokeOutcome::Pending) {
        state.outcome = SmokeOutcome::Failed(error.into());
    }
    cx.quit();
}

pub(super) fn finish_smoke_after_run(state: SharedSmokeState) -> ExitCode {
    let state = state.borrow();
    let demo = match state.demo {
        NativeDemo::Shader => "shader",
        NativeDemo::Workflow => "workflow",
    };
    match &state.outcome {
        SmokeOutcome::Passed(summary) => {
            eprintln!("real-window smoke passed for {demo}: {summary:?}");
            ExitCode::SUCCESS
        }
        SmokeOutcome::Failed(error) => {
            eprintln!("real-window smoke failed for {demo}: {error}");
            ExitCode::FAILURE
        }
        SmokeOutcome::Pending => {
            eprintln!("real-window smoke failed for {demo}: application exited before completion");
            ExitCode::FAILURE
        }
    }
}
