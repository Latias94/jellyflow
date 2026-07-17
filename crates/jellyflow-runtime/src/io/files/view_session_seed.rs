use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};

use crate::io::view_state::ViewSessionSeed;

use super::atomic_save::{AtomicSaveError, AtomicSaveFault, save_atomically};
use super::limits::{
    BoundedJsonDecodeError, BoundedJsonEncodeError, BoundedReadError, PersistenceLimitError,
    PersistenceLimits, PersistenceResource, deserialize_json_envelope_with_limits,
    deserialize_json_with_limits, read_bounded, serialize_json_pretty_with_limits,
    validate_view_session_seed_resources,
};

mod wire_v1;

use wire_v1::ViewSessionSeedWireV1;

/// Current view-session seed file version.
pub const VIEW_SESSION_SEED_FILE_VERSION: u32 = 1;

/// Stateless codec for the versioned view-session seed file.
///
/// Decode and load operations accept only the exact current wire format and expose sanitized
/// [`ViewSessionSeed`] values.
pub struct ViewSessionSeedFile;

impl ViewSessionSeedFile {
    /// Encodes a runtime seed using only the current view-session seed format.
    pub fn encode_json(seed: &ViewSessionSeed) -> Result<Vec<u8>, ViewSessionSeedFileError> {
        Self::encode_json_with_limits(seed, PersistenceLimits::default())
    }

    /// Encodes a runtime seed under explicit persistence budgets.
    pub fn encode_json_with_limits(
        seed: &ViewSessionSeed,
        limits: PersistenceLimits,
    ) -> Result<Vec<u8>, ViewSessionSeedFileError> {
        validate_view_session_seed_resources(seed, limits)?;
        Self::encode_json_after_validation(seed, limits)
    }

    /// Decodes a current seed file.
    pub fn decode_json(bytes: &[u8]) -> Result<ViewSessionSeed, ViewSessionSeedFileError> {
        Self::decode_json_with_limits(bytes, PersistenceLimits::default())
    }

    /// Decodes a seed byte slice under explicit persistence budgets.
    pub fn decode_json_with_limits(
        bytes: &[u8],
        limits: PersistenceLimits,
    ) -> Result<ViewSessionSeed, ViewSessionSeedFileError> {
        limits.ensure(PersistenceResource::FileBytes, bytes.len())?;
        let envelope =
            deserialize_json_envelope_with_limits::<ViewSessionSeedEnvelope>(bytes, limits)
                .map_err(ViewSessionSeedFileError::from)?;
        let VersionField::Present(version) = envelope.view_session_seed_version else {
            return Err(ViewSessionSeedFileError::MissingVersion);
        };
        if version != VIEW_SESSION_SEED_FILE_VERSION {
            return Err(ViewSessionSeedFileError::UnsupportedVersion {
                version,
                expected: VIEW_SESSION_SEED_FILE_VERSION,
            });
        }
        let wire = deserialize_json_with_limits::<ViewSessionSeedWireV1>(bytes, limits)
            .map_err(ViewSessionSeedFileError::from)?;
        let seed = wire.into_seed()?;
        validate_view_session_seed_resources(&seed, limits)?;
        Ok(seed)
    }

    /// Loads a current seed file.
    pub fn load_json(path: impl AsRef<Path>) -> Result<ViewSessionSeed, ViewSessionSeedFileError> {
        Self::load_json_with_limits(path, PersistenceLimits::default())
    }

