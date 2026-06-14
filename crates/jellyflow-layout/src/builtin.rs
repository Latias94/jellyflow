use jellyflow_core::CanvasSize;

use crate::dugong::DugongLayoutEngine;
use crate::engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest,
    LayoutOptions, LayoutRequest, LayoutSpacing, MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID,
    MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, TIDY_TREE_LAYOUT_ENGINE_ID,
};
use crate::family::{
    LAYERED_DAG_LAYOUT_FAMILY_ID, LayoutEngineCapability, LayoutEngineMetadata,
    LayoutFamilyMetadata, MIND_MAP_LAYOUT_FAMILY_ID,
};
use crate::freeform::MindMapFreeformLayoutEngine;
use crate::mind_map::MindMapRadialLayoutEngine;
use crate::tidy_tree::TidyTreeLayoutEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinLayoutPreset {
    Workflow,
    Tree,
    MindMap,
    Freeform,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinLayoutSpec {
    engine_id: &'static str,
    family_id: &'static str,
    family_name: &'static str,
    engine_name: &'static str,
    capabilities: &'static [LayoutEngineCapability],
    options: LayoutOptions,
}

const DEFAULT_LAYOUT_OPTIONS: LayoutOptions = LayoutOptions {
    direction: crate::engine::LayoutDirection::TopToBottom,
    spacing: LayoutSpacing {
        nodesep: 50.0,
        ranksep: 50.0,
        edgesep: 20.0,
    },
    margin: CanvasSize {
        width: 0.0,
        height: 0.0,
    },
    default_node_size: CanvasSize {
        width: 172.0,
        height: 36.0,
    },
    node_origin: (0.0, 0.0),
};

const DUGONG_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    engine_id: DUGONG_LAYOUT_ENGINE_ID,
    family_id: LAYERED_DAG_LAYOUT_FAMILY_ID,
    family_name: "Layered DAG",
    engine_name: "Dugong layered DAG",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
    ],
    options: DEFAULT_LAYOUT_OPTIONS,
};

const TIDY_TREE_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    engine_id: TIDY_TREE_LAYOUT_ENGINE_ID,
    family_id: LAYERED_DAG_LAYOUT_FAMILY_ID,
    family_name: "Layered DAG",
    engine_name: "Tidy tree",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
    ],
    options: LayoutOptions {
        spacing: LayoutSpacing {
            nodesep: 32.0,
            ranksep: 72.0,
            edgesep: 16.0,
        },
        ..DEFAULT_LAYOUT_OPTIONS
    },
};

const MIND_MAP_RADIAL_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    engine_id: MIND_MAP_RADIAL_LAYOUT_ENGINE_ID,
    family_id: MIND_MAP_LAYOUT_FAMILY_ID,
    family_name: "Mind map",
    engine_name: "Radial mind map",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
        LayoutEngineCapability::PinnedNodes,
    ],
    options: DEFAULT_LAYOUT_OPTIONS,
};

const MIND_MAP_FREEFORM_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    engine_id: MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID,
    family_id: MIND_MAP_LAYOUT_FAMILY_ID,
    family_name: "Mind map",
    engine_name: "Freeform mind map",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
        LayoutEngineCapability::PinnedNodes,
        LayoutEngineCapability::OverlapAvoidance,
    ],
    options: LayoutOptions {
        spacing: LayoutSpacing {
            nodesep: 24.0,
            ranksep: 24.0,
            edgesep: 24.0,
        },
        ..DEFAULT_LAYOUT_OPTIONS
    },
};

const BUILTIN_SPECS: [BuiltinLayoutSpec; 4] = [
    DUGONG_SPEC,
    TIDY_TREE_SPEC,
    MIND_MAP_RADIAL_SPEC,
    MIND_MAP_FREEFORM_SPEC,
];

pub(crate) fn builtin_layout_registry() -> LayoutEngineRegistry {
    let mut registry = LayoutEngineRegistry::new();

    debug_assert!(
        registry.insert_family(layered_dag_family()).is_ok(),
        "built-in layered DAG layout family should be unique"
    );
    debug_assert!(
        registry.insert_family(mind_map_family()).is_ok(),
        "built-in mind-map layout family should be unique"
    );

    debug_assert!(
        registry.insert(DugongLayoutEngine).is_ok(),
        "built-in dugong engine should be unique"
    );
    debug_assert!(
        registry.insert(TidyTreeLayoutEngine).is_ok(),
        "built-in tidy tree engine should be unique"
    );
    debug_assert!(
        registry.insert(MindMapRadialLayoutEngine).is_ok(),
        "built-in mind-map engine should be unique"
    );
    debug_assert!(
        registry.insert(MindMapFreeformLayoutEngine).is_ok(),
        "built-in freeform engine should be unique"
    );

    for spec in BUILTIN_SPECS {
        debug_assert!(
            registry.insert_metadata(engine_metadata(spec)).is_ok(),
            "built-in layout engine metadata should be unique"
        );
    }

    registry
}

pub(crate) fn layered_dag_family() -> LayoutFamilyMetadata {
    LayoutFamilyMetadata::new(LAYERED_DAG_LAYOUT_FAMILY_ID, "Layered DAG")
}

pub(crate) fn mind_map_family() -> LayoutFamilyMetadata {
    LayoutFamilyMetadata::new(MIND_MAP_LAYOUT_FAMILY_ID, "Mind map")
}

pub(crate) fn engine_metadata_by_id(id: &LayoutEngineId) -> Option<LayoutEngineMetadata> {
    builtin_spec_by_engine_id(id).map(engine_metadata)
}

pub(crate) fn builtin_request(preset: BuiltinLayoutPreset) -> LayoutEngineRequest {
    let spec = builtin_spec_by_preset(preset).expect("built-in layout preset should exist");
    LayoutEngineRequest::new(
        spec.engine_id,
        LayoutRequest::all().with_options(spec.options),
    )
}

fn builtin_spec_by_engine_id(id: &LayoutEngineId) -> Option<BuiltinLayoutSpec> {
    match id.as_str() {
        DUGONG_LAYOUT_ENGINE_ID => Some(DUGONG_SPEC),
        TIDY_TREE_LAYOUT_ENGINE_ID => Some(TIDY_TREE_SPEC),
        MIND_MAP_RADIAL_LAYOUT_ENGINE_ID => Some(MIND_MAP_RADIAL_SPEC),
        MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID => Some(MIND_MAP_FREEFORM_SPEC),
        _ => None,
    }
}

fn builtin_spec_by_preset(preset: BuiltinLayoutPreset) -> Option<BuiltinLayoutSpec> {
    match preset {
        BuiltinLayoutPreset::Workflow => Some(DUGONG_SPEC),
        BuiltinLayoutPreset::Tree => Some(TIDY_TREE_SPEC),
        BuiltinLayoutPreset::MindMap => Some(MIND_MAP_RADIAL_SPEC),
        BuiltinLayoutPreset::Freeform => Some(MIND_MAP_FREEFORM_SPEC),
    }
}

fn engine_metadata(spec: BuiltinLayoutSpec) -> LayoutEngineMetadata {
    LayoutEngineMetadata::new(spec.engine_id, spec.family_id, spec.engine_name)
        .with_capabilities(spec.capabilities.iter().copied())
}
