# Encoder Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add APNG, FFV1 MKV, and ProRes 4444 MOV output profiles while keeping PNG sequence rendering and the shared API stable.

**Architecture:** Extend schema output formats first, then make `progressbar-encoder` dispatch by profile. APNG is written natively from rendered RGBA frames with PNG/APNG chunks so it has no runtime encoder dependency. FFmpeg-backed profiles construct explicit commands and stream raw RGBA frames to `ffmpeg` stdin, with system `PATH` discovery by default and optional `output.ffmpeg_path` override.

**Tech Stack:** Rust 2021, existing renderer/core/schema/API crates, `flate2` for APNG zlib frame data, `crc32fast` for PNG chunk CRCs, `std::process::Command` for FFmpeg.

---

## File Structure

- Modify `Cargo.toml`: add workspace dependencies for `flate2` and `crc32fast`.
- Modify `crates/progressbar-schema/src/lib.rs`: add `apng`, `ffv1-mkv`, `prores4444-mov`, and optional `ffmpeg_path`.
- Modify `crates/progressbar-encoder/Cargo.toml`: consume `flate2` and `crc32fast`.
- Modify `crates/progressbar-encoder/src/lib.rs`: add APNG writer, output dispatcher, FFmpeg command plan, and FFmpeg process renderer.
- Modify `crates/progressbar-api/src/lib.rs`: keep `render_overlay` as the shared entry point and let encoder dispatch by profile.
- Modify `apps/cli/tests/cli.rs`: cover CLI rendering to APNG.
- Modify `README.md`: document output profiles and example render commands.
- Add `examples/encoder-profiles/`: small APNG and FFmpeg profile configs.

---

### Task 1: Schema Output Profiles

**Files:**
- Modify: `crates/progressbar-schema/src/lib.rs`

- [ ] **Step 1: Write failing schema tests**

Add tests inside the schema test module:

```rust
#[test]
fn parses_encoder_output_profiles() {
    let apng = ProjectConfig::from_toml_str(
        r#"
[output]
format = "apng"
path = "out/progress.apng"
"#,
    )
    .unwrap();
    assert_eq!(apng.output.format, OutputFormat::Apng);

    let ffv1 = ProjectConfig::from_toml_str(
        r#"
[output]
format = "ffv1-mkv"
path = "out/progress.mkv"
ffmpeg_path = "tools/ffmpeg.exe"
"#,
    )
    .unwrap();
    assert_eq!(ffv1.output.format, OutputFormat::Ffv1Mkv);
    assert_eq!(ffv1.output.ffmpeg_path.unwrap(), PathBuf::from("tools/ffmpeg.exe"));

    let prores = ProjectConfig::from_toml_str(
        r#"
[output]
format = "prores4444-mov"
path = "out/progress.mov"
"#,
    )
    .unwrap();
    assert_eq!(prores.output.format, OutputFormat::Prores4444Mov);
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-schema parses_encoder_output_profiles
```

Expected: compile failure because the enum variants and `ffmpeg_path` field do not exist.

- [ ] **Step 3: Implement schema changes**

Change `OutputFormat`:

```rust
pub enum OutputFormat {
    PngSequence,
    Apng,
    Ffv1Mkv,
    Prores4444Mov,
}
```

Change `OutputConfig`:

```rust
pub struct OutputConfig {
    pub format: OutputFormat,
    pub path: PathBuf,
    pub ffmpeg_path: Option<PathBuf>,
}
```

Update `Default`:

```rust
Self {
    format: OutputFormat::PngSequence,
    path: PathBuf::from("out/progress"),
    ffmpeg_path: None,
}
```

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-schema
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/progressbar-schema/src/lib.rs
git commit -m "feat: add encoder output profiles to schema"
```

---

### Task 2: Native APNG Encoder

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/progressbar-encoder/Cargo.toml`
- Modify: `crates/progressbar-encoder/src/lib.rs`

- [ ] **Step 1: Add failing APNG test**

Add test helpers and a test inside the encoder test module:

```rust
fn png_chunk_names(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut offset = 8;
    while offset + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let name = std::str::from_utf8(&bytes[offset + 4..offset + 8]).unwrap().to_string();
        names.push(name);
        offset += 12 + length;
    }
    names
}

#[test]
fn writes_apng_with_animation_chunks_and_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = ProjectConfig::default();
    config.render.width = 96;
    config.render.height = 54;
    config.render.fps = 2;
    config.bar.margin_x = 8;
    config.bar.margin_bottom = 6;
    config.bar.height = 16;
    config.output.format = progressbar_schema::OutputFormat::Apng;
    config.output.path = dir.path().join("progress.apng");
    let timeline = Timeline::parse("1 | A").unwrap();
    let mut seen = Vec::new();

    render_apng(&config, &timeline, |event| seen.push(event)).unwrap();

    let bytes = std::fs::read(&config.output.path).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let chunks = png_chunk_names(&bytes);
    assert!(chunks.contains(&"acTL".to_string()));
    assert_eq!(chunks.iter().filter(|name| *name == "fcTL").count(), 2);
    assert!(chunks.contains(&"IDAT".to_string()));
    assert!(chunks.contains(&"fdAT".to_string()));
    assert_eq!(seen.last().unwrap().completed_frames, 2);
    assert_eq!(seen.last().unwrap().total_frames, 2);
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-encoder writes_apng_with_animation_chunks_and_progress
```