    /// Loads a current seed file under explicit persistence budgets.
    pub fn load_json_with_limits(
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<ViewSessionSeed, ViewSessionSeedFileError> {
        let path = path.as_ref();
        let bytes = read_bounded(path, limits).map_err(|error| match error {
            BoundedReadError::Io(source) => ViewSessionSeedFileError::Read {
                path: path.display().to_string(),
                source,
            },
            BoundedReadError::Limit(source) => ViewSessionSeedFileError::ResourceLimit(source),
        })?;
        Self::decode_json_with_limits(&bytes, limits)
    }

    /// Loads a seed file if it exists.
    pub fn load_json_if_exists(
        path: impl AsRef<Path>,
    ) -> Result<Option<ViewSessionSeed>, ViewSessionSeedFileError> {
        Self::load_json_if_exists_with_limits(path, PersistenceLimits::default())
    }

    /// Loads a seed under explicit budgets when the file exists.
    pub fn load_json_if_exists_with_limits(
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<Option<ViewSessionSeed>, ViewSessionSeedFileError> {
        let path = path.as_ref();
        match Self::load_json_with_limits(path, limits) {
            Ok(seed) => Ok(Some(seed)),
            Err(ViewSessionSeedFileError::Read { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// Saves a runtime seed using only the current view-session seed format.
    ///
    /// The target's parent directory must already exist so a successful return can durably cover
    /// the file replacement without implicitly creating an unsynchronized directory chain.
    pub fn save_json(
        seed: &ViewSessionSeed,
        path: impl AsRef<Path>,
    ) -> Result<(), ViewSessionSeedFileError> {
        Self::save_json_with_limits(seed, path, PersistenceLimits::default())
    }

    /// Saves a runtime seed under explicit persistence budgets.
    pub fn save_json_with_limits(
        seed: &ViewSessionSeed,
        path: impl AsRef<Path>,
        limits: PersistenceLimits,
    ) -> Result<(), ViewSessionSeedFileError> {
        Self::save_json_impl(
            path.as_ref(),
            seed,
            limits,
            ViewSessionSeedFileSaveFault::None,
        )
    }

    fn save_json_impl(
        path: &Path,
        seed: &ViewSessionSeed,
        limits: PersistenceLimits,
        fault: ViewSessionSeedFileSaveFault,
    ) -> Result<(), ViewSessionSeedFileError> {
        validate_view_session_seed_resources(seed, limits)?;
        save_atomically(
            path,
            ".jellyflow-view-session-seed-",
            fault,
            || {
                #[cfg(test)]
                if fault == ViewSessionSeedFileSaveFault::Serialization {
                    return Err(ViewSessionSeedFileError::Serialize {
                        source: serde_json::Error::io(std::io::Error::other(
                            "injected view-session seed serialization failure",
                        )),
                    });
                }
                Self::encode_json_after_validation(seed, limits)
            },
            |staged_bytes| {
                let loaded = Self::decode_json_with_limits(staged_bytes, limits)?;
                if &loaded != seed {
                    return Err(ViewSessionSeedFileError::StagedSeedMismatch);
                }
                Ok(())
            },
        )
        .map_err(|error| match error {
            AtomicSaveError::Encode(error) => error,
            AtomicSaveError::Write(source) => ViewSessionSeedFileError::Write {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::ReloadValidation(source) => {
                ViewSessionSeedFileError::ReloadValidation {
                    path: path.display().to_string(),
                    source: Box::new(source),
                }
            }
            AtomicSaveError::StageSync(source) => ViewSessionSeedFileError::StageSync {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::NotCommitted(source) => ViewSessionSeedFileError::AtomicReplace {
                path: path.display().to_string(),
                source,
            },
            AtomicSaveError::OutcomeUnknown {
                source,
                recovery_path,
            } => ViewSessionSeedFileError::CommitOutcomeUnknown {
                path: path.display().to_string(),
                recovery_path,
                source,
            },
            AtomicSaveError::DurabilityUnknown {
                source,
                recovery_path,
            } => ViewSessionSeedFileError::CommitDurabilityUnknown {
                path: path.display().to_string(),
                recovery_path,
                source,
            },
        })
    }

    fn encode_json_after_validation(
        seed: &ViewSessionSeed,
        limits: PersistenceLimits,
    ) -> Result<Vec<u8>, ViewSessionSeedFileError> {
        let wire = ViewSessionSeedWireV1::from_seed(seed);
        serialize_json_pretty_with_limits(&wire, limits).map_err(|error| match error {
            BoundedJsonEncodeError::Serialize(source) => {
                ViewSessionSeedFileError::Serialize { source }
            }
            BoundedJsonEncodeError::Limit(source) => {
                ViewSessionSeedFileError::ResourceLimit(source)
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn save_json_with_fault(
        seed: &ViewSessionSeed,
        path: impl AsRef<Path>,
        fault: ViewSessionSeedFileSaveFault,
    ) -> Result<(), ViewSessionSeedFileError> {
        Self::save_json_impl(path.as_ref(), seed, PersistenceLimits::default(), fault)
    }
}

#[derive(Deserialize)]
struct ViewSessionSeedEnvelope {
    #[serde(default)]
    view_session_seed_version: VersionField,
}

#[derive(Debug, Clone, Copy, Default)]
enum VersionField {
    #[default]
    Missing,
    Present(u32),
}

impl<'de> Deserialize<'de> for VersionField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        u32::deserialize(deserializer).map(Self::Present)
    }
}

pub(crate) type ViewSessionSeedFileSaveFault = AtomicSaveFault;

/// Errors for reading and writing view-session seed files.
#[derive(Debug, thiserror::Error)]
pub enum ViewSessionSeedFileError {
    /// Read failure.
    #[error("failed to read view-session seed file: {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    /// Exact JSON decoding failure.
    #[error("failed to parse view-session seed JSON")]
    Parse { source: serde_json::Error },
    /// A configured persistence resource budget was exceeded.
    #[error(transparent)]
    ResourceLimit(#[from] PersistenceLimitError),
    /// Write failure.
    #[error("failed to write view-session seed file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    /// The validated temporary file could not be synchronized before replacement.
    #[error("failed to synchronize staged view-session seed file: {path}")]
    StageSync {
        path: String,
        source: std::io::Error,
    },
    /// JSON serialization failure.
    #[error("failed to serialize view-session seed JSON")]
    Serialize { source: serde_json::Error },
    /// The staged file decoded successfully but did not exactly reproduce the requested seed.
    #[error("staged view-session seed does not match the requested seed")]
    StagedSeedMismatch,
    /// The staged file could not be decoded and validated before replacement.
    #[error("failed to reload and validate staged view-session seed file: {path}")]
    ReloadValidation {
        path: String,
        source: Box<ViewSessionSeedFileError>,
    },
    /// The validated staged file could not atomically replace the target.
    #[error("failed to atomically replace view-session seed file: {path}")]
    AtomicReplace {
        path: String,
        source: std::io::Error,
    },
    /// Replacement may have changed the target, but Win32 could not report a definitive outcome.
    #[error(
        "view-session seed replacement outcome is unknown: {path}; inspect the target and recovery path before retrying"
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
        "view-session seed file was replaced, but commit durability is unknown: {path}; reload before retrying"
    )]
    CommitDurabilityUnknown {
        path: String,
        /// An independently retained staged recovery file. This is normally `None` after a
        /// successful replacement; the committed target is identified by `path`.
        recovery_path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// The current seed version field was absent.
    #[error("view-session seed JSON has no view_session_seed_version field")]
    MissingVersion,
    /// Unsupported current seed version.
    #[error("unsupported view-session seed version {version}; expected {expected}")]
    UnsupportedVersion { version: u32, expected: u32 },
}

impl From<BoundedJsonDecodeError> for ViewSessionSeedFileError {
    fn from(error: BoundedJsonDecodeError) -> Self {
        match error {
            BoundedJsonDecodeError::Parse(source) => Self::Parse { source },
            BoundedJsonDecodeError::Limit(source) => Self::ResourceLimit(source),
        }
    }
}
