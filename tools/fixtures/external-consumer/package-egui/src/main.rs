use jellyflow_core::{GraphBuilder, GraphId};
use jellyflow_egui::{EguiNativeFrameHost, egui};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::view::ViewSessionOptions;

fn main() {
    let context = egui::Context::default();
    assert!(context.pixels_per_point().is_finite());

    let graph_id = GraphId::from_u128(1);
    let graph = GraphBuilder::new(graph_id)
        .build()
        .expect("empty consumer graph is valid");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("consumer runtime builds");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("consumer view attaches");
    let host = EguiNativeFrameHost::new(view);

    assert_eq!(
        host.view()
            .graph_snapshot()
            .expect("consumer host reads its view")
            .graph()
            .graph_id(),
        graph_id,
    );
    host.close().expect("consumer host closes its view");
}
