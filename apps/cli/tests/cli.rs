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

#[test]
fn render_writes_apng_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("progress.apng");
    let output_toml = output.to_string_lossy().replace('\\', "\\\\");
    let config = dir.path().join("config.toml");
    let segments = dir.path().join("segments.txt");
    fs::write(
        &config,
        format!(
            r#"
[render]
width = 96
height = 54
fps = 2

[bar]
height = 16
margin_x = 8
margin_bottom = 6

[output]
format = "apng"
path = "{output_toml}"
"#
        ),
    )
    .unwrap();
    fs::write(&segments, "1 | A\n").unwrap();

    let mut cmd = Command::cargo_bin("progressbar2video").unwrap();
    cmd.arg("render")
        .arg("--config")
        .arg(config)
        .arg("--segments")
        .arg(segments)
        .assert()
        .success()
        .stdout(contains("Rendered 2/2 frames"));

    let bytes = fs::read(output).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(bytes.windows(4).any(|window| window == b"acTL"));
}
