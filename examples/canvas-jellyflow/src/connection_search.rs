use std::fmt;
use std::rc::Rc;

use jellyflow::runtime::schema::NodeCatalog;
use jellyflow::runtime::{GraphCommandOutcome, ViewSession, ViewSize};
use jellyflow_open_gpui::{
    OpenGpuiCommandTicketResult, OpenGpuiConnectionCancelReason, OpenGpuiConnectionCommitEvent,
    OpenGpuiConnectionCommitFailure, OpenGpuiConnectionCommitOutcome,
    OpenGpuiConnectionCommitRejection, OpenGpuiConnectionCommitStage, OpenGpuiConnectionPoint,
    OpenGpuiCreateFromWireCandidates, OpenGpuiCreateFromWireError, OpenGpuiCreateFromWireRequest,
    OpenGpuiNodeInteractionGuard, OpenGpuiNodeRegistry, OpenGpuiSceneHost,
};
use open_gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, KeyDownEvent,
    MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Task,
    WeakFocusHandle, Window, div, px, rgb,
};
use open_gpui_ui_components::gpui_adapter::TextInputController;
use open_gpui_ui_components::{Button, ButtonVariant, TextInput};

pub const NATIVE_GRAPH_SURFACE_SELECTOR: &str = "jellyflow-native-graph-surface";
pub const NATIVE_CONNECTION_SEARCH_SELECTOR: &str = "jellyflow-native-connection-search";
pub const NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR: &str =
    "jellyflow-native-connection-search-backdrop";
pub const NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR: &str =
    "jellyflow-native-connection-search-empty";
pub const NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-connection-search-diagnostic";
pub const NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR: &str = "jellyflow-native-search-retry";
pub const NATIVE_CONNECTION_COMMIT_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-connection-commit-diagnostic";
const NATIVE_CONNECTION_SEARCH_INPUT_SELECTOR: &str = "jellyflow-native-connection-search-input";

/// Application-owned source for one revision-bound create-from-wire query.
pub trait NativeConnectionSearchSource {
    fn candidates(
        &self,
        host: &OpenGpuiSceneHost,
        request: OpenGpuiCreateFromWireRequest,
    ) -> Result<OpenGpuiCreateFromWireCandidates, NativeConnectionSearchSourceError>;
}

/// Product-level candidate-source failure shown by the native search surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeConnectionSearchSourceError {
    message: String,
}

impl NativeConnectionSearchSourceError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeConnectionSearchSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for NativeConnectionSearchSourceError {}

struct RuntimeConnectionSearchSource;

impl NativeConnectionSearchSource for RuntimeConnectionSearchSource {
    fn candidates(
        &self,
        host: &OpenGpuiSceneHost,
        request: OpenGpuiCreateFromWireRequest,
    ) -> Result<OpenGpuiCreateFromWireCandidates, NativeConnectionSearchSourceError> {
        host.create_from_wire_candidates(request)
            .map_err(|error| NativeConnectionSearchSourceError::new(error.to_string()))
    }
}

