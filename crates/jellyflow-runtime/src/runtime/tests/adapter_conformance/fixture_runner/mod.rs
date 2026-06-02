use super::super::fixtures::make_graph;
use super::support::{assert_conformance_trace, insert_input_port};

use crate::io::{NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode};
use crate::rules::EdgeEndpoint;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent, ConformanceViewChange,
};
use crate::runtime::connection::{
    CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeRequest, ConnectionHandleConnection,
    ConnectionHandleRef, ConnectionHandleValidity, ConnectionTargetHandle, ConnectionTargetInput,
    RECONNECT_EDGE_TRANSACTION_LABEL, ReconnectEdgeRequest, ResolvedConnectionTarget,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
};
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationOptions, ViewportAnimationPlan,
    ViewportAnimationRequest, ViewportDoubleClickZoomInput, ViewportDragPanInput,
    ViewportGestureContext, ViewportGestureRejection, ViewportPointerButton, ViewportScrollInput,
    ViewportTransform,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, EdgeKind, PortDirection};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::EdgeEndpoints;

mod connection;
mod node_drag;
mod viewport;
