use std::collections::BTreeMap;

use jellyflow::{
    core::{CanvasPoint, CanvasSize, Graph, Node, NodeId, NodeKindKey},
    runtime::{
        runtime::connection::ConnectionHandleRef,
        schema::{
            MenuSurface, NodeKindViewDescriptor, NodeSurfaceProjection, NodeSurfaceSlotDescriptor,
            NodeSurfaceSlotProjection,
        },
    },
};
use serde_json::Value;

use crate::{
    OpenGpuiActionDispatchPlan, OpenGpuiActionSurface, OpenGpuiAuthoringController,
    OpenGpuiAuthoringOutcome, OpenGpuiControlEditPlan, OpenGpuiControlEventValue,
    OpenGpuiControlPlan, OpenGpuiDroppedWireInsertPlan, OpenGpuiMeasurementId, OpenGpuiMenuPlan,
    OpenGpuiNodeSurfaceLayout, OpenGpuiProductSurfacePreset, OpenGpuiRepeatableItemLayout,
    OpenGpuiRepeatableItemProjection, OpenGpuiRepeatableSurfaceLayout,
    OpenGpuiRepeatableSurfaceProjection, element_ids, plan_dropped_wire_insert,
    project_actions_for_surface, project_dropped_wire_menu, project_menu, project_slot_controls,
    repeatable_item_projection, repeatable_surface_projection,
};

/// Registration metadata for one Open GPUI custom node renderer.
///
/// This type intentionally contains no GPUI element or widget type. Hosts can map a resolved custom
/// renderer key to local GPUI code, while this crate keeps the semantic context and planning helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeRendererRegistration {
    pub renderer_key: String,
    pub label: String,
}

impl OpenGpuiNodeRendererRegistration {
    pub fn new(renderer_key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            renderer_key: renderer_key.into(),
            label: label.into(),
        }
    }
}

pub type OpenGpuiNodeRendererTable<Services, Output> =
    BTreeMap<String, Box<dyn for<'a> Fn(&OpenGpuiNodeRendererHostContext<'a, Services>) -> Output>>;

/// Adapter-local registry for deciding whether a semantic renderer key has a GPUI host renderer.
#[derive(Debug, Clone, Default)]
pub struct OpenGpuiNodeRendererRegistry {
    renderers: BTreeMap<String, OpenGpuiNodeRendererRegistration>,
}

