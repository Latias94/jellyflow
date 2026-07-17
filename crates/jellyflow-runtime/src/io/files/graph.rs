use std::path::{Path, PathBuf};

use jellyflow_core::core::{
    GRAPH_VERSION, Graph, GraphId, GraphValidationError, validate_graph_storage,
};
use serde::{Deserialize, Serialize, Serializer};

use crate::schema::{CanonicalGraph, NodeCatalog, NodeCatalogMigrationError};

use super::atomic_save::{AtomicSaveError, AtomicSaveFault, save_atomically};
use super::limits::{
    BoundedJsonDecodeError, BoundedJsonEncodeError, BoundedReadError, PersistenceLimitError,
    PersistenceLimits, PersistenceResource, deserialize_json_envelope_with_limits,
    deserialize_json_with_limits, read_bounded, serialize_json_pretty_with_limits,
    validate_graph_resources,
};

mod wire_v2;

use wire_v2::GraphFileWireV2;

/// Current graph-file envelope version (v2).
pub const GRAPH_FILE_VERSION: u32 = 2;

/// Version-dispatched graph persistence facade.
///
/// Persistence crosses a frozen V2 wire DTO so runtime model changes cannot silently alter the
/// current file format. Released V1 documents are intentionally incompatible and return
/// [`GraphFileError::UnsupportedEnvelopeVersion`] before typed payload decoding.
///
/// Persisted documents must be loaded through [`GraphFile::load_canonical_json`] or
/// [`GraphFile::load_canonical_json_if_exists`]. Raw deserialization is intentionally unavailable
/// because it would bypass known-kind migration through [`NodeCatalog`].
///
/// ```compile_fail
/// use jellyflow_runtime::io::GraphFile;
///
/// let _ = GraphFile::load_json("graph.json");
/// ```
///
/// ```compile_fail
/// use jellyflow_runtime::io::GraphFile;
///
/// let _: GraphFile = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct GraphFile {
    /// Graph id (duplicated for quick lookup / validation).
    pub graph_id: GraphId,
    /// File wrapper version.
    pub graph_version: u32,
    /// Graph document.
    pub graph: Graph,
}

impl GraphFile {
    /// Wraps a graph in the current V2 file envelope.
    pub fn from_graph(graph: Graph) -> Self {
        Self {
            graph_id: graph.graph_id(),
            graph_version: GRAPH_FILE_VERSION,
            graph,
        }
    }

    /// Validates wrapper invariants.
    pub fn validate(&self) -> Result<(), GraphFileError> {
        self.validate_wrapper()?;
        self.validate_storage()
    }

    fn validate_storage(&self) -> Result<(), GraphFileError> {
        let storage = validate_graph_storage(&self.graph);
        if !storage.is_ok() {
            return Err(GraphFileError::StorageValidation {
                errors: storage.into_errors(),
            });
        }
        Ok(())
    }

    fn validate_wrapper(&self) -> Result<(), GraphFileError> {
        if self.graph_version != GRAPH_FILE_VERSION {
            return Err(GraphFileError::UnsupportedEnvelopeVersion {
                found: self.graph_version,
                supported: GRAPH_FILE_VERSION,
            });
        }
        if self.graph_id != self.graph.graph_id() {
            return Err(GraphFileError::InconsistentGraphId);
        }
        Ok(())
    }

    /// Loads a JSON file for persistence implementation and tests.
    #[cfg(test)]
    pub(crate) fn load_json(path: impl AsRef<Path>) -> Result<Self, GraphFileError> {
        Self::load_json_with_limits(path, PersistenceLimits::default())
    }

