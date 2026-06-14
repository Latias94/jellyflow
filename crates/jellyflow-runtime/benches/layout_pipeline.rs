use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::layout::{
    LayoutEngineRequest, LayoutOptions, LayoutRequest, LayoutSpacing,
    builtin_layout_engine_registry, layout_context_from_store,
};
use jellyflow_runtime::runtime::measurement::NodeMeasurement;
use jellyflow_runtime::runtime::store::NodeGraphStore;

const NODE_SIZE: CanvasSize = CanvasSize {
    width: 160.0,
    height: 72.0,
};

fn layout_pipeline_benchmarks(c: &mut Criterion) {
    benchmark_runtime_dugong_pipeline(c);
}

fn benchmark_runtime_dugong_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_layout_dugong_pipeline");
    let registry = builtin_layout_engine_registry();

    for layer_count in [10_usize, 25, 50] {
        let graph = layered_dag_fixture(layer_count, 10);
        let node_count = graph.nodes().len();
        let request = LayoutEngineRequest::dugong(workflow_request());
        let store = measured_store(graph.clone());
        let context = layout_context_from_store(&store);
        let layout = store
            .plan_layout(&request, registry)
            .expect("layout plan for benchmark fixture");

        group.throughput(Throughput::Elements(node_count as u64));

        group.bench_with_input(
            BenchmarkId::new("context_from_store", node_count),
            &store,
            |b, store| {
                b.iter(|| black_box(layout_context_from_store(black_box(store))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("store_plan_layout", node_count),
            &store,
            |b, store| {
                b.iter(|| {
                    black_box(
                        store
                            .plan_layout(black_box(&request), black_box(registry))
                            .expect("layout plan"),
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("manual_context_then_registry_layout", node_count),
            &store,
            |b, store| {
                b.iter(|| {
                    let context = layout_context_from_store(black_box(store));
                    black_box(
                        registry
                            .layout(black_box(store.graph()), black_box(&request), &context)
                            .expect("layout plan"),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("to_transaction", node_count),
            &layout,
            |b, layout| {
                b.iter(|| {
                    black_box(
                        layout
                            .to_transaction(black_box(store.graph()))
                            .expect("layout transaction"),
                    )
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("apply", node_count), &graph, |b, graph| {
            b.iter_batched(
                || measured_store(graph.clone()),
                |mut store| {
                    black_box(
                        store
                            .apply_layout(black_box(&request), black_box(registry))
                            .expect("apply layout"),
                    )
                },
                BatchSize::SmallInput,
            );
        });

        group.bench_with_input(
            BenchmarkId::new("registry_layout_with_cached_context", node_count),
            &context,
            |b, context| {
                b.iter(|| {
                    black_box(
                        registry
                            .layout(
                                black_box(store.graph()),
                                black_box(&request),
                                black_box(context),
                            )
                            .expect("layout plan"),
                    )
                });
            },
        );
    }

    group.finish();
}

fn measured_store(graph: Graph) -> NodeGraphStore {
    let mut store = NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let nodes = store.graph().nodes().keys().copied().collect::<Vec<_>>();

    for node in nodes {
        store
            .report_node_measurement(NodeMeasurement::new(node).with_size(Some(NODE_SIZE)))
            .expect("benchmark node measurement");
    }

    store
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
    let mut builder = FixtureGraphBuilder::new(1);
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

criterion_group!(benches, layout_pipeline_benchmarks);
criterion_main!(benches);
