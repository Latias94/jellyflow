use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_runtime::NodeGraphStore;
use jellyflow_runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};

const NODE_SIZE: CanvasSize = CanvasSize {
    width: 120.0,
    height: 64.0,
};
const GRID_COLUMNS: usize = 250;
const GRID_X_SPACING: f32 = 160.0;
const GRID_Y_SPACING: f32 = 96.0;
const VIEWPORT_SEQUENCE_STEPS: usize = 16;

fn rendering_query_benchmarks(c: &mut Criterion) {
    benchmark_single_reads(c);
    benchmark_first_spatial_reads(c);
    benchmark_visibility_policies(c);
    benchmark_viewport_sequences(c);
}

fn benchmark_single_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_rendering_query_single");

    for node_count in [1_000_usize, 10_000, 50_000] {
        group.throughput(Throughput::Elements(node_count as u64));
        let graph = graph_fixture(node_count);
        let local_viewport = local_viewport();
        let full_viewport = full_viewport(node_count);

        for scenario in [
            QueryScenario::new(
                "local_origin",
                NodeGraphViewState::default(),
                local_viewport,
            ),
            QueryScenario::new(
                "local_panned",
                panned_view_state(-8_000.0, -4_000.0),
                local_viewport,
            ),
            QueryScenario::new("full_view", NodeGraphViewState::default(), full_viewport),
        ] {
            let linear = NodeGraphStore::new(
                graph.clone(),
                scenario.view_state.clone(),
                editor_config(false, true),
            );
            let spatial = NodeGraphStore::new(
                graph.clone(),
                scenario.view_state.clone(),
                editor_config(true, true),
            );

            group.bench_with_input(
                BenchmarkId::new(format!("linear_{}", scenario.name), node_count),
                &linear,
                |b, store| {
                    b.iter(|| black_box(store.rendering_query(black_box(scenario.viewport))))
                },
            );
            group.bench_with_input(
                BenchmarkId::new(format!("spatial_{}", scenario.name), node_count),
                &spatial,
                |b, store| {
                    b.iter(|| black_box(store.rendering_query(black_box(scenario.viewport))))
                },
            );
        }
    }

    group.finish();
}

fn benchmark_first_spatial_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_rendering_query_first_read");
    let viewport = local_viewport();

    for node_count in [1_000_usize, 10_000, 50_000] {
        group.throughput(Throughput::Elements(node_count as u64));
        let graph = graph_fixture(node_count);

        group.bench_function(
            BenchmarkId::new("spatial_cold_local_origin", node_count),
            |b| {
                b.iter_batched(
                    || {
                        NodeGraphStore::new(
                            graph.clone(),
                            NodeGraphViewState::default(),
                            editor_config(true, true),
                        )
                    },
                    |store| black_box(store.rendering_query(black_box(viewport))),
                    BatchSize::LargeInput,
                )
            },
        );
    }

    group.finish();
}

fn benchmark_visibility_policies(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_rendering_query_visibility_policy");
    let viewport = local_viewport();

    for node_count in [10_000_usize, 50_000] {
        group.throughput(Throughput::Elements(node_count as u64));
        let graph = graph_fixture(node_count);

        for backend in [
            BackendScenario::new("linear", false),
            BackendScenario::new("spatial", true),
        ] {
            let culled = NodeGraphStore::new(
                graph.clone(),
                NodeGraphViewState::default(),
                editor_config(backend.spatial_enabled, true),
            );
            let unculled = NodeGraphStore::new(
                graph.clone(),
                NodeGraphViewState::default(),
                editor_config(backend.spatial_enabled, false),
            );

            group.bench_with_input(
                BenchmarkId::new(format!("{}_culled_local_origin", backend.name), node_count),
                &culled,
                |b, store| b.iter(|| black_box(store.rendering_query(black_box(viewport)))),
            );
            group.bench_with_input(
                BenchmarkId::new(
                    format!("{}_unculled_local_origin", backend.name),
                    node_count,
                ),
                &unculled,
                |b, store| b.iter(|| black_box(store.rendering_query(black_box(viewport)))),
            );
        }
    }

    group.finish();
}

