use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow_runtime::ViewSize;
use open_gpui_canvas_jellyflow::blackboard::NATIVE_BLACKBOARD_WIDTH;

const DEFAULT_WINDOW_WIDTH: u32 = 960;
const DEFAULT_WINDOW_HEIGHT: u32 = 640;
#[cfg(any(test, feature = "real-window-smoke"))]
const SURFACE_SIZE_TOLERANCE: f32 = 2.0;

const WIDTH_ENV: &str = "JELLYFLOW_SMOKE_WIDTH";
const HEIGHT_ENV: &str = "JELLYFLOW_SMOKE_HEIGHT";
const ZOOM_ENV: &str = "JELLYFLOW_SMOKE_ZOOM";
const CAPTURE_DIR_ENV: &str = "JELLYFLOW_SMOKE_CAPTURE_DIR";

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

    #[cfg(any(test, feature = "real-window-smoke"))]
    const fn file_label(self) -> &'static str {
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

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SmokeCaptureState {
    Initial,
}

#[cfg(any(test, feature = "real-window-smoke"))]
impl SmokeCaptureState {
    const fn file_label(self) -> &'static str {
        match self {
            Self::Initial => "initial",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SmokeConfig {
    window_width: u32,
    window_height: u32,
    zoom: SmokeZoom,
    capture_dir: Option<PathBuf>,
}

impl Default for SmokeConfig {
    fn default() -> Self {
        Self {
            window_width: DEFAULT_WINDOW_WIDTH,
            window_height: DEFAULT_WINDOW_HEIGHT,
            zoom: SmokeZoom::default(),
            capture_dir: None,
        }
    }
}

impl SmokeConfig {
    pub(crate) fn from_env() -> Result<Self, SmokeConfigError> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Self, SmokeConfigError> {
        let defaults = Self::default();
        let window_width = parse_dimension(lookup(WIDTH_ENV), WIDTH_ENV, defaults.window_width)?;
        let window_height =
            parse_dimension(lookup(HEIGHT_ENV), HEIGHT_ENV, defaults.window_height)?;
        let minimum_width = NATIVE_BLACKBOARD_WIDTH as u32 + 1;
        if window_width < minimum_width {
            return Err(SmokeConfigError::WindowTooNarrow {
                width: window_width,
                minimum: minimum_width,
            });
        }
        let zoom = lookup(ZOOM_ENV).map_or(Ok(defaults.zoom), |value| {
            let value = value
                .into_string()
                .map_err(|_| SmokeConfigError::NonUnicodeValue(ZOOM_ENV))?;
            SmokeZoom::parse(&value)
        })?;
        let capture_dir = lookup(CAPTURE_DIR_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty());

        Ok(Self {
            window_width,
            window_height,
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
        (self.window_width as f32, self.window_height as f32)
    }

    #[cfg(any(test, feature = "real-window-smoke"))]
    pub(super) const fn zoom(&self) -> SmokeZoom {
        self.zoom
    }

    #[cfg(any(test, feature = "real-window-smoke"))]
    pub(super) fn surface_matches(&self, surface: ViewSize) -> bool {
        let expected_width = self.window_width as f32 - NATIVE_BLACKBOARD_WIDTH;
        (surface.width - expected_width).abs() <= SURFACE_SIZE_TOLERANCE
            && (surface.height - self.window_height as f32).abs() <= SURFACE_SIZE_TOLERANCE
    }

    #[cfg(any(test, feature = "real-window-smoke"))]
    pub(super) fn capture_path(&self, demo: &str, state: SmokeCaptureState) -> Option<PathBuf> {
        self.capture_dir.as_ref().map(|directory| {
            directory.join(format!(
                "jellyflow-{demo}-{}x{}-z{}-{}.png",
                self.window_width,
                self.window_height,
                self.zoom.file_label(),
                state.file_label(),
            ))
        })
    }

    #[cfg(test)]
    fn capture_dir(&self) -> Option<&std::path::Path> {
        self.capture_dir.as_deref()
    }
}

fn parse_dimension(
    value: Option<OsString>,
    name: &'static str,
    default: u32,
) -> Result<u32, SmokeConfigError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let value = value
        .into_string()
        .map_err(|_| SmokeConfigError::NonUnicodeValue(name))?;
    let parsed = value
        .parse::<u32>()
        .map_err(|_| SmokeConfigError::InvalidDimension {
            name,
            value: value.clone(),
        })?;
    if parsed == 0 {
        return Err(SmokeConfigError::InvalidDimension { name, value });
    }
    Ok(parsed)
}

#[derive(Debug)]
pub(crate) enum SmokeConfigError {
    NonUnicodeValue(&'static str),
    InvalidDimension {
        name: &'static str,
        value: String,
    },
    InvalidZoom(String),
    WindowTooNarrow {
        width: u32,
        minimum: u32,
    },
    CreateCaptureDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for SmokeConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUnicodeValue(name) => write!(formatter, "{name} must be valid UTF-8"),
            Self::InvalidDimension { name, value } => {
                write!(
                    formatter,
                    "{name} must be a positive integer, got {value:?}"
                )
            }
            Self::InvalidZoom(value) => write!(
                formatter,
                "{ZOOM_ENV} must be one of 0.5, 1, or 2, got {value:?}"
            ),
            Self::WindowTooNarrow { width, minimum } => write!(
                formatter,
                "{WIDTH_ENV} must be at least {minimum} to leave space for the graph surface, got {width}"
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
    fn defaults_match_the_compact_smoke_window() {
        let config = config(&[]).expect("default config");

        assert_eq!(config.window_size(), (960.0, 640.0));
        assert_eq!(config.zoom(), SmokeZoom::One);
        assert_eq!(config.capture_dir(), None);
        assert!(config.surface_matches(ViewSize::new(680.0, 640.0)));
    }

    #[test]
    fn parses_supported_window_zoom_and_capture_values() {
        let config = config(&[
            (WIDTH_ENV, "1440"),
            (HEIGHT_ENV, "900"),
            (ZOOM_ENV, "0.5"),
            (CAPTURE_DIR_ENV, "target/smoke captures"),
        ])
        .expect("custom config");

        assert_eq!(config.window_size(), (1440.0, 900.0));
        assert_eq!(config.zoom(), SmokeZoom::Half);
        assert_eq!(
            config.capture_dir(),
            Some(Path::new("target/smoke captures"))
        );
        assert!(config.surface_matches(ViewSize::new(1160.0, 900.0)));
    }

    #[test]
    fn capture_names_are_stable_and_encode_the_visual_matrix() {
        let config = config(&[
            (WIDTH_ENV, "1440"),
            (HEIGHT_ENV, "900"),
            (ZOOM_ENV, "2"),
            (CAPTURE_DIR_ENV, "target/captures"),
        ])
        .expect("capture config");

        assert_eq!(
            config.capture_path("workflow", SmokeCaptureState::Initial),
            Some(PathBuf::from(
                "target/captures/jellyflow-workflow-1440x900-z2-initial.png"
            ))
        );
    }

    #[test]
    fn rejects_invalid_dimensions_and_zoom_before_opening_a_window() {
        assert!(matches!(
            config(&[(WIDTH_ENV, "0")]),
            Err(SmokeConfigError::InvalidDimension {
                name: WIDTH_ENV,
                ..
            })
        ));
        assert!(matches!(
            config(&[(HEIGHT_ENV, "wide")]),
            Err(SmokeConfigError::InvalidDimension {
                name: HEIGHT_ENV,
                ..
            })
        ));
        assert!(matches!(
            config(&[(WIDTH_ENV, "280")]),
            Err(SmokeConfigError::WindowTooNarrow { .. })
        ));
        assert!(matches!(
            config(&[(ZOOM_ENV, "1.25")]),
            Err(SmokeConfigError::InvalidZoom(value)) if value == "1.25"
        ));
    }
}
