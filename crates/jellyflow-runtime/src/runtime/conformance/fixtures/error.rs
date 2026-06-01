use super::super::approval::ConformanceApprovalError;

#[derive(Debug, thiserror::Error)]
pub enum ConformanceFixtureFileError {
    #[error("failed to read conformance fixture directory: {path}")]
    ReadDirectory {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture directory entry: {path}")]
    ReadDirectoryEntry {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture directory entry type: {path}")]
    ReadDirectoryEntryType {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to read conformance fixture file: {path}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse conformance fixture JSON: {path}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to write conformance fixture file: {path}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to serialize conformance fixture JSON: {path}")]
    Serialize {
        path: String,
        source: serde_json::Error,
    },
    #[error("failed to approve conformance fixture file: {path}")]
    Approve {
        path: String,
        source: ConformanceApprovalError,
    },
}
