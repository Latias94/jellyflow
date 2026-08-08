use crate::{
    LayoutEngine, LayoutEngineCapability, LayoutEngineDescriptor, LayoutEngineId,
    LayoutEngineMetadata, LayoutEngineRegistry, LayoutError, LayoutFamilyId, LayoutFamilyMetadata,
    LayoutPlacements, PreparedLayoutInput, builtin_layout_engine_registry,
};

#[derive(Clone, Copy)]
struct NoopEngine;

impl LayoutEngine for NoopEngine {
    fn place(&self, _input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        Ok(LayoutPlacements::normalize_to_margin([]))
    }
}

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
fn builtin_registry_groups_layered_engines_by_family() {
    let registry = builtin_layout_engine_registry();
    let layered = LayoutFamilyId::layered_dag();

    let engines = registry
        .engines_for_family(&layered)
        .map(|metadata| metadata.engine.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        engines,
        vec![LayoutEngineId::dugong(), LayoutEngineId::tidy_tree()]
    );
    assert!(
        registry
            .metadata(&LayoutEngineId::dugong())
            .unwrap()
            .capabilities
            .contains(&LayoutEngineCapability::EdgeRouting)
    );
    assert!(
        registry
            .metadata(&LayoutEngineId::tidy_tree())
            .unwrap()
            .capabilities
            .contains(&LayoutEngineCapability::DirectionalLayout)
    );
}

#[test]
fn registry_reuses_equal_family_metadata_and_rejects_conflicts_atomically() {
    let mut registry = LayoutEngineRegistry::new();
    let family = LayoutFamilyMetadata::new("custom.family", "Custom family");
    registry
        .register(descriptor("custom.first", family.clone()))
        .unwrap();
    registry
        .register(descriptor("custom.second", family.clone()))
        .unwrap();

    let err = registry
        .register(descriptor(
            "custom.conflict",
            LayoutFamilyMetadata::new(family.id.clone(), "Different family"),
        ))
        .expect_err("conflicting family");

    assert_eq!(err, LayoutError::ConflictingLayoutFamily(family.id));
    assert!(!registry.contains(&LayoutEngineId::new("custom.conflict")));
}

#[test]
fn registry_rejects_descriptor_family_mismatch_without_partial_registration() {
    let mut registry = LayoutEngineRegistry::new();
    let family = LayoutFamilyMetadata::new("custom.family", "Custom family");
    let metadata = LayoutEngineMetadata::new("custom.engine", "other.family", "Custom engine");

    let err = registry
        .register(LayoutEngineDescriptor::new(
            family.clone(),
            metadata.clone(),
            NoopEngine,
        ))
        .expect_err("family mismatch");

    assert_eq!(
        err,
        LayoutError::LayoutEngineFamilyMismatch {
            engine: metadata.engine.clone(),
            metadata_family: metadata.family,
            descriptor_family: family.id,
        }
    );
    assert!(!registry.contains(&metadata.engine));
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
        vec![
            "dugong",
            "mind_map_freeform",
            "mind_map_radial",
            "tidy_tree"
        ]
    );
}

fn descriptor(id: &str, family: LayoutFamilyMetadata) -> LayoutEngineDescriptor {
    let metadata = LayoutEngineMetadata::new(id, family.id.clone(), id);
    LayoutEngineDescriptor::new(family, metadata, NoopEngine)
}
