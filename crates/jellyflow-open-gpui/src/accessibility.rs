use jellyflow::core::PortId;
use jellyflow::runtime::{NodeInstanceKey, ViewId};
use open_gpui::{
    Div, ElementId, EntityId, Role, SharedString, Stateful, StatefulInteractiveElement,
};

/// AccessKit role exposed by the native graph scene.
pub const OPEN_GPUI_SCENE_ROLE: Role = Role::Group;
/// AccessKit role exposed by each retained graph node.
pub const OPEN_GPUI_NODE_ROLE: Role = Role::Group;
/// AccessKit role exposed by a real graph port that only reports geometry.
pub const OPEN_GPUI_STATIC_PORT_ROLE: Role = Role::Image;
/// AccessKit role exposed by a real graph port that accepts native interaction.
pub const OPEN_GPUI_INTERACTIVE_PORT_ROLE: Role = Role::Button;
/// AccessKit role exposed by a recoverable host or renderer failure.
pub const OPEN_GPUI_DIAGNOSTIC_ROLE: Role = Role::Alert;

pub(crate) fn scene_semantic_element(
    element: Stateful<Div>,
    name: impl Into<SharedString>,
) -> Stateful<Div> {
    element.role(OPEN_GPUI_SCENE_ROLE).aria_label(name)
}

pub(crate) fn node_semantic_element(
    element: Stateful<Div>,
    name: impl Into<SharedString>,
    selected: bool,
) -> Stateful<Div> {
    element
        .role(OPEN_GPUI_NODE_ROLE)
        .aria_label(name)
        .aria_selected(selected)
}

pub(crate) fn diagnostic_semantic_element(
    element: Stateful<Div>,
    name: impl Into<SharedString>,
) -> Stateful<Div> {
    element.role(OPEN_GPUI_DIAGNOSTIC_ROLE).aria_label(name)
}

fn host_root(host: EntityId) -> ElementId {
    ("jellyflow-native-host", host).into()
}

fn view_root(host: EntityId, view: ViewId) -> ElementId {
    let view_root: ElementId = (host_root(host), "view").into();
    (view_root, format!("session-{}", view.get())).into()
}

fn node_root(host: EntityId, instance: &NodeInstanceKey) -> ElementId {
    let node: ElementId = (view_root(host, instance.view()), "node").into();
    let node: ElementId = (node, instance.node().0.to_string()).into();
    let node: ElementId = (node, format!("renderer-{}", instance.renderer().key())).into();
    let node: ElementId = (
        node,
        format!("renderer-incarnation-{}", instance.renderer().incarnation()),
    )
        .into();
    (
        node,
        format!("node-incarnation-{}", instance.incarnation().get()),
    )
        .into()
}

/// Returns the stable accessibility identity for one native graph scene.
#[must_use]
pub fn open_gpui_scene_element_id(host: EntityId, view: ViewId) -> ElementId {
    (view_root(host, view), "scene").into()
}

/// Returns the host-scoped identity for the scene's interaction boundary.
#[must_use]
pub fn open_gpui_interaction_element_id(host: EntityId, view: ViewId) -> ElementId {
    (view_root(host, view), "interactions").into()
}

/// Returns the stable accessibility identity for one retained node incarnation.
#[must_use]
pub fn open_gpui_node_element_id(host: EntityId, instance: &NodeInstanceKey) -> ElementId {
    (node_root(host, instance), "semantic").into()
}

/// Returns the host-scoped identity for one retained node's measurement wrapper.
#[must_use]
pub fn open_gpui_node_measurement_element_id(
    host: EntityId,
    instance: &NodeInstanceKey,
) -> ElementId {
    (node_root(host, instance), "measurement").into()
}

/// Returns the host-scoped identity for one scene measurement wrapper.
#[must_use]
pub fn open_gpui_scene_measurement_element_id(host: EntityId, view: ViewId) -> ElementId {
    (view_root(host, view), "scene-measurement").into()
}

/// Returns the stable Open GPUI identity for one real port incarnation.
#[must_use]
pub fn open_gpui_port_element_id(
    host: EntityId,
    instance: &NodeInstanceKey,
    port: PortId,
) -> ElementId {
    let port_root: ElementId = (node_root(host, instance), "port").into();
    (port_root, port.0.to_string()).into()
}

/// Returns the host-scoped identity for an interactive port's measurement wrapper.
#[must_use]
pub fn open_gpui_port_measurement_element_id(
    host: EntityId,
    instance: &NodeInstanceKey,
    port: PortId,
) -> ElementId {
    (
        open_gpui_port_element_id(host, instance, port),
        "measurement",
    )
        .into()
}

#[cfg(test)]
mod tests {
    use open_gpui::accesskit::{Action, Node};
    use open_gpui::{Element, InteractiveElement, div};

    use super::*;

    #[test]
    fn production_semantic_elements_write_the_expected_accesskit_contract() {
        // Open GPUI 0.2's TestWindow drops the accessibility callbacks and TreeUpdate. Inspecting
        // these exact production builders is the strongest test-support path available upstream.
        let scene = scene_semantic_element(div().id("scene"), "Node graph");
        let scene = accesskit_node(&scene);
        assert_eq!(scene.role(), Role::Group);
        assert_eq!(scene.label(), Some("Node graph"));
        assert!(!scene.supports_action(Action::Click));
        assert!(!scene.supports_action(Action::Focus));

        let selected_node = node_semantic_element(div().id("node"), "Condition", true);
        let selected_node = accesskit_node(&selected_node);
        assert_eq!(selected_node.role(), Role::Group);
        assert_eq!(selected_node.label(), Some("Condition"));
        assert_eq!(selected_node.is_selected(), Some(true));
        assert!(!selected_node.supports_action(Action::Click));
        assert!(!selected_node.supports_action(Action::Focus));

        let unselected_node = node_semantic_element(div().id("unselected-node"), "LLM", false);
        let unselected_node = accesskit_node(&unselected_node);
        assert_eq!(unselected_node.is_selected(), Some(false));

        let diagnostic = diagnostic_semantic_element(
            div().id("diagnostic"),
            "Missing renderer: workflow.condition",
        );
        let diagnostic = accesskit_node(&diagnostic);
        assert_eq!(diagnostic.role(), Role::Alert);
        assert_eq!(
            diagnostic.label(),
            Some("Missing renderer: workflow.condition")
        );
        assert!(!diagnostic.supports_action(Action::Click));
        assert!(!diagnostic.supports_action(Action::Focus));
    }

    fn accesskit_node(element: &impl Element) -> Node {
        let role = element.a11y_role().expect("semantic element role");
        let mut node = Node::new(role);
        element.write_a11y_info(&mut node);
        node
    }
}
