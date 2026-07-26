use jellyflow_layout::{LayoutEngineId, builtin_layout_engine_registry};
use jellyflow_runtime::{GraphRuntime, ViewSessionOptions};

fn main() {
    let registry = builtin_layout_engine_registry();
    assert!(registry.get(&LayoutEngineId::dugong()).is_some());

    let _runtime_builder = GraphRuntime::builder(Default::default());
    let _view_options = ViewSessionOptions::default();
}