Expected: compile failure because `render_apng` does not exist.

- [ ] **Step 3: Add dependencies**

In workspace `Cargo.toml` add:

```toml
crc32fast = "1.3"
flate2 = "1.0"
```

In `crates/progressbar-encoder/Cargo.toml` add:

```toml
crc32fast.workspace = true
flate2.workspace = true
```

- [ ] **Step 4: Implement APNG writer**

Add:

```rust
use crc32fast::Hasher;
use flate2::{write::ZlibEncoder, Compression};
use progressbar_renderer::RenderedFrame;
use std::io::Write;
```

Add `EncodeError` variants:

```rust
#[error("failed to create output file: {0}")]
CreateOutput(std::io::Error),
#[error("failed to write output file: {0}")]
WriteOutput(std::io::Error),
#[error("APNG frame rate {fps} is too high for APNG frame delay")]
UnsupportedApngFps { fps: u32 },
#[error("APNG frame count {total_frames} exceeds the format limit")]
TooManyApngFrames { total_frames: u64 },
```

Implement `render_apng`, `write_apng`, `write_png_chunk`, `apng_frame_control`, and `compress_rgba_frame`:

```rust
pub fn render_apng<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    let total_frames = frame_count(timeline.duration_ms(), config.render.fps);
    let total_frames_u32 = u32::try_from(total_frames)
        .map_err(|_| EncodeError::TooManyApngFrames { total_frames })?;
    let delay_den = u16::try_from(config.render.fps)
        .map_err(|_| EncodeError::UnsupportedApngFps { fps: config.render.fps })?;

    if let Some(parent) = config.output.path.parent() {
        fs::create_dir_all(parent).map_err(EncodeError::CreateDir)?;
    }
    let file = fs::File::create(&config.output.path).map_err(EncodeError::CreateOutput)?;
    write_apng(config, timeline, file, total_frames, total_frames_u32, delay_den, &mut on_progress)
}
```

`write_apng` should write signature, `IHDR`, `acTL`, then each frame with `fcTL`. Frame 0 uses `IDAT`; later frames use `fdAT` with a sequence number prefix. Each frame uses filter byte `0` per row before zlib compression.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-encoder writes_apng_with_animation_chunks_and_progress
cargo test -p progressbar-encoder
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add Cargo.toml Cargo.lock crates/progressbar-encoder/Cargo.toml crates/progressbar-encoder/src/lib.rs
git commit -m "feat: add native apng encoder"
```

---

### Task 3: FFmpeg Command Profiles

**Files:**
- Modify: `crates/progressbar-encoder/src/lib.rs`

- [ ] **Step 1: Write failing FFmpeg command tests**

Add:

```rust
#[test]
fn builds_ffv1_command_with_alpha_pixel_format() {
    let mut config = ProjectConfig::default();
    config.render.width = 320;
    config.render.height = 180;
    config.render.fps = 30;
    config.output.format = progressbar_schema::OutputFormat::Ffv1Mkv;
    config.output.path = PathBuf::from("out/progress.mkv");
    let plan = ffmpeg_command_plan(&config, 60).unwrap();
    assert_eq!(plan.program, PathBuf::from("ffmpeg"));
    assert!(plan.args.contains(&"-f".to_string()));
    assert!(plan.args.contains(&"rawvideo".to_string()));
    assert!(plan.args.contains(&"rgba".to_string()));
    assert!(plan.args.contains(&"ffv1".to_string()));
    assert!(plan.args.contains(&"bgra".to_string()));
    assert_eq!(plan.args.last().unwrap(), "out/progress.mkv");
}

