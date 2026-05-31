use crate::io::NodeGraphViewState;
use crate::runtime::events::ViewChange;
use jellyflow_core::core::Graph;

#[derive(Clone, Copy)]
pub(super) enum ViewStateMutationKind {
    FullState,
    Viewport,
    Selection,
}

impl ViewStateMutationKind {
    pub(super) fn sanitize(self, graph: &Graph, view_state: &mut NodeGraphViewState) {
        match self {
            Self::FullState | Self::Selection => view_state.sanitize_for_graph(graph),
            Self::Viewport => {}
        }
    }

    pub(super) fn changed(self, before: &NodeGraphViewState, after: &NodeGraphViewState) -> bool {
        match self {
            Self::FullState => !view_state_eq(before, after),
            Self::Viewport => before.pan != after.pan || before.zoom != after.zoom,
            Self::Selection => {
                before.selected_nodes != after.selected_nodes
                    || before.selected_edges != after.selected_edges
                    || before.selected_groups != after.selected_groups
            }
        }
    }

    pub(super) fn collect_changes(
        self,
        before: &NodeGraphViewState,
        after: &NodeGraphViewState,
    ) -> Vec<ViewChange> {
        match self {
            Self::FullState => collect_view_projection_changes(before, after),
            Self::Viewport => vec![ViewChange::viewport(after.pan, after.zoom)],
            Self::Selection => vec![selection_change(after)],
        }
    }
}

fn view_state_eq(a: &NodeGraphViewState, b: &NodeGraphViewState) -> bool {
    a.pan == b.pan
        && a.zoom == b.zoom
        && a.selected_nodes == b.selected_nodes
        && a.selected_edges == b.selected_edges
        && a.selected_groups == b.selected_groups
        && a.draw_order == b.draw_order
        && a.group_draw_order == b.group_draw_order
}

fn collect_view_projection_changes(
    before: &NodeGraphViewState,
    after: &NodeGraphViewState,
) -> Vec<ViewChange> {
    let mut changes: Vec<ViewChange> = Vec::new();
    if before.pan != after.pan || (before.zoom - after.zoom).abs() > 1.0e-6 {
        changes.push(ViewChange::viewport(after.pan, after.zoom));
    }
    if before.selected_nodes != after.selected_nodes
        || before.selected_edges != after.selected_edges
        || before.selected_groups != after.selected_groups
    {
        changes.push(selection_change(after));
    }
    changes
}

fn selection_change(view_state: &NodeGraphViewState) -> ViewChange {
    ViewChange::selection(
        view_state.selected_nodes.clone(),
        view_state.selected_edges.clone(),
        view_state.selected_groups.clone(),
    )
}
