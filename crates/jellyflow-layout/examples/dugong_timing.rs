use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_layout::{LayoutOptions, LayoutRequest, LayoutSpacing, layout_graph_with_dugong};

const NODE_SIZE: CanvasSize = CanvasSize {
    width: 160.0,
    height: 72.0,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dagreish_timing_enabled = env_flag_enabled("DUGONG_DAGREISH_TIMING");
    let order_timing_enabled = env_flag_enabled("DUGONG_ORDER_TIMING");

    if !dagreish_timing_enabled && !order_timing_enabled {
        eprintln!(
            "Set DUGONG_DAGREISH_TIMING=1 for pipeline timings; add DUGONG_ORDER_TIMING=1 for order-stage timings."
        );
    }

    for layer_count in [10_usize, 25, 50] {
        let graph = layered_dag_fixture(layer_count, 10);
        let request = workflow_request();
        let result = layout_graph_with_dugong(&graph, &request)?;

        println!(
            "fixture=layered_dag layers={} width=10 nodes={} edges={} positioned_nodes={} routed_edges={}",
            layer_count,
            graph.nodes().len(),
            graph.edges().len(),
            result.nodes.len(),
            result.edge_routes.len()
        );
    }

    Ok(())
}

fn env_flag_enabled(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

fn workflow_request() -> LayoutRequest {
    LayoutRequest::all().with_options(LayoutOptions {
        spacing: LayoutSpacing {
            nodesep: 48.0,
            ranksep: 72.0,
            edgesep: 24.0,
        },
        default_node_size: NODE_SIZE,
        ..LayoutOptions::default()
    })
}

fn layered_dag_fixture(layer_count: usize, layer_width: usize) -> Graph {
    let mut builder = FixtureGraphBuilder::new(4);
    let mut previous_layer = Vec::new();

    for _ in 0..layer_count {
        let current_layer = (0..layer_width)
            .map(|_| builder.add_node())
            .collect::<Vec<_>>();

        if !previous_layer.is_empty() {
            for (index, node) in current_layer.iter().copied().enumerate() {
                let source = previous_layer[index % previous_layer.len()];
                builder.add_edge(source, node);
                if index % 3 == 0 {
                    let extra_source = previous_layer[(index + 1) % previous_layer.len()];
                    builder.add_edge(extra_source, node);
                }
            }
        }

        previous_layer = current_layer;
    }

    builder.into_graph()
}

struct FixtureGraphBuilder {
    graph: GraphBuilder,
    next_node: u128,
    next_port: u128,
    next_edge: u128,
}

impl FixtureGraphBuilder {
    fn new(graph_id: u128) -> Self {
        Self {
            graph: GraphBuilder::new(GraphId::from_u128(graph_id)),
            next_node: 1,
            next_port: 10_000,
            next_edge: 20_000,
        }
    }

    fn add_node(&mut self) -> NodeId {
        let node = NodeId::from_u128(self.next_node);
        self.next_node += 1;
        self.graph.insert_node(node, node_fixture(node, Vec::new()));
        node
    }

    fn add_edge(&mut self, source: NodeId, target: NodeId) -> EdgeId {
        let source_port = self.add_port(source, PortDirection::Out);
        let target_port = self.add_port(target, PortDirection::In);
        let edge = EdgeId::from_u128(self.next_edge);
        self.next_edge += 1;

        self.graph
            .insert_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port));

        edge
    }

    fn add_port(&mut self, node: NodeId, dir: PortDirection) -> PortId {
        let port = PortId::from_u128(self.next_port);
        self.next_port += 1;

        let key = match dir {
            PortDirection::In => format!("in.{}", port.0),
            PortDirection::Out => format!("out.{}", port.0),
        };
        self.graph.insert_port(
            port,
            Port {
                node,
                key: PortKey::new(key),
                dir,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                connectable: None,
                connectable_start: None,
                connectable_end: None,
                ty: None,
                data: serde_json::Value::Null,
            },
        );
        self.graph
            .update_node(&node, |node| node.ports.push(port))
            .expect("node exists");

        port
    }

    fn into_graph(self) -> Graph {
        self.graph.build_unchecked()
    }
}

fn node_fixture(node: NodeId, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(format!("timing.node.{}", node.0)),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(NODE_SIZE),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}
