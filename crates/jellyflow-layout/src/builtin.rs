use std::sync::{Arc, OnceLock};

use jellyflow_core::CanvasSize;

use crate::dugong::DugongLayoutEngine;
use crate::engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutEngine, LayoutEngineRegistry, LayoutEngineRequest, LayoutError,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinLayoutFamily {
    LayeredDag,
    MindMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinLayoutEngine {
    Dugong,
    TidyTree,
    MindMapRadial,
    MindMapFreeform,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinLayoutFamilySpec {
    family: BuiltinLayoutFamily,
    id: &'static str,
    name: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltinLayoutSpec {
    preset: BuiltinLayoutPreset,
    engine: BuiltinLayoutEngine,
    engine_id: &'static str,
    family: BuiltinLayoutFamily,
    engine_name: &'static str,
    capabilities: &'static [LayoutEngineCapability],
    options: LayoutOptions,
    engine_factory: fn() -> Arc<dyn LayoutEngine>,
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

const LAYERED_DAG_FAMILY_SPEC: BuiltinLayoutFamilySpec = BuiltinLayoutFamilySpec {
    family: BuiltinLayoutFamily::LayeredDag,
    id: LAYERED_DAG_LAYOUT_FAMILY_ID,
    name: "Layered DAG",
};

const MIND_MAP_FAMILY_SPEC: BuiltinLayoutFamilySpec = BuiltinLayoutFamilySpec {
    family: BuiltinLayoutFamily::MindMap,
    id: MIND_MAP_LAYOUT_FAMILY_ID,
    name: "Mind map",
};

const BUILTIN_FAMILY_SPECS: [BuiltinLayoutFamilySpec; 2] =
    [LAYERED_DAG_FAMILY_SPEC, MIND_MAP_FAMILY_SPEC];

const DUGONG_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    preset: BuiltinLayoutPreset::Workflow,
    engine: BuiltinLayoutEngine::Dugong,
    engine_id: DUGONG_LAYOUT_ENGINE_ID,
    family: BuiltinLayoutFamily::LayeredDag,
    engine_name: "Dugong layered DAG",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
    ],
    options: DEFAULT_LAYOUT_OPTIONS,
    engine_factory: dugong_engine,
};

const TIDY_TREE_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    preset: BuiltinLayoutPreset::Tree,
    engine: BuiltinLayoutEngine::TidyTree,
    engine_id: TIDY_TREE_LAYOUT_ENGINE_ID,
    family: BuiltinLayoutFamily::LayeredDag,
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
    engine_factory: tidy_tree_engine,
};

const MIND_MAP_RADIAL_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    preset: BuiltinLayoutPreset::MindMap,
    engine: BuiltinLayoutEngine::MindMapRadial,
    engine_id: MIND_MAP_RADIAL_LAYOUT_ENGINE_ID,
    family: BuiltinLayoutFamily::MindMap,
    engine_name: "Radial mind map",
    capabilities: &[
        LayoutEngineCapability::DirectionalLayout,
        LayoutEngineCapability::EdgeRouting,
        LayoutEngineCapability::PinnedNodes,
    ],
    options: DEFAULT_LAYOUT_OPTIONS,
    engine_factory: mind_map_radial_engine,
};

const MIND_MAP_FREEFORM_SPEC: BuiltinLayoutSpec = BuiltinLayoutSpec {
    preset: BuiltinLayoutPreset::Freeform,
    engine: BuiltinLayoutEngine::MindMapFreeform,
    engine_id: MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID,
    family: BuiltinLayoutFamily::MindMap,
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
    engine_factory: mind_map_freeform_engine,
};

const BUILTIN_SPECS: [BuiltinLayoutSpec; 4] = [
    DUGONG_SPEC,
    TIDY_TREE_SPEC,
    MIND_MAP_RADIAL_SPEC,
    MIND_MAP_FREEFORM_SPEC,
];

pub(crate) fn builtin_layout_registry() -> &'static LayoutEngineRegistry {
    static REGISTRY: OnceLock<LayoutEngineRegistry> = OnceLock::new();

    REGISTRY.get_or_init(|| {
        try_builtin_layout_registry()
            .expect("built-in layout registry definition should be consistent")
    })
}

pub(crate) fn builtin_family_specs() -> &'static [BuiltinLayoutFamilySpec] {
    &BUILTIN_FAMILY_SPECS
}

