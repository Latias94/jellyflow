use super::super::fixtures::make_graph;
use super::support::{assert_conformance_trace, insert_input_port};

use crate::io::{NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode};
use crate::rules::plan_connect;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent,
};
use crate::runtime::connection::{
    ConnectionHandleConnection, ConnectionHandleRef, ConnectionHandleValidity,
    ConnectionTargetHandle, ConnectionTargetInput, ResolvedConnectionTarget,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
};
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureRejection, ViewportPointerButton,
    ViewportScrollInput,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeKind, PortDirection};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

mod connection;
mod node_drag;
mod viewport;