    pub(crate) fn load_json_with_limits(
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<Self, GraphFileError> {
        let path = path.as_ref();
        let bytes = read_bounded(path, limits).map_err(|error| match error {
            BoundedReadError::Io(source) => GraphFileError::Read {
                path: path.display().to_string(),
                source,
            },
            BoundedReadError::Limit(source) => GraphFileError::ResourceLimit(source),
        })?;

        Self::decode_json_with_limits(&bytes, path, limits)
    }

    /// Loads, validates, and atomically canonicalizes one graph document in scratch state.
    pub fn load_canonical_json(
        path: impl AsRef<Path>,
        catalog: &NodeCatalog,
    ) -> Result<CanonicalGraph, CanonicalGraphFileError> {
        Self::load_canonical_json_with_limits(path, catalog, PersistenceLimits::default())
    }

    /// Loads and canonicalizes one graph under explicit persistence budgets.
    pub fn load_canonical_json_with_limits(
        path: impl AsRef<Path>,
        catalog: &NodeCatalog,
        limits: PersistenceLimits,
    ) -> Result<CanonicalGraph, CanonicalGraphFileError> {
        Self::load_json_with_limits(path, limits)?.canonicalize_with_limits(catalog, limits)
    }

    /// Loads and canonicalizes the JSON file when it exists.
    pub fn load_canonical_json_if_exists(
        path: impl AsRef<Path>,
        catalog: &NodeCatalog,
    ) -> Result<Option<CanonicalGraph>, CanonicalGraphFileError> {
        Self::load_canonical_json_if_exists_with_limits(path, catalog, PersistenceLimits::default())
    }

    /// Loads and canonicalizes a graph under explicit budgets when the file exists.
    pub fn load_canonical_json_if_exists_with_limits(
        path: impl AsRef<Path>,
        catalog: &NodeCatalog,
        limits: PersistenceLimits,
    ) -> Result<Option<CanonicalGraph>, CanonicalGraphFileError> {
        Self::load_json_if_exists_with_limits(path, limits)?
            .map(|file| file.canonicalize_with_limits(catalog, limits))
            .transpose()
    }

    /// Canonicalizes the decoded document without mutating its concrete graph.
    pub fn canonicalize(
        &self,
        catalog: &NodeCatalog,
    ) -> Result<CanonicalGraph, CanonicalGraphFileError> {
        self.canonicalize_with_limits(catalog, PersistenceLimits::default())
    }

    /// Canonicalizes an already decoded graph under explicit resource budgets.
    pub fn canonicalize_with_limits(
        &self,
        catalog: &NodeCatalog,
        limits: PersistenceLimits,
    ) -> Result<CanonicalGraph, CanonicalGraphFileError> {
        self.validate_wrapper()?;
        validate_graph_resources(&self.graph, limits).map_err(GraphFileError::from)?;
        limits
            .ensure(
                PersistenceResource::MigrationOperations,
                catalog.required_migration_operations(&self.graph),
            )
            .map_err(GraphFileError::from)?;
        let canonical = catalog.canonicalize_graph_with_limits(&self.graph, limits)?;
        validate_graph_resources(canonical.graph(), limits).map_err(GraphFileError::from)?;
        Ok(canonical)
    }

    pub(crate) fn load_json_if_exists_with_limits(
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<Option<Self>, GraphFileError> {
        let path = path.as_ref();
        match Self::load_json_with_limits(path, limits) {
            Ok(file) => Ok(Some(file)),
            Err(GraphFileError::Read { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Saves the JSON file (pretty-printed).
    ///
    /// The target's parent directory must already exist so a successful return can durably cover
    /// the file replacement without implicitly creating an unsynchronized directory chain.
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), GraphFileError> {
        self.save_json_with_limits(path, PersistenceLimits::default())
    }

    /// Saves the JSON file under explicit persistence budgets.
    ///
    /// The same limits should be supplied to [`GraphFile::load_canonical_json_with_limits`]
    /// when reopening the file. Resource validation completes before the target is replaced.
    pub fn save_json_with_limits(
        &self,
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<(), GraphFileError> {
        self.save_json_impl(path.as_ref(), limits, GraphFileSaveFault::None)
    }

    fn save_json_impl(
        &self,
        path: &Path,
        limits: PersistenceLimits,
        fault: GraphFileSaveFault,
    ) -> Result<(), GraphFileError> {
        self.validate_wrapper()?;
        validate_graph_resources(&self.graph, limits)?;
        self.validate_storage()?;
        save_atomically(
            path,
            ".jellyflow-graph-",
            fault,
            || {
                #[cfg(test)]
                if fault == GraphFileSaveFault::Serialization {
                    return Err(GraphFileError::Serialize {
                        path: path.display().to_string(),
                        source: serde_json::Error::io(std::io::Error::other(
                            "injected graph serialization failure",
                        )),
                    });
                }
                serialize_json_pretty_with_limits(self, limits).map_err(|error| match error {
                    BoundedJsonEncodeError::Serialize(source) => GraphFileError::Serialize {
                        path: path.display().to_string(),
                        source,
                    },
                    BoundedJsonEncodeError::Limit(source) => GraphFileError::ResourceLimit(source),
                })
            },
            |staged_bytes| Self::decode_json_with_limits(staged_bytes, path, limits).map(|_| ()),
        )
        .map_err(|error| match error {
            AtomicSaveError::Encode(error) => error,
            AtomicSaveError::Write(source) => GraphFileError::Write {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::ReloadValidation(source) => GraphFileError::ReloadValidation {
                path: path.display().to_string(),
                source: Box::new(source),
            },
            AtomicSaveError::StageSync(source) => GraphFileError::StageSync {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::NotCommitted(source) => GraphFileError::AtomicReplace {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::OutcomeUnknown {
                source,
                recovery_path,
            } => GraphFileError::CommitOutcomeUnknown {
                path: path.display().to_string(),
                recovery_path,
                source,
            },
            AtomicSaveError::DurabilityUnknown {
                source,
                recovery_path,
            } => GraphFileError::CommitDurabilityUnknown {
                path: path.display().to_string(),
                recovery_path,
                source,
            },
        })
    }

    fn decode_json_with_limits(
        bytes: &[u8],
        path: &Path,
        limits: PersistenceLimits,
    ) -> Result<Self, GraphFileError> {
        limits.ensure(PersistenceResource::FileBytes, bytes.len())?;
        let envelope_result =
            deserialize_json_envelope_with_limits::<GraphFileVersionEnvelope>(bytes, limits);
        let envelope = match envelope_result {
            Ok(envelope) => envelope,
            Err(BoundedJsonDecodeError::Parse(source)) => {
                let legacy_header =
                    match deserialize_json_with_limits::<serde_json::Value>(bytes, limits) {
                        Ok(value) => legacy_node_schema_header(&value),
                        Err(BoundedJsonDecodeError::Limit(source)) => {
                            return Err(GraphFileError::ResourceLimit(source));
                        }
                        Err(BoundedJsonDecodeError::Parse(_)) => None,
                    };
                if let Some((kind, latest_kind_version)) = legacy_header {
                    return Err(GraphFileError::UnsupportedLegacyNodeSchema {
                        path: path.display().to_string(),
                        kind,
                        latest_kind_version,
                    });
                }
                return Err(GraphFileError::Parse {
                    path: path.display().to_string(),
                    source,
                });
            }
            Err(BoundedJsonDecodeError::Limit(source)) => {
                return Err(GraphFileError::ResourceLimit(source));
            }
        };
        if envelope.graph_version != GRAPH_FILE_VERSION {
            return Err(GraphFileError::UnsupportedEnvelopeVersion {
                found: envelope.graph_version,
                supported: GRAPH_FILE_VERSION,
            });
        }
        let graph_envelope = deserialize_json_envelope_with_limits::<GraphModelEnvelope>(
            bytes, limits,
        )
        .map_err(|error| match error {
            BoundedJsonDecodeError::Parse(source) => GraphFileError::Parse {
                path: path.display().to_string(),
                source,
            },
            BoundedJsonDecodeError::Limit(source) => GraphFileError::ResourceLimit(source),
        })?;
        if graph_envelope.graph.graph_version != GRAPH_VERSION {
            return Err(GraphFileError::UnsupportedGraphModelVersion {
                found: graph_envelope.graph.graph_version,
                supported: GRAPH_VERSION,
            });
        }

        let wire =
            deserialize_json_with_limits::<GraphFileWireV2>(bytes, limits).map_err(|error| {
                match error {
                    BoundedJsonDecodeError::Parse(source) => GraphFileError::Parse {
                        path: path.display().to_string(),
                        source,
                    },
                    BoundedJsonDecodeError::Limit(source) => GraphFileError::ResourceLimit(source),
                }
            })?;
        wire.into_file_with_limits(limits)
    }

    #[cfg(test)]
    pub(crate) fn save_json_with_fault(
        &self,
        path: impl AsRef<Path>,
        fault: GraphFileSaveFault,
    ) -> Result<(), GraphFileError> {
        self.save_json_impl(path.as_ref(), PersistenceLimits::default(), fault)
    }
}

impl Serialize for GraphFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        GraphFileWireV2::from_file(self).serialize(serializer)
    }
}

#[derive(Deserialize)]
struct GraphFileVersionEnvelope {
    graph_version: u32,
}

#[derive(Deserialize)]
struct GraphModelEnvelope {
    graph: GraphVersionEnvelope,
}

#[derive(Deserialize)]
struct GraphVersionEnvelope {
    graph_version: u32,
}

fn legacy_node_schema_header(value: &serde_json::Value) -> Option<(String, u32)> {
    let object = value.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    let latest_kind_version = object
        .get("latest_kind_version")?
        .as_u64()?
        .try_into()
        .ok()?;
    object.get("title")?.as_str()?;

    const LEGACY_UI_FIELDS: [&str; 8] = [
        "layout_budget",
        "surface_slots",
        "repeatable_collections",
        "actions",
        "menus",
        "inspectors",
        "blackboards",
        "chrome",
    ];
    LEGACY_UI_FIELDS
        .iter()
        .any(|field| object.contains_key(*field))
        .then(|| (kind.to_owned(), latest_kind_version))
}

pub(crate) type GraphFileSaveFault = AtomicSaveFault;

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
    /// A configured persistence resource budget was exceeded.
    #[error(transparent)]
    ResourceLimit(#[from] PersistenceLimitError),
    /// A serialized descriptor from the removed portable UI schema was supplied as a graph file.
    #[error(
        "legacy NodeSchema descriptor `{kind}` at kind version {latest_kind_version} is not a supported graph document: {path}"
    )]
    UnsupportedLegacyNodeSchema {
        path: String,
        kind: String,
        latest_kind_version: u32,
    },
    /// Write failure.
    #[error("failed to write graph file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    /// The validated temporary file could not be synchronized before replacement.
    #[error("failed to synchronize staged graph file: {path}")]
    StageSync {
        path: String,
        source: std::io::Error,
    },
    /// JSON serialization failure.
    #[error("failed to serialize graph file JSON: {path}")]
    Serialize {
        path: String,
        source: serde_json::Error,
    },
    /// Unsupported graph-file envelope version.
    #[error("unsupported graph file envelope version {found}; supported version is {supported}")]
    UnsupportedEnvelopeVersion { found: u32, supported: u32 },
    /// Unsupported graph model version inside a supported envelope.
    #[error("unsupported graph model version {found}; supported version is {supported}")]
    UnsupportedGraphModelVersion { found: u32, supported: u32 },
    /// Wrapper id mismatch.
    #[error("graph file wrapper graph_id does not match graph.graph_id()")]
    InconsistentGraphId,
    /// The decoded graph violates storage invariants.
    #[error("graph storage validation failed: {errors:?}")]
    StorageValidation { errors: Vec<GraphValidationError> },
    /// The staged file could not be decoded and validated before replacement.
    #[error("failed to reload and validate staged graph file: {path}")]
    ReloadValidation {
        path: String,
        source: Box<GraphFileError>,
    },
    /// The validated staged file could not atomically replace the target.
    #[error("failed to atomically replace graph file: {path}")]
    AtomicReplace {
        path: String,
        source: std::io::Error,
    },
    /// Replacement may have changed the target, but Win32 could not report a definitive outcome.
    #[error(
        "graph file replacement outcome is unknown: {path}; inspect the target and recovery path before retrying"
    )]
    CommitOutcomeUnknown {
        path: String,
        /// A preserved staged file that can be inspected or recovered. `None` means the staged
        /// path no longer exists; the possibly replaced target is still identified by `path`.
        recovery_path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// Replacement crossed the commit point, but directory durability could not be confirmed.
    #[error(
        "graph file was replaced, but commit durability is unknown: {path}; reload before retrying"
    )]
    CommitDurabilityUnknown {
        path: String,
        /// An independently retained staged recovery file. This is normally `None` after a
        /// successful replacement; the committed target is identified by `path`.
        recovery_path: Option<PathBuf>,
        source: std::io::Error,
    },
}

/// Failure while decoding or canonicalizing a graph document for runtime publication.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalGraphFileError {
    #[error(transparent)]
    File(#[from] GraphFileError),
    #[error(transparent)]
    Catalog(#[from] NodeCatalogMigrationError),
}
