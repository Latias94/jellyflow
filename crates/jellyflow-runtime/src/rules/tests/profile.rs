use super::fixtures::{insert_port, make_node, make_port};

use crate::profile::GraphProfile;
use crate::rules::{ConnectDecision, Diagnostic};
use jellyflow_core::core::{Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;

#[test]
fn graph_profile_default_plan_connect_uses_type_of_port() {
    struct TypedProfile;

    impl GraphProfile for TypedProfile {
        fn type_of_port(&mut self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
            graph.ports.get(&port).and_then(|port| port.ty.clone())
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out = PortId::new();
    let inn = PortId::new();
    let mut out_port = make_port(
        a,
        "out",
        PortDirection::Out,
        PortKind::Data,
        PortCapacity::Multi,
    );
    out_port.ty = Some(TypeDesc::Int);
    insert_port(&mut graph, out, out_port);

    let mut in_port = make_port(
        b,
        "in",
        PortDirection::In,
        PortKind::Data,
        PortCapacity::Single,
    );
    in_port.ty = Some(TypeDesc::String);
    insert_port(&mut graph, inn, in_port);

    let mut profile = TypedProfile;
    let plan = profile.plan_connect(&graph, out, inn, NodeGraphConnectionMode::Strict);

    assert_eq!(plan.decision, ConnectDecision::Reject);
    assert!(plan.ops.is_empty());
}
