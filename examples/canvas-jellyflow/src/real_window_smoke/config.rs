use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::native_app::NativeDemo;

#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow_runtime::geometry::ViewSize;
#[cfg(any(test, feature = "real-window-smoke"))]
use open_gpui_canvas_jellyflow::blackboard::NATIVE_BLACKBOARD_WIDTH;

const COMPACT_WINDOW_WIDTH: u32 = 960;
const COMPACT_WINDOW_HEIGHT: u32 = 640;
const NORMAL_WINDOW_WIDTH: u32 = 1360;
const NORMAL_WINDOW_HEIGHT: u32 = 800;
#[cfg(any(test, feature = "real-window-smoke"))]
const SURFACE_SIZE_TOLERANCE: f32 = 2.0;

const BOUNDS_ENV: &str = "JELLYFLOW_SMOKE_BOUNDS";
const SCENARIO_ENV: &str = "JELLYFLOW_SMOKE_SCENARIO";
const ZOOM_ENV: &str = "JELLYFLOW_SMOKE_ZOOM";
const CAPTURE_DIR_ENV: &str = "JELLYFLOW_SMOKE_CAPTURE_DIR";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SmokeBounds {
    #[default]
    Compact,
    Normal,
}

impl SmokeBounds {
    const fn window_size(self) -> (u32, u32) {
        match self {
            Self::Compact => (COMPACT_WINDOW_WIDTH, COMPACT_WINDOW_HEIGHT),
            Self::Normal => (NORMAL_WINDOW_WIDTH, NORMAL_WINDOW_HEIGHT),
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Normal => "normal",
        }
    }

