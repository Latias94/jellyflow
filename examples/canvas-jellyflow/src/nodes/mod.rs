//! Retained native node entities used by the Open GPUI product example.

mod shader;
mod topology_attempt;
mod workflow;

use jellyflow::runtime::schema::{NodeCatalog, NodeCatalogRegistrationError};
use jellyflow_open_gpui::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiGraphTheme,
    OpenGpuiNodeActivityRegistry, OpenGpuiNodeRegistry, OpenGpuiNodeRegistryBuilder,
};
use open_gpui::{App, AppContext, Entity};

use crate::model_options::WorkflowModelOptionsState;

/// Returns the graph chrome theme shared by the flagship host and native node markers.
#[must_use]
pub fn native_graph_theme() -> OpenGpuiGraphTheme {
    OpenGpuiGraphTheme::default()
}

pub use shader::{
    BLUEPRINT_ADD_INPUT_SELECTOR, BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND,
    BLUEPRINT_CALL_RENDERER, BLUEPRINT_NODE_SELECTOR, BlueprintCallNode, BlueprintPin,
    BlueprintPinAction, SHADER_FACTOR_FIELD, SHADER_MULTIPLY_DEFAULT_SIZE, SHADER_MULTIPLY_KIND,
    SHADER_MULTIPLY_RENDERER, SHADER_NODE_SELECTOR, SHADER_PREVIEW_CONTROL_SELECTOR,
    SHADER_PREVIEW_VALUE_SELECTOR, ShaderFieldStore, ShaderMultiplyNode, ShaderNodeError,
    blueprint_pin_down_selector, blueprint_pin_remove_selector, blueprint_pin_row_selector,
    blueprint_pin_scroll_selector, blueprint_pin_up_selector, blueprint_pins,
    blueprint_port_selector, plan_blueprint_pin_action, shader_connected_input_selector,
    shader_factor_accept_remote_selector, shader_factor_conflict_selector,
    shader_factor_reapply_selector, shader_inline_editor_selector, shader_inline_value_selector,
    shader_port_selector,
};
pub use workflow::{
    ConditionCaseAction, WORKFLOW_CONDITION_DEFAULT_SIZE, WORKFLOW_CONDITION_KIND,
    WORKFLOW_CONDITION_RENDERER, WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND,
    WORKFLOW_LLM_RENDERER, WORKFLOW_MODEL_FIELD, WORKFLOW_PROMPT_FIELD, WorkflowCase,
    WorkflowConditionNode, WorkflowConditionOutputs, WorkflowElseBranch, WorkflowFieldStore,
    WorkflowLlmNode, WorkflowNodeError, condition_cases, condition_outputs,
    plan_condition_case_action, workflow_condition_add_selector, workflow_condition_case_selector,
    workflow_condition_else_selector, workflow_condition_node_id,
    workflow_condition_pending_selector, workflow_condition_remove_selector,
    workflow_condition_scroll_selector, workflow_condition_up_selector, workflow_demo_graph,
    workflow_llm_node_id, workflow_model_accept_remote_selector, workflow_model_conflict_selector,
    workflow_model_option_selector, workflow_model_options_empty_selector,
    workflow_model_options_failed_selector, workflow_model_options_loading_selector,
    workflow_model_options_partial_selector, workflow_model_options_retry_selector,
    workflow_model_reapply_selector, workflow_model_selector, workflow_node_status_selector,
    workflow_port_selector, workflow_prompt_accept_remote_selector, workflow_prompt_apply_selector,
    workflow_prompt_conflict_selector, workflow_prompt_reapply_selector, workflow_prompt_selector,
};

fn sync_activity_lease(
    activity: &OpenGpuiNodeActivityRegistry,
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
}

impl NativeNodeResources {
    #[must_use]
    pub fn new(workflow_model_options: Entity<WorkflowModelOptionsState>) -> Self {
        Self {
            workflow_field_store: WorkflowFieldStore::new(),
            shader_field_store: ShaderFieldStore::new(),
            workflow_model_options,
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
    shader::register_with_field_store(registry, resources.shader_field_store);
    workflow::register(
        registry,
        resources.workflow_field_store,
        resources.workflow_model_options,
    );
}

/// Builds the immutable renderer registry consumed by an [`OpenGpuiSceneHost`].
///
/// [`OpenGpuiSceneHost`]: jellyflow_open_gpui::OpenGpuiSceneHost
#[must_use]
pub fn native_node_registry(resources: NativeNodeResources) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    register_native_nodes(&mut registry, resources);
    registry
        .seal()
        .expect("native node registrations must be valid")
}