impl OpenGpuiNodeRendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        renderer_key: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut Self {
        let renderer_key = renderer_key.into();
        self.renderers.insert(
            renderer_key.clone(),
            OpenGpuiNodeRendererRegistration::new(renderer_key, label),
        );
        self
    }

    pub fn register_many<I, K, L>(&mut self, renderers: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, L)>,
        K: Into<String>,
        L: Into<String>,
    {
        for (renderer_key, label) in renderers {
            self.register(renderer_key, label);
        }
        self
    }

    pub fn with_renderer(
        mut self,
        renderer_key: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        self.register(renderer_key, label);
        self
    }

    pub fn with_renderers<I, K, L>(mut self, renderers: I) -> Self
    where
        I: IntoIterator<Item = (K, L)>,
        K: Into<String>,
        L: Into<String>,
    {
        self.register_many(renderers);
        self
    }

    pub fn contains(&self, renderer_key: &str) -> bool {
        self.renderers.contains_key(renderer_key)
    }

    pub fn registration(&self, renderer_key: &str) -> Option<&OpenGpuiNodeRendererRegistration> {
        self.renderers.get(renderer_key)
    }

    pub fn resolve(&self, context: &OpenGpuiNodeRendererContext) -> OpenGpuiNodeRendererResolution {
        self.registration(&context.renderer_key)
            .cloned()
            .map(OpenGpuiNodeRendererResolution::Custom)
            .unwrap_or_else(|| {
                OpenGpuiNodeRendererResolution::Fallback(OpenGpuiNodeRendererFallback {
                    renderer_key: context.renderer_key.clone(),
                    reason: OpenGpuiNodeRendererFallbackReason::UnregisteredRenderer,
                })
            })
    }

    pub fn render_with<Output, Fallback>(
        &self,
        context: &OpenGpuiNodeRendererContext,
        custom_renderers: &BTreeMap<String, Box<dyn Fn(&OpenGpuiNodeRendererContext) -> Output>>,
        fallback: Fallback,
    ) -> OpenGpuiNodeRendererOutput<Output>
    where
        Fallback: FnOnce(&OpenGpuiNodeRendererContext, OpenGpuiNodeRendererFallback) -> Output,
    {
        match self.resolve(context) {
            OpenGpuiNodeRendererResolution::Custom(registration) => {
                if let Some(renderer) = custom_renderers.get(&registration.renderer_key) {
                    OpenGpuiNodeRendererOutput {
                        output: renderer(context),
                        source: OpenGpuiNodeRendererOutputSource::Custom(registration),
                    }
                } else {
                    let fallback_reason = OpenGpuiNodeRendererFallback {
                        renderer_key: registration.renderer_key,
                        reason: OpenGpuiNodeRendererFallbackReason::MissingHostRenderer,
                    };
                    OpenGpuiNodeRendererOutput {
                        output: fallback(context, fallback_reason.clone()),
                        source: OpenGpuiNodeRendererOutputSource::Fallback(fallback_reason),
                    }
                }
            }
            OpenGpuiNodeRendererResolution::Fallback(fallback_reason) => {
                OpenGpuiNodeRendererOutput {
                    output: fallback(context, fallback_reason.clone()),
                    source: OpenGpuiNodeRendererOutputSource::Fallback(fallback_reason),
                }
            }
        }
    }

    pub fn render_with_host<Services, Output, Fallback>(
        &self,
        context: &OpenGpuiNodeRendererContext,
        services: &Services,
        custom_renderers: &OpenGpuiNodeRendererTable<Services, Output>,
        fallback: Fallback,
    ) -> OpenGpuiNodeRendererOutput<Output>
    where
        Fallback: for<'a> FnOnce(
            &OpenGpuiNodeRendererHostContext<'a, Services>,
            OpenGpuiNodeRendererFallback,
        ) -> Output,
    {
        match self.resolve(context) {
            OpenGpuiNodeRendererResolution::Custom(registration) => {
                if let Some(renderer) = custom_renderers.get(&registration.renderer_key) {
                    let host_context =
                        OpenGpuiNodeRendererHostContext::custom(context, services, &registration);
                    OpenGpuiNodeRendererOutput {
                        output: renderer(&host_context),
                        source: OpenGpuiNodeRendererOutputSource::Custom(registration),
                    }
                } else {
                    let fallback_reason = OpenGpuiNodeRendererFallback {
                        renderer_key: registration.renderer_key,
                        reason: OpenGpuiNodeRendererFallbackReason::MissingHostRenderer,
                    };
                    let host_context = OpenGpuiNodeRendererHostContext::for_fallback(
                        context,
                        services,
                        &fallback_reason,
                    );
                    OpenGpuiNodeRendererOutput {
                        output: fallback(&host_context, fallback_reason.clone()),
                        source: OpenGpuiNodeRendererOutputSource::Fallback(fallback_reason),
                    }
                }
            }
            OpenGpuiNodeRendererResolution::Fallback(fallback_reason) => {
                let host_context = OpenGpuiNodeRendererHostContext::for_fallback(
                    context,
                    services,
                    &fallback_reason,
                );
                OpenGpuiNodeRendererOutput {
                    output: fallback(&host_context, fallback_reason.clone()),
                    source: OpenGpuiNodeRendererOutputSource::Fallback(fallback_reason),
                }
            }
        }
    }
}

/// Result of resolving a semantic renderer key against the adapter-local registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiNodeRendererResolution {
    Custom(OpenGpuiNodeRendererRegistration),
    Fallback(OpenGpuiNodeRendererFallback),
}

