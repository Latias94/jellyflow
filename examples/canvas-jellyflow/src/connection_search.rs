use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use jellyflow_open_gpui::host::{
    InitialViewportPolicy, OpenGpuiConnectionSearchError, OpenGpuiSceneHost,
};
use jellyflow_open_gpui::navigation::OpenGpuiNavigationConfig;
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
use jellyflow_runtime::geometry::{ViewPoint, ViewSize};
use jellyflow_runtime::graph::CreateFromWireCandidates;
use jellyflow_runtime::interaction::{
    CanvasInteractionPhase, ConnectionSearchRequest, ConnectionSearchStatus,
    ConnectionSettlementCapability, ConnectionSettlementPresentation, InteractionTerminalReason,
};
use jellyflow_runtime::view::{ViewCloseReason, ViewSession};
use open_gpui::prelude::FluentBuilder;
use open_gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, KeyDownEvent,
    MouseButton, ParentElement, Render, Role, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Task, Window, div, px,
};
use open_gpui_ui_components::gpui_adapter::TextInputController;
use open_gpui_ui_components::{Button, ButtonVariant, TextInput};

use crate::product_catalog::{NativeProductComposition, native_product_catalog};
use crate::product_theme::NativeProductTheme;

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
pub const NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR: &str =
    "jellyflow-native-connection-search-results";
const NATIVE_CONNECTION_SEARCH_RESULTS_ROLE: Role = Role::ListBox;
const NATIVE_CONNECTION_SEARCH_INPUT_SELECTOR: &str = "jellyflow-native-connection-search-input";
const NATIVE_CONNECTION_SEARCH_SAFE_INSET: f32 = 12.0;
const NATIVE_CONNECTION_SEARCH_PREFERRED_WIDTH: f32 = 320.0;
const NATIVE_CONNECTION_SEARCH_MAX_HEIGHT: f32 = 480.0;
const NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_MAX_HEIGHT: f32 = 96.0;
const NATIVE_CONNECTION_SEARCH_READY_CHROME_HEIGHT: f32 = 112.0;
const NATIVE_CONNECTION_SEARCH_ERROR_CHROME_HEIGHT: f32 = 216.0;
const NATIVE_CONNECTION_SEARCH_MIN_RESULTS_HEIGHT: f32 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct NativeConnectionSearchLayout {
    left: f32,
    top: f32,
    width: f32,
    max_height: f32,
    results_max_height: f32,
}

impl NativeConnectionSearchLayout {
    fn resolve(surface: ViewSize, anchor: ViewPoint, has_diagnostic: bool) -> Self {
        let surface_width = finite_non_negative(surface.width);
        let surface_height = finite_non_negative(surface.height);
        let available_width = (surface_width - 2.0 * NATIVE_CONNECTION_SEARCH_SAFE_INSET).max(0.0);
        let available_height =
            (surface_height - 2.0 * NATIVE_CONNECTION_SEARCH_SAFE_INSET).max(0.0);
        let width = NATIVE_CONNECTION_SEARCH_PREFERRED_WIDTH.min(available_width);
        let max_height = NATIVE_CONNECTION_SEARCH_MAX_HEIGHT.min(available_height);
        let reserved_height = if has_diagnostic {
            NATIVE_CONNECTION_SEARCH_ERROR_CHROME_HEIGHT
        } else {
            NATIVE_CONNECTION_SEARCH_READY_CHROME_HEIGHT
        };
        let results_max_height = (max_height - reserved_height)
            .max(NATIVE_CONNECTION_SEARCH_MIN_RESULTS_HEIGHT)
            .min(max_height);
        let right_limit = (surface_width - NATIVE_CONNECTION_SEARCH_SAFE_INSET - width).max(0.0);
        let bottom_limit =
            (surface_height - NATIVE_CONNECTION_SEARCH_SAFE_INSET - max_height).max(0.0);
        Self {
            left: finite_non_negative(anchor.x).clamp(
                NATIVE_CONNECTION_SEARCH_SAFE_INSET.min(right_limit),
                right_limit,
            ),
            top: finite_non_negative(anchor.y).clamp(
                NATIVE_CONNECTION_SEARCH_SAFE_INSET.min(bottom_limit),
                bottom_limit,
            ),
            width,
            max_height,
            results_max_height,
        }
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Background result for one application-owned connection-search load.
pub type NativeConnectionSearchFuture = Pin<
    Box<
        dyn Future<Output = Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>>
            + Send
            + 'static,
    >,
>;

/// Application-owned asynchronous source for one revision-bound create-from-wire query.
///
/// Jellyflow obtains `authoritative` from the host on the UI thread, then invokes and polls this
/// source on Open GPUI's background executor. Implementations must not access Open GPUI entities or
/// window state. Dropping the returned future must cancel any work that should not outlive the
/// search surface.
pub trait NativeConnectionSearchSource: Send + Sync + 'static {
    fn candidates(
        &self,
        request: ConnectionSearchRequest,
        authoritative: CreateFromWireCandidates,
    ) -> NativeConnectionSearchFuture;
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
        authoritative: CreateFromWireCandidates,
    ) -> NativeConnectionSearchFuture {
        Box::pin(async move { Ok(authoritative) })
    }
}

/// Application-owned graph surface that composes the adapter host with native search UI.
///
/// The adapter supplies inert requests and semantic candidates. This component owns all Open GPUI
/// controls, filtering, focus, diagnostics, and visual treatment.
pub struct NativeGraphSurface {
    host: Entity<OpenGpuiSceneHost>,
    product_theme: NativeProductTheme,
    product_catalog: NativeProductComposition,
    search_source: Arc<dyn NativeConnectionSearchSource>,
    query_controller: Entity<TextInputController>,
    search: Option<NativeConnectionSearch>,
    focus_search_on_next_render: bool,
    surface_focus: FocusHandle,
    search_focus_domain: FocusHandle,
    candidate_focus_handles: HashMap<NativeCandidateIdentity, FocusHandle>,
    search_scroll: ScrollHandle,
    search_load_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

struct NativeConnectionSearch {
    request: ConnectionSearchRequest,
    state: NativeConnectionSearchState<CreateFromWireCandidates>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeSearchLoadGeneration(u64);

impl NativeSearchLoadGeneration {
    const FIRST: Self = Self(1);

    fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeSearchProductStatus {
    Loading,
    Retrying,
    Ready,
    SourceError,
    StaleCandidate,
    Uncertain {
        capability: ConnectionSettlementCapability,
        can_retry: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeConnectionSearchProjection {
    Closed,
    Loading,
    Retrying,
    Ready,
    FilteredEmpty,
    SourceError,
    StaleCandidate,
    Committing,
    Uncertain,
    Terminal,
    Cancelled,
    Retired,
    HostClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeCandidateNavigation {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeCandidateSemantics {
    role: Role,
    selected: bool,
    disabled: bool,
    tab_stop: bool,
}

fn native_candidate_semantics(
    active: bool,
    candidate_actions_enabled: bool,
) -> NativeCandidateSemantics {
    NativeCandidateSemantics {
        role: Role::ListBoxOption,
        selected: active,
        disabled: !candidate_actions_enabled,
        tab_stop: active && candidate_actions_enabled,
    }
}

fn native_candidate_commit_target(
    active: Option<&NativeCandidateIdentity>,
    query_focused: bool,
    focused_candidate: Option<&NativeCandidateIdentity>,
) -> Option<NativeCandidateIdentity> {
    let active = active?;
    match focused_candidate {
        Some(focused) if focused == active => Some(focused.clone()),
        Some(_) => None,
        None if query_focused => Some(active.clone()),
        None => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NativeCandidateIdentity {
    kind: String,
    kind_version: u32,
    port: String,
}

impl NativeCandidateIdentity {
    fn from_candidate(candidate: &jellyflow_runtime::schema::CreateFromWireCandidate) -> Self {
        Self {
            kind: candidate.node_kind().0.clone(),
            kind_version: candidate.kind_version(),
            port: candidate.port().0.clone(),
        }
    }
}

struct NativeConnectionSearchState<C> {
    load_generation: NativeSearchLoadGeneration,
    status: NativeSearchProductStatus,
    candidates: Option<C>,
    rows: Vec<NativeCandidateRow>,
    visible: Vec<NativeCandidateIdentity>,
    visible_row_indices: Vec<usize>,
    active: Option<NativeCandidateIdentity>,
    query: String,
    diagnostic: Option<String>,
    commit_started: bool,
}

struct NativeCandidateRow {
    authoritative_index: usize,
    identity: NativeCandidateIdentity,
    selector: String,
    title: String,
    detail: String,
    searchable: String,
}

impl<C> NativeConnectionSearchState<C> {
    fn loading() -> Self {
        Self {
            load_generation: NativeSearchLoadGeneration::FIRST,
            status: NativeSearchProductStatus::Loading,
            candidates: None,
            rows: Vec::new(),
            visible: Vec::new(),
            visible_row_indices: Vec::new(),
            active: None,
            query: String::new(),
            diagnostic: None,
            commit_started: false,
        }
    }

    fn pending_load(&self) -> Option<NativeSearchLoadGeneration> {
        matches!(
            self.status,
            NativeSearchProductStatus::Loading | NativeSearchProductStatus::Retrying
        )
        .then_some(self.load_generation)
    }

    fn begin_retry(&mut self) -> Option<NativeSearchLoadGeneration> {
        if self.commit_started {
            return None;
        }
        let Some(generation) = self.load_generation.next() else {
            self.status = NativeSearchProductStatus::SourceError;
            self.diagnostic = Some("Search retry limit reached".to_owned());
            return None;
        };
        self.load_generation = generation;
        self.status = NativeSearchProductStatus::Retrying;
        self.candidates = None;
        self.rows.clear();
        self.visible.clear();
        self.visible_row_indices.clear();
        self.diagnostic = None;
        Some(generation)
    }

    fn finish_load(
        &mut self,
        generation: NativeSearchLoadGeneration,
        result: Result<(C, Vec<NativeCandidateRow>), String>,
    ) -> bool {
        if self.pending_load() != Some(generation) {
            return false;
        }
        match result {
            Ok((candidates, rows)) => {
                self.candidates = Some(candidates);
                self.rows = rows;
                self.status = NativeSearchProductStatus::Ready;
                self.diagnostic = None;
                self.rebuild_visible_rows();
            }
            Err(message) => {
                self.candidates = None;
                self.rows.clear();
                self.visible.clear();
                self.visible_row_indices.clear();
                self.active = None;
                self.status = NativeSearchProductStatus::SourceError;
                self.diagnostic = Some(message);
            }
        }
        true
    }

    fn set_query(&mut self, query: &str) -> bool {
        if self.commit_started {
            return false;
        }
        let query = query.trim().to_lowercase();
        if self.query == query {
            return false;
        }
        self.query = query;
        self.rebuild_visible_rows();
        true
    }

    fn rebuild_visible_rows(&mut self) {
        let mut visible = Vec::new();
        let mut visible_row_indices = Vec::new();
        for (index, row) in self.rows.iter().enumerate() {
            if self.query.is_empty() || row.searchable.contains(&self.query) {
                visible.push(row.identity.clone());
                visible_row_indices.push(index);
            }
        }
        self.visible = visible;
        self.visible_row_indices = visible_row_indices;
        if self
            .active
            .as_ref()
            .is_none_or(|active| !self.visible.contains(active))
        {
            self.active = self.visible.first().cloned();
        }
    }

    fn visible_row_indices(&self) -> &[usize] {
        &self.visible_row_indices
    }

    fn active_visible_index(&self) -> Option<usize> {
        let active = self.active.as_ref()?;
        self.visible
            .iter()
            .position(|candidate| candidate == active)
    }

    fn focus(&mut self, identity: &NativeCandidateIdentity) -> bool {
        if self.status != NativeSearchProductStatus::Ready
            || self.commit_started
            || self.active.as_ref() == Some(identity)
            || !self.visible.contains(identity)
        {
            return false;
        }
        self.active = Some(identity.clone());
        true
    }

    fn navigate(&mut self, navigation: NativeCandidateNavigation) -> bool {
        if self.status != NativeSearchProductStatus::Ready
            || self.commit_started
            || self.visible.is_empty()
        {
            return false;
        }
        let next = self
            .active
            .as_ref()
            .and_then(|active| {
                self.visible
                    .iter()
                    .position(|candidate| candidate == active)
            })
            .map_or(0, |index| match navigation {
                NativeCandidateNavigation::Previous => {
                    index.checked_sub(1).unwrap_or(self.visible.len() - 1)
                }
                NativeCandidateNavigation::Next => (index + 1) % self.visible.len(),
            });
        let candidate = self.visible[next].clone();
        if self.active.as_ref() == Some(&candidate) {
            return false;
        }
        self.active = Some(candidate);
        true
    }

    fn freeze_for_commit(&mut self, identity: &NativeCandidateIdentity) -> bool {
        if self.commit_started
            || self.status != NativeSearchProductStatus::Ready
            || !self.visible.contains(identity)
        {
            return false;
        }
        self.active = Some(identity.clone());
        self.commit_started = true;
        true
    }

    fn mark_source_error(&mut self, message: String) {
        self.commit_started = false;
        self.status = NativeSearchProductStatus::SourceError;
        self.diagnostic = Some(message);
    }

    fn mark_stale_candidate(&mut self, message: String) {
        self.commit_started = false;
        self.status = NativeSearchProductStatus::StaleCandidate;
        self.diagnostic = Some(message);
    }

    fn apply_runtime_feedback(
        &mut self,
        status: Option<ConnectionSearchStatus>,
        terminal: Option<InteractionTerminalReason>,
        settlement: Option<ConnectionSettlementPresentation>,
    ) -> bool {
        let uncertain_can_retry = matches!(
            settlement,
            Some(
                ConnectionSettlementPresentation::Uncertain {
                    can_retry: true,
                    ..
                } | ConnectionSettlementPresentation::Retryable {
                    can_retry: true,
                    ..
                }
            )
        );
        let uncertain_capability = match settlement {
            Some(
                ConnectionSettlementPresentation::Uncertain { capability, .. }
                | ConnectionSettlementPresentation::Retryable { capability, .. },
            ) => Some(capability),
            Some(ConnectionSettlementPresentation::AwaitingPublication) | None => None,
        };
        let diagnostic = match status {
            Some(ConnectionSearchStatus::Error) => {
                Some("Search failed; retry to query the current graph.".to_owned())
            }
            Some(ConnectionSearchStatus::CommitRejected) => Some(match terminal {
                Some(InteractionTerminalReason::Conflict) => {
                    "Graph changed before the connection was applied; retry with refreshed candidates."
                        .to_owned()
                }
                Some(InteractionTerminalReason::TransportFailure) => {
                    "Connection result is unavailable; retry after the command queue recovers."
                        .to_owned()
                }
                _ => "Connection was rejected; retry with refreshed candidates.".to_owned(),
            }),
            Some(ConnectionSearchStatus::Uncertain) => Some(if uncertain_can_retry {
                "The authoritative graph does not contain this connection; retry or cancel."
                    .to_owned()
            } else {
                "Waiting for a later authoritative graph revision before retry is available."
                    .to_owned()
            }),
            Some(
                ConnectionSearchStatus::Loading
                | ConnectionSearchStatus::Ready { .. }
                | ConnectionSearchStatus::Empty,
            )
            | None => None,
        };
        match (status, diagnostic) {
            (Some(ConnectionSearchStatus::Error), Some(diagnostic))
                if self.pending_load().is_none()
                    && self.diagnostic.as_ref() != Some(&diagnostic) =>
            {
                self.mark_source_error(diagnostic);
                true
            }
            (Some(ConnectionSearchStatus::CommitRejected), Some(diagnostic))
                if self.diagnostic.as_ref() != Some(&diagnostic) =>
            {
                if terminal == Some(InteractionTerminalReason::Conflict) {
                    self.mark_stale_candidate(diagnostic);
                } else {
                    self.mark_source_error(diagnostic);
                }
                true
            }
            (Some(ConnectionSearchStatus::Uncertain), Some(diagnostic))
                if self.status
                    != (NativeSearchProductStatus::Uncertain {
                        capability: uncertain_capability
                            .expect("uncertain settlement always carries exact authority"),
                        can_retry: uncertain_can_retry,
                    })
                    || self.diagnostic.as_ref() != Some(&diagnostic) =>
            {
                self.commit_started = false;
                self.status = NativeSearchProductStatus::Uncertain {
                    capability: uncertain_capability
                        .expect("uncertain settlement always carries exact authority"),
                    can_retry: uncertain_can_retry,
                };
                self.diagnostic = Some(diagnostic);
                true
            }
            _ => false,
        }
    }

    fn projection(
        &self,
        phase: CanvasInteractionPhase,
        terminal: Option<InteractionTerminalReason>,
        close_reason: Option<ViewCloseReason>,
    ) -> NativeConnectionSearchProjection {
        native_connection_search_projection(Some(self), phase, terminal, close_reason)
    }
}

fn native_connection_search_projection<C>(
    state: Option<&NativeConnectionSearchState<C>>,
    phase: CanvasInteractionPhase,
    terminal: Option<InteractionTerminalReason>,
    close_reason: Option<ViewCloseReason>,
) -> NativeConnectionSearchProjection {
    if close_reason.is_some() || phase == CanvasInteractionPhase::Closed {
        return NativeConnectionSearchProjection::HostClosed;
    }
    if phase == CanvasInteractionPhase::Committing {
        return match terminal {
            Some(InteractionTerminalReason::Uncertain) => {
                NativeConnectionSearchProjection::Uncertain
            }
            Some(InteractionTerminalReason::Applied | InteractionTerminalReason::Noop) | None => {
                NativeConnectionSearchProjection::Committing
            }
            Some(_) => NativeConnectionSearchProjection::Terminal,
        };
    }
    match terminal {
        Some(InteractionTerminalReason::Cancelled) => {
            return NativeConnectionSearchProjection::Cancelled;
        }
        Some(InteractionTerminalReason::OwnerRetired) => {
            return NativeConnectionSearchProjection::Retired;
        }
        Some(_) => return NativeConnectionSearchProjection::Terminal,
        _ => {}
    }
    let Some(state) = state else {
        return NativeConnectionSearchProjection::Closed;
    };
    if state.commit_started || phase == CanvasInteractionPhase::Committing {
        return NativeConnectionSearchProjection::Committing;
    }
    match state.status {
        NativeSearchProductStatus::Loading => NativeConnectionSearchProjection::Loading,
        NativeSearchProductStatus::Retrying => NativeConnectionSearchProjection::Retrying,
        NativeSearchProductStatus::Ready if state.visible.is_empty() => {
            NativeConnectionSearchProjection::FilteredEmpty
        }
        NativeSearchProductStatus::Ready => NativeConnectionSearchProjection::Ready,
        NativeSearchProductStatus::SourceError => NativeConnectionSearchProjection::SourceError,
        NativeSearchProductStatus::StaleCandidate => {
            NativeConnectionSearchProjection::StaleCandidate
        }
        NativeSearchProductStatus::Uncertain { .. } => NativeConnectionSearchProjection::Uncertain,
    }
}

impl NativeConnectionSearchState<CreateFromWireCandidates> {
    fn begin_commit(
        &mut self,
        identity: &NativeCandidateIdentity,
    ) -> Option<(CreateFromWireCandidates, usize)> {
        if self.commit_started || self.status != NativeSearchProductStatus::Ready {
            return None;
        }
        let row = self
            .rows
            .iter()
            .find(|row| &row.identity == identity && self.visible.contains(identity))?;
        let candidates = self.candidates.as_ref()?;
        let candidate = candidates.candidates().get(row.authoritative_index)?;
        if NativeCandidateIdentity::from_candidate(candidate) != *identity {
            self.mark_stale_candidate(
                "Candidate identity changed; refresh compatible nodes.".to_owned(),
            );
            return None;
        }
        let candidate_index = row.authoritative_index;
        let candidates = candidates.clone();
        if !self.freeze_for_commit(identity) {
            return None;
        }
        Some((candidates, candidate_index))
    }
}

impl NativeGraphSurface {
    /// Builds a surface with the default Workflow/Dify product theme.
    #[must_use]
    pub fn new(view: ViewSession, registry: OpenGpuiNodeRegistry, cx: &mut Context<Self>) -> Self {
        Self::new_with_search_source_and_visual_profile(
            view,
            registry,
            Arc::new(RuntimeConnectionSearchSource),
            crate::nodes::NativeGraphVisualProfile::default(),
            OpenGpuiNavigationConfig::default(),
            InitialViewportPolicy::Preserve,
            cx,
        )
    }

    /// Builds a product surface from an explicitly injected visual profile.
    #[must_use]
    pub fn new_with_visual_profile(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
        visual_profile: crate::nodes::NativeGraphVisualProfile,
        navigation: OpenGpuiNavigationConfig,
        initial_viewport_policy: InitialViewportPolicy,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_search_source_and_visual_profile(
            view,
            registry,
            Arc::new(RuntimeConnectionSearchSource),
            visual_profile,
            navigation,
            initial_viewport_policy,
            cx,
        )
    }

    /// Builds a default Workflow/Dify surface with an application-provided search source.
    #[must_use]
    pub fn new_with_search_source(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
        search_source: Arc<dyn NativeConnectionSearchSource>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_search_source_and_visual_profile(
            view,
            registry,
            search_source,
            crate::nodes::NativeGraphVisualProfile::default(),
            OpenGpuiNavigationConfig::default(),
            InitialViewportPolicy::Preserve,
            cx,
        )
    }

    /// Builds a product surface with explicit search and visual dependencies.
    #[must_use]
    pub fn new_with_search_source_and_visual_profile(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
        search_source: Arc<dyn NativeConnectionSearchSource>,
        visual_profile: crate::nodes::NativeGraphVisualProfile,
        navigation: OpenGpuiNavigationConfig,
        initial_viewport_policy: InitialViewportPolicy,
        cx: &mut Context<Self>,
    ) -> Self {
        let product_catalog = NativeProductComposition::compose(
            view.catalog_owned(),
            native_product_catalog().expect("built-in product catalog must be valid"),
            &registry,
        )
        .expect("native product catalog, semantic catalog, and registry must compose");
        let product_theme = visual_profile.product_theme();
        let (graph_theme, edge_painters) = visual_profile.into_host_parts();
        let surface_focus = cx.focus_handle();
        let search_focus_domain = cx.focus_handle();
        let host_focus = surface_focus.clone();
        let host = cx.new(move |_| {
            OpenGpuiSceneHost::new_measured_surface(view, registry)
                .expect("valid Open GPUI scene host")
                .with_graph_focus_handle(host_focus)
                .with_navigation_config(navigation)
                .with_initial_viewport_policy(initial_viewport_policy)
                .with_graph_theme(graph_theme)
                .with_edge_painter_registry(edge_painters)
        });
        let query_controller = cx.new(TextInputController::new);
        let host_subscription = cx.observe(&host, |surface, _, cx| {
            if surface.sync_host_state(cx) {
                cx.notify();
            }
        });
        let query_subscription = cx.observe(&query_controller, |surface, _, cx| {
            let query = surface.query_controller.read(cx).value().to_string();
            let changed = surface
                .search
                .as_mut()
                .is_some_and(|search| search.state.set_query(&query));
            if changed {
                let index = surface
                    .search
                    .as_ref()
                    .and_then(|search| search.state.active_visible_index())
                    .unwrap_or(0);
                surface.search_scroll.scroll_to_item(index);
                cx.notify();
            }
        });
        Self {
            host,
            product_theme,
            product_catalog,
            search_source,
            query_controller,
            search: None,
            focus_search_on_next_render: false,
            surface_focus,
            search_focus_domain,
            candidate_focus_handles: HashMap::new(),
            search_scroll: ScrollHandle::new(),
            search_load_task: None,
            _subscriptions: vec![host_subscription, query_subscription],
        }
    }

    #[must_use]
    pub fn host(&self) -> &Entity<OpenGpuiSceneHost> {
        &self.host
    }

    #[must_use]
    pub fn graph_has_focus(&self, window: &Window, cx: &open_gpui::App) -> bool {
        self.surface_focus.contains_focused(window, cx)
    }

    #[must_use]
    pub fn connection_search_has_focus(&self, window: &Window, cx: &open_gpui::App) -> bool {
        self.search_focus_domain.contains_focused(window, cx)
    }

    fn sync_host_state(&mut self, cx: &mut Context<Self>) -> bool {
        self.sync_search(cx)
    }

    fn sync_search(&mut self, cx: &mut Context<Self>) -> bool {
        let host = self.host.read(cx);
        let request = host.connection_search_request();
        let presentation = host.interaction_presentation();
        let phase = presentation.phase();
        let terminal = presentation.terminal();
        let connection = presentation.connection();
        let search_status = connection.and_then(|connection| connection.search());
        let settlement = presentation.connection_settlement();
        match request {
            Some(request)
                if self
                    .search
                    .as_ref()
                    .is_none_or(|search| search.request != request) =>
            {
                self.search_load_task = None;
                self.candidate_focus_handles.clear();
                self.query_controller
                    .update(cx, |controller, cx| controller.set_value("", cx));
                let mut state = NativeConnectionSearchState::loading();
                state.apply_runtime_feedback(search_status, terminal, settlement);
                self.search = Some(NativeConnectionSearch { request, state });
                self.focus_search_on_next_render = true;
                true
            }
            None if phase == CanvasInteractionPhase::Committing && self.search.is_some() => {
                self.search.as_mut().is_some_and(|search| {
                    search
                        .state
                        .apply_runtime_feedback(search_status, terminal, settlement)
                })
            }
            None => {
                if self.search.take().is_some() {
                    self.search_load_task = None;
                    self.candidate_focus_handles.clear();
                    self.focus_search_on_next_render = false;
                    self.query_controller
                        .update(cx, |controller, cx| controller.set_value("", cx));
                    true
                } else {
                    false
                }
            }
            Some(_) => {
                let Some(search) = self.search.as_mut() else {
                    return false;
                };
                search
                    .state
                    .apply_runtime_feedback(search_status, terminal, settlement)
            }
        }
    }

    fn query_search_candidates(
        &self,
        request: &ConnectionSearchRequest,
        generation: NativeSearchLoadGeneration,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>> {
        let search_is_current = self.search.as_ref().is_some_and(|search| {
            search.request.eq(request) && search.state.pending_load() == Some(generation)
        });
        if !search_is_current
            || self.host.read(cx).connection_search_request().as_ref() != Some(request)
        {
            return None;
        }
        Some(
            self.host
                .update(cx, |host, cx| {
                    host.connection_search_candidates(request, window, cx)
                })
                .map_err(|error| NativeConnectionSearchSourceError::new(error.to_string())),
        )
    }

    fn publish_search_result(
        &mut self,
        request: &ConnectionSearchRequest,
        generation: NativeSearchLoadGeneration,
        result: Result<CreateFromWireCandidates, NativeConnectionSearchSourceError>,
        cx: &mut Context<Self>,
    ) {
        let search_is_current = self.search.as_ref().is_some_and(|search| {
            search.request.eq(request) && search.state.pending_load() == Some(generation)
        });
        if !search_is_current
            || self.host.read(cx).connection_search_request().as_ref() != Some(request)
        {
            return;
        }
        self.search_load_task = None;
        let result = result
            .map(|candidates| {
                let rows = Self::candidate_rows(&self.product_catalog, &candidates);
                (candidates, rows)
            })
            .map_err(|error| format!("Search failed: {error}"));
        let changed = self
            .search
            .as_mut()
            .filter(|search| search.request.eq(request))
            .is_some_and(|search| search.state.finish_load(generation, result));
        if changed {
            self.reconcile_candidate_focus_handles(cx);
            let index = self
                .search
                .as_ref()
                .and_then(|search| search.state.active_visible_index())
                .unwrap_or(0);
            self.search_scroll.scroll_to_item(index);
            cx.notify();
        }
    }

    fn schedule_search_load(
        &mut self,
        request: ConnectionSearchRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self
            .search
            .as_ref()
            .filter(|search| search.request.eq(&request))
            .and_then(|search| search.state.pending_load())
        else {
            return;
        };
        if self.search_load_task.is_some() {
            return;
        }
        let window_handle = window.window_handle();
        let source = Arc::clone(&self.search_source);
        self.search_load_task = Some(cx.spawn(async move |surface, cx| {
            cx.background_executor().timer(Duration::ZERO).await;
            let Some(authoritative) = cx.update(|cx| {
                let Ok(query) = window_handle.update(cx, |_, window, cx| {
                    surface.update(cx, |surface, cx| {
                        surface.query_search_candidates(&request, generation, window, cx)
                    })
                }) else {
                    return None;
                };
                query.ok().flatten()
            }) else {
                return;
            };
            let result = match authoritative {
                Ok(authoritative) => {
                    let source_request = request.clone();
                    cx.background_executor()
                        .spawn(async move {
                            match AssertUnwindSafe(async move {
                                source.candidates(source_request, authoritative).await
                            })
                            .catch_unwind()
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => Err(NativeConnectionSearchSourceError::new(
                                    "connection search source panicked",
                                )),
                            }
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            cx.update(|cx| {
                surface
                    .update(cx, |surface, cx| {
                        surface.publish_search_result(&request, generation, result, cx);
                    })
                    .ok();
            });
        }));
    }

    fn candidate_rows(
        product_catalog: &NativeProductComposition,
        candidates: &CreateFromWireCandidates,
    ) -> Vec<NativeCandidateRow> {
        product_catalog
            .project_candidates(candidates.candidates())
            .into_iter()
            .map(|candidate| {
                let identity = NativeCandidateIdentity {
                    kind: candidate.kind().0.clone(),
                    kind_version: candidate.kind_version(),
                    port: candidate.port().0.clone(),
                };
                NativeCandidateRow {
                    authoritative_index: candidate.authoritative_index(),
                    identity,
                    selector: native_connection_search_candidate_selector(
                        &candidate.kind().0,
                        &candidate.port().0,
                    ),
                    title: candidate.title().to_owned(),
                    detail: format!("{} / {}", candidate.kind().0, candidate.port().0),
                    searchable: format!(
                        "{} {} {} {} {}",
                        candidate.title(),
                        candidate.kind().0,
                        candidate.port().0,
                        candidate
                            .category()
                            .iter()
                            .map(AsRef::as_ref)
                            .collect::<Vec<&str>>()
                            .join(" "),
                        candidate
                            .keywords()
                            .iter()
                            .map(AsRef::as_ref)
                            .collect::<Vec<&str>>()
                            .join(" ")
                    )
                    .to_lowercase(),
                }
            })
            .collect()
    }

    fn retry_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.search.as_ref().map(|search| search.request.clone()) else {
            return;
        };
        if let Some((capability, can_retry)) = self.search.as_ref().and_then(|search| match search
            .state
            .status
        {
            NativeSearchProductStatus::Uncertain {
                capability,
                can_retry,
            } => Some((capability, can_retry)),
            _ => None,
        }) {
            if !can_retry {
                return;
            }
            let result = self.host.update(cx, |host, cx| {
                host.retry_connection_settlement(capability, window, cx)
            });
            if let Some(search) = self.search.as_mut() {
                match result {
                    Ok(()) => {
                        search.state.status = NativeSearchProductStatus::Loading;
                        search.state.diagnostic = None;
                        search.state.commit_started = true;
                    }
                    Err(error) => {
                        search.state.diagnostic = Some(format!("Connection retry failed: {error}"));
                    }
                }
            }
            cx.notify();
            return;
        }
        if self
            .search
            .as_ref()
            .is_some_and(|search| search.state.commit_started)
        {
            return;
        }
        let result = self.host.update(cx, |host, cx| {
            host.retry_connection_search(&request, window, cx)
        });
        if let Err(error) = result {
            if let Some(search) = self.search.as_mut() {
                search
                    .state
                    .mark_source_error(format!("Search retry failed: {error}"));
            }
            cx.notify();
            return;
        }
        if self
            .search
            .as_mut()
            .and_then(|search| search.state.begin_retry())
            .is_none()
        {
            return;
        }
        self.candidate_focus_handles.clear();
        self.search_load_task = None;
        self.schedule_search_load(request, window, cx);
    }

    fn commit_candidate(
        &mut self,
        identity: &NativeCandidateIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let presentation = self.host.read(cx).interaction_presentation();
        if presentation.phase() != CanvasInteractionPhase::ConnectionSearch
            || presentation.terminal().is_some()
            || !matches!(
                presentation
                    .connection()
                    .and_then(|connection| connection.search()),
                Some(ConnectionSearchStatus::Ready { .. })
            )
        {
            return;
        }
        let Some((request, candidates, candidate_index)) =
            self.search.as_mut().and_then(|search| {
                search
                    .state
                    .begin_commit(identity)
                    .map(|(candidates, index)| (search.request.clone(), candidates, index))
            })
        else {
            return;
        };
        let Some(candidate) = candidates.candidates().get(candidate_index) else {
            if let Some(search) = self.search.as_mut() {
                search.state.mark_stale_candidate(
                    "Candidate identity changed; refresh compatible nodes.".to_owned(),
                );
            }
            cx.notify();
            return;
        };
        let creation = match self.product_catalog.instantiate(
            candidate.node_kind(),
            candidate.kind_version(),
            request.canvas_position(),
        ) {
            Ok(creation) => creation,
            Err(error) => {
                if let Some(search) = self.search.as_mut() {
                    search.state.mark_stale_candidate(error.to_string());
                }
                cx.notify();
                return;
            }
        };
        let result = self.host.update(cx, |host, cx| {
            let result = host.select_connection_search_candidate(
                &request,
                &candidates,
                candidate_index,
                creation,
                window,
                cx,
            );
            cx.notify();
            result
        });
        match result {
            Ok(_) => {
                cx.notify();
            }
            Err(error) => {
                if let Some(search) = self.search.as_mut() {
                    let message = format!("Connection rejected: {error}");
                    match error {
                        OpenGpuiConnectionSearchError::CandidateNoLongerValid
                        | OpenGpuiConnectionSearchError::CandidateUnavailable { .. }
                        | OpenGpuiConnectionSearchError::CandidateSetMismatch
                        | OpenGpuiConnectionSearchError::CandidateCreationMismatch
                        | OpenGpuiConnectionSearchError::StaleGeneration
                        | OpenGpuiConnectionSearchError::InactiveRequest => {
                            search.state.mark_stale_candidate(message);
                        }
                        OpenGpuiConnectionSearchError::Query(_) => {
                            search.state.mark_source_error(message);
                        }
                        OpenGpuiConnectionSearchError::CandidatePortUnavailable { .. }
                        | OpenGpuiConnectionSearchError::InvalidSelection => {
                            search.state.mark_stale_candidate(message);
                        }
                        OpenGpuiConnectionSearchError::Interaction(_) => {
                            search.state.mark_source_error(message);
                        }
                    }
                }
                cx.notify();
            }
        }
    }

    fn reconcile_candidate_focus_handles(&mut self, cx: &mut Context<Self>) {
        let identities = self
            .search
            .as_ref()
            .map(|search| {
                search
                    .state
                    .rows
                    .iter()
                    .map(|row| row.identity.clone())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        self.candidate_focus_handles
            .retain(|identity, _| identities.contains(identity));
        for identity in identities {
            self.candidate_focus_handles
                .entry(identity)
                .or_insert_with(|| cx.focus_handle());
        }
    }

    fn focused_candidate_identity(&self, window: &Window) -> Option<NativeCandidateIdentity> {
        self.candidate_focus_handles
            .iter()
            .find_map(|(identity, focus)| focus.is_focused(window).then(|| identity.clone()))
    }

    fn candidate_actions_enabled(&self, cx: &open_gpui::App) -> bool {
        let Some(search) = self.search.as_ref() else {
            return false;
        };
        let host = self.host.read(cx);
        let presentation = host.interaction_presentation();
        search.state.projection(
            presentation.phase(),
            presentation.terminal(),
            host.close_reason(),
        ) == NativeConnectionSearchProjection::Ready
    }

    fn focus_candidate(
        &mut self,
        identity: &NativeCandidateIdentity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.candidate_actions_enabled(cx) {
            return false;
        }
        let Some(focus) = self.candidate_focus_handles.get(identity).cloned() else {
            return false;
        };
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        if !search.state.visible.contains(identity) {
            return false;
        }
        let active_changed = search.state.focus(identity);
        let focus_changed = !focus.is_focused(window);
        if focus_changed {
            focus.focus(window, cx);
        }
        if let Some(index) = search.state.active_visible_index() {
            self.search_scroll.scroll_to_item(index);
        }
        if active_changed || focus_changed {
            cx.notify();
        }
        true
    }

    fn commit_active_candidate(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.candidate_actions_enabled(cx) {
            return false;
        }
        let query_focused = self.query_controller.focus_handle(cx).is_focused(window);
        let focused_candidate = self.focused_candidate_identity(window);
        let Some(identity) = native_candidate_commit_target(
            self.search
                .as_ref()
                .and_then(|search| search.state.active.as_ref()),
            query_focused,
            focused_candidate.as_ref(),
        ) else {
            return false;
        };
        self.commit_candidate(&identity, window, cx);
        true
    }

    fn navigate_candidates(
        &mut self,
        navigation: NativeCandidateNavigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.candidate_actions_enabled(cx) {
            return false;
        }
        let query_focused = self.query_controller.focus_handle(cx).is_focused(window);
        let focused_candidate = self.focused_candidate_identity(window);
        if !query_focused && focused_candidate.is_none() {
            return false;
        }
        if query_focused {
            let Some(active) = self
                .search
                .as_ref()
                .and_then(|search| search.state.active.clone())
            else {
                return false;
            };
            return self.focus_candidate(&active, window, cx);
        }

        let Some(focused) = focused_candidate else {
            return false;
        };
        let Some(next) = self.search.as_mut().and_then(|search| {
            search.state.focus(&focused);
            search.state.navigate(navigation);
            search.state.active.clone()
        }) else {
            return false;
        };
        self.focus_candidate(&next, window, cx)
    }

    fn cancel_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(request) = self.search.as_ref().map(|search| search.request.clone()) else {
            return false;
        };
        let presentation = self.host.read(cx).interaction_presentation();
        let phase = presentation.phase();
        let uncertain = match presentation.connection_settlement() {
            Some(
                ConnectionSettlementPresentation::Uncertain { capability, .. }
                | ConnectionSettlementPresentation::Retryable { capability, .. },
            ) => Some(capability),
            Some(ConnectionSettlementPresentation::AwaitingPublication) | None => None,
        };
        let cancellation_error = if let Some(capability) = uncertain {
            self.host
                .update(cx, |host, cx| {
                    host.cancel_connection_settlement(capability, window, cx)
                })
                .err()
                .map(|error| error.to_string())
        } else {
            if phase == CanvasInteractionPhase::Committing
                || self
                    .search
                    .as_ref()
                    .is_some_and(|search| search.state.commit_started)
            {
                return false;
            }
            self.host
                .update(cx, |host, cx| {
                    host.cancel_connection_search(&request, window, cx)
                })
                .err()
                .map(|error| error.to_string())
        };
        if let Some(error) = cancellation_error {
            if let Some(search) = self.search.as_mut() {
                search.state.diagnostic = Some(format!("Connection cancel failed: {error}"));
            }
            cx.notify();
            return false;
        }
        self.close_search(window, cx);
        true
    }

    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search = None;
        self.search_load_task = None;
        self.candidate_focus_handles.clear();
        self.focus_search_on_next_render = false;
        self.query_controller
            .update(cx, |controller, cx| controller.set_value("", cx));
        window.refresh();
        cx.notify();
    }

    fn render_search(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<impl open_gpui::IntoElement + use<>> {
        let theme = self.product_theme;
        let search = self.search.as_ref()?;
        let row_indices = search.state.visible_row_indices();
        let rows_empty = row_indices.is_empty();
        let host = self.host.read(cx);
        let overlay_boundary = host.overlay_boundary();
        let presentation = host.interaction_presentation();
        let projection = search.state.projection(
            presentation.phase(),
            presentation.terminal(),
            host.close_reason(),
        );
        let committing = projection == NativeConnectionSearchProjection::Committing;
        let commit_locked = matches!(
            projection,
            NativeConnectionSearchProjection::Committing
                | NativeConnectionSearchProjection::Uncertain
        );
        let candidate_actions_enabled = projection == NativeConnectionSearchProjection::Ready;
        let surface_size = host.surface_size().unwrap_or_else(|| {
            let fallback = host.frame_bounds().size;
            ViewSize::new(fallback.width, fallback.height)
        });
        let anchor = search.request.view_position();
        let surface = cx.weak_entity();
        let backdrop_surface = surface.clone();
        let retry_surface = surface.clone();
        let diagnostic = search.state.diagnostic.clone();
        let show_retry = match projection {
            NativeConnectionSearchProjection::Uncertain => matches!(
                search.state.status,
                NativeSearchProductStatus::Uncertain {
                    can_retry: true,
                    ..
                }
            ),
            NativeConnectionSearchProjection::SourceError
            | NativeConnectionSearchProjection::StaleCandidate
            | NativeConnectionSearchProjection::Terminal => !committing,
            _ => false,
        };
        let layout =
            NativeConnectionSearchLayout::resolve(surface_size, anchor, diagnostic.is_some());
        let mut results = div()
            .id(NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR)
            .debug_selector(|| NATIVE_CONNECTION_SEARCH_RESULTS_SELECTOR.to_owned())
            .role(NATIVE_CONNECTION_SEARCH_RESULTS_ROLE)
            .aria_label("Compatible nodes")
            .min_h(px(0.0))
            .max_h(px(layout.results_max_height))
            .flex()
            .flex_col()
            .gap(px(theme.spacing.sm))
            .overflow_y_scroll()
            .track_scroll(&self.search_scroll)
            .scrollbar_width(px(7.0));
        for &row_index in row_indices {
            let row = &search.state.rows[row_index];
            let candidate_surface = surface.clone();
            let candidate_focus_surface = surface.clone();
            let candidate_a11y_surface = surface.clone();
            let identity = row.identity.clone();
            let focus_identity = identity.clone();
            let a11y_identity = identity.clone();
            let active = search.state.active.as_ref() == Some(&identity);
            let semantics = native_candidate_semantics(active, candidate_actions_enabled);
            let candidate_focus = self
                .candidate_focus_handles
                .get(&identity)
                .expect("candidate focus handles are reconciled before rendering")
                .clone();
            let selector = row.selector.clone();
            let debug_selector = selector.clone();
            let accessible_label = format!("{}; {}", row.title, row.detail);
            results = results.child(
                div()
                    .id(selector)
                    .debug_selector(move || debug_selector)
                    .min_w(px(0.0))
                    .w_full()
                    .min_h(px(42.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .flex()
                    .items_center()
                    .gap(px(theme.spacing.md))
                    .rounded(px(theme.radius.control))
                    .border_1()
                    .border_color(theme.border.subtle)
                    .role(semantics.role)
                    .aria_label(accessible_label)
                    .aria_selected(semantics.selected)
                    .aria_disabled(semantics.disabled)
                    .track_focus(&candidate_focus)
                    .tab_stop(semantics.tab_stop)
                    .focus_visible(move |style| style.border_color(theme.selection.focus))
                    .when(active, |row| {
                        row.bg(theme.selection.surface)
                            .border_color(theme.selection.border)
                    })
                    .when(!candidate_actions_enabled, |row| {
                        row.opacity(0.56).cursor_not_allowed()
                    })
                    .when(candidate_actions_enabled, |row| {
                        row.cursor_pointer()
                            .hover(move |style| style.bg(theme.control.hover))
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                let _ = candidate_surface.update(cx, |surface, cx| {
                                    surface.commit_candidate(&identity, window, cx)
                                });
                            })
                    })
                    .on_a11y_action(open_gpui::accesskit::Action::Focus, move |_, window, cx| {
                        let _ = candidate_focus_surface.update(cx, |surface, cx| {
                            surface.focus_candidate(&focus_identity, window, cx);
                        });
                    })
                    .on_a11y_action(open_gpui::accesskit::Action::Click, move |_, window, cx| {
                        let _ = candidate_a11y_surface.update(cx, |surface, cx| {
                            surface.commit_candidate(&a11y_identity, window, cx);
                        });
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .child(row.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(112.0))
                            .truncate()
                            .text_xs()
                            .text_color(theme.text.muted)
                            .child(row.detail.clone()),
                    ),
            );
        }
        if matches!(
            projection,
            NativeConnectionSearchProjection::Loading
                | NativeConnectionSearchProjection::Retrying
                | NativeConnectionSearchProjection::Committing
        ) {
            results = results.child(
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_LOADING_SELECTOR.to_owned())
                    .py(px(16.0))
                    .text_sm()
                    .text_color(theme.text.muted)
                    .child(match projection {
                        NativeConnectionSearchProjection::Committing => "Applying connection...",
                        NativeConnectionSearchProjection::Retrying => {
                            "Refreshing compatible nodes..."
                        }
                        _ => "Loading compatible nodes...",
                    }),
            );
        } else if rows_empty {
            results = results.child(
                div()
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_EMPTY_SELECTOR.to_owned())
                    .py(px(16.0))
                    .text_sm()
                    .text_color(theme.text.muted)
                    .child(if search.state.query.is_empty() {
                        "No compatible nodes"
                    } else {
                        "No matching nodes"
                    }),
            );
        }
        let panel = div()
            .debug_selector(|| NATIVE_CONNECTION_SEARCH_SELECTOR.to_owned())
            .w(px(layout.width))
            .max_h(px(layout.max_height))
            .p(px(theme.spacing.lg))
            .flex()
            .flex_col()
            .gap(px(theme.spacing.md))
            .rounded(px(theme.radius.panel))
            .border_1()
            .border_color(theme.overlay.border)
            .bg(theme.overlay.surface)
            .text_color(theme.text.primary)
            .track_focus(&self.search_focus_domain)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().text_sm().child("Add connected node"))
            .child(
                div()
                    .debug_selector(|| native_connection_search_input_selector().to_owned())
                    .child(
                        TextInput::new("jellyflow-native-connection-search-filter", "Filter nodes")
                            .controller(self.query_controller.clone())
                            .placeholder("Search compatible nodes")
                            .disabled(commit_locked),
                    ),
            )
            .child(results)
            .children(diagnostic.map(|message| {
                div()
                    .id(NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR)
                    .debug_selector(|| NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_SELECTOR.to_owned())
                    .p(px(8.0))
                    .max_h(px(NATIVE_CONNECTION_SEARCH_DIAGNOSTIC_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .scrollbar_width(px(7.0))
                    .rounded(px(theme.radius.control))
                    .bg(theme.overlay.diagnostic_surface)
                    .text_xs()
                    .text_color(theme.status.danger.text)
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
                    cx.stop_propagation();
                    let _ = backdrop_surface
                        .update(cx, |surface, cx| surface.cancel_search(window, cx));
                })
                .child(
                    div()
                        .absolute()
                        .left(px(layout.left))
                        .top(px(layout.top))
                        .child(
                            overlay_boundary.wrap_focus_domain(&self.search_focus_domain, panel),
                        ),
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
            .filter(|search| search.state.pending_load().is_some())
            .map(|search| search.request.clone())
        {
            self.schedule_search_load(request, window, cx);
        }
        if self.focus_search_on_next_render && self.search.is_some() {
            let focus = self.query_controller.focus_handle(cx);
            if focus.is_focused(window) {
                self.focus_search_on_next_render = false;
            } else {
                focus.focus(window, cx);
                if focus.is_focused(window) {
                    self.focus_search_on_next_render = false;
                } else {
                    window.defer(cx, move |window, cx| focus.focus(window, cx));
                }
            }
        }
        let search = self.render_search(cx);
        div()
            .debug_selector(|| NATIVE_GRAPH_SURFACE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .on_key_down(cx.listener(|surface, event: &KeyDownEvent, window, cx| {
                if surface.search.is_none()
                    || event.keystroke.modifiers.modified()
                    || event.keystroke.is_ime_in_progress()
                    || surface
                        .query_controller
                        .read(cx)
                        .marked_range_utf16()
                        .is_some()
                {
                    return;
                }
                let key = event.keystroke.key.as_str();
                let handled = if key.eq_ignore_ascii_case("escape") {
                    surface.cancel_search(window, cx)
                } else if key.eq_ignore_ascii_case("up") {
                    surface.navigate_candidates(NativeCandidateNavigation::Previous, window, cx)
                } else if key.eq_ignore_ascii_case("down") {
                    surface.navigate_candidates(NativeCandidateNavigation::Next, window, cx)
                } else if key.eq_ignore_ascii_case("enter") {
                    surface.commit_active_candidate(window, cx)
                } else {
                    false
                };
                if handled {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(div().absolute().size_full().child(self.host.clone()))
            .children(search)
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

#[cfg(test)]
mod tests {
    use jellyflow_runtime::geometry::{ViewPoint, ViewSize};
    use open_gpui::{ParentElement, Render, Styled, VisualContext};

    use super::*;

    fn candidate_identity(kind: &str) -> NativeCandidateIdentity {
        NativeCandidateIdentity {
            kind: kind.to_owned(),
            kind_version: 1,
            port: "value".to_owned(),
        }
    }

    fn candidate_row(kind: &str, searchable: &str, index: usize) -> NativeCandidateRow {
        NativeCandidateRow {
            authoritative_index: index,
            identity: candidate_identity(kind),
            selector: format!("candidate-{kind}"),
            title: kind.to_owned(),
            detail: format!("{kind} / value"),
            searchable: searchable.to_owned(),
        }
    }

    fn ready_search_state() -> NativeConnectionSearchState<()> {
        let mut state = NativeConnectionSearchState::loading();
        let generation = state.pending_load().expect("initial load generation");
        assert!(state.finish_load(
            generation,
            Ok((
                (),
                vec![
                    candidate_row("alpha", "alpha math", 0),
                    candidate_row("beta", "beta texture", 1),
                    candidate_row("gamma", "gamma output", 2),
                ],
            )),
        ));
        state
    }

    #[test]
    fn candidate_filter_opens_on_first_and_preserves_stable_identity() {
        let mut state = ready_search_state();
        assert_eq!(state.active, Some(candidate_identity("alpha")));

        assert!(state.focus(&candidate_identity("beta")));
        assert!(state.set_query("texture"));
        assert_eq!(state.active, Some(candidate_identity("beta")));
        assert_eq!(state.visible_row_indices(), &[1]);

        assert!(state.set_query("output"));
        assert_eq!(state.active, Some(candidate_identity("gamma")));

        assert!(state.set_query("missing"));
        assert!(state.visible.is_empty());
        assert_eq!(state.active, None);

        assert!(state.set_query(""));
        assert_eq!(state.active, Some(candidate_identity("alpha")));
    }

    #[test]
    fn refresh_preserves_a_still_available_active_identity() {
        let mut state = ready_search_state();
        assert!(state.focus(&candidate_identity("beta")));
        state.mark_stale_candidate("refresh".to_owned());
        let generation = state.begin_retry().expect("refresh generation");

        assert!(state.finish_load(
            generation,
            Ok((
                (),
                vec![
                    candidate_row("gamma", "gamma output", 0),
                    candidate_row("beta", "beta texture", 1),
                ],
            )),
        ));
        assert_eq!(state.active, Some(candidate_identity("beta")));
    }

    #[test]
    fn focus_and_keyboard_navigation_share_active_identity_and_wrap() {
        let mut state = ready_search_state();

        assert!(state.navigate(NativeCandidateNavigation::Previous));
        assert_eq!(state.active, Some(candidate_identity("gamma")));
        assert!(state.navigate(NativeCandidateNavigation::Next));
        assert_eq!(state.active, Some(candidate_identity("alpha")));
        assert!(state.focus(&candidate_identity("beta")));
        assert_eq!(state.active, Some(candidate_identity("beta")));
    }

    #[test]
    fn candidate_semantics_form_one_roving_listbox_focus_stop() {
        let inactive = native_candidate_semantics(false, true);
        let active = native_candidate_semantics(true, true);
        let disabled = native_candidate_semantics(true, false);

        assert_eq!(NATIVE_CONNECTION_SEARCH_RESULTS_ROLE, Role::ListBox);
        assert_eq!(inactive.role, Role::ListBoxOption);
        assert!(!inactive.selected);
        assert!(!inactive.disabled);
        assert!(!inactive.tab_stop);
        assert_eq!(active.role, Role::ListBoxOption);
        assert!(active.selected);
        assert!(!active.disabled);
        assert!(active.tab_stop);
        assert_eq!(disabled.role, Role::ListBoxOption);
        assert!(disabled.selected);
        assert!(disabled.disabled);
        assert!(!disabled.tab_stop);
    }

    #[test]
    fn enter_target_requires_query_authority_or_a_focused_active_match() {
        let active = candidate_identity("alpha");
        let other = candidate_identity("beta");

        assert_eq!(
            native_candidate_commit_target(Some(&active), true, None),
            Some(active.clone()),
        );
        assert_eq!(
            native_candidate_commit_target(Some(&active), false, Some(&active)),
            Some(active.clone()),
        );
        assert_eq!(
            native_candidate_commit_target(Some(&active), false, Some(&other)),
            None,
        );
        assert_eq!(
            native_candidate_commit_target(Some(&active), false, None),
            None,
        );
    }

    #[test]
    fn commit_latch_accepts_once_and_freezes_filter_focus_and_navigation() {
        let mut state = ready_search_state();
        let candidate = candidate_identity("beta");

        assert!(state.freeze_for_commit(&candidate));
        assert!(!state.freeze_for_commit(&candidate));
        assert!(!state.set_query("gamma"));
        assert!(!state.focus(&candidate_identity("gamma")));
        assert!(!state.navigate(NativeCandidateNavigation::Next));
        assert_eq!(state.active, Some(candidate));
    }

    #[test]
    fn completion_from_an_old_load_generation_cannot_replace_retry_results() {
        let mut state = NativeConnectionSearchState::loading();
        let first = state.pending_load().expect("first load generation");
        state.mark_source_error("first query failed".to_owned());
        let retry = state.begin_retry().expect("retry generation");

        assert!(!state.finish_load(first, Ok((11_u8, vec![candidate_row("stale", "stale", 0)])),));
        assert_eq!(state.status, NativeSearchProductStatus::Retrying);
        assert!(state.candidates.is_none());

        assert!(state.finish_load(retry, Ok((22_u8, vec![candidate_row("fresh", "fresh", 0)])),));
        assert_eq!(state.candidates, Some(22));
        assert_eq!(state.active, Some(candidate_identity("fresh")));
    }

    #[test]
    fn product_projection_covers_search_and_host_terminal_states() {
        assert_eq!(
            native_connection_search_projection::<()>(
                None,
                CanvasInteractionPhase::Idle,
                None,
                None,
            ),
            NativeConnectionSearchProjection::Closed,
        );

        let mut state = NativeConnectionSearchState::<()>::loading();
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::Loading,
        );
        state.status = NativeSearchProductStatus::Retrying;
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::Retrying,
        );
        state = ready_search_state();
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::Ready,
        );
        assert!(state.set_query("missing"));
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::FilteredEmpty,
        );
        state.mark_source_error("source".to_owned());
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::SourceError,
        );
        state.mark_stale_candidate("stale".to_owned());
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::StaleCandidate,
        );
        state.commit_started = true;
        assert_eq!(
            state.projection(CanvasInteractionPhase::ConnectionSearch, None, None),
            NativeConnectionSearchProjection::Committing,
        );
        state.commit_started = false;
        assert_eq!(
            state.projection(
                CanvasInteractionPhase::ConnectionSearch,
                Some(InteractionTerminalReason::Conflict),
                None,
            ),
            NativeConnectionSearchProjection::Terminal,
        );
        assert_eq!(
            state.projection(
                CanvasInteractionPhase::ConnectionSearch,
                Some(InteractionTerminalReason::Cancelled),
                None,
            ),
            NativeConnectionSearchProjection::Cancelled,
        );
        assert_eq!(
            state.projection(
                CanvasInteractionPhase::ConnectionSearch,
                Some(InteractionTerminalReason::OwnerRetired),
                None,
            ),
            NativeConnectionSearchProjection::Retired,
        );
        assert_eq!(
            state.projection(
                CanvasInteractionPhase::Closed,
                Some(InteractionTerminalReason::ViewClosed),
                Some(ViewCloseReason::HostClosed),
            ),
            NativeConnectionSearchProjection::HostClosed,
        );
    }

    #[test]
    fn compact_product_surface_clamps_search_to_the_twelve_pixel_safe_area() {
        let layout = NativeConnectionSearchLayout::resolve(
            ViewSize::new(680.0, 640.0),
            ViewPoint::new(670.0, 630.0),
            true,
        );

        assert_eq!(layout.width, 320.0);
        assert_eq!(layout.max_height, 480.0);
        assert_eq!(layout.results_max_height, 264.0);
        assert_eq!(layout.left, 348.0);
        assert_eq!(layout.top, 148.0);
        assert!(layout.left + layout.width <= 680.0 - NATIVE_CONNECTION_SEARCH_SAFE_INSET);
        assert!(layout.top + layout.max_height <= 640.0 - NATIVE_CONNECTION_SEARCH_SAFE_INSET);
    }

    #[test]
    fn narrower_surfaces_shrink_the_panel_and_keep_a_bounded_results_viewport() {
        let layout = NativeConnectionSearchLayout::resolve(
            ViewSize::new(260.0, 300.0),
            ViewPoint::new(f32::INFINITY, f32::NAN),
            true,
        );

        assert_eq!(layout.left, 12.0);
        assert_eq!(layout.top, 12.0);
        assert_eq!(layout.width, 236.0);
        assert_eq!(layout.max_height, 276.0);
        assert_eq!(layout.results_max_height, 60.0);
    }

    struct FocusRestorationFixture {
        surface: FocusHandle,
        hidden_search_input: FocusHandle,
    }

    impl Render for FocusRestorationFixture {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl open_gpui::IntoElement {
            div()
                .size_full()
                .track_focus(&self.surface)
                .child(div().size_full().track_focus(&self.hidden_search_input))
        }
    }

    #[open_gpui::test]
    fn graph_focus_domain_contains_descendant_focus(cx: &mut open_gpui::TestAppContext) {
        let (fixture, cx) = cx.add_window_view(|_, cx| FocusRestorationFixture {
            surface: cx.focus_handle(),
            hidden_search_input: cx.focus_handle(),
        });
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });

        cx.update_window_entity(&fixture, |fixture, window, cx| {
            window.focus(&fixture.hidden_search_input, cx);

            assert!(fixture.surface.contains_focused(window, cx));
            assert!(!fixture.surface.is_focused(window));
        });
    }
}
