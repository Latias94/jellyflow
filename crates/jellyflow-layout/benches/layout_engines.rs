use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder as CoreGraphBuilder,
    GraphId, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_layout::{
    LayoutOptions, LayoutRequest, LayoutSpacing, layout_graph_with_dugong,
    layout_graph_with_tidy_tree,
};

const NODE_SIZE: CanvasSize = CanvasSize {
    width: 160.0,
    height: 72.0,
};

fn layout_engine_benchmarks(c: &mut Criterion) {
    benchmark_tidy_tree_balanced(c);
    benchmark_tidy_tree_wide(c);
    benchmark_dugong_layered(c);
}

fn benchmark_tidy_tree_balanced(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_tidy_tree_balanced");

    for depth in [4_usize, 6, 8] {
        let graph = balanced_tree_fixture(depth, 3);
        let node_count = graph.nodes().len();
        let request = tree_request();

        group.throughput(Throughput::Elements(node_count as u64));
        group.bench_with_input(
            BenchmarkId::new("plan_branching_3", node_count),
            &graph,
            |b, graph| {
                b.iter(|| {
                    black_box(
                        layout_graph_with_tidy_tree(black_box(graph), black_box(&request))
                            .expect("tidy tree layout"),
                    )
                })
            },
        );
    }

    group.finish();
}

fn benchmark_tidy_tree_wide(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_tidy_tree_wide");

    for child_count in [100_usize, 1_000, 5_000] {
        let graph = wide_tree_fixture(child_count);
        let node_count = graph.nodes().len();
        let request = tree_request();

        group.throughput(Throughput::Elements(node_count as u64));
        group.bench_with_input(
            BenchmarkId::new("plan_root_children", node_count),
            &graph,
            |b, graph| {
                b.iter(|| {
                    black_box(
                        layout_graph_with_tidy_tree(black_box(graph), black_box(&request))
                            .expect("tidy tree layout"),
                    )
                })
            },
        );
    }

    group.finish();
}

fn benchmark_dugong_layered(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_dugong_layered");

    for layer_count in [10_usize, 25, 50] {
        let graph = layered_dag_fixture(layer_count, 10);
        let node_count = graph.nodes().len();
        let request = workflow_request();
        let layout = layout_graph_with_dugong(&graph, &request)
            .expect("dugong layout for benchmark fixture");

        group.throughput(Throughput::Elements(node_count as u64));

        group.bench_with_input(BenchmarkId::new("plan", node_count), &graph, |b, graph| {
            b.iter(|| {
                black_box(
                    layout_graph_with_dugong(black_box(graph), black_box(&request))
                        .expect("dugong layout"),
                )
            })
        });

        group.bench_with_input(
            BenchmarkId::new("to_transaction", node_count),
            &layout,
            |b, layout| {
                b.iter(|| {
                    black_box(
                        layout
                            .to_transaction(black_box(&graph))
                            .expect("transaction"),
                    );
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("plan_then_to_transaction", node_count),
            &graph,
            |b, graph| {
                b.iter(|| {
                    let layout = layout_graph_with_dugong(black_box(graph), black_box(&request))
                        .expect("dugong layout");
                    let tx = layout
                        .to_transaction(black_box(graph))
                        .expect("transaction");
                    black_box((layout, tx));
                })
            },
        );
    }

    group.finish();
}

fn tree_request() -> LayoutRequest {
    LayoutRequest::all().with_options(LayoutOptions {
        spacing: LayoutSpacing {
            nodesep: 32.0,
            ranksep: 72.0,
            edgesep: 16.0,
        },
        default_node_size: NODE_SIZE,
        ..LayoutOptions::default()
    })
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

fn balanced_tree_fixture(depth: usize, branching: usize) -> Graph {
    let mut builder = FixtureGraphBuilder::new(1);
    let root = builder.add_node();
    let mut frontier = vec![root];

    for _ in 0..depth {
        let mut next_frontier = Vec::with_capacity(frontier.len() * branching);
        for parent in frontier {
            for _ in 0..branching {
                let child = builder.add_node();
                builder.add_edge(parent, child);
                next_frontier.push(child);
            }
        }
        frontier = next_frontier;
    }

    builder.into_graph()
}

fn wide_tree_fixture(child_count: usize) -> Graph {
    let mut builder = FixtureGraphBuilder::new(2);
    let root = builder.add_node();

    for _ in 0..child_count {
        let child = builder.add_node();
        builder.add_edge(root, child);
    }

    builder.into_graph()
}

fn layered_dag_fixture(layer_count: usize, layer_width: usize) -> Graph {
    let mut builder = FixtureGraphBuilder::new(3);
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
    graph: CoreGraphBuilder,
    next_node: u128,
    next_port: u128,
    next_edge: u128,
}

impl FixtureGraphBuilder {
    fn new(graph_id: u128) -> Self {
        Self {
            graph: CoreGraphBuilder::new(GraphId::from_u128(graph_id)),
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

        self.graph.insert_edge(
            edge,
            Edge {
                kind: EdgeKind::Data,
                from: source_port,
                to: target_port,
                hidden: false,
                selectable: None,
                focusable: None,
                interaction_width: None,
                deletable: None,
                reconnectable: None,
            },
        );

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
        kind: NodeKindKey::new(format!("bench.node.{}", node.0)),
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

criterion_group!(benches, layout_engine_benchmarks);
criterion_main!(benches);
