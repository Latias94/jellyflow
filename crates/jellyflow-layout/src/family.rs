use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::builtin::{BuiltinLayoutEngine, engine_metadata, layered_dag_family, mind_map_family};
use crate::engine::LayoutEngineId;

/// Stable family id for DAG/layered graph layout engines.
pub const LAYERED_DAG_LAYOUT_FAMILY_ID: &str = "layered_dag";
/// Stable family id for mind-map layout engines.
pub const MIND_MAP_LAYOUT_FAMILY_ID: &str = "mind_map";

/// Stable identifier for a layout engine family.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayoutFamilyId(String);

impl LayoutFamilyId {
    /// Creates a new family id.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the built-in DAG/layered family id.
    pub fn layered_dag() -> Self {
        Self::new(LAYERED_DAG_LAYOUT_FAMILY_ID)
    }

    /// Returns the built-in mind-map family id.
    pub fn mind_map() -> Self {
        Self::new(MIND_MAP_LAYOUT_FAMILY_ID)
    }

    /// Returns this id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LayoutFamilyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for LayoutFamilyId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LayoutFamilyId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Metadata for a group of layout engines with related behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutFamilyMetadata {
    pub id: LayoutFamilyId,
    pub name: String,
}

impl LayoutFamilyMetadata {
    /// Creates family metadata.
    pub fn new(id: impl Into<LayoutFamilyId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    /// Returns metadata for Jellyflow's built-in DAG/layered family.
    pub fn layered_dag() -> Self {
        layered_dag_family()
    }

    /// Returns metadata for Jellyflow's built-in mind-map family.
    pub fn mind_map() -> Self {
        mind_map_family()
    }
}

/// Public capabilities hosts can use when choosing a layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutEngineCapability {
    /// Engine can orient its layout by [`crate::LayoutDirection`].
    DirectionalLayout,
    /// Engine reports routed edge points.
    EdgeRouting,
    /// Engine honors pinned nodes from [`crate::LayoutContext`].
    PinnedNodes,
    /// Engine can resolve overlap among visible nodes.
    OverlapAvoidance,
}

/// Discovery metadata for one layout engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutEngineMetadata {
    pub engine: LayoutEngineId,
    pub family: LayoutFamilyId,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<LayoutEngineCapability>,
}

impl LayoutEngineMetadata {
    /// Creates engine discovery metadata.
    pub fn new(
        engine: impl Into<LayoutEngineId>,
        family: impl Into<LayoutFamilyId>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            engine: engine.into(),
            family: family.into(),
            name: name.into(),
            capabilities: BTreeSet::new(),
        }
    }

    /// Adds capabilities.
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = LayoutEngineCapability>,
    ) -> Self {
        self.capabilities.extend(capabilities);
        self
    }

    /// Returns metadata for the built-in `dugong` engine.
    pub fn dugong() -> Self {
        engine_metadata(BuiltinLayoutEngine::Dugong)
    }

    /// Returns metadata for the built-in tidy tree engine.
    pub fn tidy_tree() -> Self {
        engine_metadata(BuiltinLayoutEngine::TidyTree)
    }

    /// Returns metadata for the built-in radial mind-map engine.
    pub fn mind_map_radial() -> Self {
        engine_metadata(BuiltinLayoutEngine::MindMapRadial)
    }

    /// Returns metadata for the built-in freeform mind-map engine.
    pub fn mind_map_freeform() -> Self {
        engine_metadata(BuiltinLayoutEngine::MindMapFreeform)
    }
}
