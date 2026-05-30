//! Graph and editor-state persistence formats.

use std::path::Path;

use serde::{Deserialize, Serialize};

use jellyflow_core::core::{Graph, GraphId};

use super::config::NodeGraphEditorConfig;
use super::view_state::{NodeGraphPureViewState, NodeGraphViewState};

/// Graph file format version (v1).
pub const GRAPH_FILE_VERSION: u32 = 1;

/// Editor-state file format version.
pub const EDITOR_STATE_FILE_VERSION: u32 = 1;

/// Graph persistence file (v1).
///
/// This wrapper enables stable schema evolution while keeping the inner `Graph` model reusable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFileV1 {
    /// Graph id (duplicated for quick lookup / validation).
    pub graph_id: GraphId,
    /// File wrapper version.
    pub graph_version: u32,
    /// Graph document.
    pub graph: Graph,
}

impl GraphFileV1 {
    /// Wraps a graph into a v1 file object.
    pub fn from_graph(graph: Graph) -> Self {
        Self {
            graph_id: graph.graph_id,
            graph_version: GRAPH_FILE_VERSION,
            graph,
        }
    }

    /// Validates wrapper invariants.
    pub fn validate(&self) -> Result<(), GraphFileError> {
        if self.graph_id != self.graph.graph_id {
            return Err(GraphFileError::InconsistentGraphId);
        }
        Ok(())
    }

    /// Loads a JSON file.
    ///
    /// Backward compatibility: accepts both the wrapped form and a plain `Graph` root object.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, GraphFileError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| GraphFileError::Read {
            path: path.display().to_string(),
            source,
        })?;

        match serde_json::from_slice::<Self>(&bytes) {
            Ok(v) => {
                v.validate()?;
                Ok(v)
            }
            Err(new_err) => match serde_json::from_slice::<Graph>(&bytes) {
                Ok(graph) => Ok(Self::from_graph(graph)),
                Err(_old_err) => Err(GraphFileError::Parse {
                    path: path.display().to_string(),
                    source: new_err,
                }),
            },
        }
    }

    /// Loads the JSON file if it exists.
    pub fn load_json_if_exists(path: impl AsRef<Path>) -> Result<Option<Self>, GraphFileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::load_json(path).map(Some)
    }

    /// Saves the JSON file (pretty-printed).
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), GraphFileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| GraphFileError::Write {
                path: path.display().to_string(),
                source,
            })?;
        }
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|source| GraphFileError::Serialize {
                path: path.display().to_string(),
                source,
            })?;
        std::fs::write(path, bytes).map_err(|source| GraphFileError::Write {
            path: path.display().to_string(),
            source,
        })
    }
}

/// Errors for reading/writing graph files.
#[derive(Debug, thiserror::Error)]
pub enum GraphFileError {
    /// Read failure.
    #[error("failed to read graph file: {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    /// JSON parse failure.
    #[error("failed to parse graph file JSON: {path}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    /// Write failure.
    #[error("failed to write graph file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    /// JSON serialization failure.
    #[error("failed to serialize graph file JSON: {path}")]
    Serialize {
        path: String,
        source: serde_json::Error,
    },
    /// Wrapper id mismatch.
    #[error("graph file wrapper graph_id does not match graph.graph_id")]
    InconsistentGraphId,
}

/// Errors for reading/writing editor-state files.
#[derive(Debug, thiserror::Error)]
pub enum NodeGraphEditorStateFileError {
    /// Read failure.
    #[error("failed to read node graph editor-state file: {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    /// JSON parse failure.
    #[error("failed to parse node graph editor-state file JSON: {path}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    /// Write failure.
    #[error("failed to write node graph editor-state file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    /// JSON serialization failure.
    #[error("failed to serialize node graph editor-state JSON: {path}")]
    Serialize {
        path: String,
        source: serde_json::Error,
    },
    /// Wrapper id mismatch.
    #[error("editor-state file wrapper graph_id does not match requested graph_id")]
    InconsistentGraphId,
    /// Unsupported wrapper version.
    #[error("unsupported node graph editor-state version {version}; expected {expected}")]
    UnsupportedVersion { version: u32, expected: u32 },
}

/// Project-scoped editor-state persistence file.
///
/// The graph document is saved separately by `GraphFileV1`; this file owns only user/editor state:
/// pure canvas view state plus persisted editor policy and runtime tuning.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeGraphEditorStateFile {
    /// Graph id.
    pub graph_id: GraphId,
    /// Editor-state schema version.
    pub editor_state_version: u32,
    /// Pure view-state payload.
    pub view_state: NodeGraphViewState,
    /// Persisted editor policy and runtime tuning.
    pub editor_config: NodeGraphEditorConfig,
}

impl NodeGraphEditorStateFile {
    /// Wraps editor state for a graph.
    pub fn new(
        graph_id: GraphId,
        view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
    ) -> Self {
        Self {
            graph_id,
            editor_state_version: EDITOR_STATE_FILE_VERSION,
            view_state,
            editor_config,
        }
    }

    /// Loads a JSON file.
    pub fn load_json(
        path: impl AsRef<Path>,
        graph_id: GraphId,
    ) -> Result<Self, NodeGraphEditorStateFileError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| NodeGraphEditorStateFileError::Read {
            path: path.display().to_string(),
            source,
        })?;

        let persisted: PersistedNodeGraphEditorStateFile =
            serde_json::from_slice(&bytes).map_err(|source| {
                NodeGraphEditorStateFileError::Parse {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        if persisted.graph_id != graph_id {
            return Err(NodeGraphEditorStateFileError::InconsistentGraphId);
        }
        if persisted.editor_state_version != EDITOR_STATE_FILE_VERSION {
            return Err(NodeGraphEditorStateFileError::UnsupportedVersion {
                version: persisted.editor_state_version,
                expected: EDITOR_STATE_FILE_VERSION,
            });
        }

        Ok(Self {
            graph_id: persisted.graph_id,
            editor_state_version: persisted.editor_state_version,
            view_state: NodeGraphViewState::from(persisted.view_state),
            editor_config: persisted.editor_config,
        })
    }

    /// Loads the JSON file if it exists.
    pub fn load_json_if_exists(
        path: impl AsRef<Path>,
        graph_id: GraphId,
    ) -> Result<Option<Self>, NodeGraphEditorStateFileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::load_json(path, graph_id).map(Some)
    }

    /// Saves the JSON file (pretty-printed).
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeGraphEditorStateFileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                NodeGraphEditorStateFileError::Write {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        let persisted = PersistedNodeGraphEditorStateFile {
            graph_id: self.graph_id,
            editor_state_version: EDITOR_STATE_FILE_VERSION,
            view_state: NodeGraphPureViewState::from(&self.view_state),
            editor_config: self.editor_config.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&persisted).map_err(|source| {
            NodeGraphEditorStateFileError::Serialize {
                path: path.display().to_string(),
                source,
            }
        })?;
        std::fs::write(path, bytes).map_err(|source| NodeGraphEditorStateFileError::Write {
            path: path.display().to_string(),
            source,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedNodeGraphEditorStateFile {
    graph_id: GraphId,
    editor_state_version: u32,
    view_state: NodeGraphPureViewState,
    #[serde(default, skip_serializing_if = "NodeGraphEditorConfig::is_default")]
    editor_config: NodeGraphEditorConfig,
}
