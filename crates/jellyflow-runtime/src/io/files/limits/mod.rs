//! Persistence resource budgets and bounded file decoding.

mod resource_validation;
mod serde_budget;
mod serde_encode;

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub(crate) use resource_validation::{
    PayloadCounter, validate_graph_resources, validate_view_session_seed_resources,
};
pub(crate) use serde_budget::{
    deserialize_json_envelope_with_limits, deserialize_json_with_limits,
};
pub(crate) use serde_encode::{BoundedJsonEncodeError, serialize_json_pretty_with_limits};

const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_ENTITIES: usize = 250_000;
const DEFAULT_MAX_COLLECTION_ITEMS: usize = DEFAULT_MAX_ENTITIES;
const DEFAULT_MAX_STRING_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_JSON_VALUES: usize = 2_000_000;
const MAX_SUPPORTED_JSON_DEPTH: usize = 128;
const DEFAULT_MAX_JSON_DEPTH: usize = MAX_SUPPORTED_JSON_DEPTH;
const DEFAULT_MAX_MIGRATION_OPERATIONS: usize = 100_000;
const DEFAULT_MAX_MIGRATION_ISSUES: usize = 1_024;
const DEFAULT_MAX_MIGRATION_ISSUE_MESSAGE_BYTES: usize = 256 * 1024;

/// Resource budgets applied while loading and saving persisted graph and view-session state.
///
/// The file-byte limit bounds encoded output and temporary parser scratch space. String, value,
/// collection, and entity limits bound retained semantic data; an escaped string may use parser
/// scratch up to the file-byte limit before its decoded length is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceLimits {
    max_bytes: usize,
    max_entities: usize,
    max_collection_items: usize,
    max_string_bytes: usize,
    max_json_values: usize,
    max_json_depth: usize,
    max_migration_operations: usize,
    max_migration_issues: usize,
    max_migration_issue_message_bytes: usize,
}

impl PersistenceLimits {
    /// Returns the default persistence budgets.
    pub const fn new() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_entities: DEFAULT_MAX_ENTITIES,
            max_collection_items: DEFAULT_MAX_COLLECTION_ITEMS,
            max_string_bytes: DEFAULT_MAX_STRING_BYTES,
            max_json_values: DEFAULT_MAX_JSON_VALUES,
            max_json_depth: DEFAULT_MAX_JSON_DEPTH,
            max_migration_operations: DEFAULT_MAX_MIGRATION_OPERATIONS,
            max_migration_issues: DEFAULT_MAX_MIGRATION_ISSUES,
            max_migration_issue_message_bytes: DEFAULT_MAX_MIGRATION_ISSUE_MESSAGE_BYTES,
        }
    }

    /// Replaces the maximum number of bytes encoded to or accepted from one persistence document.
    pub const fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Replaces the maximum number of graph entities, structural references, or persisted view IDs.
    pub const fn with_max_entities(mut self, max_entities: usize) -> Self {
        self.max_entities = max_entities;
        self
    }

    /// Replaces the cumulative item budget for variable-length persisted maps and sequences.
    pub const fn with_max_collection_items(mut self, max_collection_items: usize) -> Self {
        self.max_collection_items = max_collection_items;
        self
    }

    /// Replaces the cumulative UTF-8 byte budget for persisted owned strings and map keys.
    pub const fn with_max_string_bytes(mut self, max_string_bytes: usize) -> Self {
        self.max_string_bytes = max_string_bytes;
        self
    }

    /// Replaces the cumulative value budget for dynamically decoded JSON payloads.
    pub const fn with_max_json_values(mut self, max_json_values: usize) -> Self {
        self.max_json_values = max_json_values;
        self
    }

    /// Replaces the nesting-depth budget for persisted JSON documents and payloads.
    ///
    /// Values above the parser's supported depth are capped at 128 so this budget remains the
    /// single recursion limit for persistence decoding.
    pub const fn with_max_json_depth(mut self, max_json_depth: usize) -> Self {
        self.max_json_depth = if max_json_depth > MAX_SUPPORTED_JSON_DEPTH {
            MAX_SUPPORTED_JSON_DEPTH
        } else {
            max_json_depth
        };
        self
    }

    /// Replaces the number of known-kind migrator invocations allowed per graph load.
    pub const fn with_max_migration_operations(mut self, max_migration_operations: usize) -> Self {
        self.max_migration_operations = max_migration_operations;
        self
    }

    /// Replaces the number of migration incompatibilities retained in one diagnostic.
    pub const fn with_max_migration_issues(mut self, max_migration_issues: usize) -> Self {
        self.max_migration_issues = max_migration_issues;
        self
    }

    /// Replaces the cumulative UTF-8 byte budget for retained migrator error messages.
    pub const fn with_max_migration_issue_message_bytes(
        mut self,
        max_migration_issue_message_bytes: usize,
    ) -> Self {
        self.max_migration_issue_message_bytes = max_migration_issue_message_bytes;
        self
    }

    /// Returns the maximum encoded or accepted persistence-document length.
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    /// Returns the graph entity/reference or view-ID budget.
    pub const fn max_entities(self) -> usize {
        self.max_entities
    }

    /// Returns the cumulative variable-length collection-item budget.
    pub const fn max_collection_items(self) -> usize {
        self.max_collection_items
    }

    /// Returns the cumulative persisted string and map-key byte budget.
    pub const fn max_string_bytes(self) -> usize {
        self.max_string_bytes
    }

    /// Returns the cumulative dynamically decoded JSON value budget.
    pub const fn max_json_values(self) -> usize {
        self.max_json_values
    }

    /// Returns the persisted JSON nesting-depth budget.
    pub const fn max_json_depth(self) -> usize {
        self.max_json_depth
    }

    /// Returns the known-kind migrator invocation budget.
    pub const fn max_migration_operations(self) -> usize {
        self.max_migration_operations
    }

    /// Returns the number of migration incompatibilities retained in one diagnostic.
    pub const fn max_migration_issues(self) -> usize {
        self.max_migration_issues
    }

    /// Returns the cumulative retained migrator-message byte budget.
    pub const fn max_migration_issue_message_bytes(self) -> usize {
        self.max_migration_issue_message_bytes
    }

    pub(crate) fn ensure(
        self,
        resource: PersistenceResource,
        actual: usize,
    ) -> Result<(), PersistenceLimitError> {
        let limit = match resource {
            PersistenceResource::FileBytes => self.max_bytes,
            PersistenceResource::Entities => self.max_entities,
            PersistenceResource::CollectionItems => self.max_collection_items,
            PersistenceResource::StringBytes => self.max_string_bytes,
            PersistenceResource::JsonValues => self.max_json_values,
            PersistenceResource::JsonDepth => self.max_json_depth,
            PersistenceResource::MigrationOperations => self.max_migration_operations,
        };
        if actual > limit {
            return Err(PersistenceLimitError {
                resource,
                limit,
                actual,
            });
        }
        Ok(())
    }
}

