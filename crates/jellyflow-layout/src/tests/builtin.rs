use std::collections::BTreeSet;

use crate::builtin::{builtin_family_specs, builtin_layout_specs, builtin_request};
use crate::{
    LayoutEngineId, LayoutFamilyId, LayoutPresetBuilder, LayoutScope,
    builtin_layout_engine_registry,
};

#[test]
fn builtin_registry_is_derived_from_specs() {
    let registry = builtin_layout_engine_registry();

    let family_ids = registry
        .family_ids()
        .map(LayoutFamilyId::as_str)
        .collect::<BTreeSet<_>>();
    let spec_family_ids = builtin_family_specs()
        .iter()
        .map(|spec| spec.id())
        .collect::<BTreeSet<_>>();

    assert_eq!(family_ids, spec_family_ids);

    for family in builtin_family_specs() {
        let metadata = registry
            .family(&LayoutFamilyId::from(family.id()))
            .expect("built-in family metadata should be registered");

        assert_eq!(metadata.name, family.name());
    }

    for spec in builtin_layout_specs() {
        let engine = LayoutEngineId::from(spec.engine_id());
        let metadata = registry
            .metadata(&engine)
            .expect("built-in engine metadata should be registered");

        assert!(registry.get(&engine).is_some());
        assert_eq!(metadata.name, spec.engine_name());
        assert_eq!(metadata.family.as_str(), spec.family_id());
        assert_eq!(
            metadata.capabilities,
            spec.capabilities().iter().copied().collect()
        );
    }
}

#[test]
fn builtin_presets_are_derived_from_specs() {
    for spec in builtin_layout_specs() {
        let request = builtin_request(spec.preset());

        assert_eq!(request.engine.as_str(), spec.engine_id());
        assert_eq!(request.layout.options, spec.options());
        assert_eq!(request.layout.scope, LayoutScope::All);
    }
}

#[test]
fn public_preset_builders_match_registered_builtin_metadata() {
    let registry = builtin_layout_engine_registry();
    let presets = [
        LayoutPresetBuilder::workflow().build(),
        LayoutPresetBuilder::tree().build(),
        LayoutPresetBuilder::mind_map().build(),
        LayoutPresetBuilder::freeform().build(),
    ];

    for request in presets {
        let metadata = registry
            .metadata(&request.engine)
            .expect("preset engine metadata is registered");

        assert!(registry.get(&request.engine).is_some());
        assert!(
            registry.family(&metadata.family).is_some(),
            "preset engine {} points to missing family {}",
            request.engine,
            metadata.family
        );
        assert_eq!(request.layout.scope, LayoutScope::All);
    }
}
