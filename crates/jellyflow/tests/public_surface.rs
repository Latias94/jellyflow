use jellyflow::prelude::*;
use jellyflow::{core, layout, runtime};

#[test]
fn facade_exposes_graph_runtime_and_view_session_entrypoints() {
    let graph = Graph::new(GraphId::new());
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("graph runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view session");

    assert_eq!(view.graph_snapshot().unwrap().graph().graph_id(), graph_id);
    let _request = FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 800.0,
                height: 600.0,
            },
        },
        RendererStamp::new("consumer.node", 1),
    );
    let _ = AdapterEnvironment::default();
    let _ = HandlePosition::Right;
}

#[test]
fn facade_exposes_revisioned_runtime_commands_without_raw_ops_in_the_prelude() {
    let graph = Graph::new(GraphId::new());
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("graph runtime");
    let outcome = runtime
        .execute(GraphCommand::topology(0, TopologyEdit::new()))
        .expect("command executes");
    assert!(matches!(outcome, GraphCommandOutcome::Noop(_)));
    let symbol = core::SymbolId::new();
    runtime
        .execute(GraphCommand::symbols(
            0,
            SymbolEdit::new().add(
                symbol,
                core::Symbol {
                    name: "facade symbol".to_owned(),
                    ty: None,
                    default_value: None,
                    meta: Default::default(),
                },
            ),
        ))
        .expect("facade symbol command")
        .applied()
        .expect("facade symbol command applies");
    let _: jellyflow::SymbolEdit = SymbolEdit::new();

    let facade_source = include_str!("../src/lib.rs");
    let prelude = facade_source
        .split_once("pub mod prelude")
        .expect("facade prelude")
        .1;
    assert!(!prelude.contains("GraphOp"));
    assert!(!prelude.contains("GraphTransaction"));
    assert!(!prelude.contains("NodeGraphStore"));
    assert!(!prelude.contains("NodeGraphPatch"));
    assert!(!prelude.contains("DispatchOutcome"));
    assert!(!prelude.contains("DispatchError"));
}

#[test]
fn facade_modules_expose_underlying_crates() {
    let _: core::Graph = Graph::new(GraphId::new());

    let registry = builtin_layout_engine_registry();
    assert!(registry.get(&LayoutEngineId::dugong()).is_some());
    assert!(registry.get(&LayoutEngineId::tidy_tree()).is_some());
    assert!(
        registry
            .engines_for_family(&LayoutFamilyId::mind_map())
            .count()
            >= 2
    );
    let _ = std::mem::size_of::<LayoutEngineRegistry>();
    let _ = std::mem::size_of::<LayoutContext>();
    let _ = std::mem::size_of::<LayoutEngineRequest>();
    let _ = std::mem::size_of::<LayoutPresetBuilder>();
    let _ = std::mem::size_of::<LayoutRequest>();
    let _ = std::mem::size_of::<LayoutResult>();
    let _preset_request = LayoutPresetBuilder::workflow().build();
    let _: fn() -> &'static layout::LayoutEngineRegistry = layout::builtin_layout_engine_registry;
    let _ = layout::TIDY_TREE_LAYOUT_ENGINE_ID;
    let _ = std::mem::size_of::<layout::TidyTreeLayoutEngine>();

    let _ = std::mem::size_of::<runtime::GraphRuntime>();
    let _ = std::mem::size_of::<runtime::ViewSession>();
}