/// Application-owned graph surface that composes the adapter host with native search UI.
///
/// The adapter supplies inert requests and semantic candidates. This component owns all Open GPUI
/// controls, filtering, focus, diagnostics, and visual treatment.
pub struct NativeGraphSurface {
    host: Entity<OpenGpuiSceneHost>,
    catalog: NodeCatalog,
    search_source: Rc<dyn NativeConnectionSearchSource>,
    query_controller: Entity<TextInputController>,
    search: Option<NativeConnectionSearch>,
    pending_commit_request: Option<OpenGpuiCreateFromWireRequest>,
    pending_commit_task: Option<Task<()>>,
    connection_commit_diagnostic: Option<NativeConnectionCommitDiagnostic>,
    focus_search_on_next_render: bool,
    cleanup_search_focus: bool,
    previous_focus: Option<WeakFocusHandle>,
    surface_focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

struct NativeConnectionSearch {
    request: OpenGpuiCreateFromWireRequest,
    anchor: OpenGpuiConnectionPoint,
    candidates: Option<OpenGpuiCreateFromWireCandidates>,
    rows: Vec<NativeCandidateRow>,
    diagnostic: Option<String>,
}

struct NativeCandidateRow {
    index: usize,
    selector: String,
    title: String,
    detail: String,
    searchable: String,
}

#[derive(PartialEq, Eq)]
struct NativeConnectionCommitDiagnostic {
    code: String,
    message: String,
}

impl NativeConnectionCommitDiagnostic {
    fn from_event(event: OpenGpuiConnectionCommitEvent) -> Option<Self> {
        let OpenGpuiConnectionCommitOutcome::Failed(failure) = event.into_outcome() else {
            return None;
        };
        let (code, message) = match failure {
            OpenGpuiConnectionCommitFailure::Preparation(rejection) => {
                let code = match &rejection {
                    OpenGpuiConnectionCommitRejection::StaleGeneration { .. } => {
                        "connection.stale_generation"
                    }
                    OpenGpuiConnectionCommitRejection::InvalidPhase { .. } => {
                        "connection.invalid_phase"
                    }
                    OpenGpuiConnectionCommitRejection::RevisionUnavailable => {
                        "connection.revision_unavailable"
                    }
                    OpenGpuiConnectionCommitRejection::GeometryUnavailable { .. } => {
                        "connection.geometry_unavailable"
                    }
                };
                (code.to_owned(), rejection.to_string())
            }
            OpenGpuiConnectionCommitFailure::PolicyRejected(evaluation) => {
                evaluation.diagnostics().first().map_or_else(
                    || {
                        (
                            "connection.policy_rejected".to_owned(),
                            "Connection policy rejected this edit.".to_owned(),
                        )
                    },
                    |diagnostic| (diagnostic.key.clone(), diagnostic.message.clone()),
                )
            }
            OpenGpuiConnectionCommitFailure::Conflict(conflict) => (
                "connection.revision_conflict".to_owned(),
                format!(
                    "Graph changed from revision {} to {}; retry the connection.",
                    conflict.expected_revision(),
                    conflict.current().revision()
                ),
            ),
            OpenGpuiConnectionCommitFailure::Rejected(rejection) => {
                rejection.diagnostics().first().map_or_else(
                    || {
                        (
                            "connection.command_rejected".to_owned(),
                            rejection.message().to_owned(),
                        )
                    },
                    |diagnostic| (diagnostic.key.clone(), diagnostic.message.clone()),
                )
            }
            OpenGpuiConnectionCommitFailure::Session { stage, error } => {
                let code = match stage {
                    OpenGpuiConnectionCommitStage::Evaluation => {
                        "connection.evaluation_unavailable"
                    }
                    OpenGpuiConnectionCommitStage::Execution => "connection.execution_unavailable",
                };
                (code.to_owned(), error.to_string())
            }
            OpenGpuiConnectionCommitFailure::CommandEnqueue(error) => (
                "connection.command_queue_unavailable".to_owned(),
                error.to_string(),
            ),
            OpenGpuiConnectionCommitFailure::Command(error) => (
                "connection.command_result_unavailable".to_owned(),
                error.to_string(),
            ),
        };
        Some(Self { code, message })
    }
}

impl NativeGraphSurface {
    #[must_use]
    pub fn new(view: ViewSession, registry: OpenGpuiNodeRegistry, cx: &mut Context<Self>) -> Self {
        Self::new_with_search_source(view, registry, Rc::new(RuntimeConnectionSearchSource), cx)
    }

    #[must_use]
    pub fn new_with_search_source(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
        search_source: Rc<dyn NativeConnectionSearchSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        let catalog = view.catalog().clone();
        let host = cx.new(|_| OpenGpuiSceneHost::new_measured_surface(view, registry));
        let query_controller = cx.new(TextInputController::new);
        let host_subscription = cx.observe(&host, |surface, _, cx| {
            if surface.sync_host_state(cx) {
                cx.notify();
            }
        });
        let query_subscription = cx.observe(&query_controller, |_, _, cx| cx.notify());
        Self {
            host,
            catalog,
            search_source,
            query_controller,
            search: None,
            pending_commit_request: None,
            pending_commit_task: None,
            connection_commit_diagnostic: None,
            focus_search_on_next_render: false,
            cleanup_search_focus: false,
            previous_focus: None,
            surface_focus: cx.focus_handle(),
            _subscriptions: vec![host_subscription, query_subscription],
        }
    }

    #[must_use]
    pub fn host(&self) -> &Entity<OpenGpuiSceneHost> {
        &self.host
    }

    fn sync_host_state(&mut self, cx: &mut Context<Self>) -> bool {
        let search_changed = self.sync_search(cx);
        // Create-from-wire outcomes return directly to `commit_candidate`; only ordinary
        // port-to-port and reconnect outcomes enter this host event slot.
        let commit_changed = self.sync_connection_commit(cx);
        search_changed || commit_changed
    }

