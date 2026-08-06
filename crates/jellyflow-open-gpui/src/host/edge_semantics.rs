use std::rc::Rc;

use jellyflow_core::EdgeId;
use open_gpui::{
    AnyElement, App, Bounds, Element, ElementId, FocusHandle, GlobalElementId, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, ParentElement, SharedString, Styled, Window, div,
    px,
};

use crate::accessibility::OPEN_GPUI_EDGE_ROLE;

/// Stable screen-space size used by edge accessibility and debug bounds.
pub(super) const OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW: f32 = 22.0;

pub(super) fn open_gpui_edge_semantic_selector(edge: EdgeId) -> String {
    format!("jellyflow-edge-semantic-{edge:?}")
}

pub(super) type OpenGpuiEdgeActivation = Rc<dyn Fn(&mut Window, &mut App) -> bool + 'static>;

/// A focusable edge semantic that deliberately owns no pointer hitbox.
///
/// Runtime path picking remains the only pointer authority. The child supplies semantic layout and
/// a zero-area tab anchor; focused wire chrome is resolved with the ordinary edge paint recipe.
/// The graph root handles keyboard activation from the exact focus capability, while this wrapper
/// registers accessibility actions without using a pointer hitbox that would intercept edge picks.
pub(super) struct OpenGpuiEdgeSemanticElement {
    id: ElementId,
    name: SharedString,
    selected: bool,
    focus: FocusHandle,
    activation: Option<OpenGpuiEdgeActivation>,
    child: Option<AnyElement>,
}

impl OpenGpuiEdgeSemanticElement {
    pub(super) fn new(
        id: ElementId,
        selector: String,
        name: impl Into<SharedString>,
        selected: bool,
        focus: FocusHandle,
        activation: Option<OpenGpuiEdgeActivation>,
        left: f32,
        top: f32,
        size: f32,
        child: impl IntoElement,
    ) -> Self {
        // `set_focus_handle` participates in key dispatch but does not insert a tab stop. The
        // zero-area native anchor supplies tab order without creating a pointer hitbox over the
        // edge's semantic bounds.
        let tab_anchor = div()
            .absolute()
            .w(px(0.0))
            .h(px(0.0))
            .track_focus(&focus)
            .tab_stop(true);
        let child = div()
            .debug_selector(move || selector.clone())
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(size))
            .h(px(size))
            .child(child)
            .child(tab_anchor);
        Self {
            id,
            name: name.into(),
            selected,
            focus,
            activation,
            child: Some(child.into_any_element()),
        }
    }
}

impl Element for OpenGpuiEdgeSemanticElement {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = self.child.take().expect("edge semantic child missing");
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<open_gpui::Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        child.prepaint(window, cx);
        window.set_focus_handle(&self.focus, cx);
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<open_gpui::Pixels>,
        child: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);

        if let Some(activation) = self.activation.clone()
            && let Some(id) = id
        {
            window.on_a11y_action(
                id.accesskit_node_id(),
                open_gpui::accesskit::Action::Click,
                move |_, window, cx| {
                    if activation(window, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                },
            );
        }

        if let Some(id) = id {
            let focus = self.focus.clone();
            window.on_a11y_action(
                id.accesskit_node_id(),
                open_gpui::accesskit::Action::Focus,
                move |_, window, cx| window.focus(&focus, cx),
            );
        }
    }

    fn a11y_role(&self) -> Option<open_gpui::accesskit::Role> {
        Some(OPEN_GPUI_EDGE_ROLE)
    }

    fn write_a11y_info(&self, node: &mut open_gpui::accesskit::Node) {
        node.set_label(self.name.to_string());
        node.set_selected(self.selected);
        node.add_action(open_gpui::accesskit::Action::Focus);
        if self.activation.is_some() {
            node.add_action(open_gpui::accesskit::Action::Click);
        }
    }
}

impl IntoElement for OpenGpuiEdgeSemanticElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
