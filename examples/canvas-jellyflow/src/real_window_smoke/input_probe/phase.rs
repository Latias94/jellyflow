#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProbePhase {
    #[default]
    Fit,
    FitWait,
    CaptureZoom,
    CaptureZoomWait,
    Capture,
    RestoreFit,
    RestoreFitWait,
    PlanInput,
    NodePress,
    NodeMove,
    NodeWaitActive,
    NodeRelease,
    NodeWaitSettled,
    PanPress,
    PanMove,
    PanRelease,
    PanWait,
    Zoom,
    ZoomWait,
    FirstEdgePress,
    FirstEdgeRelease,
    FirstEdgeWaitSelected,
    FirstEdgeDelete,
    FirstEdgeWaitDeleted,
    ConnectPress,
    ConnectMove,
    ConnectWaitCandidate,
    ConnectRelease,
    ConnectWaitCommitted,
    SecondEdgePress,
    SecondEdgeRelease,
    SecondEdgeWaitSelected,
    SecondEdgeDelete,
    SecondEdgeWaitDeleted,
    ReconnectEdgePress,
    ReconnectEdgeRelease,
    ReconnectEdgeWaitSelected,
    ReconnectPress,
    ReconnectMove,
    ReconnectWaitCandidate,
    ReconnectRelease,
    ReconnectWaitCommitted,
    SearchPress,
    SearchMove,
    SearchWaitDrag,
    SearchRelease,
    SearchWaitOpen,
    SearchEscape,
    SearchWaitClosed,
    Complete,
}

impl ProbePhase {
    pub(super) fn timeout_reason(self) -> &'static str {
        match self {
            Self::CaptureZoomWait => "input_capture_zoom_timeout",
            Self::FitWait => "input_fit_timeout",
            Self::RestoreFitWait => "input_restore_fit_timeout",
            Self::NodeWaitActive => "input_node_activation_timeout",
            Self::NodeWaitSettled => "input_node_settlement_timeout",
            Self::PanWait => "input_pan_timeout",
            Self::ZoomWait => "input_zoom_timeout",
            Self::FirstEdgeWaitSelected => "input_first_edge_selection_timeout",
            Self::SecondEdgeWaitSelected => "input_second_edge_selection_timeout",
            Self::FirstEdgeWaitDeleted | Self::SecondEdgeWaitDeleted => "input_edge_delete_timeout",
            Self::ConnectWaitCandidate => "input_connection_candidate_timeout",
            Self::ConnectWaitCommitted => "input_connection_commit_timeout",
            Self::ReconnectEdgeWaitSelected => "input_reconnect_selection_timeout",
            Self::ReconnectWaitCandidate => "input_reconnect_candidate_timeout",
            Self::ReconnectWaitCommitted => "input_reconnect_commit_timeout",
            Self::SearchWaitDrag => "input_search_drag_timeout",
            Self::SearchWaitOpen => "input_search_open_timeout",
            Self::SearchWaitClosed => "input_search_close_timeout",
            Self::CaptureZoom
            | Self::Capture
            | Self::RestoreFit
            | Self::Fit
            | Self::PlanInput
            | Self::NodePress
            | Self::NodeMove
            | Self::NodeRelease
            | Self::PanPress
            | Self::PanMove
            | Self::PanRelease
            | Self::Zoom
            | Self::FirstEdgePress
            | Self::FirstEdgeRelease
            | Self::FirstEdgeDelete
            | Self::ConnectPress
            | Self::ConnectMove
            | Self::ConnectRelease
            | Self::SecondEdgePress
            | Self::SecondEdgeRelease
            | Self::SecondEdgeDelete
            | Self::ReconnectEdgePress
            | Self::ReconnectEdgeRelease
            | Self::ReconnectPress
            | Self::ReconnectMove
            | Self::ReconnectRelease
            | Self::SearchPress
            | Self::SearchMove
            | Self::SearchRelease
            | Self::SearchEscape
            | Self::Complete => "input_phase_timeout",
        }
    }
}
