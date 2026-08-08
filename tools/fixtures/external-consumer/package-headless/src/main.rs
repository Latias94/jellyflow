use jellyflow_layout::{LayoutEngineId, builtin_layout_engine_registry};
use jellyflow_runtime::commands::{GraphCommand, NodeGeometryEdit, TopologyEdit};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::view::ViewSessionOptions;

fn readme_commands_compile(runtime: &GraphRuntime) {
    let version = runtime
        .snapshot()
        .expect("README command example reads a graph snapshot")
        .version();
    let _topology = GraphCommand::topology(version, TopologyEdit::new());
    let _geometry =
        GraphCommand::set_node_geometry(version, std::iter::empty::<NodeGeometryEdit>());
}

fn main() {
    let registry = builtin_layout_engine_registry();
    assert!(registry.contains(&LayoutEngineId::dugong()));

    let _runtime_builder = GraphRuntime::builder(Default::default());
    let _view_options = ViewSessionOptions::default();
    let _readme_commands_compile: fn(&GraphRuntime) = readme_commands_compile;
}
