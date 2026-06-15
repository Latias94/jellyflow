use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jellyflow_core::{
    CanvasPoint, CanvasSize, Graph, GraphId, NodeKindKey, PortCapacity, PortDirection, PortKey,
    PortKind,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
use jellyflow_runtime::runtime::create_node::CreateNodeRequest;
use jellyflow_runtime::schema::{NodeRegistry, NodeSchema, PortDecl};
use serde_json::json;

fn schema_create_node_benchmarks(c: &mut Criterion) {
    benchmark_view_descriptors(c);
    benchmark_node_instantiation(c);
    benchmark_store_create_node(c);
}

fn benchmark_view_descriptors(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_schema_view_descriptors");

    for schema_count in [10_usize, 100, 1_000] {
        group.throughput(Throughput::Elements(schema_count as u64));
        let registry = registry_fixture(schema_count, 4);

        group.bench_with_input(
            BenchmarkId::new("descriptor_list", schema_count),
            &registry,
            |b, registry| b.iter(|| black_box(registry.view_descriptors())),
        );
    }

    group.finish();
}

fn benchmark_node_instantiation(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_schema_node_instantiation");
    let pos = CanvasPoint { x: 24.0, y: 48.0 };

    for port_count in [1_usize, 4, 16] {
        group.throughput(Throughput::Elements(port_count as u64));
        let registry = registry_fixture(100, port_count);
        let kind = NodeKindKey::new("bench.kind.050");
        let alias = NodeKindKey::new("bench.alias.050");

        group.bench_with_input(
            BenchmarkId::new("canonical_kind", port_count),
            &registry,
            |b, registry| {
                b.iter(|| {
                    black_box(
                        registry
                            .instantiate_node(black_box(&kind), black_box(pos))
                            .expect("schema exists"),
                    )
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("alias_kind", port_count),
            &registry,
            |b, registry| {
                b.iter(|| {
                    black_box(
                        registry
                            .instantiate_node(black_box(&alias), black_box(pos))
                            .expect("schema exists"),
                    )
                })
            },
        );
    }

    group.finish();
}

fn benchmark_store_create_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_schema_store_create_node");
    let pos = CanvasPoint { x: 24.0, y: 48.0 };

    for port_count in [1_usize, 4, 16] {
        group.throughput(Throughput::Elements(port_count as u64));
        let registry = registry_fixture(100, port_count);
        let request = CreateNodeRequest::new(NodeKindKey::new("bench.alias.050"), pos);

        group.bench_with_input(
            BenchmarkId::new("dispatch_empty_graph", port_count),
            &request,
            |b, request| {
                b.iter_batched(
                    empty_store,
                    |mut store| {
                        black_box(
                            store
                                .apply_create_node_from_schema(
                                    black_box(&registry),
                                    black_box(request.clone()),
                                )
                                .expect("create node"),
                        )
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn empty_store() -> NodeGraphStore {
    NodeGraphStore::new(
        Graph::new(GraphId::from_u128(1)),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    )
}

fn registry_fixture(schema_count: usize, port_count: usize) -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    for index in 0..schema_count {
        registry.register(schema_fixture(index, port_count));
    }
    registry
}

fn schema_fixture(index: usize, port_count: usize) -> NodeSchema {
    NodeSchema::builder(
        format!("bench.kind.{index:03}"),
        format!("Bench Node {index}"),
    )
    .alias(format!("bench.alias.{index:03}"))
    .category(["Bench".to_owned(), format!("Group {}", index % 10)])
    .keywords([format!("node-{index}"), "benchmark".to_owned()])
    .renderer_key("bench-node")
    .default_size(CanvasSize {
        width: 160.0,
        height: 96.0,
    })
    .ports((0..port_count).map(port_decl_fixture))
    .default_data(json!({
        "index": index,
        "label": format!("node {index}"),
        "enabled": true
    }))
    .build()
}

fn port_decl_fixture(index: usize) -> PortDecl {
    PortDecl {
        key: PortKey::new(format!("port.{index:02}")),
        dir: if index.is_multiple_of(2) {
            PortDirection::In
        } else {
            PortDirection::Out
        },
        kind: PortKind::Data,
        capacity: if index.is_multiple_of(3) {
            PortCapacity::Single
        } else {
            PortCapacity::Multi
        },
        ty: None,
        label: Some(format!("Port {index}")),
    }
}

criterion_group!(benches, schema_create_node_benchmarks);
criterion_main!(benches);
