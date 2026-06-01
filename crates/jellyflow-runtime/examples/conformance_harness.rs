use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

use jellyflow_runtime::runtime::conformance::ConformanceFixtureDirectory;
use serde::Serialize;

const EXIT_SUCCESS: i32 = 0;
const EXIT_FAILURE: i32 = 1;
const USAGE: &str = "usage: conformance_harness <check|approve> <fixture-dir>";

fn main() {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let code = run_cli(env::args_os().skip(1), &mut stdout, &mut stderr);
    std::process::exit(code);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Approve,
}

#[derive(Debug)]
struct Command {
    mode: Mode,
    fixture_dir: PathBuf,
}

fn run_cli<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    let command = match parse_args(args) {
        Ok(command) => command,
        Err(message) => {
            let _ = writeln!(stderr, "{message}\n\n{USAGE}");
            return EXIT_FAILURE;
        }
    };

    match command.mode {
        Mode::Check => check_fixtures(command.fixture_dir, stdout, stderr),
        Mode::Approve => approve_fixtures(command.fixture_dir, stdout, stderr),
    }
}

fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mode = args
        .next()
        .ok_or_else(|| "missing mode argument".to_owned())?;
    let fixture_dir = args
        .next()
        .ok_or_else(|| "missing fixture directory argument".to_owned())?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".to_owned());
    }

    let mode = match mode.to_string_lossy().as_ref() {
        "check" => Mode::Check,
        "approve" => Mode::Approve,
        other => return Err(format!("unsupported mode `{other}`")),
    };

    Ok(Command {
        mode,
        fixture_dir: PathBuf::from(fixture_dir),
    })
}

fn check_fixtures<W, E>(fixture_dir: PathBuf, stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let directory = match ConformanceFixtureDirectory::load_json(&fixture_dir) {
        Ok(directory) => directory,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to load fixture directory `{}`: {error}",
                fixture_dir.display()
            );
            return EXIT_FAILURE;
        }
    };

    let report = directory.run();
    if write_json(stdout, &report).is_err() {
        let _ = writeln!(stderr, "failed to write conformance check report");
        return EXIT_FAILURE;
    }

    if report.is_match() {
        EXIT_SUCCESS
    } else {
        EXIT_FAILURE
    }
}

fn approve_fixtures<W, E>(fixture_dir: PathBuf, stdout: &mut W, stderr: &mut E) -> i32
where
    W: Write,
    E: Write,
{
    let directory = match ConformanceFixtureDirectory::load_json(&fixture_dir) {
        Ok(directory) => directory,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to load fixture directory `{}`: {error}",
                fixture_dir.display()
            );
            return EXIT_FAILURE;
        }
    };

    match directory.approve_actual_traces_to_json() {
        Ok(report) => {
            if write_json(stdout, &report).is_err() {
                let _ = writeln!(stderr, "failed to write conformance approval report");
                return EXIT_FAILURE;
            }
            EXIT_SUCCESS
        }
        Err(error) => {
            let _ = writeln!(
                stderr,
                "failed to approve fixture directory `{}`: {error}",
                fixture_dir.display()
            );
            EXIT_FAILURE
        }
    }
}

fn write_json<W, T>(writer: &mut W, value: &T) -> Result<(), io::Error>
where
    W: Write,
    T: Serialize,
{
    serde_json::to_writer_pretty(&mut *writer, value)?;
    writeln!(writer)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use jellyflow_core::core::{CanvasPoint, Graph, GraphId};
    use jellyflow_runtime::runtime::conformance::{
        ConformanceAction, ConformanceScenario, ConformanceSuite,
    };

    #[test]
    fn check_returns_failure_for_stale_fixture_directory() {
        let root = conformance_temp_dir("check-stale");
        std::fs::create_dir_all(&root).expect("create fixture root");
        stale_viewport_suite()
            .save_json(root.join("suite.json"))
            .expect("save stale fixture");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_cli(
            [OsString::from("check"), root.clone().into_os_string()],
            &mut stdout,
            &mut stderr,
        );
        let _ = std::fs::remove_dir_all(&root);

        let output = String::from_utf8(stdout).expect("utf8 stdout");
        assert_eq!(code, EXIT_FAILURE);
        assert!(String::from_utf8(stderr).expect("utf8 stderr").is_empty());
        assert!(output.contains("\"reports\""));
        assert!(output.contains("\"mismatches\""));
    }

    #[test]
    fn approve_updates_fixture_directory_and_check_then_passes() {
        let root = conformance_temp_dir("approve-then-check");
        std::fs::create_dir_all(&root).expect("create fixture root");
        stale_viewport_suite()
            .save_json(root.join("suite.json"))
            .expect("save stale fixture");

        let mut approve_stdout = Vec::new();
        let mut approve_stderr = Vec::new();
        let approve_code = run_cli(
            [OsString::from("approve"), root.clone().into_os_string()],
            &mut approve_stdout,
            &mut approve_stderr,
        );
        let mut check_stdout = Vec::new();
        let mut check_stderr = Vec::new();
        let check_code = run_cli(
            [OsString::from("check"), root.clone().into_os_string()],
            &mut check_stdout,
            &mut check_stderr,
        );
        let _ = std::fs::remove_dir_all(&root);

        let approve_output = String::from_utf8(approve_stdout).expect("utf8 approve stdout");
        assert_eq!(approve_code, EXIT_SUCCESS);
        assert!(
            String::from_utf8(approve_stderr)
                .expect("utf8 approve stderr")
                .is_empty()
        );
        assert!(approve_output.contains("\"changed\": true"));
        assert_eq!(check_code, EXIT_SUCCESS);
        assert!(
            String::from_utf8(check_stderr)
                .expect("utf8 check stderr")
                .is_empty()
        );
        assert!(
            String::from_utf8(check_stdout)
                .expect("utf8 check stdout")
                .contains("\"reports\"")
        );
    }

    #[test]
    fn missing_arguments_return_usage_error() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_cli([OsString::from("check")], &mut stdout, &mut stderr);

        assert_eq!(code, EXIT_FAILURE);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .expect("utf8 stderr")
                .contains(USAGE)
        );
    }

    fn stale_viewport_suite() -> ConformanceSuite {
        ConformanceSuite::new("harness suite").with_scenarios([ConformanceScenario::new(
            "viewport approval",
            Graph::new(GraphId::new()),
        )
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 10.0, y: 20.0 },
            1.5,
        )])])
    }

    fn conformance_temp_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "jellyflow-conformance-harness-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }
}
