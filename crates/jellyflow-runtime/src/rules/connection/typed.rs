use crate::rules::ConnectPlan;
use crate::runtime::policy::GraphRuntimePolicy;
use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeDesc};

use super::common::{ConnectionPlanningContext, type_compatibility_rejection};
use super::connect::plan_resolved_connect_using;

pub(crate) fn plan_connect_typed_with_mode_and_policy_using(
    graph: &Graph,
    edge: EdgeId,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    policy: &GraphRuntimePolicy,
    mut type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &dyn TypeCompatibility,
    query: &mut dyn ConnectionPlanningContext,
) -> ConnectPlan {
    let planned = match plan_resolved_connect_using(graph, a, b, mode, policy, edge, query) {
        Ok(planned) => planned,
        Err(plan) => return plan,
    };
    if let Some(rejection) =
        type_compatibility_rejection(graph, planned.endpoints(), &mut type_of, compat)
    {
        return rejection;
    }

    planned.into_plan()
}