#[test]
fn builds_prores_command_with_explicit_ffmpeg_path() {
    let mut config = ProjectConfig::default();
    config.output.format = progressbar_schema::OutputFormat::Prores4444Mov;
    config.output.path = PathBuf::from("out/progress.mov");
    config.output.ffmpeg_path = Some(PathBuf::from("tools/ffmpeg.exe"));
    let plan = ffmpeg_command_plan(&config, 12).unwrap();
    assert_eq!(plan.program, PathBuf::from("tools/ffmpeg.exe"));
    assert!(plan.args.contains(&"prores_ks".to_string()));
    assert!(plan.args.contains(&"4444".to_string()));
    assert!(plan.args.contains(&"yuva444p10le".to_string()));
    assert_eq!(plan.args.last().unwrap(), "out/progress.mov");
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-encoder builds_ffv1_command_with_alpha_pixel_format
```

Expected: compile failure because `ffmpeg_command_plan` does not exist.

- [ ] **Step 3: Implement command plan**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegCommandPlan {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
}
```

Implement:

```rust
pub fn ffmpeg_command_plan(
    config: &ProjectConfig,
    total_frames: u64,
) -> Result<FfmpegCommandPlan, EncodeError> {
    let program = config
        .output
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("ffmpeg"));
    let mut args = vec![
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", config.render.width, config.render.height),
        "-r".to_string(),
        config.render.fps.to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-frames:v".to_string(),
        total_frames.to_string(),
    ];
    match config.output.format {
        progressbar_schema::OutputFormat::Ffv1Mkv => {
            args.extend(["-c:v", "ffv1", "-level", "3", "-pix_fmt", "bgra"].map(String::from));
        }
        progressbar_schema::OutputFormat::Prores4444Mov => {
            args.extend([
                "-c:v",
                "prores_ks",
                "-profile:v",
                "4444",
                "-pix_fmt",
                "yuva444p10le",
            ]
            .map(String::from));
        }
        _ => return Err(EncodeError::UnsupportedFfmpegFormat),
    }
    args.push(config.output.path.to_string_lossy().to_string());
    Ok(FfmpegCommandPlan { program, args })
}
```

Add `UnsupportedFfmpegFormat`.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-encoder builds_ffv1_command_with_alpha_pixel_format
cargo test -p progressbar-encoder builds_prores_command_with_explicit_ffmpeg_path
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/progressbar-encoder/src/lib.rs
git commit -m "feat: add ffmpeg output command profiles"
```

---

### Task 4: Encoder Dispatch and FFmpeg Rendering

**Files:**
- Modify: `crates/progressbar-encoder/src/lib.rs`
- Modify: `crates/progressbar-api/src/lib.rs`

- [ ] **Step 1: Write failing dispatcher tests**

Add encoder test:

```rust
#[test]
fn render_overlay_dispatches_apng_profile() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = ProjectConfig::default();
    config.render.width = 96;
    config.render.height = 54;
    config.render.fps = 2;
    config.bar.margin_x = 8;
    config.bar.margin_bottom = 6;
    config.bar.height = 16;
    config.output.format = progressbar_schema::OutputFormat::Apng;
    config.output.path = dir.path().join("progress.apng");
    let timeline = Timeline::parse("1 | A").unwrap();
    render_overlay(&config, &timeline, |_| {}).unwrap();
    let bytes = std::fs::read(&config.output.path).unwrap();
    assert!(png_chunk_names(&bytes).contains(&"acTL".to_string()));
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-encoder render_overlay_dispatches_apng_profile
```

Expected: compile failure because encoder-level `render_overlay` does not exist, or failure because APNG is not dispatched.

- [ ] **Step 3: Implement encoder dispatch**

Add:

```rust
pub fn render_overlay<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    match config.output.format {
        progressbar_schema::OutputFormat::PngSequence => {
            render_png_sequence(config, timeline, on_progress)
        }
        progressbar_schema::OutputFormat::Apng => render_apng(config, timeline, on_progress),
        progressbar_schema::OutputFormat::Ffv1Mkv
        | progressbar_schema::OutputFormat::Prores4444Mov => {
            render_ffmpeg(config, timeline, on_progress)
        }
    }
}
```

Change API `render_overlay` to call `progressbar_encoder::render_overlay`.

- [ ] **Step 4: Implement FFmpeg renderer**

Add `EncodeError` variants:

```rust
#[error("failed to start ffmpeg `{program}`: {source}")]
FfmpegSpawn { program: String, source: std::io::Error },
#[error("failed to write raw frame data to ffmpeg: {0}")]
FfmpegStdin(std::io::Error),
#[error("ffmpeg exited unsuccessfully with status {status}")]
FfmpegExit { status: String },
```

Implement `render_ffmpeg`:

```rust
pub fn render_ffmpeg<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    let total_frames = frame_count(timeline.duration_ms(), config.render.fps);
    if let Some(parent) = config.output.path.parent() {
        fs::create_dir_all(parent).map_err(EncodeError::CreateDir)?;
    }
    let plan = ffmpeg_command_plan(config, total_frames)?;
    let mut child = std::process::Command::new(&plan.program)
        .args(&plan.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|source| EncodeError::FfmpegSpawn {
            program: plan.program.to_string_lossy().to_string(),
            source,
        })?;
    {
        let stdin = child.stdin.as_mut().expect("stdin was configured as piped");
        for frame_index in 0..total_frames {
            let timestamp_ms = frame_timestamp_ms(frame_index, config.render.fps);
            let frame = render_frame(config, timeline, timestamp_ms)
                .map_err(|error| EncodeError::Render(error.to_string()))?;
            stdin.write_all(&frame.rgba).map_err(EncodeError::FfmpegStdin)?;
            on_progress(RenderProgress {
                completed_frames: frame_index + 1,
                total_frames,
            });
        }
    }
    let status = child.wait().map_err(EncodeError::FfmpegStdin)?;
    if status.success() {
        Ok(())
    } else {
        Err(EncodeError::FfmpegExit {
            status: status.to_string(),
        })
    }
}
```

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-encoder
cargo test -p progressbar-api
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/progressbar-encoder/src/lib.rs crates/progressbar-api/src/lib.rs
git commit -m "feat: dispatch overlay rendering by output profile"
```

