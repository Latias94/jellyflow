use super::super::fixtures::make_graph;
use super::support::{assert_conformance_trace, insert_input_port};

use crate::io::{NodeGraphPanInertiaTuning, NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode};
use crate::rules::EdgeEndpoint;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceConnectEdgeSessionContract,
    ConformanceNodeDragSessionContract, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent, ConformanceViewChange,
};
use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionHandleConnection, ConnectionHandleRef, ConnectionHandleValidity,
    ConnectionTargetCandidate, ConnectionTargetFromHandlesInput, ConnectionTargetHandle,
    ConnectionTargetInput, RECONNECT_EDGE_TRANSACTION_LABEL, ReconnectEdgeRequest,
    ResolvedConnectionTarget,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeGraphGestureEvent,
};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use crate::runtime::resize::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeDirection, NodeResizeRequest,
};
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationOptions, ViewportAnimationPlan,
    ViewportAnimationRequest, ViewportDoubleClickZoomInput, ViewportDragPanInput,
    ViewportGestureContext, ViewportGestureRejection, ViewportPanInertiaRequest,
    ViewportPanRequest, ViewportPointerButton, ViewportScrollInput, ViewportTransform,
    ViewportZoomRequest, plan_viewport_pan_inertia,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, EdgeKind, Group, GroupId, NodeExtent, NodeId,
    PortDirection,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::EdgeEndpoints;

mod connection;
mod node_drag;
mod node_resize;
mod viewport;
