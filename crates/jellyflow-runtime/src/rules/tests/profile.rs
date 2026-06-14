use super::fixtures::{insert_node, insert_typed_data_input, insert_typed_data_output};

use crate::profile::GraphProfile;
use crate::rules::Diagnostic;
use jellyflow_core::core::{Graph, GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;

#[test]
fn graph_profile_default_plan_connect_uses_type_of_port() {
    struct TypedProfile;

    impl GraphProfile for TypedProfile {
        fn type_of_port(&mut self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            graph.ports().get(&port).and_then(|port| port.ty.clone())
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_typed_data_output(
        &mut graph,
        out,
        a,
        "out",
        PortCapacity::Multi,
        TypeDesc::Int,
    );
    insert_typed_data_input(
        &mut graph,
        inn,
        b,
        "in",
        PortCapacity::Single,
        TypeDesc::String,
    );

    let mut profile = TypedProfile;
    let plan = profile.plan_connect(&graph, out, inn, NodeGraphConnectionMode::Strict);

    assert!(plan.is_reject());
    assert!(plan.ops().is_empty());
}
