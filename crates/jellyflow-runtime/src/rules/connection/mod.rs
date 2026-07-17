mod common;
mod connect;
mod reconnect;
mod typed;

pub(crate) use common::{
    ConnectionPlanningContext, ConnectionPortFacts, connection_semantics_accepts,
    edge_type_compatibility_diagnostic,
};
pub(crate) use reconnect::plan_reconnect_edge_typed_with_mode_and_policy_using;
pub(crate) use typed::plan_connect_typed_with_mode_and_policy_using;
