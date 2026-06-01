use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::ConformanceFixtureFileError;
use super::suite_file::{ConformanceSuiteFile, ConformanceSuiteFileReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectory {
    pub root: PathBuf,
    pub files: Vec<ConformanceSuiteFile>,
}

impl ConformanceFixtureDirectory {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ConformanceFixtureFileError> {
        let root = path.as_ref();
        let mut suite_paths = Vec::new();
        collect_conformance_json_files(root, &mut suite_paths)?;
        suite_paths.sort();
        let files = suite_paths
            .into_iter()
            .map(ConformanceSuiteFile::load_json)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            root: root.to_path_buf(),
            files,
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

    pub fn run(&self) -> ConformanceFixtureDirectoryReport {
        ConformanceFixtureDirectoryReport {
            root: self.root.clone(),
            reports: self.files.iter().map(ConformanceSuiteFile::run).collect(),
        }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceFixtureDirectoryReport {
    pub root: PathBuf,
    pub reports: Vec<ConformanceSuiteFileReport>,
}

impl ConformanceFixtureDirectoryReport {
    pub fn is_match(&self) -> bool {
        self.reports.iter().all(|report| report.report.is_match())
    }

    pub fn file_count(&self) -> usize {
        self.reports.len()
    }

    pub fn scenario_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.scenario_count())
            .sum()
    }

    pub fn failed_files(&self) -> usize {
        self.reports
            .iter()
            .filter(|report| !report.report.is_match())
            .count()
    }

    pub fn failed_scenarios(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.failed_scenarios())
            .sum()
    }

    pub fn error_count(&self) -> usize {
        self.reports
            .iter()
            .map(|report| report.report.errors.len())
            .sum()
    }
}

impl fmt::Display for ConformanceFixtureDirectoryReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_match() {
            return write!(
                f,
                "conformance fixture directory `{}` matched {} file(s), {} scenario(s)",
                self.root.display(),
                self.file_count(),
                self.scenario_count()
            );
        }

        writeln!(
            f,
            "conformance fixture directory `{}` failed: {} file(s), {} scenario(s), {} execution error(s)",
            self.root.display(),
            self.failed_files(),
            self.failed_scenarios(),
            self.error_count()
        )?;
        for report in self
            .reports
            .iter()
            .filter(|report| !report.report.is_match())
            .take(8)
        {
            writeln!(f, "  file `{}`: {}", report.path.display(), report.report)?;
        }
        Ok(())
    }
}

fn collect_conformance_json_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), ConformanceFixtureFileError> {
    let entries =
        std::fs::read_dir(dir).map_err(|source| ConformanceFixtureFileError::ReadDirectory {
            path: dir.display().to_string(),
            source,
        })?;

    for entry in entries {
        let entry = entry.map_err(|source| ConformanceFixtureFileError::ReadDirectoryEntry {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| {
            ConformanceFixtureFileError::ReadDirectoryEntryType {
                path: path.display().to_string(),
                source,
            }
        })?;

        if file_type.is_dir() {
            collect_conformance_json_files(&path, files)?;
        } else if file_type.is_file() && is_conformance_json_path(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn is_conformance_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}
