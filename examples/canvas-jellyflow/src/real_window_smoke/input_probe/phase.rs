#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ProbePhase {
    #[default]
    Fit,
    FitWait,
    CaptureZoom,
    CaptureZoomWait,
    Capture,
    PlanInput,
    NodePress,
    NodeMove,
    NodeWaitActive,
    NodeAutoPanMove,
    NodeAutoPanWait,
    NodeReleaseFrameWait,
    NodeRelease,
    NodeAuthoritativeFrameWait,
    NodeWaitSettled,
    TabForward,
    TabForwardWait,
    TabBackward,
    TabBackwardWait,
    PanPress,
    PanMove,
    PanRelease,
    PanWait,
    Zoom,
    ZoomWait,
    ZoomRestore,
    ZoomRestoreWait,
    FirstEdgePress,
    FirstEdgeRelease,
    FirstEdgeWaitSelected,
    FirstEdgeDelete,
    FirstEdgeWaitDeleted,
    ConnectPress,
    ConnectAutoPanMove,
    ConnectAutoPanWait,
    ConnectMaterializeTarget,
    ConnectWaitCandidate,
    ConnectRelease,
    ConnectVerifyReleaseFrame,
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
    ReconnectAutoPanMove,
    ReconnectAutoPanWait,
    ReconnectMaterializeTarget,
    ReconnectWaitCandidate,
    ReconnectRelease,
    ReconnectVerifyReleaseFrame,
    ReconnectWaitCommitted,
    SearchPress,
    SearchMove,
    SearchWaitDrag,
    SearchRelease,
    SearchWaitOpen,
    SearchEscape,
    SearchWaitClosed,
    FinalWait,
    LodSet,
    LodWait,
    LodCapture,
    Complete,
}

impl ProbePhase {
    pub(super) fn timeout_reason(self) -> &'static str {
        match self {
            Self::CaptureZoomWait => "input_capture_zoom_timeout",
            Self::FitWait => "input_fit_timeout",
            Self::NodeWaitActive => "input_node_activation_timeout",
            Self::NodeAutoPanWait => "input_node_autopan_timeout",
            Self::NodeReleaseFrameWait => "input_node_release_frame_timeout",
            Self::NodeAuthoritativeFrameWait => "input_node_authoritative_frame_timeout",
            Self::NodeWaitSettled => "input_node_settlement_timeout",
            Self::TabForwardWait => "input_tab_focus_timeout",
            Self::TabBackwardWait => "input_shift_tab_focus_timeout",
            Self::PanWait => "input_pan_timeout",
            Self::ZoomWait => "input_zoom_timeout",
            Self::ZoomRestoreWait => "input_zoom_restore_timeout",
            Self::FirstEdgeWaitSelected => "input_first_edge_selection_timeout",
            Self::SecondEdgeWaitSelected => "input_second_edge_selection_timeout",
            Self::FirstEdgeWaitDeleted | Self::SecondEdgeWaitDeleted => "input_edge_delete_timeout",
            Self::ConnectWaitCandidate => "input_connection_candidate_timeout",
            Self::ConnectAutoPanWait => "input_connection_autopan_timeout",
            Self::ConnectMaterializeTarget => "input_connection_target_materialization_timeout",
            Self::ConnectWaitCommitted => "input_connection_commit_timeout",
            Self::ConnectVerifyReleaseFrame => "paint_connection_release_discontinuity",
            Self::ReconnectEdgeWaitSelected => "input_reconnect_selection_timeout",
            Self::ReconnectAutoPanWait => "input_reconnect_autopan_timeout",
            Self::ReconnectMaterializeTarget => "input_reconnect_target_materialization_timeout",
            Self::ReconnectWaitCandidate => "input_reconnect_candidate_timeout",
            Self::ReconnectWaitCommitted => "input_reconnect_commit_timeout",
            Self::ReconnectVerifyReleaseFrame => "paint_reconnect_release_discontinuity",
            Self::SearchWaitDrag => "input_search_drag_timeout",
            Self::SearchWaitOpen => "input_search_open_timeout",
            Self::SearchWaitClosed => "input_search_close_timeout",
            Self::FinalWait => "input_final_frame_timeout",
            Self::LodWait => "input_lod_sample_timeout",
            Self::CaptureZoom
            | Self::Capture
            | Self::Fit
            | Self::PlanInput
            | Self::NodePress
            | Self::NodeMove
            | Self::NodeAutoPanMove
            | Self::NodeRelease
            | Self::TabForward
            | Self::TabBackward
            | Self::PanPress
            | Self::PanMove
            | Self::PanRelease
            | Self::Zoom
            | Self::ZoomRestore
            | Self::FirstEdgePress
            | Self::FirstEdgeRelease
            | Self::FirstEdgeDelete
            | Self::ConnectPress
            | Self::ConnectAutoPanMove
            | Self::ConnectRelease
            | Self::SecondEdgePress
            | Self::SecondEdgeRelease
            | Self::SecondEdgeDelete
            | Self::ReconnectEdgePress
            | Self::ReconnectEdgeRelease
            | Self::ReconnectPress
            | Self::ReconnectAutoPanMove
            | Self::ReconnectRelease
            | Self::SearchPress
            | Self::SearchMove
            | Self::SearchRelease
            | Self::SearchEscape
            | Self::LodSet
            | Self::LodCapture
            | Self::Complete => "input_phase_timeout",
        }
    }
}
