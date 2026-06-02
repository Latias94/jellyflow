use super::super::fixtures::make_graph;

use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceSuite,
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange, run_conformance_scenario,
    run_conformance_suite,
};
use crate::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd,
    ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::viewport::{ViewportPanRequest, ViewportZoomRequest};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeKind};

mod scenario;
mod suite;
mod viewport;
