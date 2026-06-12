use crate::{
    LayoutEngineCapability, LayoutEngineId, LayoutEngineMetadata, LayoutEngineRegistry,
    LayoutError, LayoutFamilyId, LayoutFamilyMetadata, builtin_layout_engine_registry,
};

#[test]
fn builtin_registry_groups_mind_map_engines_by_family() {
    let registry = builtin_layout_engine_registry();
    let mind_map = LayoutFamilyId::mind_map();

    let engines = registry
        .engines_for_family(&mind_map)
        .map(|metadata| metadata.engine.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        engines,
        vec![
            LayoutEngineId::mind_map_freeform(),
            LayoutEngineId::mind_map_radial(),
        ]
    );
    assert_eq!(registry.family(&mind_map).unwrap().name, "Mind map");
    assert!(
        registry
            .metadata(&LayoutEngineId::mind_map_freeform())
            .unwrap()
            .capabilities
            .contains(&LayoutEngineCapability::OverlapAvoidance)
    );
}

#[test]
fn builtin_registry_keeps_dugong_in_layered_family() {
    let registry = builtin_layout_engine_registry();
    let layered = LayoutFamilyId::layered_dag();

    let engines = registry
        .engines_for_family(&layered)
        .map(|metadata| metadata.engine.clone())
        .collect::<Vec<_>>();

    assert_eq!(engines, vec![LayoutEngineId::dugong()]);
    assert!(
        registry
            .metadata(&LayoutEngineId::dugong())
            .unwrap()
            .capabilities
            .contains(&LayoutEngineCapability::EdgeRouting)
    );
}

#[test]
fn registry_rejects_duplicate_family_metadata() {
    let mut registry = LayoutEngineRegistry::new();
    let family = LayoutFamilyMetadata::new("custom.family", "Custom family");
    registry.insert_family(family.clone()).unwrap();

    let err = registry
        .insert_family(family.clone())
        .expect_err("duplicate family");

    assert_eq!(err, LayoutError::DuplicateLayoutFamily(family.id));
}

#[test]
fn registry_rejects_duplicate_engine_metadata() {
    let mut registry = LayoutEngineRegistry::new();
    let metadata = LayoutEngineMetadata::new("custom.engine", "custom.family", "Custom engine");
    registry.insert_metadata(metadata.clone()).unwrap();

    let err = registry
        .insert_metadata(metadata.clone())
        .expect_err("duplicate metadata");

    assert_eq!(
        err,
        LayoutError::DuplicateLayoutEngineMetadata(metadata.engine)
    );
}

#[test]
fn family_and_metadata_ids_are_deterministic() {
    let registry = builtin_layout_engine_registry();

    assert_eq!(
        registry
            .family_ids()
            .map(LayoutFamilyId::as_str)
            .collect::<Vec<_>>(),
        vec!["layered_dag", "mind_map"]
    );
    assert_eq!(
        registry
            .engine_metadata()
            .map(|metadata| metadata.engine.as_str())
            .collect::<Vec<_>>(),
        vec!["dugong", "mind_map_freeform", "mind_map_radial"]
    );
}