    fn sync_connection_commit(&mut self, cx: &mut Context<Self>) -> bool {
        let event = self
            .host
            .update(cx, |host, _| host.take_last_connection_commit());
        let Some(event) = event else {
            return false;
        };
        let diagnostic = NativeConnectionCommitDiagnostic::from_event(event);
        if self.connection_commit_diagnostic == diagnostic {
            return false;
        }
        self.connection_commit_diagnostic = diagnostic;
        true
    }

    fn sync_search(&mut self, cx: &mut Context<Self>) -> bool {
        let request = self.host.read(cx).create_from_wire_request();
        match request {
            Some(request)
                if self
                    .search
                    .as_ref()
                    .is_none_or(|search| search.request != request) =>
            {
                self.query_controller
                    .update(cx, |controller, cx| controller.set_value("", cx));
                self.search = Some(self.load_search(request, cx));
                self.connection_commit_diagnostic = None;
                self.focus_search_on_next_render = true;
                true
            }
            None => {
                if self.search.take().is_some() {
                    self.focus_search_on_next_render = false;
                    self.cleanup_search_focus = true;
                    self.pending_commit_request = None;
                    self.query_controller
                        .update(cx, |controller, cx| controller.set_value("", cx));
                    true
                } else {
                    false
                }
            }
            Some(_) => false,
        }
    }

    fn load_search(
        &self,
        request: OpenGpuiCreateFromWireRequest,
        cx: &mut Context<Self>,
    ) -> NativeConnectionSearch {
        let host = self.host.read(cx);
        let anchor = host
            .create_from_wire_local_position(request)
            .ok()
            .flatten()
            .unwrap_or_else(|| request.position());
        match self.search_source.candidates(host, request) {
            Ok(candidates) => {
                let rows = self.candidate_rows(&candidates);
                NativeConnectionSearch {
                    request,
                    anchor,
                    candidates: Some(candidates),
                    rows,
                    diagnostic: None,
                }
            }
            Err(error) => NativeConnectionSearch {
                request,
                anchor,
                candidates: None,
                rows: Vec::new(),
                diagnostic: Some(format!("Search failed: {error}")),
            },
        }
    }