    fn parse(value: &str) -> Result<Self, SmokeConfigError> {
        match value {
            "compact" => Ok(Self::Compact),
            "normal" => Ok(Self::Normal),
            _ => Err(SmokeConfigError::InvalidBounds(value.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SmokeScenario {
    #[default]
    Interaction,
    LodThreshold,
}

impl SmokeScenario {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Interaction => "interaction",
            Self::LodThreshold => "lod-threshold",
        }
    }

    fn parse(value: &str) -> Result<Self, SmokeConfigError> {
        match value {
            "interaction" => Ok(Self::Interaction),
            "lod-threshold" => Ok(Self::LodThreshold),
            _ => Err(SmokeConfigError::InvalidScenario(value.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SmokeZoom {
    Half,
    #[default]
    One,
    Two,
}

impl SmokeZoom {
    #[cfg(feature = "real-window-smoke")]
    pub(super) const fn value(self) -> f32 {
        match self {
            Self::Half => 0.5,
            Self::One => 1.0,
            Self::Two => 2.0,
        }
    }

    pub(super) const fn file_label(self) -> &'static str {
        match self {
            Self::Half => "0.5",
            Self::One => "1",
            Self::Two => "2",
        }
    }

    fn parse(value: &str) -> Result<Self, SmokeConfigError> {
        match value {
            "0.5" => Ok(Self::Half),
            "1" | "1.0" => Ok(Self::One),
            "2" | "2.0" => Ok(Self::Two),
            _ => Err(SmokeConfigError::InvalidZoom(value.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SmokeCaptureState {
    Initial,
    Selected,
    ConnectionPreview,
    ConnectionCommitted,
    ReconnectSelected,
    ReconnectPreview,
    ReconnectCommitted,
    SearchOpen,
    Final,
    Lod086,
    Lod085,
    Lod084,
}

impl SmokeCaptureState {
    pub(super) const fn file_label(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Selected => "selected",
            Self::ConnectionPreview => "connection-preview",
            Self::ConnectionCommitted => "connection-committed",
            Self::ReconnectSelected => "reconnect-selected",
            Self::ReconnectPreview => "reconnect-preview",
            Self::ReconnectCommitted => "reconnect-committed",
            Self::SearchOpen => "search-open",
            Self::Final => "final",
            Self::Lod086 => "lod-0.86",
            Self::Lod085 => "lod-0.85",
            Self::Lod084 => "lod-0.84",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SmokeConfig {
    bounds: SmokeBounds,
    scenario: SmokeScenario,
    zoom: SmokeZoom,
    capture_dir: Option<PathBuf>,
}

impl SmokeConfig {
    pub(crate) fn from_env() -> Result<Self, SmokeConfigError> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Self, SmokeConfigError> {
        let defaults = Self::default();
        let bounds = parse_named(
            lookup(BOUNDS_ENV),
            BOUNDS_ENV,
            defaults.bounds,
            SmokeBounds::parse,
        )?;
        let scenario = parse_named(
            lookup(SCENARIO_ENV),
            SCENARIO_ENV,
            defaults.scenario,
            SmokeScenario::parse,
        )?;
        let zoom = parse_named(lookup(ZOOM_ENV), ZOOM_ENV, defaults.zoom, SmokeZoom::parse)?;
        let capture_dir = lookup(CAPTURE_DIR_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty());

        Ok(Self {
            bounds,
            scenario,
            zoom,
            capture_dir,
        })
    }

    pub(crate) fn prepare_capture_dir(&self) -> Result<(), SmokeConfigError> {
        let Some(path) = self.capture_dir.as_deref() else {
            return Ok(());
        };
        std::fs::create_dir_all(path).map_err(|source| SmokeConfigError::CreateCaptureDir {
            path: path.to_path_buf(),
            source,
        })
    }

    pub(super) fn window_size(&self) -> (f32, f32) {
        let (width, height) = self.bounds.window_size();
        (width as f32, height as f32)
    }

    pub(super) const fn bounds(&self) -> SmokeBounds {
        self.bounds
    }

    pub(super) const fn scenario(&self) -> SmokeScenario {
        self.scenario
    }

    pub(super) const fn zoom(&self) -> SmokeZoom {
        self.zoom
    }

    #[cfg(test)]
    pub(super) fn for_test(bounds: SmokeBounds, scenario: SmokeScenario, zoom: SmokeZoom) -> Self {
        Self {
            bounds,
            scenario,
            zoom,
            capture_dir: None,
        }
    }

    #[cfg(any(test, feature = "real-window-smoke"))]
    pub(super) fn surface_matches(&self, surface: ViewSize) -> bool {
        let (window_width, window_height) = self.bounds.window_size();
        let expected_width = window_width as f32 - NATIVE_BLACKBOARD_WIDTH;
        (surface.width - expected_width).abs() <= SURFACE_SIZE_TOLERANCE
            && (surface.height - window_height as f32).abs() <= SURFACE_SIZE_TOLERANCE
    }

    pub(super) fn capture_path(
        &self,
        demo: NativeDemo,
        state: SmokeCaptureState,
    ) -> Option<PathBuf> {
        self.capture_dir.as_ref().map(|directory| {
            let (width, height) = self.bounds.window_size();
            directory.join(format!(
                "jellyflow-{}-{}-{width}x{height}-z{}-{}-{}.png",
                demo.label(),
                self.bounds.label(),
                self.zoom.file_label(),
                self.scenario.label(),
                state.file_label(),
            ))
        })
    }

    #[cfg(test)]
    fn capture_dir(&self) -> Option<&std::path::Path> {
        self.capture_dir.as_deref()
    }
}

fn parse_named<T: Copy>(
    value: Option<OsString>,
    name: &'static str,
    default: T,
    parse: impl FnOnce(&str) -> Result<T, SmokeConfigError>,
) -> Result<T, SmokeConfigError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let value = value
        .into_string()
        .map_err(|_| SmokeConfigError::NonUnicodeValue(name))?;
    parse(&value)
}

#[derive(Debug)]
pub(crate) enum SmokeConfigError {
    NonUnicodeValue(&'static str),
    InvalidBounds(String),
    InvalidScenario(String),
    InvalidZoom(String),
    CreateCaptureDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for SmokeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUnicodeValue(name) => write!(formatter, "{name} must be valid UTF-8"),
            Self::InvalidBounds(value) => write!(
                formatter,
                "{BOUNDS_ENV} must be compact or normal, got {value:?}"
            ),
            Self::InvalidScenario(value) => write!(
                formatter,
                "{SCENARIO_ENV} must be interaction or lod-threshold, got {value:?}"
            ),
            Self::InvalidZoom(value) => write!(
                formatter,
                "{ZOOM_ENV} must be one of 0.5, 1, or 2, got {value:?}"
            ),
            Self::CreateCaptureDir { path, source } => {
                write!(formatter, "failed to create {}: {source}", path.display())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn config(values: &[(&str, &str)]) -> Result<SmokeConfig, SmokeConfigError> {
        let values = values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect::<BTreeMap<_, _>>();
        SmokeConfig::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn parses_supported_values_and_rejects_invalid_names() {
        let parsed = config(&[
            (BOUNDS_ENV, "normal"),
            (SCENARIO_ENV, "lod-threshold"),
            (ZOOM_ENV, "0.5"),
            (CAPTURE_DIR_ENV, "target/smoke captures"),
        ])
        .expect("custom config");

        assert_eq!(parsed.window_size(), (1360.0, 800.0));
        assert_eq!(parsed.bounds(), SmokeBounds::Normal);
        assert_eq!(parsed.scenario(), SmokeScenario::LodThreshold);
        assert_eq!(parsed.zoom(), SmokeZoom::Half);
        assert_eq!(
            parsed.capture_dir(),
            Some(Path::new("target/smoke captures"))
        );
        assert!(parsed.surface_matches(ViewSize::new(1080.0, 800.0)));

        assert!(matches!(
            config(&[(BOUNDS_ENV, "wide")]),
            Err(SmokeConfigError::InvalidBounds(value)) if value == "wide"
        ));
        assert!(matches!(
            config(&[(SCENARIO_ENV, "everything")]),
            Err(SmokeConfigError::InvalidScenario(value)) if value == "everything"
        ));
        assert!(matches!(
            config(&[(ZOOM_ENV, "1.25")]),
            Err(SmokeConfigError::InvalidZoom(value)) if value == "1.25"
        ));
    }
}
