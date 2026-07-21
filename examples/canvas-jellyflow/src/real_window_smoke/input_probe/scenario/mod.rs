mod connect;
mod edge_delete;
mod reconnect;
mod search;
mod setup_node;
mod viewport;

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn advance(
    probe: &mut RealWindowSmokeProbe,
    window: &mut Window,
    cx: &mut App,
    surface: &Entity<NativeGraphSurface>,
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    surface_size: ViewSize,
) -> Result<Option<SmokeInputEvidence>, &'static str> {
    match probe.phase {
        ProbePhase::CaptureZoom
        | ProbePhase::CaptureZoomWait
        | ProbePhase::Capture
        | ProbePhase::Fit
        | ProbePhase::FitWait
        | ProbePhase::RestoreFit
        | ProbePhase::RestoreFitWait
        | ProbePhase::PlanInput
        | ProbePhase::NodePress
        | ProbePhase::NodeMove
        | ProbePhase::NodeWaitActive
        | ProbePhase::NodeRelease
        | ProbePhase::NodeWaitSettled => {
            probe.advance_setup_node(window, cx, surface, view, expectations, surface_size)
        }
        ProbePhase::PanPress
        | ProbePhase::PanMove
        | ProbePhase::PanRelease
        | ProbePhase::PanWait
        | ProbePhase::Zoom
        | ProbePhase::ZoomWait => probe.advance_viewport(window, cx, surface, view, surface_size),
        ProbePhase::FirstEdgePress
        | ProbePhase::FirstEdgeRelease
        | ProbePhase::FirstEdgeWaitSelected
        | ProbePhase::FirstEdgeDelete
        | ProbePhase::FirstEdgeWaitDeleted
        | ProbePhase::SecondEdgePress
        | ProbePhase::SecondEdgeRelease
        | ProbePhase::SecondEdgeWaitSelected
        | ProbePhase::SecondEdgeDelete
        | ProbePhase::SecondEdgeWaitDeleted => probe.advance_edge_delete(window, cx, surface, view),
        ProbePhase::ConnectPress
        | ProbePhase::ConnectMove
        | ProbePhase::ConnectWaitCandidate
        | ProbePhase::ConnectRelease
        | ProbePhase::ConnectWaitCommitted => {
            probe.advance_connect(window, cx, surface, view, surface_size)
        }
        ProbePhase::ReconnectEdgePress
        | ProbePhase::ReconnectEdgeRelease
        | ProbePhase::ReconnectEdgeWaitSelected
        | ProbePhase::ReconnectPress
        | ProbePhase::ReconnectMove
        | ProbePhase::ReconnectWaitCandidate
        | ProbePhase::ReconnectRelease
        | ProbePhase::ReconnectWaitCommitted => {
            probe.advance_reconnect(window, cx, surface, view, surface_size)
        }
        ProbePhase::SearchPress
        | ProbePhase::SearchMove
        | ProbePhase::SearchWaitDrag
        | ProbePhase::SearchRelease
        | ProbePhase::SearchWaitOpen
        | ProbePhase::SearchEscape
        | ProbePhase::SearchWaitClosed => {
            probe.advance_search(window, cx, surface, view, surface_size)
        }
        ProbePhase::Complete => probe.complete(),
    }
}
