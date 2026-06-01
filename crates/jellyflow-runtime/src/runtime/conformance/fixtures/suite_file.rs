use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::super::reports::ConformanceSuiteReport;
use super::super::scenario::ConformanceSuite;
use super::error::ConformanceFixtureFileError;

impl ConformanceSuite {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| ConformanceFixtureFileError::Read {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| ConformanceFixtureFileError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn load_json_if_exists(
        path: impl AsRef<Path>,
    ) -> Result<Option<Self>, ConformanceFixtureFileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::load_json(path).map(Some)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), ConformanceFixtureFileError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                ConformanceFixtureFileError::Write {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| {
            ConformanceFixtureFileError::Serialize {
                path: path.display().to_string(),
                source,
            }
        })?;
        std::fs::write(path, bytes).map_err(|source| ConformanceFixtureFileError::Write {
            path: path.display().to_string(),
            source,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFile {
    pub path: PathBuf,
    pub suite: ConformanceSuite,
}

impl ConformanceSuiteFile {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let path = path.as_ref();
        Ok(Self {
            path: path.to_path_buf(),
            suite: ConformanceSuite::load_json(path)?,
        })
    }

    pub fn run(&self) -> ConformanceSuiteFileReport {
        ConformanceSuiteFileReport {
            path: self.path.clone(),
            report: self.suite.run(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceSuiteFileReport {
    pub path: PathBuf,
    pub report: ConformanceSuiteReport,
}