fn benchmark_viewport_sequences(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_rendering_query_viewport_sequences");
    let viewport = local_viewport();

    for node_count in [10_000_usize, 50_000] {
        group.throughput(Throughput::Elements(
            (node_count * VIEWPORT_SEQUENCE_STEPS) as u64,
        ));
        let graph = graph_fixture(node_count);
        let pan_sequence = pan_sequence();
        let zoom_sequence = zoom_sequence();
        let full_viewport = full_viewport(node_count);

        for backend in [
            BackendScenario::new("linear", false),
            BackendScenario::new("spatial", true),
        ] {
            group.bench_with_input(
                BenchmarkId::new(
                    format!(
                        "{}_pan_sequence_{}_steps",
                        backend.name, VIEWPORT_SEQUENCE_STEPS
                    ),
                    node_count,
                ),
                &pan_sequence,
                |b, sequence| {
                    b.iter_batched(
                        || {
                            let store = NodeGraphStore::new(
                                graph.clone(),
                                NodeGraphViewState::default(),
                                editor_config(backend.spatial_enabled, true),
                            );
                            black_box(store.rendering_query(black_box(viewport)));
                            store
                        },
                        |mut store| {
                            for pan in sequence {
                                store.set_viewport(*pan, 1.0);
                                black_box(store.rendering_query(black_box(viewport)));
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );

            group.bench_with_input(
                BenchmarkId::new(
                    format!(
                        "{}_full_view_sequence_{}_steps",
                        backend.name, VIEWPORT_SEQUENCE_STEPS
                    ),
                    node_count,
                ),
                &full_viewport,
                |b, full_viewport| {
                    b.iter_batched(
                        || {
                            let store = NodeGraphStore::new(
                                graph.clone(),
                                NodeGraphViewState::default(),
                                editor_config(backend.spatial_enabled, true),
                            );
                            black_box(store.rendering_query(black_box(*full_viewport)));
                            store
                        },
                        |store| {
                            for _ in 0..VIEWPORT_SEQUENCE_STEPS {
                                black_box(store.rendering_query(black_box(*full_viewport)));
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );

            group.bench_with_input(
                BenchmarkId::new(
                    format!(
                        "{}_zoom_sequence_{}_steps",
                        backend.name, VIEWPORT_SEQUENCE_STEPS
                    ),
                    node_count,
                ),
                &zoom_sequence,
                |b, sequence| {
                    b.iter_batched(
                        || {
                            let store = NodeGraphStore::new(
                                graph.clone(),
                                NodeGraphViewState::default(),
                                editor_config(backend.spatial_enabled, true),
                            );
                            black_box(store.rendering_query(black_box(viewport)));
                            store
                        },
                        |mut store| {
                            for (pan, zoom) in sequence {
                                store.set_viewport(*pan, *zoom);
                                black_box(store.rendering_query(black_box(viewport)));
                            }
                        },
                        BatchSize::LargeInput,
                    )
                },
            );
        }
    }

    group.finish();
}

#[derive(Clone, Copy)]
struct BackendScenario {
    name: &'static str,
    spatial_enabled: bool,
}

impl BackendScenario {
    fn new(name: &'static str, spatial_enabled: bool) -> Self {
        Self {
            name,
            spatial_enabled,
        }
    }
}

#[derive(Clone)]
struct QueryScenario {
    name: &'static str,
    view_state: NodeGraphViewState,
    viewport: CanvasSize,
}

impl QueryScenario {
    fn new(name: &'static str, view_state: NodeGraphViewState, viewport: CanvasSize) -> Self {
        Self {
            name,
            view_state,
            viewport,
        }
    }
}

fn editor_config(
    spatial_enabled: bool,
    only_render_visible_elements: bool,
) -> NodeGraphEditorConfig {
    NodeGraphEditorConfig::default()
        .with_spatial_index_enabled(spatial_enabled)
        .with_only_render_visible_elements(only_render_visible_elements)
}

fn local_viewport() -> CanvasSize {
    CanvasSize {
        width: 1_200.0,
        height: 800.0,
    }
}

fn full_viewport(node_count: usize) -> CanvasSize {
    CanvasSize {
        width: GRID_COLUMNS as f32 * GRID_X_SPACING,
        height: (node_count.div_ceil(GRID_COLUMNS)) as f32 * GRID_Y_SPACING,
    }
}

fn pan_sequence() -> Vec<CanvasPoint> {
    (0..VIEWPORT_SEQUENCE_STEPS)
        .map(|step| CanvasPoint {
            x: -8_000.0 - step as f32 * 72.0,
            y: -4_000.0 - step as f32 * 48.0,
        })
        .collect()
}

fn zoom_sequence() -> Vec<(CanvasPoint, f32)> {
    (0..VIEWPORT_SEQUENCE_STEPS)
        .map(|step| {
            (
                CanvasPoint {
                    x: -8_000.0,
                    y: -4_000.0,
                },
                0.75 + step as f32 * 0.05,
            )
        })
        .collect()
}

fn panned_view_state(x: f32, y: f32) -> NodeGraphViewState {
    NodeGraphViewState {
        pan: CanvasPoint { x, y },
        ..NodeGraphViewState::default()
    }
}

fn graph_fixture(node_count: usize) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));

    for index in 0..node_count {
        let node = node_id(index);
        let out = out_port_id(index);
        let input = in_port_id(index);
        graph.insert_node(node, node_fixture(index, out, input));
        graph.insert_port(out, port_fixture(node, "out", PortDirection::Out));
        graph.insert_port(input, port_fixture(node, "in", PortDirection::In));
    }

    for index in 0..node_count.saturating_sub(1) {
        graph.insert_edge(
            edge_id(index),
            Edge::new(EdgeKind::Data, out_port_id(index), in_port_id(index + 1)),
        );
    }

    graph.build_unchecked()
}

fn node_fixture(index: usize, out: PortId, input: PortId) -> Node {
    Node {
        kind: NodeKindKey::new("bench.node"),
        kind_version: 1,
        pos: node_position(index),
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
        ports: vec![out, input],
        data: serde_json::Value::Null,
    }
}

fn node_position(index: usize) -> CanvasPoint {
    CanvasPoint {
        x: (index % GRID_COLUMNS) as f32 * GRID_X_SPACING,
        y: (index / GRID_COLUMNS) as f32 * GRID_Y_SPACING,
    }
}

fn port_fixture(node: NodeId, key: &str, dir: PortDirection) -> Port {
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
    }
}

fn node_id(index: usize) -> NodeId {
    NodeId::from_u128(1_000_000 + index as u128)
}

fn out_port_id(index: usize) -> PortId {
    PortId::from_u128(2_000_000 + index as u128)
}

fn in_port_id(index: usize) -> PortId {
    PortId::from_u128(3_000_000 + index as u128)
}

fn edge_id(index: usize) -> EdgeId {
    EdgeId::from_u128(4_000_000 + index as u128)
}

criterion_group!(benches, rendering_query_benchmarks);
criterion_main!(benches);