    fn candidate_rows(
        &self,
        candidates: &OpenGpuiCreateFromWireCandidates,
    ) -> Vec<NativeCandidateRow> {
        candidates
            .candidates()
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                let spec = self.catalog.get(candidate.node_kind())?;
                Some(NativeCandidateRow {
                    index,
                    selector: native_connection_search_candidate_selector(
                        &candidate.node_kind().0,
                        &candidate.port().0,
                    ),
                    title: spec.title().to_owned(),
                    detail: format!("{} / {}", candidate.node_kind().0, candidate.port().0),
                    searchable: format!(
                        "{} {} {} {} {}",
                        spec.title(),
                        candidate.node_kind().0,
                        candidate.port().0,
                        spec.category().join(" "),
                        spec.keywords().join(" ")
                    )
                    .to_lowercase(),
                })
            })
            .collect()
    }

    fn retry_search(&mut self, cx: &mut Context<Self>) {
        if self.pending_commit_request.is_some() {
            return;
        }
        let Some(request) = self.search.as_ref().map(|search| search.request) else {
            return;
        };
        self.search = Some(self.load_search(request, cx));
        cx.notify();
    }

    fn commit_candidate(
        &mut self,
        candidate_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_commit_request.is_some() {
            return;
        }
        let Some(query) = self
            .search
            .as_ref()
            .and_then(|search| search.candidates.clone())
        else {
            return;
        };
        let request = query.request();
        let result = self.host.update(cx, |host, cx| {
            let result = host.commit_create_from_wire(&query, candidate_index, cx);
            cx.notify();
            result
        });
        match result {
            Ok(submission) => {
                self.pending_commit_request = Some(request);
                let ticket = submission.into_ticket();
                self.pending_commit_task = Some(cx.spawn_in(window, async move |surface, cx| {
                    let result = ticket.await;
                    let _ = surface.update_in(cx, |surface, window, cx| {
                        surface.finish_candidate(request, result, window, cx);
                    });
                }));
                cx.notify();
            }
            Err(OpenGpuiCreateFromWireError::StaleQuery {
                current_revision, ..
            }) => {
                let mut search = self.load_search(request, cx);
                search.diagnostic = Some(format!(
                    "Graph changed at revision {current_revision}; results were refreshed."
                ));
                self.search = Some(search);
                cx.notify();
            }
            Err(error) => {
                if let Some(search) = self.search.as_mut() {
                    search.diagnostic = Some(format!("Connection rejected: {error}"));
                }
                cx.notify();
            }
        }
    }

    fn finish_candidate(
        &mut self,
        request: OpenGpuiCreateFromWireRequest,
        result: OpenGpuiCommandTicketResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_commit_request != Some(request) {
            return;
        }
        self.pending_commit_request = None;
        match result {
            Ok(GraphCommandOutcome::Applied(_) | GraphCommandOutcome::Noop(_)) => {
                self.close_search(window, cx);
            }
            Ok(GraphCommandOutcome::Conflict(conflict)) => {
                let mut search = self.load_search(request, cx);
                search.diagnostic = Some(format!(
                    "Graph changed from revision {} to {}; retry after reviewing refreshed results.",
                    conflict.expected_revision(),
                    conflict.current().revision()
                ));
                self.search = Some(search);
                cx.notify();
            }
            Ok(GraphCommandOutcome::Rejected(rejection)) => {
                if let Some(search) = self.search.as_mut() {
                    search.diagnostic =
                        Some(format!("Connection rejected: {}", rejection.message()));
                }
                cx.notify();
            }
            Err(error) => {
                if let Some(search) = self.search.as_mut() {
                    search.diagnostic = Some(format!(
                        "Connection result is uncertain; refresh before retrying: {error}"
                    ));
                }
                cx.notify();
            }
        }
    }

    fn cancel_search(
        &mut self,
        reason: OpenGpuiConnectionCancelReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(request) = self.search.as_ref().map(|search| search.request) else {
            return;
        };
        if self.pending_commit_request == Some(request) {
            if let Some(search) = self.search.as_mut() {
                search.diagnostic = Some("Connection change is still being applied.".to_owned());
            }
            cx.notify();
            return;
        }
        let _ = self.host.update(cx, |host, cx| {
            let result = host.cancel_create_from_wire(request, reason);
            cx.notify();
            result
        });
        self.close_search(window, cx);
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.pending_commit_request = None;
        self.focus_search_on_next_render = false;
        self.cleanup_search_focus = false;
        self.query_controller
            .update(cx, |controller, cx| controller.set_value("", cx));
        if let Some(focus) = self.previous_focus.take().and_then(|focus| focus.upgrade()) {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.surface_focus, cx);
        }
        window.refresh();
        cx.notify();
    }

    fn filtered_row_indices(&self, cx: &Context<Self>) -> Vec<usize> {
        let Some(search) = self.search.as_ref() else {
            return Vec::new();
        };
        let query = self.query_controller.read(cx).value().trim().to_lowercase();
        search
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                (query.is_empty() || row.searchable.contains(&query)).then_some(index)
            })
            .collect()
    }

    fn render_search(&self, cx: &mut Context<Self>) -> Option<impl open_gpui::IntoElement> {
        let search = self.search.as_ref()?;
        let row_indices = self.filtered_row_indices(cx);
        let rows_empty = row_indices.is_empty();
        let host = self.host.read(cx);
        let surface_size = host.surface_size().unwrap_or_else(|| {
            let fallback = host.frame_bounds().size;
            ViewSize::new(fallback.width, fallback.height)
        });
        let left = search
            .anchor
            .x()
            .clamp(12.0, (surface_size.width - 332.0).max(12.0));
        let top = search
            .anchor
            .y()
            .clamp(12.0, (surface_size.height - 360.0).max(12.0));
        let surface = cx.weak_entity();
        let backdrop_surface = surface.clone();
        let retry_surface = surface.clone();
        let diagnostic = search.diagnostic.clone();
        let show_retry = diagnostic.is_some();
        let mut results = div().flex().flex_col().gap(px(6.0));
        for row_index in row_indices {
            let row = &search.rows[row_index];
            let candidate_surface = surface.clone();
            let candidate_row_surface = surface.clone();
            let index = row.index;
            let selector = row.selector.clone();
            let debug_selector = selector.clone();
            results = results.child(
                div()
                    .id(selector)
                    .debug_selector(move || debug_selector)
                    .on_click(move |_, window, cx| {
                        let _ = candidate_row_surface.update(cx, |surface, cx| {
                            surface.commit_candidate(index, window, cx)
                        });
                    })
                    .child(
                        Button::new(
                            format!("jellyflow-native-search-candidate-{index}"),
                            format!("{}  {}", row.title, row.detail),
                        )
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| {
                            let _ = candidate_surface.update(cx, |surface, cx| {
                                surface.commit_candidate(index, window, cx)
                            });
                        }),
                    ),
            );
        }
        if rows_empty {
            results = results.child(
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR.to_owned())
                    .py(px(16.0))
                    .text_sm()
                    .text_color(rgb(0x9aa3af))
                    .child("No compatible nodes"),
            );
        }
        let panel = div()
            .debug_selector(|| NATIVE_CONNECTION_SEARCH_SELECTOR.to_owned())
            .w(px(320.0))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x3b4350))
            .bg(rgb(0x1d2128))
            .text_color(rgb(0xecf0f5))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().text_sm().child("Add connected node"))
            .child(
                div()
                    .debug_selector(|| native_connection_search_input_selector().to_owned())
                    .child(
                        TextInput::new("jellyflow-native-connection-search-filter", "Filter nodes")
                            .controller(self.query_controller.clone())
                            .placeholder("Search compatible nodes"),
                    ),
            )
            .child(results)
            .children(diagnostic.map(|message| {
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR.to_owned())
                    .p(px(8.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x3a292b))
                    .text_xs()
                    .text_color(rgb(0xffb4a8))
                    .child(message)
            }))
            .children(show_retry.then(|| {
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR.to_owned())
                    .child(
                        Button::new(NATIVE_CONNECTION_SEARCH_RETRY_SELECTOR, "Retry")
                            .variant(ButtonVariant::Secondary)
                            .on_click(move |_, _, cx| {
                                let _ = retry_surface
                                    .update(cx, |surface, cx| surface.retry_search(cx));
                            }),
                    )
            }));
        Some(
            div()
                .debug_selector(|| NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR.to_owned())
                .absolute()
                .size_full()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    let _ = backdrop_surface.update(cx, |surface, cx| {
                        surface.cancel_search(
                            OpenGpuiConnectionCancelReason::OutsideClick,
                            window,
                            cx,
                        )
                    });
                })
                .child(
                    div()
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .child(OpenGpuiNodeInteractionGuard::overlay(panel)),
                ),
        )
    }

    fn render_connection_commit_diagnostic(&self) -> Option<impl open_gpui::IntoElement> {
        let diagnostic = self.connection_commit_diagnostic.as_ref()?;
        let code = diagnostic.code.clone();
        let code_selector = native_connection_commit_diagnostic_code_selector(&code);
        Some(
            div()
                .debug_selector(|| NATIVE_CONNECTION_COMMIT_DIAGNOSTIC_SELECTOR.to_owned())
                .absolute()
                .top(px(12.0))
                .right(px(12.0))
                .w(px(360.0))
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(5.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(0x70434a))
                .bg(rgb(0x2d2024))
                .text_color(rgb(0xffd6d2))
                .child(div().text_sm().child("Connection rejected"))
                .child(
                    div()
                        .debug_selector(move || code_selector.clone())
                        .text_xs()
                        .text_color(rgb(0xffa5ad))
                        .child(code),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xeab9b5))
                        .child(diagnostic.message.clone()),
                ),
        )
    }
}

