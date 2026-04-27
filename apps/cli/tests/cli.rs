use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

#[test]
fn validate_accepts_example_project() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let segments = dir.path().join("segments.txt");
    fs::write(&config, "[render]\nwidth = 320\nheight = 180\nfps = 30\n").unwrap();
    fs::write(&segments, "2 | 开场\n").unwrap();

    let mut cmd = Command::cargo_bin("progressbar2video").unwrap();
    cmd.arg("validate")
        .arg("--config")
        .arg(config)
        .arg("--segments")
        .arg(segments)
        .assert()
        .success()
        .stdout(contains("Project is valid."));
}
