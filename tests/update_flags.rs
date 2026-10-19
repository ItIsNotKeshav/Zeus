//! The update flags reach fastframe-update before anything else in `main`.
//!
//! The previous release's helper runs the installed executable as
//! `zapfast --apply-update <job>` and relaunches the new one with
//! `--update-receipt <job>` or `--update-error <message>`. Those must be
//! handled before the command line is parsed, before the single-instance
//! lock, and before any directory, log or setting is touched.

use std::path::Path;
use std::process::{Command, Output};

/// Runs the built executable with every per-user location inside `home`.
fn zapfast(home: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zeus"));
    command.args(arguments).env_remove("RUST_LOG");
    for variable in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
    ] {
        command.env(variable, home);
    }
    command.output().expect("the zapfast executable runs")
}

fn entries(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn apply_update_runs_the_helper_before_anything_else() {
    let home = tempfile::tempdir().unwrap();
    let job = home.path().join("missing").join("handoff.json");
    let output = zapfast(home.path(), &["--apply-update", job.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The helper reports a job it cannot read and exits with 1; clap would
    // have rejected the unknown flag with 2 and its usage text.
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(!stderr.contains("Usage"), "stderr: {stderr}");
    assert!(!stderr.is_empty());
    // No directory, lock, log or setting was created on the way.
    assert_eq!(entries(home.path()), Vec::<String>::new());
}

#[test]
fn update_receipt_and_error_are_taken_off_the_command_line() {
    let home = tempfile::tempdir().unwrap();
    let job = home.path().join("handoff.json");
    let output = zapfast(
        home.path(),
        &[
            "--update-receipt",
            job.to_str().unwrap(),
            "--update-error",
            "The update could not start. The previous version has been restored.",
            "--version",
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // An update helper checks this exact answer: the command's name and
    // version.
    assert!(output.status.success(), "stderr: {stderr}");
    assert_eq!(
        stdout.trim(),
        format!("zeus {}", env!("CARGO_PKG_VERSION")),
        "stderr: {stderr}"
    );
    assert_eq!(entries(home.path()), Vec::<String>::new());
}