impl Render for NativeGraphSurface {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.sync_host_state(cx);
        if self.cleanup_search_focus {
            self.cleanup_search_focus = false;
            if let Some(focus) = self.previous_focus.take().and_then(|focus| focus.upgrade()) {
                window.focus(&focus, cx);
            } else {
                window.focus(&self.surface_focus, cx);
            }
        }
        if self.focus_search_on_next_render && self.search.is_some() {
            self.previous_focus = window.focused(cx).map(|focus| focus.downgrade());
            window.focus(&self.query_controller.focus_handle(cx), cx);
            self.focus_search_on_next_render = false;
        }
        div()
            .debug_selector(|| NATIVE_GRAPH_SURFACE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .track_focus(&self.surface_focus)
            .on_key_down(cx.listener(|surface, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.eq_ignore_ascii_case("escape") && surface.search.is_some() {
                    window.prevent_default();
                    cx.stop_propagation();
                    surface.cancel_search(OpenGpuiConnectionCancelReason::Escape, window, cx);
                }
            }))
            .child(div().absolute().size_full().child(self.host.clone()))
            .children(self.render_connection_commit_diagnostic())
            .children(self.render_search(cx))
    }
}

#[must_use]
pub const fn native_connection_search_input_selector() -> &'static str {
    NATIVE_CONNECTION_SEARCH_INPUT_SELECTOR
}

#[must_use]
pub fn native_connection_search_candidate_selector(kind: &str, port: &str) -> String {
    format!("jellyflow-native-connection-search-candidate-{kind}-{port}")
}

#[must_use]
pub fn native_connection_commit_diagnostic_code_selector(code: &str) -> String {
    format!("jellyflow-native-connection-commit-diagnostic-code-{code}")
}
