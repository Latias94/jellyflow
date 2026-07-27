use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, NodeId, NodeKindKey, PortId,
    Symbol, SymbolId,
};
use jellyflow_runtime::schema::NodeCatalog;
use open_gpui_canvas_jellyflow::nodes::SHADER_MULTIPLY_KIND;

pub(crate) fn shader_demo_graph(catalog: &NodeCatalog) -> Result<Graph, String> {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0x7000));
    graph.insert_symbol(
        SymbolId::from_u128(0x7001),
        Symbol {
            name: "exposure".to_owned(),
            ty: Some(TypeDesc::Float),
            default_value: Some(serde_json::json!(1.0)),
            meta: serde_json::json!({ "group": "material" }),
        },
    );
    let nodes = [
        (NodeId::from_u128(0x7100), CanvasPoint { x: 40.0, y: 300.0 }),
        (
            NodeId::from_u128(0x7200),
            CanvasPoint { x: 400.0, y: 120.0 },
        ),
        (
            NodeId::from_u128(0x7300),
            CanvasPoint { x: 760.0, y: 330.0 },
        ),
    ];

    for (index, (node_id, position)) in nodes.into_iter().enumerate() {
        let input_a = PortId::from_u128(0x8000 + index as u128 * 3);
        let input_b = PortId::from_u128(0x8001 + index as u128 * 3);
        let output = PortId::from_u128(0x8002 + index as u128 * 3);
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(SHADER_MULTIPLY_KIND),
                node_id,
                position,
                [input_a, input_b, output],
            )
            .map_err(|error| format!("shader catalog ports do not match: {error:?}"))?;
        let (node_id, mut node, ports) = creation.into_parts();
        node.data = serde_json::json!({
            "title": format!("Multiply {}", index + 1),
            "factor": 0.5 + index as f32 * 0.25,
            "preview": 1.0 + index as f32 * 0.5,
        });
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
    }

    graph.insert_edge(
        EdgeId::from_u128(0x9000),
        Edge::new(
            EdgeKind::Data,
            PortId::from_u128(0x8002),
            PortId::from_u128(0x8003),
        ),
    );
    graph.insert_edge(
        EdgeId::from_u128(0x9001),
        Edge::new(
            EdgeKind::Data,
            PortId::from_u128(0x8005),
            PortId::from_u128(0x8006),
        ),
    );
    graph
        .build()
        .map_err(|errors| format!("invalid shader demo graph: {errors:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_runtime::graph::GraphRuntime;
    use open_gpui_canvas_jellyflow::ShaderGraphProfile;
    use open_gpui_canvas_jellyflow::nodes::native_node_catalog;

    #[test]
    fn shader_demo_starts_the_headless_runtime_with_typed_edges() {
        let catalog = native_node_catalog().expect("native node catalog");
        let graph = shader_demo_graph(&catalog).expect("valid shader demo graph");
        let runtime = GraphRuntime::builder(catalog)
            .with_profile(Box::new(ShaderGraphProfile))
            .build(graph)
            .expect("shader demo runtime");
        let snapshot = runtime.snapshot().expect("shader demo snapshot");

        assert_eq!(snapshot.graph().nodes().len(), 3);
        assert_eq!(snapshot.graph().edges().len(), 2);
        assert_eq!(snapshot.graph().symbols().len(), 1);
        assert!(
            snapshot
                .graph()
                .ports()
                .values()
                .all(|port| port.ty == Some(TypeDesc::Float))
        );
    }

    #[test]
    fn shader_demo_static_ports_match_the_native_catalog() {
        let catalog = native_node_catalog().expect("native node catalog");
        let graph = shader_demo_graph(&catalog).expect("valid shader demo graph");

        for node in graph.nodes().values() {
            let spec = catalog.get(&node.kind).expect("demo node kind in catalog");
            assert_eq!(node.size, spec.default_size());
            assert_eq!(node.ports.len(), spec.ports().len());
            for (port_id, port_spec) in node.ports.iter().zip(spec.ports()) {
                let port = graph.port(port_id).expect("demo port");
                assert_eq!(&port.key, port_spec.key());
                assert_eq!(port.dir, port_spec.direction());
                assert_eq!(port.kind, port_spec.kind());
                assert_eq!(port.capacity, port_spec.capacity());
                assert_eq!(port.ty.as_ref(), port_spec.ty());
            }
        }
    }

    #[test]
    fn shader_demo_reserves_readable_wire_gutters_between_nodes() {
        let catalog = native_node_catalog().expect("native node catalog");
        let graph = shader_demo_graph(&catalog).expect("valid shader demo graph");
        let mut nodes = graph.nodes().values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.pos.x.total_cmp(&right.pos.x));

        for pair in nodes.windows(2) {
            let left = pair[0];
            let right = pair[1];
            let left_width = left
                .size
                .expect("shader node has an authoritative size")
                .width;
            assert!(right.pos.x - left.pos.x - left_width >= 96.0);
        }
    }
}