impl OpenGpuiNodeRendererResolution {
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiNodeRendererFallback {
    pub renderer_key: String,
    pub reason: OpenGpuiNodeRendererFallbackReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiNodeRendererFallbackReason {
    UnregisteredRenderer,
    MissingHostRenderer,
}

#[derive(Debug)]
pub struct OpenGpuiNodeRendererOutput<Output> {
    pub output: Output,
    pub source: OpenGpuiNodeRendererOutputSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiNodeRendererOutputSource {
    Custom(OpenGpuiNodeRendererRegistration),
    Fallback(OpenGpuiNodeRendererFallback),
}

/// Host-owned services plus semantic renderer context for one resolved node renderer.
///
/// The services type is generic so Open GPUI hosts can pass local dispatch handles,
/// measurement collectors, weak entities, or test markers without this adapter crate importing
/// concrete widget or event-loop types.
#[derive(Debug)]
pub struct OpenGpuiNodeRendererHostContext<'a, Services> {
    semantic: &'a OpenGpuiNodeRendererContext,
    services: &'a Services,
    registration: Option<&'a OpenGpuiNodeRendererRegistration>,
    fallback: Option<&'a OpenGpuiNodeRendererFallback>,
}

impl<'a, Services> OpenGpuiNodeRendererHostContext<'a, Services> {
    pub fn new(
        semantic: &'a OpenGpuiNodeRendererContext,
        services: &'a Services,
        registration: Option<&'a OpenGpuiNodeRendererRegistration>,
        fallback: Option<&'a OpenGpuiNodeRendererFallback>,
    ) -> Self {
        Self {
            semantic,
            services,
            registration,
            fallback,
        }
    }

    fn custom(
        semantic: &'a OpenGpuiNodeRendererContext,
        services: &'a Services,
        registration: &'a OpenGpuiNodeRendererRegistration,
    ) -> Self {
        Self::new(semantic, services, Some(registration), None)
    }

    fn for_fallback(
        semantic: &'a OpenGpuiNodeRendererContext,
        services: &'a Services,
        fallback: &'a OpenGpuiNodeRendererFallback,
    ) -> Self {
        Self::new(semantic, services, None, Some(fallback))
    }

    pub fn semantic(&self) -> &'a OpenGpuiNodeRendererContext {
        self.semantic
    }

    pub fn services(&self) -> &'a Services {
        self.services
    }

    pub fn registration(&self) -> Option<&'a OpenGpuiNodeRendererRegistration> {
        self.registration
    }

    pub fn fallback(&self) -> Option<&'a OpenGpuiNodeRendererFallback> {
        self.fallback
    }

    pub fn node_id(&self) -> NodeId {
        self.semantic.node_id
    }

    pub fn renderer_key(&self) -> &str {
        &self.semantic.renderer_key
    }

    pub fn surface_slots(&self) -> &[NodeSurfaceSlotProjection] {
        &self.semantic.surface_slots
    }

    pub fn repeatables(&self) -> &[OpenGpuiRepeatableSurfaceLayout] {
        &self.semantic.repeatables
    }

    pub fn repeatable_items(&self) -> &[OpenGpuiRepeatableItemLayout] {
        &self.semantic.repeatable_items
    }

    pub fn action_menus(&self) -> &[OpenGpuiMenuPlan] {
        &self.semantic.action_menus
    }

    pub fn toolbar_menu(&self) -> &OpenGpuiMenuPlan {
        &self.semantic.toolbar_menu
    }

    pub fn slot_measurement_id(&self, slot_key: impl Into<String>) -> OpenGpuiMeasurementId {
        self.semantic.slot_measurement_id(slot_key)
    }

    pub fn control_measurement_id(
        &self,
        slot_key: impl AsRef<str>,
        control_key: impl Into<String>,
    ) -> OpenGpuiMeasurementId {
        self.semantic.control_measurement_id(slot_key, control_key)
    }

    pub fn repeatable_item_measurement_id(
        &self,
        slot_key: impl Into<String>,
        item_id: impl Into<String>,
    ) -> OpenGpuiMeasurementId {
        self.semantic
            .repeatable_item_measurement_id(slot_key, item_id)
    }

    pub fn anchor_measurement_id(&self, anchor_key: impl Into<String>) -> OpenGpuiMeasurementId {
        self.semantic.anchor_measurement_id(anchor_key)
    }

    pub fn control_element_id(
        &self,
        control_scope: impl AsRef<str>,
        control_key: impl AsRef<str>,
        index: usize,
    ) -> String {
        element_ids::open_gpui_control_element_id(
            self.semantic.node_id,
            control_scope,
            control_key,
            index,
        )
    }

    pub fn action_button_element_id(
        &self,
        menu_key: impl AsRef<str>,
        action_key: impl AsRef<str>,
        index: usize,
    ) -> String {
        element_ids::open_gpui_action_button_element_id(
            Some(self.semantic.node_id),
            menu_key,
            action_key,
            index,
        )
    }

    pub fn action_menu_element_id(
        &self,
        menu_key: impl AsRef<str>,
        id_suffix: impl AsRef<str>,
    ) -> String {
        element_ids::open_gpui_action_menu_element_id(
            Some(self.semantic.node_id),
            menu_key,
            id_suffix,
        )
    }

    pub fn chrome_fallback_button_element_id(&self) -> String {
        element_ids::open_gpui_chrome_fallback_button_element_id(
            self.semantic.node_id,
            &self.semantic.node_kind,
        )
    }
}

