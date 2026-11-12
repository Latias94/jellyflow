use std::rc::Rc;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::view::ViewSessionOptions;
use open_gpui::AppContext;
use open_gpui_canvas_jellyflow::model_options::{
    WorkflowModelOption, WorkflowModelOptionsCompleteness, WorkflowModelOptionsFailure,
    WorkflowModelOptionsPublishOutcome, WorkflowModelOptionsResponse, WorkflowModelOptionsState,
};
use open_gpui_canvas_jellyflow::nodes::{
    NativeNodeResources, native_node_catalog, native_node_registry, workflow_demo_graph,
    workflow_llm_node_id, workflow_model_option_selector, workflow_model_options_empty_selector,
    workflow_model_options_failed_selector, workflow_model_options_loading_selector,
    workflow_model_options_partial_selector, workflow_model_options_retry_selector,
    workflow_model_selector,
};
use open_gpui_ui_components::gpui_adapter::init_text_input;

#[open_gpui::test]
fn model_options_entity_rejects_stale_publish_and_requires_retryable_failure(
    cx: &mut open_gpui::TestAppContext,
) {
    let options = cx.update(|cx| cx.new(|_| WorkflowModelOptionsState::loading()));
    let initial = options.read_with(cx, |state, _| state.request_token());

    let published = cx.update(|cx| {
        options.update(cx, |state, cx| {
            state.publish(
                initial,
                WorkflowModelOptionsResponse::Failed(WorkflowModelOptionsFailure::retryable(
                    "catalog unavailable",
                )),
                cx,
            )
        })
    });
    assert_eq!(published, WorkflowModelOptionsPublishOutcome::Published);

    let retried = cx
        .update(|cx| options.update(cx, WorkflowModelOptionsState::retry))
        .expect("retryable failure starts a new request");
    assert_ne!(retried, initial);
    assert!(options.read_with(cx, |state, _| state.status().is_loading()));

    let stale = cx.update(|cx| {
        options.update(cx, |state, cx| {
            state.publish(
                initial,
                WorkflowModelOptionsResponse::Ready {
                    options: vec![WorkflowModelOption::new("stale", "Stale")],
                    completeness: WorkflowModelOptionsCompleteness::Complete,
                },
                cx,
            )
        })
    });
    assert_eq!(
        stale,
        WorkflowModelOptionsPublishOutcome::Stale {
            expected: retried,
            received: initial,
        }
    );
    assert!(options.read_with(cx, |state, _| state.status().is_loading()));

    cx.update(|cx| {
        options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(
                    retried,
                    WorkflowModelOptionsResponse::Failed(
                        WorkflowModelOptionsFailure::non_retryable("account disabled"),
                    ),
                    cx,
                ),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    let retry_error = cx.update(|cx| options.update(cx, WorkflowModelOptionsState::retry));
    assert!(retry_error.is_err());
    assert!(options.read_with(cx, |state, _| state.status().is_failed()));
}

#[open_gpui::test]
fn native_model_select_exposes_resource_states_without_clearing_persisted_value(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let catalog = native_node_catalog().expect("native catalog");
    let runtime = Rc::new(
        GraphRuntime::builder(catalog.clone())
            .build(workflow_demo_graph().expect("workflow graph"))
            .expect("workflow runtime"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("workflow view");
    let model_options = cx.update(|cx| cx.new(|_| WorkflowModelOptionsState::loading()));
    let resources = NativeNodeResources::new(model_options.clone());
    let (host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(view, native_node_registry(resources, &catalog), viewport())
            .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let llm = workflow_llm_node_id();
    let select = workflow_model_selector(llm);
    assert!(cx.debug_bounds(&select).is_some());
    assert!(
        cx.debug_bounds(&workflow_model_options_loading_selector(llm))
            .is_some()
    );
    assert_eq!(model_value(&runtime), "gpt-4.1");

    let trigger = cx.debug_bounds(&select).expect("model select trigger");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_option_selector(llm, "gpt-4.1"))
            .is_none(),
        "loading keeps the Select mounted but disabled"
    );

    let loading = model_options.read_with(cx, |state, _| state.request_token());
    cx.update(|_, cx| {
        model_options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(loading, WorkflowModelOptionsResponse::Empty, cx),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    draw(cx);
    assert!(cx.debug_bounds(&select).is_some());
    assert!(
        cx.debug_bounds(&workflow_model_options_empty_selector(llm))
            .is_some()
    );
    assert_eq!(model_value(&runtime), "gpt-4.1");
    let trigger = cx.debug_bounds(&select).expect("empty model select");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_option_selector(llm, "gpt-4.1"))
            .is_none(),
        "empty state keeps the Select mounted but disabled"
    );

    let failed_request = cx
        .update(|_, cx| model_options.update(cx, WorkflowModelOptionsState::begin_request))
        .expect("manual refresh starts a new request");
    cx.update(|_, cx| {
        model_options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(
                    failed_request,
                    WorkflowModelOptionsResponse::Failed(WorkflowModelOptionsFailure::retryable(
                        "provider timed out",
                    )),
                    cx,
                ),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_options_failed_selector(llm))
            .is_some()
    );
    let retry = cx
        .debug_bounds(&workflow_model_options_retry_selector(llm))
        .expect("retryable failure exposes one explicit retry");
    cx.simulate_click(retry.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_options_loading_selector(llm))
            .is_some()
    );
    let retry_request = model_options.read_with(cx, |state, _| state.request_token());
    assert_ne!(retry_request, failed_request);
    assert_eq!(model_value(&runtime), "gpt-4.1");

    cx.update(|_, cx| {
        model_options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(
                    retry_request,
                    WorkflowModelOptionsResponse::Ready {
                        options: vec![
                            WorkflowModelOption::new("claude-disabled", "Claude (disabled)")
                                .disabled("provider entitlement required"),
                            WorkflowModelOption::new("gemini-2.5-pro", "Gemini 2.5 Pro"),
                        ],
                        completeness: WorkflowModelOptionsCompleteness::Partial,
                    },
                    cx,
                ),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_options_partial_selector(llm))
            .is_some()
    );
    assert_eq!(model_value(&runtime), "gpt-4.1");

    let trigger = cx.debug_bounds(&select).expect("enabled model select");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    let disabled = cx
        .debug_bounds(&workflow_model_option_selector(llm, "claude-disabled"))
        .expect("disabled model option remains visible");
    cx.simulate_click(disabled.center(), Default::default());
    draw(cx);
    assert_eq!(model_value(&runtime), "gpt-4.1");

    let enabled = cx
        .debug_bounds(&workflow_model_option_selector(llm, "gemini-2.5-pro"))
        .expect("partial response keeps available options selectable");
    cx.simulate_click(enabled.center(), Default::default());
    draw(cx);
    assert_eq!(model_value(&runtime), "gemini-2.5-pro");

    let non_retryable_request = cx
        .update(|_, cx| model_options.update(cx, WorkflowModelOptionsState::begin_request))
        .expect("application refresh starts a new request");
    cx.update(|_, cx| {
        model_options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(
                    non_retryable_request,
                    WorkflowModelOptionsResponse::Failed(
                        WorkflowModelOptionsFailure::non_retryable("workspace access revoked"),
                    ),
                    cx,
                ),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    draw(cx);
    assert!(cx.debug_bounds(&select).is_some());
    assert!(
        cx.debug_bounds(&workflow_model_options_failed_selector(llm))
            .is_some()
    );
    assert!(
        cx.debug_bounds(&workflow_model_options_retry_selector(llm))
            .is_none(),
        "non-retryable failures never expose a retry action"
    );
    assert_eq!(model_value(&runtime), "gemini-2.5-pro");
    let trigger = cx.debug_bounds(&select).expect("failed model select");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_option_selector(llm, "gemini-2.5-pro"))
            .is_none(),
        "failed state keeps the Select mounted but disabled"
    );

    let complete_request = cx
        .update(|_, cx| model_options.update(cx, WorkflowModelOptionsState::begin_request))
        .expect("application reload starts a new request");
    cx.update(|_, cx| {
        model_options.update(cx, |state, cx| {
            assert_eq!(
                state.publish(
                    complete_request,
                    WorkflowModelOptionsResponse::Ready {
                        options: vec![
                            WorkflowModelOption::new("gemini-2.5-pro", "Gemini 2.5 Pro"),
                            WorkflowModelOption::new("gpt-4.1", "GPT-4.1"),
                        ],
                        completeness: WorkflowModelOptionsCompleteness::Complete,
                    },
                    cx,
                ),
                WorkflowModelOptionsPublishOutcome::Published,
            );
        });
    });
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_options_loading_selector(llm))
            .is_none()
    );
    assert!(
        cx.debug_bounds(&workflow_model_options_empty_selector(llm))
            .is_none()
    );
    assert!(
        cx.debug_bounds(&workflow_model_options_failed_selector(llm))
            .is_none()
    );
    assert!(
        cx.debug_bounds(&workflow_model_options_partial_selector(llm))
            .is_none()
    );
    assert_eq!(model_value(&runtime), "gemini-2.5-pro");
    let trigger = cx.debug_bounds(&select).expect("complete model select");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    let gpt = cx
        .debug_bounds(&workflow_model_option_selector(llm, "gpt-4.1"))
        .expect("complete response options are selectable");
    cx.simulate_click(gpt.center(), Default::default());
    draw(cx);
    assert_eq!(model_value(&runtime), "gpt-4.1");

    let trigger = cx.debug_bounds(&select).expect("complete model select");
    cx.simulate_click(trigger.center(), Default::default());
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_option_selector(llm, "gemini-2.5-pro"))
            .is_some()
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 1);
        assert_eq!(host.resource_snapshot().protected_leases(), 1);
    });
    let _loading_request = cx
        .update(|_, cx| model_options.update(cx, WorkflowModelOptionsState::begin_request))
        .expect("refresh while open starts a new request");
    draw(cx);
    assert!(
        cx.debug_bounds(&workflow_model_option_selector(llm, "gemini-2.5-pro"))
            .is_none(),
        "resource invalidation closes the controlled model overlay"
    );
    cx.read_entity(&host, |host, _| {
        assert_eq!(host.resource_snapshot().protected_instances(), 0);
        assert_eq!(host.resource_snapshot().protected_leases(), 0);
    });
    assert_eq!(model_value(&runtime), "gpt-4.1");
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 1_100.0,
            height: 720.0,
        },
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn model_value(runtime: &GraphRuntime) -> String {
    runtime
        .snapshot()
        .expect("runtime snapshot")
        .graph()
        .node(&workflow_llm_node_id())
        .expect("workflow LLM")
        .data["model"]
        .as_str()
        .expect("model string")
        .to_owned()
}