impl Default for PersistenceLimits {
    fn default() -> Self {
        Self::new()
    }
}

/// Persistence resource whose configured budget was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceResource {
    /// Serialized file or byte-slice length.
    FileBytes,
    /// Graph entities/references or persisted view IDs.
    Entities,
    /// Items in variable-length persisted maps and sequences.
    CollectionItems,
    /// Cumulative UTF-8 bytes in persisted owned strings and map keys.
    StringBytes,
    /// Cumulative dynamically decoded JSON values.
    JsonValues,
    /// Persisted JSON nesting depth.
    JsonDepth,
    /// Known-kind migrator invocations.
    MigrationOperations,
}

impl fmt::Display for PersistenceResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FileBytes => "file bytes",
            Self::Entities => "entities",
            Self::CollectionItems => "collection items",
            Self::StringBytes => "string bytes",
            Self::JsonValues => "JSON values",
            Self::JsonDepth => "JSON depth",
            Self::MigrationOperations => "migration operations",
        })
    }
}

/// A persisted document exceeded a configured resource budget.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("persistence {resource} limit exceeded: observed {actual}, limit {limit}")]
pub struct PersistenceLimitError {
    resource: PersistenceResource,
    limit: usize,
    actual: usize,
}

impl PersistenceLimitError {
    /// Returns the resource whose budget was exceeded.
    pub const fn resource(&self) -> PersistenceResource {
        self.resource
    }

    /// Returns the configured budget.
    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Returns the amount observed before loading stopped.
    pub const fn actual(&self) -> usize {
        self.actual
    }
}

pub(crate) enum BoundedReadError {
    Io(io::Error),
    Limit(PersistenceLimitError),
}

pub(crate) fn read_bounded(
    path: &Path,
    limits: PersistenceLimits,
) -> Result<Vec<u8>, BoundedReadError> {
    let file = File::open(path).map_err(BoundedReadError::Io)?;
    let probe_bytes = limits.max_bytes().saturating_add(1);
    let mut reader = file.take(u64::try_from(probe_bytes).unwrap_or(u64::MAX));
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(BoundedReadError::Io)?;
    limits
        .ensure(PersistenceResource::FileBytes, bytes.len())
        .map_err(BoundedReadError::Limit)?;
    Ok(bytes)
}

pub(crate) enum BoundedJsonDecodeError {
    Parse(serde_json::Error),
    Limit(PersistenceLimitError),
}
