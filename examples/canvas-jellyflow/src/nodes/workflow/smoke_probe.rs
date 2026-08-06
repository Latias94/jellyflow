use std::collections::BTreeMap;

use jellyflow_core::NodeId;
use jellyflow_runtime::geometry::{ViewPoint, ViewRect, ViewSize};
use open_gpui::{App, Bounds, Global, IntoElement, Pixels, measured_element};

/// Real prepaint bounds for the flagship Workflow Condition smoke flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkflowConditionSmokeLayout {
    generation: u64,
    root: ViewRect,
    header: ViewRect,
    field: ViewRect,
    field_enabled: bool,
    value: ViewRect,
    add: ViewRect,
    add_enabled: bool,
    scroll: ViewRect,
    first_case: ViewRect,
}

impl WorkflowConditionSmokeLayout {
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn root(self) -> ViewRect {
        self.root
    }

    #[must_use]
    pub const fn header(self) -> ViewRect {
        self.header
    }

    #[must_use]
    pub const fn field(self) -> ViewRect {
        self.field
    }

    #[must_use]
    pub const fn field_enabled(self) -> bool {
        self.field_enabled
    }

    #[must_use]
    pub const fn value(self) -> ViewRect {
        self.value
    }

    #[must_use]
    pub const fn add(self) -> ViewRect {
        self.add
    }

    #[must_use]
    pub const fn add_enabled(self) -> bool {
        self.add_enabled
    }

    #[must_use]
    pub const fn scroll(self) -> ViewRect {
        self.scroll
    }

    #[must_use]
    pub const fn first_case(self) -> ViewRect {
        self.first_case
    }
}

#[derive(Default)]
struct WorkflowConditionSmokeLayouts {
    nodes: BTreeMap<NodeId, WorkflowConditionSmokeLayoutDraft>,
}

impl Global for WorkflowConditionSmokeLayouts {}

#[derive(Default)]
struct WorkflowConditionSmokeLayoutDraft {
    generation: u64,
    root: Option<ViewRect>,
    header: Option<ViewRect>,
    field: Option<ViewRect>,
    field_enabled: Option<bool>,
    value: Option<ViewRect>,
    add: Option<ViewRect>,
    add_enabled: Option<bool>,
    scroll: Option<ViewRect>,
    first_case: Option<ViewRect>,
}

#[derive(Clone, Copy)]
enum WorkflowConditionSmokeTarget {
    Header,
    Field { enabled: bool },
    Value,
    Add { enabled: bool },
    Scroll,
    FirstCase,
}

pub(super) fn measure_root(node: NodeId, child: impl IntoElement) -> impl IntoElement {
    measured_element(
        format!("jellyflow-smoke-workflow-condition-root-{}", node.0),
        child,
        move |_, bounds, _, _, cx| begin_layout(node, bounds, cx),
    )
}

pub(super) fn measure_header(node: NodeId, child: impl IntoElement) -> impl IntoElement {
    measure_target(node, WorkflowConditionSmokeTarget::Header, "header", child)
}

pub(super) fn measure_field(
    node: NodeId,
    enabled: bool,
    child: impl IntoElement,
) -> impl IntoElement {
    measure_target(
        node,
        WorkflowConditionSmokeTarget::Field { enabled },
        "field",
        child,
    )
}

pub(super) fn measure_value(node: NodeId, child: impl IntoElement) -> impl IntoElement {
    measure_target(node, WorkflowConditionSmokeTarget::Value, "value", child)
}

pub(super) fn measure_add(
    node: NodeId,
    enabled: bool,
    child: impl IntoElement,
) -> impl IntoElement {
    measure_target(
        node,
        WorkflowConditionSmokeTarget::Add { enabled },
        "add",
        child,
    )
}

pub(super) fn measure_scroll(node: NodeId, child: impl IntoElement) -> impl IntoElement {
    measure_target(node, WorkflowConditionSmokeTarget::Scroll, "scroll", child)
}

pub(super) fn measure_first_case(node: NodeId, child: impl IntoElement) -> impl IntoElement {
    measure_target(
        node,
        WorkflowConditionSmokeTarget::FirstCase,
        "first-case",
        child,
    )
}

fn measure_target(
    node: NodeId,
    target: WorkflowConditionSmokeTarget,
    label: &'static str,
    child: impl IntoElement,
) -> impl IntoElement {
    measured_element(
        format!("jellyflow-smoke-workflow-condition-{label}-{}", node.0),
        child,
        move |_, bounds, _, _, cx| record_target(node, target, bounds, cx),
    )
}

fn begin_layout(node: NodeId, bounds: Bounds<Pixels>, cx: &mut App) {
    let layouts = cx.default_global::<WorkflowConditionSmokeLayouts>();
    let generation = layouts
        .nodes
        .get(&node)
        .map_or(1, |draft| draft.generation.saturating_add(1));
    layouts.nodes.insert(
        node,
        WorkflowConditionSmokeLayoutDraft {
            generation,
            root: view_rect(bounds),
            ..WorkflowConditionSmokeLayoutDraft::default()
        },
    );
}

fn record_target(
    node: NodeId,
    target: WorkflowConditionSmokeTarget,
    bounds: Bounds<Pixels>,
    cx: &mut App,
) {
    let Some(draft) = cx
        .default_global::<WorkflowConditionSmokeLayouts>()
        .nodes
        .get_mut(&node)
    else {
        return;
    };
    let bounds = view_rect(bounds);
    match target {
        WorkflowConditionSmokeTarget::Header => draft.header = bounds,
        WorkflowConditionSmokeTarget::Field { enabled } => {
            draft.field = bounds;
            draft.field_enabled = Some(enabled);
        }
        WorkflowConditionSmokeTarget::Value => draft.value = bounds,
        WorkflowConditionSmokeTarget::Add { enabled } => {
            draft.add = bounds;
            draft.add_enabled = Some(enabled);
        }
        WorkflowConditionSmokeTarget::Scroll => draft.scroll = bounds,
        WorkflowConditionSmokeTarget::FirstCase => draft.first_case = bounds,
    }
}

fn view_rect(bounds: Bounds<Pixels>) -> Option<ViewRect> {
    let rect = ViewRect::new(
        ViewPoint::new(bounds.origin.x.as_f32(), bounds.origin.y.as_f32()),
        ViewSize::new(bounds.size.width.as_f32(), bounds.size.height.as_f32()),
    );
    rect.is_positive_finite().then_some(rect)
}

#[must_use]
pub fn workflow_condition_smoke_layout(
    cx: &App,
    node: NodeId,
) -> Option<WorkflowConditionSmokeLayout> {
    let draft = cx
        .try_global::<WorkflowConditionSmokeLayouts>()?
        .nodes
        .get(&node)?;
    Some(WorkflowConditionSmokeLayout {
        generation: draft.generation,
        root: draft.root?,
        header: draft.header?,
        field: draft.field?,
        field_enabled: draft.field_enabled?,
        value: draft.value?,
        add: draft.add?,
        add_enabled: draft.add_enabled?,
        scroll: draft.scroll?,
        first_case: draft.first_case?,
    })
}