---

### Task 5: CLI, Examples, and Verification

**Files:**
- Modify: `apps/cli/tests/cli.rs`
- Modify: `README.md`
- Create: `examples/encoder-profiles/apng.toml`
- Create: `examples/encoder-profiles/ffv1.toml`
- Create: `examples/encoder-profiles/prores4444.toml`
- Create: `examples/encoder-profiles/segments.txt`

- [ ] **Step 1: Write CLI APNG integration test**

Add:

```rust
#[test]
fn render_writes_apng_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("progress.apng");
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
path = "{}"
"#,
            output.display()
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
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-cli render_writes_apng_output
```

Expected: FAIL until dispatch and schema changes are wired through the CLI binary.

- [ ] **Step 3: Add examples**

Create `examples/encoder-profiles/segments.txt`:

```txt
00:00:01.000 | 开场
00:00:02.000 | 重点段落
```

Create three configs using the same render/bar/text sections and different outputs:

```toml
[render]
width = 640
height = 360
fps = 10
background = "transparent"

[bar]
position = "bottom"
height = 56
margin_x = 48
margin_bottom = 28
corner_radius = 6
track_color = "#FFFFFF33"
fill_color = "#4DA3FF"
divider_color = "#FFFFFFAA"

[playback_progress]
enabled = true
height = 5
offset_y = 0
color = "#FFFFFF"
track_color = "#00000033"
thumb_enabled = true
thumb_radius = 6
thumb_color = "#FFFFFF"

[text]
font_family = "Microsoft YaHei"
font_size = 24
min_font_size = 14
color = "#FFFFFF"
display_mode = "all-segments"
overflow = "auto"
```

For `apng.toml`:

```toml
[output]
format = "apng"
path = "examples/encoder-profiles/out/progress.apng"
```

For `ffv1.toml`:

```toml
[output]
format = "ffv1-mkv"
path = "examples/encoder-profiles/out/progress.mkv"
```

For `prores4444.toml`:

```toml
[output]
format = "prores4444-mov"
path = "examples/encoder-profiles/out/progress.mov"
```

- [ ] **Step 4: Update README**

Document:

```markdown
Output profiles:

- `png-sequence`: directory of transparent PNG frames.
- `apng`: single transparent animated PNG, best for short overlays.
- `ffv1-mkv`: FFmpeg-backed mathematically lossless alpha video.
- `prores4444-mov`: FFmpeg-backed editing intermediate with alpha.

APNG:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/apng.toml --segments examples/encoder-profiles/segments.txt
```

FFV1:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/ffv1.toml --segments examples/encoder-profiles/segments.txt
```
```

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/apng.toml --segments examples/encoder-profiles/segments.txt
```

If `ffmpeg` is available, also run:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/ffv1.toml --segments examples/encoder-profiles/segments.txt
```

Expected:

- Formatting passes.
- All tests pass.
- APNG file is written.
- FFmpeg command construction tests pass even if runtime FFmpeg is not installed.

- [ ] **Step 6: Commit**

```powershell
git add README.md apps/cli/tests/cli.rs examples/encoder-profiles
git commit -m "docs: add encoder profile examples"
```

---

## Self-Review Notes

Spec coverage:

- `apng`: native writer preserves RGBA alpha, writes APNG timing chunks, reports progress.
- `ffv1-mkv`: FFmpeg command uses raw RGBA input and `ffv1` with alpha-capable `bgra`.
- `prores4444-mov`: FFmpeg command uses `prores_ks`, 4444 profile, and alpha-capable `yuva444p10le`.
- Structured errors: encoder returns distinct output, APNG, spawn, stdin, and exit errors.
- GUI compatibility: all new rendering remains behind `progressbar_api::render_overlay`, so Tauri can call Rust directly later.
