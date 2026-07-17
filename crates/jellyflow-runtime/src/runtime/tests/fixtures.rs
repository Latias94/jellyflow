use jellyflow_core::core::{Graph, GraphBuilder};

pub(super) trait GraphFixtureMutExt {
    fn with_detached_builder<R>(&mut self, f: impl FnOnce(&mut GraphBuilder) -> R) -> R;
}

impl GraphFixtureMutExt for Graph {
    fn with_detached_builder<R>(&mut self, f: impl FnOnce(&mut GraphBuilder) -> R) -> R {
        let mut builder = GraphBuilder::from_detached_graph(self.clone());
        let result = f(&mut builder);
        *self = builder.build_unchecked();
        result
    }
}