/// Renderer-neutral state that host-side GPUI node renderers commonly need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenGpuiNodeRendererState {
    pub selected: bool,
    pub hovered: bool,
    pub focused: bool,
    pub dragging: bool,
    pub resizing: bool,
    pub disabled: bool,
    pub hidden: bool,
}

/// Descriptor-derived context passed from the Open GPUI adapter to host-owned node renderers.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiNodeRendererContext {
    pub node_id: NodeId,
    pub node_kind: String,
    pub renderer_key: String,
    pub title: String,
    pub summary: Option<String>,
    pub state: OpenGpuiNodeRendererState,
    pub node_size: CanvasSize,
    pub node_data: Value,
    pub surface_projection: NodeSurfaceProjection,
    pub surface_preset: OpenGpuiProductSurfacePreset,
    pub surface_slots: Vec<NodeSurfaceSlotProjection>,
    pub slot_descriptors: Vec<NodeSurfaceSlotDescriptor>,
    pub surface_layout: OpenGpuiNodeSurfaceLayout,
    pub repeatables: Vec<OpenGpuiRepeatableSurfaceLayout>,
    pub repeatable_items: Vec<OpenGpuiRepeatableItemLayout>,
    pub action_menus: Vec<OpenGpuiMenuPlan>,
    pub toolbar_menu: OpenGpuiMenuPlan,
}

impl OpenGpuiNodeRendererContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: NodeId,
        node: &Node,
        graph: &Graph,
        descriptor: &NodeKindViewDescriptor,
        state: OpenGpuiNodeRendererState,
        node_size: CanvasSize,
        surface_projection: NodeSurfaceProjection,
        surface_slots: Vec<NodeSurfaceSlotProjection>,
        title: impl Into<String>,
        summary: Option<String>,
    ) -> Self {
        let repeatables = repeatable_surface_projection(descriptor, &node.data);
        let repeatable_items = repeatable_item_projection(descriptor, node, graph, &node_id);
        let surface_preset = OpenGpuiProductSurfacePreset::from_descriptor(descriptor);
        let surface_layout = OpenGpuiNodeSurfaceLayout::with_repeatable_items(
            surface_slots
                .iter()
                .cloned()
                .map(|slot| {
                    let descriptor_slot = descriptor.surface_slot(&slot.key).cloned();
                    (slot, descriptor_slot)
                })
                .collect(),
            repeatables,
            repeatable_items,
            node_size,
            surface_projection.slot_limit,
        );
        let toolbar_surface = OpenGpuiActionSurface::Toolbar {
            node_kind: Some(descriptor.kind.0.clone()),
        };
        let toolbar_menu = descriptor
            .menus
            .iter()
            .find(|menu| menu.surface == MenuSurface::Toolbar)
            .map(|menu| project_menu(descriptor, menu, &toolbar_surface))
            .filter(|menu| !menu.actions.is_empty())
            .unwrap_or_else(|| project_actions_for_surface(descriptor, &toolbar_surface));
        let node_surface = OpenGpuiActionSurface::Node {
            node_kind: descriptor.kind.0.clone(),
        };
        let mut action_menus = descriptor
            .menus
            .iter()
            .filter(|menu| menu.surface == MenuSurface::Node)
            .map(|menu| project_menu(descriptor, menu, &node_surface))
            .filter(|menu| !menu.actions.is_empty())
            .collect::<Vec<_>>();
        if action_menus.is_empty() {
            let synthetic = project_actions_for_surface(descriptor, &node_surface);
            if !synthetic.actions.is_empty() {
                action_menus.push(synthetic);
            }
        }

        Self {
            node_id,
            node_kind: descriptor.kind.0.clone(),
            renderer_key: descriptor.renderer_key.clone(),
            title: title.into(),
            summary,
            state,
            node_size,
            node_data: node.data.clone(),
            surface_projection,
            surface_preset,
            surface_slots,
            slot_descriptors: descriptor.surface_slots.clone(),
            repeatables: surface_layout.repeatables.clone(),
            repeatable_items: surface_layout.repeatable_items.clone(),
            surface_layout,
            action_menus,
            toolbar_menu,
        }
    }

    pub fn slot_descriptor(&self, slot_key: &str) -> Option<&NodeSurfaceSlotDescriptor> {
        self.slot_descriptors
            .iter()
            .find(|descriptor| descriptor.key == slot_key)
    }

    pub fn slot_controls(&self, slot_key: &str) -> Vec<OpenGpuiControlPlan> {
        self.slot_descriptor(slot_key)
            .map(|slot| project_slot_controls(&self.node_data, slot))
            .unwrap_or_default()
    }

    pub fn control(&self, control_key: &str) -> Option<OpenGpuiControlPlan> {
        self.slot_descriptors
            .iter()
            .flat_map(|slot| project_slot_controls(&self.node_data, slot))
            .find(|control| control.key == control_key)
    }

    pub fn slot_measurement_id(&self, slot_key: impl Into<String>) -> OpenGpuiMeasurementId {
        OpenGpuiMeasurementId::slot(self.node_id, slot_key)
    }

    pub fn control_measurement_id(
        &self,
        slot_key: impl AsRef<str>,
        control_key: impl Into<String>,
    ) -> OpenGpuiMeasurementId {
        OpenGpuiMeasurementId::control_in_slot(self.node_id, slot_key, control_key)
    }

    pub fn repeatable_item_measurement_id(
        &self,
        slot_key: impl Into<String>,
        item_id: impl Into<String>,
    ) -> OpenGpuiMeasurementId {
        OpenGpuiMeasurementId::repeatable_item(self.node_id, slot_key, item_id)
    }

    pub fn anchor_measurement_id(&self, anchor_key: impl Into<String>) -> OpenGpuiMeasurementId {
        OpenGpuiMeasurementId::anchor(self.node_id, anchor_key)
    }

    pub fn plan_control_event(
        &self,
        control_key: &str,
        event: OpenGpuiControlEventValue,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        let Some(control) = self.control(control_key) else {
            return Err(format!(
                "control `{control_key}` is not available in renderer context"
            ));
        };
        OpenGpuiAuthoringController.plan_control_event(
            self.node_id,
            &self.authoring_node(),
            &control,
            event,
        )
    }

    pub fn plan_menu_action_dispatch(
        &self,
        menu_key: &str,
        action_key: &str,
    ) -> Option<OpenGpuiAuthoringOutcome<OpenGpuiActionDispatchPlan>> {
        self.action_menus
            .iter()
            .chain(std::iter::once(&self.toolbar_menu))
            .find(|menu| menu.key == menu_key)
            .map(|menu| OpenGpuiAuthoringController.plan_menu_action_dispatch(menu, action_key))
    }

    pub fn dropped_wire_menu(
        &self,
        registry: &jellyflow::runtime::schema::NodeRegistry,
        source: ConnectionHandleRef,
        pointer: CanvasPoint,
    ) -> OpenGpuiMenuPlan {
        project_dropped_wire_menu(registry, source, None, pointer)
    }

    pub fn plan_dropped_wire_insert(
        &self,
        menu: &OpenGpuiMenuPlan,
        action_key: &str,
        source: ConnectionHandleRef,
        pointer: CanvasPoint,
    ) -> Option<OpenGpuiDroppedWireInsertPlan> {
        plan_dropped_wire_insert(menu, action_key, source, pointer)
    }

    fn authoring_node(&self) -> Node {
        Node {
            kind: NodeKindKey::new(self.node_kind.clone()),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(self.node_size),
            hidden: self.state.hidden,
            collapsed: false,
            ports: Vec::new(),
            data: self.node_data.clone(),
        }
    }
}

