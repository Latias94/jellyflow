use crate::{LayoutEngineId, LayoutPresetBuilder, LayoutScope, builtin_layout_engine_registry};

#[test]
fn public_preset_builders_match_registered_builtin_metadata() {
    let registry = builtin_layout_engine_registry();
    let presets = [
        (
            LayoutPresetBuilder::workflow().build(),
            LayoutEngineId::dugong(),
        ),
        (
            LayoutPresetBuilder::tree().build(),
            LayoutEngineId::tidy_tree(),
        ),
        (
            LayoutPresetBuilder::mind_map().build(),
            LayoutEngineId::mind_map_radial(),
        ),
        (
            LayoutPresetBuilder::freeform().build(),
            LayoutEngineId::mind_map_freeform(),
        ),
    ];

    for (request, expected_engine) in presets {
        assert_eq!(request.engine, expected_engine);
        let metadata = registry
            .metadata(&request.engine)
            .expect("preset engine metadata is registered");

        assert!(registry.contains(&request.engine));
        assert!(
            registry.family(&metadata.family).is_some(),
            "preset engine {} points to missing family {}",
            request.engine,
            metadata.family
        );
        assert_eq!(request.layout.scope, LayoutScope::All);
    }
}
