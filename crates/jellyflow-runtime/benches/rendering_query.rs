use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
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

fn rendering_query_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_rendering_query");

    for node_count in [1_000_usize, 10_000, 50_000] {
        group.throughput(Throughput::Elements(node_count as u64));
        let graph = graph_fixture(node_count);
        let local_viewport = CanvasSize {
            width: 1_200.0,
            height: 800.0,
        };
        let full_viewport = CanvasSize {
            width: GRID_COLUMNS as f32 * GRID_X_SPACING,
            height: (node_count.div_ceil(GRID_COLUMNS)) as f32 * GRID_Y_SPACING,
        };

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
                NodeGraphEditorConfig::default(),
            );
            let spatial = NodeGraphStore::new(
                graph.clone(),
                scenario.view_state.clone(),
                NodeGraphEditorConfig::default().with_spatial_index_enabled(true),
            );

            group.bench_with_input(
                BenchmarkId::new(format!("linear_{}", scenario.name), node_count),
                &linear,
                |b, store| b.iter(|| store.rendering_query(scenario.viewport)),
            );
            group.bench_with_input(
                BenchmarkId::new(format!("spatial_{}", scenario.name), node_count),
                &spatial,
                |b, store| b.iter(|| store.rendering_query(scenario.viewport)),
            );
        }
    }

    group.finish();
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

fn panned_view_state(x: f32, y: f32) -> NodeGraphViewState {
    NodeGraphViewState {
        pan: CanvasPoint { x, y },
        ..NodeGraphViewState::default()
    }
}

fn graph_fixture(node_count: usize) -> Graph {
    let mut graph = Graph::new(GraphId::from_u128(1));

    for index in 0..node_count {
        let node = node_id(index);
        let out = out_port_id(index);
        let input = in_port_id(index);
        graph.nodes.insert(node, node_fixture(index, out, input));
        graph
            .ports
            .insert(out, port_fixture(node, "out", PortDirection::Out));
        graph
            .ports
            .insert(input, port_fixture(node, "in", PortDirection::In));
    }

    for index in 0..node_count.saturating_sub(1) {
        graph.edges.insert(
            edge_id(index),
            Edge {
                kind: EdgeKind::Data,
                from: out_port_id(index),
                to: in_port_id(index + 1),
                hidden: false,
                selectable: None,
                focusable: None,
                interaction_width: None,
                deletable: None,
                reconnectable: None,
            },
        );
    }

    graph
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
