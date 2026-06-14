use crate::runtime::conformance::{ConformanceScenario, run_conformance_scenario};
use jellyflow_core::core::{
    Graph, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn assert_conformance_trace(scenario: &ConformanceScenario) {
    let report = run_conformance_scenario(scenario).expect("run conformance scenario");
    assert!(report.is_match(), "{report}");
}

pub(super) fn insert_input_port(graph: &mut Graph, node: NodeId, key: &str) -> PortId {
    let port_id = PortId::new();
    let from = graph.nodes().get(&node).expect("node exists").ports.clone();
    let mut to = from.clone();
    to.push(port_id);

    GraphTransaction::from_ops([
        GraphOp::AddPort {
            id: port_id,
            port: Port {
                node,
                key: PortKey::new(key),
                dir: PortDirection::In,
                kind: PortKind::Data,
                capacity: PortCapacity::Single,
                connectable: None,
                connectable_start: None,
                connectable_end: None,
                ty: None,
                data: serde_json::Value::Null,
            },
        },
        GraphOp::SetNodePorts { id: node, from, to },
    ])
    .apply_to(graph)
    .expect("insert input port");

    port_id
}
