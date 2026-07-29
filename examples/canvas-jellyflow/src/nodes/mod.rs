//! Retained native node entities used by the Open GPUI product example.

mod shader;
mod workflow;

use jellyflow_open_gpui::activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::chrome::OpenGpuiEdgePainterRegistry;
use jellyflow_open_gpui::registry::{OpenGpuiNodeRegistry, OpenGpuiNodeRegistryBuilder};
use jellyflow_open_gpui::theme::OpenGpuiGraphTheme;
use jellyflow_runtime::schema::{NodeCatalog, NodeCatalogRegistrationError};
use open_gpui::{App, AppContext, Entity};

use crate::model_options::WorkflowModelOptionsState;
use crate::product_theme::NativeProductTheme;

/// Application-owned visual configuration shared by one flagship graph surface and its nodes.
///
/// The profile is constructed by the product entry point, then passed independently to the host
/// and retained node factories. It contains no process-global state, so two graph products can
/// select different visual vocabularies in the same process.
#[derive(Debug)]
pub struct NativeGraphVisualProfile {
    product_theme: NativeProductTheme,
}

impl NativeGraphVisualProfile {
    /// Returns the Shader product profile with typed Float chrome and a stronger wire hierarchy.
    #[must_use]
    pub fn shader() -> Self {
        Self {
            product_theme: NativeProductTheme::shader_blueprint(),
        }
    }

    /// Returns the Workflow product profile with its independent neutral wire vocabulary.
    #[must_use]
    pub fn workflow() -> Self {
        Self {
            product_theme: NativeProductTheme::workflow_dify(),
        }
    }

    /// Returns the application theme shared by the shell, host, and retained node entities.
    #[must_use]
    pub const fn product_theme(&self) -> NativeProductTheme {
        self.product_theme
    }

    /// Returns the graph-chrome projection shared with the native scene host.
    #[must_use]
    pub fn graph_theme(&self) -> OpenGpuiGraphTheme {
        self.product_theme.graph_theme()
    }

    /// Splits this profile into the values consumed by one native scene host.
    #[must_use]
    pub fn into_host_parts(self) -> (OpenGpuiGraphTheme, OpenGpuiEdgePainterRegistry) {
        (
            self.product_theme.graph_theme(),
            self.product_theme.edge_painter_registry(),
        )
    }
}

impl Default for NativeGraphVisualProfile {
    fn default() -> Self {
        Self::workflow()
    }
}

pub use shader::{
    BLUEPRINT_ADD_INPUT_SELECTOR, BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND,
    BLUEPRINT_CALL_RENDERER, BLUEPRINT_DISCARD_SELECTOR, BLUEPRINT_NODE_SELECTOR,
    BlueprintCallNode, SHADER_FACTOR_FIELD, SHADER_FLOAT_STYLE_SLOT, SHADER_MULTIPLY_DEFAULT_SIZE,
    SHADER_MULTIPLY_KIND, SHADER_MULTIPLY_RENDERER, SHADER_NODE_SELECTOR,
    SHADER_PREVIEW_CONTROL_SELECTOR, SHADER_PREVIEW_VALUE_SELECTOR, ShaderFieldStore,
    ShaderMultiplyNode, ShaderNodeError, blueprint_body_selector,
    blueprint_header_metadata_selector, blueprint_header_title_selector,
    blueprint_pin_down_selector, blueprint_pin_label_selector, blueprint_pin_remove_selector,
    blueprint_pin_row_selector, blueprint_pin_scroll_selector, blueprint_pin_up_selector,
    blueprint_port_label_selector, blueprint_port_selector, shader_body_selector,
    shader_connected_input_selector, shader_factor_accept_remote_selector,
    shader_factor_conflict_selector, shader_factor_reapply_selector,
    shader_header_metadata_selector, shader_header_title_selector, shader_inline_editor_selector,
    shader_inline_value_selector, shader_port_label_selector, shader_port_selector,
};
pub use workflow::{
    WORKFLOW_CONDITION_DEFAULT_SIZE, WORKFLOW_CONDITION_KIND, WORKFLOW_CONDITION_RENDERER,
    WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND, WORKFLOW_LLM_RENDERER, WORKFLOW_MODEL_FIELD,
    WORKFLOW_PROMPT_FIELD, WorkflowConditionNode, WorkflowFieldStore, WorkflowLlmNode,
    WorkflowNodeError, workflow_condition_add_selector, workflow_condition_body_selector,
    workflow_condition_case_actions_selector, workflow_condition_case_label_selector,
    workflow_condition_case_selector, workflow_condition_case_value_selector,
    workflow_condition_diagnostic_selector, workflow_condition_else_label_selector,
    workflow_condition_else_selector, workflow_condition_field_selector,
    workflow_condition_node_id, workflow_condition_node_selector,
    workflow_condition_pending_selector, workflow_condition_provider_failed_selector,
    workflow_condition_remove_selector, workflow_condition_scroll_selector,
    workflow_condition_up_selector, workflow_condition_value_selector, workflow_demo_graph,
    workflow_header_metadata_selector, workflow_header_title_selector, workflow_llm_body_selector,
    workflow_llm_node_id, workflow_llm_node_selector, workflow_model_accept_remote_selector,
    workflow_model_conflict_selector, workflow_model_option_selector,
    workflow_model_options_empty_selector, workflow_model_options_failed_selector,
    workflow_model_options_loading_selector, workflow_model_options_partial_selector,
    workflow_model_options_retry_selector, workflow_model_reapply_selector,
    workflow_model_selector, workflow_node_status_selector, workflow_port_selector,
    workflow_prompt_accept_remote_selector, workflow_prompt_apply_selector,
    workflow_prompt_conflict_selector, workflow_prompt_reapply_selector, workflow_prompt_selector,
};

