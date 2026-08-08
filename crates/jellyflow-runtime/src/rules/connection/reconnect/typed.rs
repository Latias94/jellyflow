use crate::rules::{ConnectPlan, EdgeEndpoint};
use crate::runtime::policy::GraphRuntimePolicy;
use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{TypeCompatibility, TypeDesc};

use super::super::common::{ConnectionPlanningContext, type_compatibility_rejection};
use super::planner::plan_resolved_reconnect_using;

pub(crate) fn plan_reconnect_edge_typed_with_mode_and_policy_using(
    graph: &Graph,
    edge_id: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
    mode: NodeGraphConnectionMode,
    policy: &GraphRuntimePolicy,
    mut type_of: impl FnMut(&Graph, PortId) -> Option<TypeDesc>,
    compat: &dyn TypeCompatibility,
    query: &mut dyn ConnectionPlanningContext,
) -> ConnectPlan {
    let planned = match plan_resolved_reconnect_using(
        graph, edge_id, endpoint, new_port, mode, policy, query,
    ) {
        Ok(planned) => planned,
        Err(rejection) => return rejection,
    };

    if let Some(rejection) =
        type_compatibility_rejection(graph, planned.endpoints(), &mut type_of, compat)
    {
        return rejection;
    }

    planned.into_plan()
}