pub fn open_gpui_node_renderer_context(
    node_id: NodeId,
    node: &Node,
    graph: &Graph,
    descriptor: &NodeKindViewDescriptor,
    state: OpenGpuiNodeRendererState,
    surface_projection: NodeSurfaceProjection,
    surface_slots: Vec<NodeSurfaceSlotProjection>,
) -> OpenGpuiNodeRendererContext {
    let node_size = node.size.or(descriptor.default_size).unwrap_or(CanvasSize {
        width: 228.0,
        height: 168.0,
    });
    let title = node
        .data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(descriptor.title.as_str())
        .to_owned();
    let summary = node
        .data
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| node.data.get("description").and_then(Value::as_str))
        .map(str::to_owned);

    OpenGpuiNodeRendererContext::new(
        node_id,
        node,
        graph,
        descriptor,
        state,
        node_size,
        surface_projection,
        surface_slots,
        title,
        summary,
    )
}

pub fn open_gpui_renderer_repeatable_surfaces(
    descriptor: &NodeKindViewDescriptor,
    data: &Value,
) -> Vec<OpenGpuiRepeatableSurfaceProjection> {
    repeatable_surface_projection(descriptor, data)
}

pub fn open_gpui_renderer_repeatable_items(
    descriptor: &NodeKindViewDescriptor,
    node: &Node,
    graph: &Graph,
    node_id: &NodeId,
) -> Vec<OpenGpuiRepeatableItemProjection> {
    repeatable_item_projection(descriptor, node, graph, node_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        core::{GraphBuilder, GraphId},
        runtime::{
            runtime::create_node::CreateNodeRequest,
            schema::{NodeKitRegistry, NodeSurfaceProjection},
        },
    };
    use serde_json::json;

    #[test]
    fn renderer_registry_routes_known_keys_and_unknown_fallback() {
        let registry = OpenGpuiNodeRendererRegistry::new()
            .with_renderer("decision-card", "Decision")
            .with_renderer("shader-card", "Shader");
        let decision = renderer_context("demo.llm");
        let shader = renderer_context("demo.shader.mix");

        assert!(registry.resolve(&decision).is_custom());
        assert!(registry.resolve(&shader).is_custom());

        let mut unknown = decision.clone();
        unknown.renderer_key = "unregistered-card".to_owned();
        assert_eq!(
            registry.resolve(&unknown),
            OpenGpuiNodeRendererResolution::Fallback(OpenGpuiNodeRendererFallback {
                renderer_key: "unregistered-card".to_owned(),
                reason: OpenGpuiNodeRendererFallbackReason::UnregisteredRenderer,
            })
        );
    }

    #[test]
    fn renderer_context_exposes_semantic_measurement_ids_and_plans() {
        let context = renderer_context("demo.llm");

        assert!(
            context
                .surface_slots
                .iter()
                .any(|slot| slot.key == "field.prompt")
        );
        assert!(
            context
                .surface_layout
                .slots
                .iter()
                .any(|slot| slot.slot.key == "field.prompt")
        );
        assert!(context.control("control.prompt").is_some());
        assert_eq!(
            context.slot_measurement_id("field.prompt").element_id(),
            format!("jellyflow-node:{}:slot:field.prompt", context.node_id.0)
        );
        assert_eq!(
            context
                .control_measurement_id("field.prompt", "control.prompt")
                .element_id(),
            format!(
                "jellyflow-node:{}:control:field.prompt:control.prompt",
                context.node_id.0
            )
        );

        let plan = context
            .plan_control_event(
                "control.prompt",
                OpenGpuiControlEventValue::Text("Summarize this document".to_owned()),
            )
            .expect("control edit plans")
            .into_plan()
            .expect("planned edit");
        assert_eq!(plan.invalidation.nodes.len(), 1);

        let dispatch = context
            .plan_menu_action_dispatch("synthetic.Toolbar", "action.llm.run")
            .expect("toolbar menu exists")
            .into_plan()
            .expect("action dispatch plans");
        assert_eq!(dispatch.action_key, "action.llm.run");
    }

    #[test]
    fn renderer_context_exposes_repeatable_item_measurement_ids() {
        let context = renderer_context("demo.shader.mix");
        let factor = context
            .repeatable_items
            .iter()
            .find(|item| item.projection.item_id == "factor")
            .expect("factor repeatable item");

        assert_eq!(
            context
                .repeatable_item_measurement_id(
                    factor.projection.slot_key.clone(),
                    factor.projection.item_id.clone()
                )
                .element_id(),
            format!(
                "jellyflow-node:{}:repeatable:{}:item:factor",
                context.node_id.0, factor.projection.slot_key
            )
        );
        assert_eq!(
            context
                .anchor_measurement_id(factor.projection.anchor.clone())
                .element_id(),
            format!(
                "jellyflow-node:{}:anchor:{}",
                context.node_id.0, factor.projection.anchor
            )
        );
    }

    #[test]
    fn renderer_registry_can_call_host_renderer_or_fallback_generically() {
        let registry =
            OpenGpuiNodeRendererRegistry::new().with_renderer("decision-card", "Decision");
        let context = renderer_context("demo.llm");
        let mut renderers: BTreeMap<String, Box<dyn Fn(&OpenGpuiNodeRendererContext) -> String>> =
            BTreeMap::new();
        renderers.insert(
            "decision-card".to_owned(),
            Box::new(|context| format!("custom:{}", context.renderer_key)),
        );

        let output = registry.render_with(&context, &renderers, |context, fallback| {
            format!("fallback:{}:{:?}", context.renderer_key, fallback.reason)
        });

        assert_eq!(output.output, "custom:decision-card");
        assert!(matches!(
            output.source,
            OpenGpuiNodeRendererOutputSource::Custom(_)
        ));

        let registry_without_host =
            OpenGpuiNodeRendererRegistry::new().with_renderer("decision-card", "Decision");
        let output = registry_without_host.render_with(
            &context,
            &BTreeMap::<String, Box<dyn Fn(&OpenGpuiNodeRendererContext) -> String>>::new(),
            |context, fallback| format!("fallback:{}:{:?}", context.renderer_key, fallback.reason),
        );
        assert_eq!(output.output, "fallback:decision-card:MissingHostRenderer");
    }

    #[test]
    fn renderer_facade_passes_semantic_context_ids_and_host_services() {
        #[derive(Debug)]
        struct HostServices {
            marker: &'static str,
        }

        let registry = OpenGpuiNodeRendererRegistry::new().with_renderer("shader-card", "Shader");
        let context = renderer_context("demo.shader.mix");
        let services = HostServices {
            marker: "layout-pass",
        };
        let mut renderers: OpenGpuiNodeRendererTable<HostServices, String> = BTreeMap::new();
        renderers.insert(
            "shader-card".to_owned(),
            Box::new(|host| {
                assert_eq!(host.services().marker, "layout-pass");
                assert_eq!(
                    host.registration()
                        .map(|registration| registration.label.as_str()),
                    Some("Shader")
                );
                assert!(!host.surface_slots().is_empty());
                assert!(!host.repeatables().is_empty());
                assert!(!host.repeatable_items().is_empty());

                format!(
                    "custom:{}:{}:{}:{}:{}:{}:{}",
                    host.renderer_key(),
                    host.surface_slots().len(),
                    host.repeatables().len(),
                    host.repeatable_items().len(),
                    host.toolbar_menu().key,
                    host.slot_measurement_id("shader.inputs").element_id(),
                    host.control_element_id("shader.inputs", "control.name", 0)
                )
            }),
        );

        let output =
            registry.render_with_host(&context, &services, &renderers, |host, fallback| {
                format!("fallback:{}:{:?}", host.renderer_key(), fallback.reason)
            });

        assert!(output.output.starts_with("custom:shader-card:"));
        assert!(output.output.contains(":jellyflow-node:"));
        assert!(output.output.contains(":jellyflow-control:"));
        assert!(matches!(
            output.source,
            OpenGpuiNodeRendererOutputSource::Custom(_)
        ));
    }

    #[test]
    fn renderer_facade_reports_missing_host_and_unregistered_fallbacks() {
        #[derive(Debug)]
        struct HostServices {
            marker: &'static str,
        }

        let context = renderer_context("demo.llm");
        let services = HostServices { marker: "host" };
        let renderers: OpenGpuiNodeRendererTable<HostServices, String> = BTreeMap::new();
        let registry =
            OpenGpuiNodeRendererRegistry::new().with_renderer("decision-card", "Decision");

        let missing_host =
            registry.render_with_host(&context, &services, &renderers, |host, fallback| {
                assert_eq!(host.services().marker, "host");
                assert_eq!(host.fallback(), Some(&fallback));
                format!("fallback:{}:{:?}", host.renderer_key(), fallback.reason)
            });
        assert_eq!(
            missing_host.output,
            "fallback:decision-card:MissingHostRenderer"
        );
        assert!(matches!(
            missing_host.source,
            OpenGpuiNodeRendererOutputSource::Fallback(OpenGpuiNodeRendererFallback {
                reason: OpenGpuiNodeRendererFallbackReason::MissingHostRenderer,
                ..
            })
        ));

        let mut unknown = context.clone();
        unknown.renderer_key = "unknown-card".to_owned();
        let unregistered =
            registry.render_with_host(&unknown, &services, &renderers, |host, fallback| {
                assert_eq!(
                    host.chrome_fallback_button_element_id(),
                    format!(
                        "jellyflow-chrome-run-fallback:{}:{}",
                        host.node_id().0,
                        host.semantic().node_kind
                    )
                );
                assert_eq!(host.fallback(), Some(&fallback));
                format!("fallback:{}:{:?}", host.renderer_key(), fallback.reason)
            });
        assert_eq!(
            unregistered.output,
            "fallback:unknown-card:UnregisteredRenderer"
        );
        assert!(matches!(
            unregistered.source,
            OpenGpuiNodeRendererOutputSource::Fallback(OpenGpuiNodeRendererFallback {
                reason: OpenGpuiNodeRendererFallbackReason::UnregisteredRenderer,
                ..
            })
        ));
    }

    fn renderer_context(kind: &str) -> OpenGpuiNodeRendererContext {
        let kit_registry = NodeKitRegistry::builtin();
        let registry = kit_registry.node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new(kind))
            .unwrap_or_else(|| panic!("missing descriptor `{kind}`"));
        let schema = registry
            .get(&descriptor.kind)
            .unwrap_or_else(|| panic!("missing schema `{}`", descriptor.kind.0));
        let instantiation = schema.instantiate(
            CreateNodeRequest::new(NodeKindKey::new(kind), CanvasPoint::default()).pos,
        );
        let (node_id, mut node, ports) = instantiation.into_parts();
        if kind == "demo.llm" {
            node.data = json!({
                "fields": { "prompt": "" },
                "config": { "model": { "temperature": 0.7, "stream": false } },
                "meta": { "model": "gpt-4o-mini" },
                "title": "LLM"
            });
        }
        let mut graph_builder = GraphBuilder::new(GraphId::from_u128(0x0f_67_70_75_69))
            .with_node(node_id, node.clone());
        for (port_id, port) in ports {
            graph_builder = graph_builder.with_port(port_id, port);
        }
        let graph = graph_builder.build_unchecked();
        let layout_hints = kit_registry
            .layout_hints_for_kind(&descriptor.kind)
            .expect("builtin descriptor has layout hints");
        let surface_projection = NodeSurfaceProjection::from_layout_hints(layout_hints, 1.0);
        let surface_slots =
            descriptor.surface_slots_projection(&node.data, Some(layout_hints), 1.0);
        open_gpui_node_renderer_context(
            node_id,
            &node,
            &graph,
            &descriptor,
            OpenGpuiNodeRendererState::default(),
            surface_projection,
            surface_slots,
        )
    }
}
