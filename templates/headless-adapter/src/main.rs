use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

use jellyflow_headless_adapter_template::{
    approve_fixture_directory, check_builtin_suite, check_fixture_directory,
};
use serde_json::to_writer_pretty;

const USAGE: &str = "usage: headless-adapter-template check [fixture-dir]\n       headless-adapter-template approve <fixture-dir>";

fn main() {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let code = run_cli(env::args_os().skip(1), &mut stdout, &mut stderr);
    std::process::exit(code);
}

fn run_cli<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = OsString>,
    W: Write,
    E: Write,
{
    match run(args, stdout) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(error) => {
            let _ = writeln!(stderr, "{error}\n\n{USAGE}");
            1
        }
    }
}

fn run<I, W>(args: I, stdout: &mut W) -> Result<bool, String>
where
    I: IntoIterator<Item = OsString>,
    W: Write,
{
    let mut args = args.into_iter();
    let mode = args.next().unwrap_or_else(|| OsString::from("check"));

    match mode.to_string_lossy().as_ref() {
        "check" => {
            let fixture_dir = args.next().map(PathBuf::from);
            if args.next().is_some() {
                return Err("unexpected extra arguments".to_owned());
            }
            if let Some(fixture_dir) = fixture_dir {
                let report = check_fixture_directory(fixture_dir)?;
                write_json(stdout, &report)?;
                Ok(report.is_match())
            } else {
                let report = check_builtin_suite();
                write_json(stdout, &report)?;
                Ok(report.is_match())
            }
        }
        "approve" => {
            let fixture_dir = args
                .next()
                .ok_or_else(|| "missing fixture directory argument".to_owned())?;
            if args.next().is_some() {
                return Err("unexpected extra arguments".to_owned());
            }
            let report = approve_fixture_directory(PathBuf::from(fixture_dir))?;
            write_json(stdout, &report)?;
            Ok(report.is_approvable())
        }
        other => Err(format!("unsupported mode `{other}`")),
    }
}

fn write_json<W, T>(writer: &mut W, value: &T) -> Result<(), String>
where
    W: Write,
    T: serde::Serialize,
{
    to_writer_pretty(&mut *writer, value).map_err(|err| err.to_string())?;
    writeln!(writer).map_err(|err| err.to_string())
}
