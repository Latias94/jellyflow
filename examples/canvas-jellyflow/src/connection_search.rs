use std::fmt;
use std::rc::Rc;

use jellyflow::runtime::schema::NodeCatalog;
use jellyflow::runtime::{
    CanvasInteractionPhase, ConnectionSearchRequest, ConnectionSearchStatus,
    CreateFromWireCandidates, InteractionTerminalReason, ViewSession, ViewSize,
};
use jellyflow_open_gpui::{
    OpenGpuiNodeRegistry, OpenGpuiOverlayInteractionBoundary, OpenGpuiSceneHost,
};
use open_gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, KeyDownEvent,
    MouseButton, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription,
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
pub const NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR: &str =
    "jellyflow-native-connection-search-loading";
const NATIVE_CONNECTION_SEARCH_INPUT_SELECTOR: &str = "jellyflow-native-connection-search-input";

/// Application-owned source for one revision-bound create-from-wire query.
pub trait NativeConnectionSearchSource {
    fn candidates(
        &self,
        request: ConnectionSearchRequest,
        query: &mut dyn FnMut() -> Result<
            CreateFromWireCandidates,
            NativeConnectionSearchSourceError,
        >,
    ) -> Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>;
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
        _request: ConnectionSearchRequest,
        query: &mut dyn FnMut() -> Result<
            CreateFromWireCandidates,
            NativeConnectionSearchSourceError,
        >,
    ) -> Result<CreateFromWireCandidates, NativeConnectionSearchSourceError> {
        query()
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
    focus_search_on_next_render: bool,
    cleanup_search_focus: bool,
    previous_focus: Option<WeakFocusHandle>,
    surface_focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

struct NativeConnectionSearch {
    request: ConnectionSearchRequest,
    candidates: Option<CreateFromWireCandidates>,
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
        let host = cx.new(|_| {
            OpenGpuiSceneHost::new_measured_surface(view, registry)
                .with_graph_theme(crate::nodes::native_graph_theme())
        });
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
        self.sync_search(cx)
    }

    fn sync_search(&mut self, cx: &mut Context<Self>) -> bool {
        let host = self.host.read(cx);
        let request = host.connection_search_request();
        let presentation = host.interaction_presentation();
        match request {
            Some(request)
                if self
                    .search
                    .as_ref()
                    .is_none_or(|search| search.request != request) =>
            {
                self.query_controller
                    .update(cx, |controller, cx| controller.set_value("", cx));
                self.search = Some(NativeConnectionSearch {
                    request,
                    candidates: None,
                    rows: Vec::new(),
                    diagnostic: None,
                });
                self.focus_search_on_next_render = true;
                true
            }
            None if presentation.phase() == CanvasInteractionPhase::Committing
                && self.search.is_some() =>
            {
                false
            }
            None => {
                if self.search.take().is_some() {
                    self.focus_search_on_next_render = false;
                    self.cleanup_search_focus = true;
                    self.query_controller
                        .update(cx, |controller, cx| controller.set_value("", cx));
                    true
                } else {
                    false
                }
            }
            Some(_) => {
                let diagnostic = presentation
                    .connection()
                    .and_then(|connection| connection.search())
                    .and_then(|status| match status {
                        ConnectionSearchStatus::Error => {
                            Some("Search failed; retry to query the current graph.".to_owned())
                        }
                        ConnectionSearchStatus::CommitRejected => Some(match presentation.terminal()
                        {
                            Some(InteractionTerminalReason::Conflict) => {
                                "Graph changed before the connection was applied; retry with refreshed candidates."
                                    .to_owned()
                            }
                            Some(InteractionTerminalReason::TransportFailure) => {
                                "Connection result is unavailable; retry after the command queue recovers."
                                    .to_owned()
                            }
                            _ => "Connection was rejected; retry with refreshed candidates."
                                .to_owned(),
                        }),
                        ConnectionSearchStatus::Loading
                        | ConnectionSearchStatus::Ready { .. }
                        | ConnectionSearchStatus::Empty => None,
                    });
                let Some(search) = self.search.as_mut() else {
                    return false;
                };
                if diagnostic.is_some() && search.diagnostic != diagnostic {
                    search.diagnostic = diagnostic;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn load_search(
        &mut self,
        request: ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = self.host.clone();
        let source = Rc::clone(&self.search_source);
        let mut query = || {
            host.update(cx, |host, cx| {
                host.connection_search_candidates(request, window, cx)
            })
            .map_err(|error| NativeConnectionSearchSourceError::new(error.to_string()))
        };
        let result = source.candidates(request, &mut query);
        let Some(search) = self
            .search
            .as_mut()
            .filter(|search| search.request == request)
        else {
            return;
        };
        match result {
            Ok(candidates) => {
                search.rows = Self::candidate_rows(&self.catalog, &candidates);
                search.candidates = Some(candidates);
                search.diagnostic = None;
            }
            Err(error) => {
                search.candidates = None;
                search.rows.clear();
                search.diagnostic = Some(format!("Search failed: {error}"));
            }
        }
        cx.notify();
    }

    fn candidate_rows(
        catalog: &NodeCatalog,
        candidates: &CreateFromWireCandidates,
    ) -> Vec<NativeCandidateRow> {
        candidates
            .candidates()
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                let spec = catalog.get(candidate.node_kind())?;
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

    fn retry_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.search.as_ref().map(|search| search.request) else {
            return;
        };
        let result = self.host.update(cx, |host, cx| {
            host.retry_connection_search(request, window, cx)
        });
        if let Err(error) = result {
            if let Some(search) = self.search.as_mut() {
                search.diagnostic = Some(format!("Search retry failed: {error}"));
            }
            cx.notify();
            return;
        }
        if let Some(search) = self.search.as_mut() {
            search.candidates = None;
            search.rows.clear();
            search.diagnostic = None;
        }
        self.load_search(request, window, cx);
    }

    fn commit_candidate(
        &mut self,
        candidate_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.host.read(cx).interaction_presentation().phase()
            != CanvasInteractionPhase::ConnectionSearch
        {
            return;
        }
        let Some((request, candidates)) = self.search.as_ref().and_then(|search| {
            search
                .candidates
                .clone()
                .map(|candidates| (search.request, candidates))
        }) else {
            return;
        };
        let result = self.host.update(cx, |host, cx| {
            let result = host.select_connection_search_candidate(
                request,
                &candidates,
                candidate_index,
                window,
                cx,
            );
            cx.notify();
            result
        });
        match result {
            Ok(_) => {
                if let Some(search) = self.search.as_mut() {
                    search.diagnostic = None;
                }
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

    fn cancel_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.search.as_ref().map(|search| search.request) else {
            return;
        };
        if self.host.read(cx).interaction_presentation().phase()
            == CanvasInteractionPhase::Committing
        {
            return;
        }
        if let Some(focus) = self
            .previous_focus
            .as_ref()
            .and_then(WeakFocusHandle::upgrade)
        {
            window.focus(&focus, cx);
        }
        let _ = self.host.update(cx, |host, cx| {
            let result = host.cancel_connection_search(request, window, cx);
            cx.notify();
            result
        });
        self.close_search(window, cx);
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.focus_search_on_next_render = false;
        self.cleanup_search_focus = false;
        self.query_controller
            .update(cx, |controller, cx| controller.set_value("", cx));
        self.previous_focus = None;
        if window.focused(cx).is_none() {
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
        let presentation = host.interaction_presentation();
        let search_status = presentation
            .connection()
            .and_then(|connection| connection.search());
        let committing = presentation.phase() == CanvasInteractionPhase::Committing;
        let surface_size = host.surface_size().unwrap_or_else(|| {
            let fallback = host.frame_bounds().size;
            ViewSize::new(fallback.width, fallback.height)
        });
        let anchor = search.request.position().view();
        let left = anchor.x.clamp(12.0, (surface_size.width - 332.0).max(12.0));
        let top = anchor
            .y
            .clamp(12.0, (surface_size.height - 360.0).max(12.0));
        let surface = cx.weak_entity();
        let backdrop_surface = surface.clone();
        let retry_surface = surface.clone();
        let diagnostic = search.diagnostic.clone();
        let show_retry = diagnostic.is_some() && !committing;
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
                        .disabled(committing)
                        .on_click(move |_, window, cx| {
                            let _ = candidate_surface.update(cx, |surface, cx| {
                                surface.commit_candidate(index, window, cx)
                            });
                        }),
                    ),
            );
        }
        if committing
            || matches!(search_status, Some(ConnectionSearchStatus::Loading))
                && search.candidates.is_none()
                && search.diagnostic.is_none()
        {
            results = results.child(
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR.to_owned())
                    .py(px(16.0))
                    .text_sm()
                    .text_color(rgb(0x9aa3af))
                    .child(if committing {
                        "Applying connection..."
                    } else {
                        "Loading compatible nodes..."
                    }),
            );
        } else if rows_empty {
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
                            .on_click(move |_, window, cx| {
                                let _ = retry_surface
                                    .update(cx, |surface, cx| surface.retry_search(window, cx));
                            }),
                    )
            }));
        Some(
            div()
                .debug_selector(|| NATIVE_CONNECTION_SEARCH_BACKDROP_SELECTOR.to_owned())
                .absolute()
                .size_full()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    let _ = backdrop_surface
                        .update(cx, |surface, cx| surface.cancel_search(window, cx));
                })
                .child(
                    div()
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .child(OpenGpuiOverlayInteractionBoundary::wrap(panel)),
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
        if let Some(request) = self
            .search
            .as_ref()
            .filter(|search| search.candidates.is_none() && search.diagnostic.is_none())
            .map(|search| search.request)
        {
            self.load_search(request, window, cx);
        }
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
                    surface.cancel_search(window, cx);
                }
            }))
            .child(div().absolute().size_full().child(self.host.clone()))
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
