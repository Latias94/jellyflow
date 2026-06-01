use crate::runtime::conformance::{ConformanceScenario, run_conformance_scenario};
use jellyflow_core::core::{
    Graph, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

pub(super) fn assert_conformance_trace(scenario: &ConformanceScenario) {
    let report = run_conformance_scenario(scenario).expect("run conformance scenario");
    assert!(report.is_match(), "{report}");
}

pub(super) fn insert_input_port(graph: &mut Graph, node: NodeId, key: &str) -> PortId {
    let port_id = PortId::new();
    graph
        .nodes
        .get_mut(&node)
        .expect("node exists")
        .ports
        .push(port_id);
    graph.ports.insert(
        port_id,
        Port {
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
    );
    port_id
}
