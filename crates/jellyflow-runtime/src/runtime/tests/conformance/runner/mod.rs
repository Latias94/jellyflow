use super::super::fixtures::make_graph;

use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceBehavior, ConformanceCallbackEvent,
    ConformanceEdgeEndpointPosition, ConformanceLayoutEdgePosition,
    ConformanceLayoutFactsConnectionTargetExpectation, ConformanceLayoutFactsContract,
    ConformanceLayoutFactsExpectation, ConformanceNodeDragSessionContract,
    ConformanceNodePointerResizeRequest, ConformanceNodeResizeSessionContract, ConformanceScenario,
    ConformanceSuite, ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange,
    run_conformance_scenario, run_conformance_suite,
};
use crate::runtime::connection::{
    ConnectionHandleConnection, ConnectionHandleRef, ConnectionHandleValidity,
    ConnectionTargetCandidate, ConnectionTargetFromHandlesInput, ConnectionTargetHandle,
    ResolvedConnectionTarget,
};
use crate::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use crate::runtime::drag::{
    NODE_DRAG_TRANSACTION_LABEL, NODE_NUDGE_TRANSACTION_LABEL, NodeNudgeDirection, NodeNudgeRequest,
};
use crate::runtime::events::{
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome,
    NodeResizeStart, NodeResizeUpdate, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome,
    ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use crate::runtime::measurement::{MeasuredHandle, NodeMeasurement};
use crate::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeDirection, NodeResizeRequest,
};
use crate::runtime::selection::{SelectionBoxInput, SelectionBoxOptions};
use crate::runtime::viewport::{
    ViewportAnimationEasing, ViewportAnimationFrame, ViewportAnimationOptions,
    ViewportAnimationPlan, ViewportAnimationRequest, ViewportDoubleClickZoomInput,
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeKind, Group, GroupId, NodeExtent, PortDirection,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

mod scenario;
mod suite;
mod viewport;
