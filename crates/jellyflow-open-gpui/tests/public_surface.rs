use std::mem::size_of;

use jellyflow_open_gpui::activity::{OpenGpuiActivityKind, OpenGpuiNodeActivityHandle};
use jellyflow_open_gpui::authoring::{
    OPEN_GPUI_COMMAND_JOURNAL_CAPACITY, OpenGpuiCommandEnqueueError, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketError, OpenGpuiCommandTicketResult, OpenGpuiNodeBinding,
    OpenGpuiNodeDragHandle, OpenGpuiPortAnchor, OpenGpuiPortHandle, OpenGpuiPortSide,
};
use jellyflow_open_gpui::chrome::{
    OpenGpuiEdgeHaloStyle, OpenGpuiEdgeVisualFacts, OpenGpuiPortMarkerShape, OpenGpuiPortTone,
};
use jellyflow_open_gpui::host::{
    OpenGpuiBlankCanvasContextHandler, OpenGpuiBlankCanvasContextRequest,
    OpenGpuiConnectionOptions, OpenGpuiSceneHost,
};
use jellyflow_open_gpui::interaction::OpenGpuiPortEvent;
use jellyflow_open_gpui::navigation::{
    OpenGpuiInputBindings, OpenGpuiInputPreset, OpenGpuiNavigationConfig,
    OpenGpuiSelectionBindings, OpenGpuiSelectionModifier, OpenGpuiWheelBinding,
};
use jellyflow_open_gpui::presentation::OpenGpuiNodePresentationSnapshot;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeMountFacts, OpenGpuiNodeOverviewMount, OpenGpuiNodeRegistry,
};
use jellyflow_open_gpui::retained_field::RetainedFieldPhase;
use jellyflow_open_gpui::scale_policy::{OpenGpuiNodeScalePolicy, OpenGpuiSemanticLodPolicy};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentSizePolicy, OpenGpuiNodeSizePolicy};
use jellyflow_open_gpui::theme::{
    OpenGpuiGraphStyleRecipe, OpenGpuiGraphTheme, OpenGpuiPortMarkerShapes,
};

#[test]
fn owner_modules_are_the_only_public_import_seams() {
    let _ = OPEN_GPUI_COMMAND_JOURNAL_CAPACITY;

    let _ = size_of::<OpenGpuiActivityKind>();
    let _ = size_of::<OpenGpuiNodeActivityHandle>();
    let _ = size_of::<OpenGpuiCommandEnqueueError>();
    let _ = size_of::<OpenGpuiCommandTicket>();
    let _ = size_of::<OpenGpuiCommandTicketError>();
    let _ = size_of::<OpenGpuiCommandTicketResult>();
    let _ = size_of::<OpenGpuiNodeBinding>();
    let _ = size_of::<OpenGpuiNodeDragHandle>();
    let _ = size_of::<OpenGpuiPortAnchor>();
    let _ = size_of::<OpenGpuiPortHandle>();
    let _ = size_of::<OpenGpuiPortSide>();
    let _ = size_of::<OpenGpuiPortTone>();
    let _ = size_of::<OpenGpuiPortMarkerShape>();
    let _ = size_of::<OpenGpuiPortMarkerShapes>();
    let _ = size_of::<OpenGpuiEdgeVisualFacts>();
    let _ = size_of::<OpenGpuiEdgeHaloStyle>();
    let _ = size_of::<OpenGpuiConnectionOptions>();
    let _ = size_of::<OpenGpuiBlankCanvasContextRequest>();
    let _: Option<OpenGpuiBlankCanvasContextHandler> = None;
    let _ = size_of::<OpenGpuiSceneHost>();
    let _ = size_of::<OpenGpuiPortEvent>();
    let _ = size_of::<OpenGpuiNavigationConfig>();
    let _ = size_of::<OpenGpuiInputBindings>();
    let _ = size_of::<OpenGpuiInputPreset>();
    let _ = size_of::<OpenGpuiSelectionBindings>();
    let _ = size_of::<OpenGpuiSelectionModifier>();
    let _ = size_of::<OpenGpuiWheelBinding>();
    let _ = size_of::<OpenGpuiNodePresentationSnapshot>();
    let _ = size_of::<OpenGpuiNodeMountFacts>();
    let _ = size_of::<OpenGpuiNodeOverviewMount>();
    let _ = size_of::<OpenGpuiNodeRegistry>();
    let _ = size_of::<RetainedFieldPhase>();
    let _ = size_of::<OpenGpuiNodeScalePolicy>();
    let _ = size_of::<OpenGpuiSemanticLodPolicy>();
    let _ = size_of::<OpenGpuiContentSizePolicy>();
    let _ = size_of::<OpenGpuiNodeSizePolicy>();
    let _ = size_of::<OpenGpuiGraphTheme>();
    let _ = size_of::<OpenGpuiGraphStyleRecipe>();
}