fn sync_activity_lease(
    activity: &OpenGpuiNodeActivityHandle,
    lease: &mut Option<OpenGpuiActivityLease>,
    kind: OpenGpuiActivityKind,
    active: bool,
) -> Result<(), OpenGpuiActivityAcquireError> {
    if active && lease.is_none() {
        *lease = Some(activity.acquire(kind)?);
    } else if !active {
        lease.take();
    }
    Ok(())
}

/// Application-owned resources captured by retained product node factories.
#[derive(Clone)]
pub struct NativeNodeResources {
    workflow_field_store: WorkflowFieldStore,
    shader_field_store: ShaderFieldStore,
    workflow_model_options: Entity<WorkflowModelOptionsState>,
    product_theme: NativeProductTheme,
}

impl NativeNodeResources {
    #[must_use]
    pub fn new(workflow_model_options: Entity<WorkflowModelOptionsState>) -> Self {
        Self {
            workflow_field_store: WorkflowFieldStore::new(),
            shader_field_store: ShaderFieldStore::new(),
            workflow_model_options,
            product_theme: NativeProductTheme::default(),
        }
    }

    #[must_use]
    pub fn ready_default(cx: &mut App) -> Self {
        Self::new(cx.new(|_| WorkflowModelOptionsState::ready_default()))
    }

    #[must_use]
    pub fn with_field_stores(
        mut self,
        workflow_field_store: WorkflowFieldStore,
        shader_field_store: ShaderFieldStore,
    ) -> Self {
        self.workflow_field_store = workflow_field_store;
        self.shader_field_store = shader_field_store;
        self
    }

    /// Injects the product theme used by retained node surfaces and ports.
    #[must_use]
    pub fn with_product_theme(mut self, product_theme: NativeProductTheme) -> Self {
        self.product_theme = product_theme;
        self
    }
}

/// Builds the semantic node catalog paired with [`native_node_registry`].
pub fn native_node_catalog() -> Result<NodeCatalog, NodeCatalogRegistrationError> {
    let mut catalog = NodeCatalog::new();
    for spec in shader::catalog_specs()
        .into_iter()
        .chain(workflow::catalog_specs())
    {
        catalog.register(spec)?;
    }
    Ok(catalog)
}

/// Registers every retained native node with explicit application-owned resources.
pub fn register_native_nodes(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    resources: NativeNodeResources,
) {
    shader::register_with_field_store(
        registry,
        resources.shader_field_store,
        resources.product_theme,
    );
    workflow::register(
        registry,
        resources.workflow_field_store,
        resources.workflow_model_options,
        resources.product_theme,
    );
}

/// Builds the immutable renderer registry consumed by an [`OpenGpuiSceneHost`].
///
/// [`OpenGpuiSceneHost`]: jellyflow_open_gpui::host::OpenGpuiSceneHost
#[must_use]
pub fn native_node_registry(
    resources: NativeNodeResources,
    catalog: &NodeCatalog,
) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_native_nodes(&mut registry, resources);
    registry
        .seal(catalog)
        .expect("native node registrations must be valid")
}

#[cfg(test)]
mod tests {
    use jellyflow_core::TypeDesc;

    use super::{NativeGraphVisualProfile, SHADER_FLOAT_STYLE_SLOT};

    #[test]
    fn shader_visual_profile_keeps_typed_ports_and_wires_on_one_explicit_theme() {
        let profile = NativeGraphVisualProfile::shader();
        let expected_theme = profile.graph_theme();
        let (host_theme, painters) = profile.into_host_parts();

        assert_eq!(host_theme, expected_theme);
        assert_eq!(host_theme.metrics.edge_ordinary_width, 2.0);
        assert_eq!(host_theme.metrics.edge_hovered_width, 2.5);
        assert_eq!(host_theme.metrics.edge_selected_width, 3.0);
        assert_eq!(host_theme.metrics.edge_active_width, 3.0);
        assert!(host_theme.style_slot(SHADER_FLOAT_STYLE_SLOT).is_some());
        assert_eq!(
            painters.style_slot_for_type(&TypeDesc::Float),
            Some(SHADER_FLOAT_STYLE_SLOT)
        );
    }

    #[test]
    fn workflow_profile_remains_independent_and_neutral_by_default() {
        let workflow = NativeGraphVisualProfile::workflow();
        let shader = NativeGraphVisualProfile::shader();
        let workflow_product = workflow.product_theme();
        let shader_product = shader.product_theme();
        let (workflow_theme, workflow_painters) = workflow.into_host_parts();
        let (shader_theme, _) = shader.into_host_parts();

        assert!(workflow_painters.is_empty());
        assert_eq!(workflow_theme.style_slot(SHADER_FLOAT_STYLE_SLOT), None);
        assert_ne!(workflow_theme.metrics, shader_theme.metrics);
        assert_ne!(workflow_product, shader_product);
    }
}
