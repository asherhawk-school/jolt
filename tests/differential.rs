//! `cargo test` entry point for the differential test suite in tests/run.sh:
//! every program is compiled by both jolt and javac and must print the same output.

use std::process::Command;

#[test]
fn matches_javac_output() {
    if Command::new("javac").arg("-version").output().is_err() {
        eprintln!("skipping: javac is not installed");
        return;
    }
    let status = Command::new("bash")
        .arg("tests/run.sh")
        .env("JOLT", env!("CARGO_BIN_EXE_jolt"))
        .status()
        .expect("failed to run tests/run.sh");
    assert!(status.success(), "differential tests failed");
}