pub(crate) fn builtin_layout_specs() -> &'static [BuiltinLayoutSpec] {
    &BUILTIN_SPECS
}

fn try_builtin_layout_registry() -> Result<LayoutEngineRegistry, LayoutError> {
    let mut registry = LayoutEngineRegistry::new();

    for family in builtin_family_specs() {
        registry.insert_family(family.metadata())?;
    }

    for spec in builtin_layout_specs() {
        registry.insert_shared(spec.engine())?;
        registry.insert_metadata(spec.metadata())?;
    }

    Ok(registry)
}

pub(crate) fn layered_dag_family() -> LayoutFamilyMetadata {
    family_spec(BuiltinLayoutFamily::LayeredDag)
        .expect("built-in layered DAG family should exist")
        .metadata()
}

pub(crate) fn mind_map_family() -> LayoutFamilyMetadata {
    family_spec(BuiltinLayoutFamily::MindMap)
        .expect("built-in mind-map family should exist")
        .metadata()
}

pub(crate) fn engine_metadata(engine: BuiltinLayoutEngine) -> LayoutEngineMetadata {
    builtin_spec_by_engine(engine)
        .expect("built-in layout engine metadata should exist")
        .metadata()
}

pub(crate) fn builtin_request(preset: BuiltinLayoutPreset) -> LayoutEngineRequest {
    let spec = builtin_spec_by_preset(preset).expect("built-in layout preset should exist");
    spec.request()
}

fn builtin_spec_by_preset(preset: BuiltinLayoutPreset) -> Option<BuiltinLayoutSpec> {
    builtin_layout_specs()
        .iter()
        .copied()
        .find(|spec| spec.preset == preset)
}

fn builtin_spec_by_engine(engine: BuiltinLayoutEngine) -> Option<BuiltinLayoutSpec> {
    builtin_layout_specs()
        .iter()
        .copied()
        .find(|spec| spec.engine == engine)
}

fn family_spec(family: BuiltinLayoutFamily) -> Option<BuiltinLayoutFamilySpec> {
    builtin_family_specs()
        .iter()
        .copied()
        .find(|spec| spec.family == family)
}

fn dugong_engine() -> Arc<dyn LayoutEngine> {
    Arc::new(DugongLayoutEngine)
}

fn tidy_tree_engine() -> Arc<dyn LayoutEngine> {
    Arc::new(TidyTreeLayoutEngine)
}

fn mind_map_radial_engine() -> Arc<dyn LayoutEngine> {
    Arc::new(MindMapRadialLayoutEngine)
}

fn mind_map_freeform_engine() -> Arc<dyn LayoutEngine> {
    Arc::new(MindMapFreeformLayoutEngine)
}

impl BuiltinLayoutFamilySpec {
    pub(crate) fn id(&self) -> &'static str {
        self.id
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    fn metadata(&self) -> LayoutFamilyMetadata {
        LayoutFamilyMetadata::new(self.id, self.name)
    }
}

impl BuiltinLayoutSpec {
    pub(crate) fn preset(&self) -> BuiltinLayoutPreset {
        self.preset
    }

    pub(crate) fn engine_id(&self) -> &'static str {
        self.engine_id
    }

    pub(crate) fn family_id(&self) -> &'static str {
        self.family_spec().id
    }

    pub(crate) fn engine_name(&self) -> &'static str {
        self.engine_name
    }

    pub(crate) fn capabilities(&self) -> &'static [LayoutEngineCapability] {
        self.capabilities
    }

    pub(crate) fn options(&self) -> LayoutOptions {
        self.options
    }

    fn engine(&self) -> Arc<dyn LayoutEngine> {
        (self.engine_factory)()
    }

    fn request(&self) -> LayoutEngineRequest {
        LayoutEngineRequest::new(
            self.engine_id,
            LayoutRequest::all().with_options(self.options),
        )
    }

    fn metadata(&self) -> LayoutEngineMetadata {
        LayoutEngineMetadata::new(self.engine_id, self.family_id(), self.engine_name)
            .with_capabilities(self.capabilities.iter().copied())
    }

    fn family_spec(&self) -> BuiltinLayoutFamilySpec {
        family_spec(self.family).expect("built-in layout family should exist")
    }
}
