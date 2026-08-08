use jellyflow_core::core::{CanvasPoint, EdgeId, GraphId, GroupId, NodeId};
use serde::{Deserialize, Serialize};

use crate::io::view_state::{ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed};

use super::{VIEW_SESSION_SEED_FILE_VERSION, ViewSessionSeedFileError};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewSessionSeedWireV1 {
    graph_id: GraphId,
    view_session_seed_version: u32,
    viewport: ViewportWireV1,
    selection: SelectionWireV1,
    stacking: StackingWireV1,
}

impl ViewSessionSeedWireV1 {
    pub(super) fn from_seed(seed: &ViewSessionSeed) -> Self {
        Self {
            graph_id: seed.graph_id(),
            view_session_seed_version: VIEW_SESSION_SEED_FILE_VERSION,
            viewport: ViewportWireV1::from_seed(seed.viewport()),
            selection: SelectionWireV1::from_seed(seed.selection()),
            stacking: StackingWireV1::from_seed(seed.stacking()),
        }
    }

    pub(super) fn into_seed(self) -> Result<ViewSessionSeed, ViewSessionSeedFileError> {
        if self.view_session_seed_version != VIEW_SESSION_SEED_FILE_VERSION {
            return Err(ViewSessionSeedFileError::UnsupportedVersion {
                version: self.view_session_seed_version,
                expected: VIEW_SESSION_SEED_FILE_VERSION,
            });
        }
        Ok(ViewSessionSeed::from_parts(
            self.graph_id,
            self.viewport.into_seed(),
            self.selection.into_seed(),
            self.stacking.into_seed(),
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ViewportWireV1 {
    pan: CanvasPointWireV1,
    zoom: f32,
}

impl ViewportWireV1 {
    fn from_seed(seed: ViewportSeed) -> Self {
        Self {
            pan: CanvasPointWireV1::from_point(seed.pan()),
            zoom: seed.zoom(),
        }
    }

    pub(super) fn into_seed(self) -> ViewportSeed {
        ViewportSeed::new(self.pan.into_point(), self.zoom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CanvasPointWireV1 {
    x: f32,
    y: f32,
}

impl CanvasPointWireV1 {
    fn from_point(point: CanvasPoint) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }

    pub(super) fn into_point(self) -> CanvasPoint {
        CanvasPoint {
            x: self.x,
            y: self.y,
        }
    }
}

impl Default for CanvasPointWireV1 {
    fn default() -> Self {
        Self::from_point(CanvasPoint::default())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionWireV1 {
    nodes: Vec<NodeId>,
    edges: Vec<EdgeId>,
    groups: Vec<GroupId>,
}

impl SelectionWireV1 {
    fn from_seed(seed: &ViewSelectionSeed) -> Self {
        Self {
            nodes: seed.nodes().to_vec(),
            edges: seed.edges().to_vec(),
            groups: seed.groups().to_vec(),
        }
    }

    fn into_seed(self) -> ViewSelectionSeed {
        ViewSelectionSeed::new(self.nodes, self.edges, self.groups)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StackingWireV1 {
    nodes: Vec<NodeId>,
    edges: Vec<EdgeId>,
    groups: Vec<GroupId>,
}

impl StackingWireV1 {
    fn from_seed(seed: &ViewStackingSeed) -> Self {
        Self {
            nodes: seed.nodes().to_vec(),
            edges: seed.edges().to_vec(),
            groups: seed.groups().to_vec(),
        }
    }

    fn into_seed(self) -> ViewStackingSeed {
        ViewStackingSeed::new(self.nodes, self.edges, self.groups)
    }
}
