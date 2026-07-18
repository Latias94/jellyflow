use std::collections::{BTreeMap, BTreeSet};

use jellyflow::core::{EdgeId, NodeId, PortId};
use jellyflow::runtime::EdgeEndpoint;
use open_gpui::{Context, FocusHandle, Window};

use crate::semantic_zoom::OpenGpuiNodePresentation;

use super::OpenGpuiSceneHost;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OpenGpuiGraphFocusTarget {
    Node {
        node: NodeId,
        presentation: OpenGpuiNodePresentation,
    },
    Port {
        node: NodeId,
        port: PortId,
    },
    Edge(EdgeId),
    ReconnectSource(EdgeId),
    ReconnectTarget(EdgeId),
}

impl OpenGpuiGraphFocusTarget {
    pub(super) fn reconnect(edge: EdgeId, endpoint: EdgeEndpoint) -> Self {
        match endpoint {
            EdgeEndpoint::From => Self::ReconnectSource(edge),
            EdgeEndpoint::To => Self::ReconnectTarget(edge),
        }
    }

    fn is_retained_action(self) -> bool {
        matches!(
            self,
            Self::Edge(_) | Self::ReconnectSource(_) | Self::ReconnectTarget(_)
        )
    }

    fn same_logical_node(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Node { node: left, .. }, Self::Node { node: right, .. }) if left == right
        )
    }
}

#[derive(Clone)]
pub(super) struct OpenGpuiGraphFocusEntry {
    pub(super) target: OpenGpuiGraphFocusTarget,
    pub(super) handle: FocusHandle,
}

impl OpenGpuiGraphFocusEntry {
    pub(super) fn new(target: OpenGpuiGraphFocusTarget, handle: FocusHandle) -> Self {
        Self { target, handle }
    }
}

#[derive(Default)]
pub(super) struct OpenGpuiGraphFocusCoordinator {
    actions: BTreeMap<OpenGpuiGraphFocusTarget, FocusHandle>,
    previous: Vec<OpenGpuiGraphFocusEntry>,
    return_target: Option<OpenGpuiGraphFocusTarget>,
}

impl OpenGpuiGraphFocusCoordinator {
    pub(super) fn has_entries(&self) -> bool {
        !self.actions.is_empty() || !self.previous.is_empty() || self.return_target.is_some()
    }

    pub(super) fn action_handle(
        &mut self,
        target: OpenGpuiGraphFocusTarget,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> FocusHandle {
        self.actions
            .entry(target)
            .or_insert_with(|| cx.focus_handle())
            .clone()
            .tab_stop(true)
    }

    pub(super) fn remember_return_target(&mut self, target: OpenGpuiGraphFocusTarget) {
        self.return_target = Some(target);
    }

    pub(super) fn reconcile(
        &mut self,
        current: Vec<OpenGpuiGraphFocusEntry>,
        root: &FocusHandle,
        allow_return: bool,
        window: &mut Window,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) {
        if allow_return
            && let Some(target) = self.return_target.take()
            && (root.is_focused(window) || window.focused(cx).is_none())
        {
            if let Some(entry) = current.iter().find(|entry| entry.target == target) {
                window.focus(&entry.handle, cx);
            } else {
                window.focus(root, cx);
            }
        } else if allow_return {
            self.return_target = None;
        }

        if let Some(index) = self.disappeared_focus_index(&current, window, cx) {
            if let Some(fallback) = self.fallback_for(index, &current) {
                window.focus(&fallback.handle, cx);
            } else {
                window.focus(root, cx);
            }
        }

        let retained = current
            .iter()
            .filter_map(|entry| entry.target.is_retained_action().then_some(entry.target))
            .collect::<BTreeSet<_>>();
        self.actions.retain(|target, _| retained.contains(target));
        self.previous = current;
    }

    fn disappeared_focus_index(
        &self,
        current: &[OpenGpuiGraphFocusEntry],
        window: &Window,
        cx: &Context<OpenGpuiSceneHost>,
    ) -> Option<usize> {
        if let Some(index) = self
            .previous
            .iter()
            .position(|entry| entry.handle.is_focused(window))
        {
            return (!current
                .iter()
                .any(|entry| entry.handle == self.previous[index].handle))
            .then_some(index);
        }

        self.previous.iter().enumerate().find_map(|(index, entry)| {
            let OpenGpuiGraphFocusTarget::Node { .. } = entry.target else {
                return None;
            };
            let same_entry_survives = current
                .iter()
                .any(|current| current.target == entry.target && current.handle == entry.handle);
            let presentation_changed = current
                .iter()
                .any(|current| current.target.same_logical_node(entry.target));
            (!same_entry_survives
                && presentation_changed
                && entry.handle.contains_focused(window, cx))
            .then_some(index)
        })
    }

    fn fallback_for<'a>(
        &self,
        removed: usize,
        current: &'a [OpenGpuiGraphFocusEntry],
    ) -> Option<&'a OpenGpuiGraphFocusEntry> {
        let removed_target = self.previous[removed].target;
        if let Some(replacement) = current.iter().find(|entry| {
            entry.target == removed_target || entry.target.same_logical_node(removed_target)
        }) {
            return Some(replacement);
        }
        self.previous[removed + 1..]
            .iter()
            .find_map(|previous| {
                current
                    .iter()
                    .find(|current| current.target == previous.target)
            })
            .or_else(|| {
                self.previous[..removed].iter().rev().find_map(|previous| {
                    current
                        .iter()
                        .find(|current| current.target == previous.target)
                })
            })
    }

    pub(super) fn close(&mut self, window: &mut Window) {
        self.actions.clear();
        self.previous.clear();
        self.return_target = None;
        window.blur();
    }
}
