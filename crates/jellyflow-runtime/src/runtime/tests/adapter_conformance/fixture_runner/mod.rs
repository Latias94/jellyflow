use super::super::fixtures::{
    GraphFixtureUpdateExt, fixture_insert_group, fixture_insert_node, make_graph,
};
use super::support::{assert_conformance_trace, insert_input_port};

use crate::io::{NodeGraphPanInertiaTuning, NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode};
use crate::rules::EdgeEndpoint;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceConnectEdgeSessionContract,
    ConformanceDeleteSelectionContract, ConformanceDeleteSelectionDuringNodeDragContract,
    ConformanceLayoutEdgeRouteFacts, ConformanceLayoutFactsExpectation,
    ConformanceNodeDragSessionContract, ConformanceRenderingQueryContract, ConformanceScenario,
    ConformanceTraceEvent, ConformanceViewChange,
};
use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionEndIntent, ConnectionHandleConnection, ConnectionHandleRef,
    ConnectionHandleValidity, ConnectionTargetCandidate, ConnectionTargetFromHandlesInput,
    ConnectionTargetHandle, ConnectionTargetInput, RECONNECT_EDGE_TRANSACTION_LABEL,
    ReconnectEdgeRequest, ResolvedConnectionTarget, resolve_connection_lifecycle,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeGraphGestureEvent,
};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use crate::runtime::measurement::{MeasuredHandle, NodeMeasurement};
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
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeRouteKind, EdgeViewDescriptor,
    Graph, GraphBuilder, GraphId, Group, GroupId, Node, NodeExtent, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

mod connection;
mod edge_route;
mod node_drag;
mod node_resize;
mod viewport;
