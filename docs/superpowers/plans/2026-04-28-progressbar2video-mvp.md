# ProgressBar2Video MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first runnable ProgressBar2Video slice: config parsing, segment parsing, duration-proportional layout, transparent preview PNG rendering, PNG sequence export, and a thin CLI over the shared Rust API.

**Architecture:** Create a Rust workspace with focused crates for schema, core timeline/layout logic, renderer, encoder, API orchestration, and CLI. The renderer writes transparent RGBA frames and draws the segmented bar, optional playback-progress overlay, and clipped labels. The CLI only calls `progressbar-api`; it must not own config, timeline, rendering, or encoding behavior.

**Tech Stack:** Rust 2021, Cargo workspace, `serde`, `toml`, `thiserror`, `schemars`, `tiny-skia`, `cosmic-text`, `png`, `clap`, `tempfile`, `assert_cmd`, `predicates`.

---

## Scope

This plan implements the MVP core path from the approved spec:

- Rust workspace and package boundaries.
- TOML config defaults and validation.
- Segment text parsing.
- Duration-derived overlay length and frame count.
- Duration-proportional segment layout.
- Long-text strategy selection rules, including `auto` requiring `min_font_size` before scroll fallback.
- Transparent preview frame rendering.
- Optional playback-progress overlay layer above the segmented bar and below text.
- PNG sequence rendering without storing all frames in memory.
- Thin CLI commands for validation, preview, and render.

Tauri desktop GUI, APNG, FFmpeg profiles, and bundled FFmpeg distribution get their own implementation plans after this MVP is stable.

## File Structure

- Create `Cargo.toml`: workspace member list and shared dependency versions.
- Create `.gitignore`: Rust, Node, render-output, and editor ignores.
- Create `README.md`: MVP usage, segment format, and example commands.
- Create `crates/progressbar-schema/`: config structs, defaults, color parsing, validation, JSON Schema export.
- Create `crates/progressbar-core/`: time parsing, segment parsing, timeline model, layout, text strategy selection.
- Create `crates/progressbar-renderer/`: RGBA frame rendering, PNG preview writing, text rendering spike, alpha tests.
- Create `crates/progressbar-encoder/`: PNG sequence export and progress events.
- Create `crates/progressbar-api/`: file-based orchestration used by CLI and future GUI.
- Create `apps/cli/`: `progressbar2video` binary with `validate`, `preview-frame`, and `render` commands.
- Create `examples/basic/`: sample `segments.txt` and `config.toml`.

---

### Task 1: Workspace Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `README.md`
- Create: `crates/progressbar-schema/Cargo.toml`
- Create: `crates/progressbar-schema/src/lib.rs`
- Create: `crates/progressbar-core/Cargo.toml`
- Create: `crates/progressbar-core/src/lib.rs`
- Create: `crates/progressbar-renderer/Cargo.toml`
- Create: `crates/progressbar-renderer/src/lib.rs`
- Create: `crates/progressbar-encoder/Cargo.toml`
- Create: `crates/progressbar-encoder/src/lib.rs`
- Create: `crates/progressbar-api/Cargo.toml`
- Create: `crates/progressbar-api/src/lib.rs`
- Create: `apps/cli/Cargo.toml`
- Create: `apps/cli/src/main.rs`

- [ ] **Step 1: Create workspace manifests and minimal crate entry points**

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
  "crates/progressbar-schema",
  "crates/progressbar-core",
  "crates/progressbar-renderer",
  "crates/progressbar-encoder",
  "crates/progressbar-api",
  "apps/cli"
]

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"

[workspace.dependencies]
anyhow = "1.0"
assert_cmd = "2.0"
clap = { version = "4.5", features = ["derive"] }
cosmic-text = "0.19"
png = "0.17"
predicates = "3.1"
schemars = "0.8"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tempfile = "3.10"
thiserror = "1.0"
tiny-skia = "0.11"
toml = "0.8"
```

`.gitignore`:

```gitignore
/target/
/.idea/
/.vscode/
*.pdb
*.log
/out/
/examples/**/out/
```

`README.md`:

```markdown
# ProgressBar2Video

ProgressBar2Video generates transparent progress-bar overlay assets for video editing.

The MVP reads a TOML config and a segment text file, then writes a transparent preview PNG or a PNG sequence whose duration matches the final segment end time.
```

Each crate `Cargo.toml` should use workspace package metadata and these dependencies:

```toml
[package]
name = "progressbar-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
progressbar-schema = { path = "../progressbar-schema" }
thiserror.workspace = true
```

Use the same package block for the other crates, changing only `name`. Dependency blocks:

`crates/progressbar-schema/Cargo.toml`:

```toml
[dependencies]
schemars.workspace = true
serde.workspace = true
thiserror.workspace = true
toml.workspace = true
```

`crates/progressbar-renderer/Cargo.toml`:

```toml
[dependencies]
cosmic-text.workspace = true
png.workspace = true
progressbar-core = { path = "../progressbar-core" }
progressbar-schema = { path = "../progressbar-schema" }
thiserror.workspace = true
tiny-skia.workspace = true
```

`crates/progressbar-encoder/Cargo.toml`:

```toml
[dependencies]
progressbar-core = { path = "../progressbar-core" }
progressbar-renderer = { path = "../progressbar-renderer" }
progressbar-schema = { path = "../progressbar-schema" }
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`crates/progressbar-api/Cargo.toml`:

```toml
[dependencies]
progressbar-core = { path = "../progressbar-core" }
progressbar-encoder = { path = "../progressbar-encoder" }
progressbar-renderer = { path = "../progressbar-renderer" }
progressbar-schema = { path = "../progressbar-schema" }
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`apps/cli/Cargo.toml`:

```toml
[package]
name = "progressbar-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "progressbar2video"
path = "src/main.rs"

[dependencies]
clap.workspace = true
progressbar-api = { path = "../../crates/progressbar-api" }

[dev-dependencies]
assert_cmd.workspace = true
tempfile.workspace = true
```

Each `src/lib.rs` starts with a single public marker so the workspace compiles:

```rust
pub fn crate_ready() -> bool {
    true
}
```

`apps/cli/src/main.rs`:

```rust
fn main() {
    println!("progressbar2video");
}
```

- [ ] **Step 2: Run workspace check**

Run: `cargo check --workspace`

Expected: PASS with all six packages checked.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml .gitignore README.md crates apps
git commit -m "chore: scaffold Rust workspace"
```

---

### Task 2: Config Schema, Defaults, and Validation

**Files:**
- Modify: `crates/progressbar-schema/Cargo.toml`
- Replace: `crates/progressbar-schema/src/lib.rs`

- [ ] **Step 1: Write failing config tests**

Add tests in `crates/progressbar-schema/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_transparent_overlay_with_auto_text() {
        let config = ProjectConfig::default();
        assert_eq!(config.render.width, 1920);
        assert_eq!(config.render.height, 1080);
        assert_eq!(config.render.fps, 60);
        assert_eq!(config.render.background, Background::Transparent);
        assert_eq!(config.text.overflow, OverflowMode::Auto);
        assert_eq!(config.text.min_font_size, 18);
        assert!(config.playback_progress.enabled);
    }

    #[test]
    fn validates_auto_overflow_requires_min_font_size() {
        let mut config = ProjectConfig::default();
        config.text.overflow = OverflowMode::Auto;
        config.text.min_font_size = 0;
        let error = config.validate().unwrap_err();
        assert!(matches!(error, ConfigError::InvalidTextSize { .. }));
    }

    #[test]
    fn parses_example_toml() {
        let toml = r##"
[render]
width = 1280
height = 720
fps = 30
background = "transparent"

[bar]
position = "bottom"
height = 64
margin_x = 60
margin_bottom = 24
corner_radius = 6
track_color = "#FFFFFF33"
fill_color = "#4DA3FF"
divider_color = "#FFFFFFAA"

[playback_progress]
enabled = true
height = 6
offset_y = 0
color = "#FFFFFF"
track_color = "#00000033"
thumb_enabled = true
thumb_radius = 7
thumb_color = "#FFFFFF"

[text]
font_family = "Microsoft YaHei"
font_size = 28
min_font_size = 18
color = "#FFFFFF"
display_mode = "all-segments"
overflow = "auto"

[output]
format = "png-sequence"
path = "out/progress"
"##;
        let config = ProjectConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.render.width, 1280);
        assert_eq!(config.text.display_mode, TextDisplayMode::AllSegments);
        assert_eq!(config.output.format, OutputFormat::PngSequence);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-schema`

Expected: FAIL with unresolved types such as `ProjectConfig` and `OverflowMode`.

- [ ] **Step 3: Implement config structs, defaults, parser, and validation**

Replace `crates/progressbar-schema/src/lib.rs` with:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Background {
    Transparent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BarPosition {
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TextDisplayMode {
    AllSegments,
    ActiveOnly,
    PastCurrent,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowMode {
    Shrink,
    Ellipsis,
    Rotate,
    Scroll,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    PngSequence,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub background: Background,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            background: Background::Transparent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct BarConfig {
    pub position: BarPosition,
    pub height: u32,
    pub margin_x: u32,
    pub margin_bottom: u32,
    pub corner_radius: u32,
    pub track_color: String,
    pub fill_color: String,
    pub divider_color: String,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            position: BarPosition::Bottom,
            height: 72,
            margin_x: 80,
            margin_bottom: 36,
            corner_radius: 8,
            track_color: "#FFFFFF33".to_string(),
            fill_color: "#4DA3FF".to_string(),
            divider_color: "#FFFFFFAA".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct PlaybackProgressConfig {
    pub enabled: bool,
    pub height: u32,
    pub offset_y: i32,
    pub color: String,
    pub track_color: String,
    pub thumb_enabled: bool,
    pub thumb_radius: u32,
    pub thumb_color: String,
}

impl Default for PlaybackProgressConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            height: 6,
            offset_y: 0,
            color: "#FFFFFF".to_string(),
            track_color: "#00000033".to_string(),
            thumb_enabled: true,
            thumb_radius: 7,
            thumb_color: "#FFFFFF".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct TextConfig {
    pub font_family: String,
    pub font_size: u32,
    pub min_font_size: u32,
    pub color: String,
    pub display_mode: TextDisplayMode,
    pub overflow: OverflowMode,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            font_family: "Microsoft YaHei".to_string(),
            font_size: 28,
            min_font_size: 18,
            color: "#FFFFFF".to_string(),
            display_mode: TextDisplayMode::AllSegments,
            overflow: OverflowMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(default)]
pub struct OutputConfig {
    pub format: OutputFormat,
    pub path: PathBuf,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::PngSequence,
            path: PathBuf::from("out/progress"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(default)]
pub struct ProjectConfig {
    pub render: RenderConfig,
    pub bar: BarConfig,
    pub playback_progress: PlaybackProgressConfig,
    pub text: TextConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("failed to parse TOML config: {0}")]
    Parse(String),
    #[error("render dimensions and fps must be greater than zero")]
    InvalidRenderDimensions,
    #[error("bar dimensions or margins do not fit inside the render frame")]
    InvalidBarGeometry,
    #[error("text font_size and min_font_size must be greater than zero, and min_font_size must not exceed font_size")]
    InvalidTextSize { font_size: u32, min_font_size: u32 },
    #[error("color value `{value}` is not a supported #RRGGBB or #RRGGBBAA color")]
    InvalidColor { value: String },
}

impl ProjectConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(input).map_err(|error| ConfigError::Parse(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.render.width == 0 || self.render.height == 0 || self.render.fps == 0 {
            return Err(ConfigError::InvalidRenderDimensions);
        }
        if self.bar.height == 0
            || self.bar.margin_x.saturating_mul(2) >= self.render.width
            || self.bar.height + self.bar.margin_bottom > self.render.height
        {
            return Err(ConfigError::InvalidBarGeometry);
        }
        if self.text.font_size == 0
            || self.text.min_font_size == 0
            || self.text.min_font_size > self.text.font_size
        {
            return Err(ConfigError::InvalidTextSize {
                font_size: self.text.font_size,
                min_font_size: self.text.min_font_size,
            });
        }
        validate_color(&self.bar.track_color)?;
        validate_color(&self.bar.fill_color)?;
        validate_color(&self.bar.divider_color)?;
        validate_color(&self.playback_progress.color)?;
        validate_color(&self.playback_progress.track_color)?;
        validate_color(&self.playback_progress.thumb_color)?;
        validate_color(&self.text.color)?;
        Ok(())
    }
}

pub fn validate_color(value: &str) -> Result<(), ConfigError> {
    let hex = value.strip_prefix('#').ok_or_else(|| ConfigError::InvalidColor {
        value: value.to_string(),
    })?;
    let valid_len = hex.len() == 6 || hex.len() == 8;
    let valid_digits = hex.chars().all(|ch| ch.is_ascii_hexdigit());
    if valid_len && valid_digits {
        Ok(())
    } else {
        Err(ConfigError::InvalidColor {
            value: value.to_string(),
        })
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-schema`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-schema
git commit -m "feat: add project config schema"
```

---

### Task 3: Segment Parsing and Timeline Duration

**Files:**
- Modify: `crates/progressbar-core/Cargo.toml`
- Replace: `crates/progressbar-core/src/lib.rs`

- [ ] **Step 1: Write failing segment parser tests**

Add tests in `crates/progressbar-core/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_segments_from_end_points() {
        let text = r#"
# intro comment
00:00:12.500 | 开场
00:01:05.000 | 背景介绍
80.250 | 核心演示
"#;
        let timeline = Timeline::parse(text).unwrap();
        assert_eq!(timeline.duration_ms(), 80_250);
        assert_eq!(timeline.segments.len(), 3);
        assert_eq!(timeline.segments[0].start_ms, 0);
        assert_eq!(timeline.segments[0].end_ms, 12_500);
        assert_eq!(timeline.segments[1].start_ms, 12_500);
        assert_eq!(timeline.segments[1].end_ms, 65_000);
        assert_eq!(timeline.segments[2].label, "核心演示");
    }

    #[test]
    fn rejects_non_increasing_end_times() {
        let error = Timeline::parse("10 | A\n9 | B").unwrap_err();
        assert!(matches!(error, SegmentParseError::NonIncreasingTime { line: 2, .. }));
    }

    #[test]
    fn parses_supported_time_formats() {
        assert_eq!(parse_time_ms("12.500").unwrap(), 12_500);
        assert_eq!(parse_time_ms("01:05.250").unwrap(), 65_250);
        assert_eq!(parse_time_ms("01:02:03.004").unwrap(), 3_723_004);
    }

    #[test]
    fn computes_active_segment_at_timestamp() {
        let timeline = Timeline::parse("10 | A\n20 | B").unwrap();
        assert_eq!(timeline.active_segment_index(0), Some(0));
        assert_eq!(timeline.active_segment_index(10_000), Some(1));
        assert_eq!(timeline.active_segment_index(20_000), Some(1));
        assert_eq!(timeline.active_segment_index(20_001), None);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-core`

Expected: FAIL with unresolved types such as `Timeline` and `SegmentParseError`.

- [ ] **Step 3: Implement segment parsing**

Replace `crates/progressbar-core/src/lib.rs` with:

```rust
use thiserror::Error;

pub type TimeMs = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: TimeMs,
    pub end_ms: TimeMs,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SegmentParseError {
    #[error("line {line}: missing separator `|`")]
    MissingSeparator { line: usize },
    #[error("line {line}: missing segment label")]
    MissingLabel { line: usize },
    #[error("line {line}: invalid time `{value}`")]
    InvalidTime { line: usize, value: String },
    #[error("line {line}: end time {end_ms}ms must be greater than previous end time {previous_end_ms}ms")]
    NonIncreasingTime {
        line: usize,
        previous_end_ms: TimeMs,
        end_ms: TimeMs,
    },
    #[error("segment file contains no segments")]
    Empty,
}

impl Timeline {
    pub fn parse(input: &str) -> Result<Self, SegmentParseError> {
        let mut segments = Vec::new();
        let mut previous_end_ms = 0;

        for (index, raw_line) in input.lines().enumerate() {
            let line_number = index + 1;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (time_text, label_text) = trimmed
                .split_once('|')
                .ok_or(SegmentParseError::MissingSeparator { line: line_number })?;
            let label = label_text.trim();
            if label.is_empty() {
                return Err(SegmentParseError::MissingLabel { line: line_number });
            }

            let end_ms = parse_time_ms(time_text.trim()).map_err(|_| SegmentParseError::InvalidTime {
                line: line_number,
                value: time_text.trim().to_string(),
            })?;
            if end_ms <= previous_end_ms {
                return Err(SegmentParseError::NonIncreasingTime {
                    line: line_number,
                    previous_end_ms,
                    end_ms,
                });
            }

            segments.push(Segment {
                start_ms: previous_end_ms,
                end_ms,
                label: label.to_string(),
            });
            previous_end_ms = end_ms;
        }

        if segments.is_empty() {
            return Err(SegmentParseError::Empty);
        }

        Ok(Self { segments })
    }

    pub fn duration_ms(&self) -> TimeMs {
        self.segments.last().map(|segment| segment.end_ms).unwrap_or(0)
    }

    pub fn active_segment_index(&self, timestamp_ms: TimeMs) -> Option<usize> {
        if self.segments.is_empty() {
            return None;
        }
        self.segments
            .iter()
            .position(|segment| timestamp_ms >= segment.start_ms && timestamp_ms < segment.end_ms)
            .or_else(|| {
                let last_index = self.segments.len() - 1;
                (timestamp_ms == self.segments[last_index].end_ms).then_some(last_index)
            })
    }
}

pub fn parse_time_ms(value: &str) -> Result<TimeMs, ()> {
    let parts: Vec<&str> = value.split(':').collect();
    match parts.as_slice() {
        [seconds] => parse_seconds_ms(seconds),
        [minutes, seconds] => {
            let minutes = minutes.parse::<u64>().map_err(|_| ())?;
            Ok(minutes * 60_000 + parse_seconds_ms(seconds)?)
        }
        [hours, minutes, seconds] => {
            let hours = hours.parse::<u64>().map_err(|_| ())?;
            let minutes = minutes.parse::<u64>().map_err(|_| ())?;
            Ok(hours * 3_600_000 + minutes * 60_000 + parse_seconds_ms(seconds)?)
        }
        _ => Err(()),
    }
}

fn parse_seconds_ms(value: &str) -> Result<TimeMs, ()> {
    let (seconds_text, millis_text) = value.split_once('.').unwrap_or((value, "0"));
    let seconds = seconds_text.parse::<u64>().map_err(|_| ())?;
    let millis = match millis_text.len() {
        0 => 0,
        1 => millis_text.parse::<u64>().map_err(|_| ())? * 100,
        2 => millis_text.parse::<u64>().map_err(|_| ())? * 10,
        _ => millis_text[..3].parse::<u64>().map_err(|_| ())?,
    };
    Ok(seconds * 1_000 + millis)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-core`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-core
git commit -m "feat: parse segment timelines"
```

---

### Task 4: Duration-Proportional Layout and Frame Count

**Files:**
- Modify: `crates/progressbar-core/src/lib.rs`

- [ ] **Step 1: Write failing layout tests**

Add tests:

```rust
#[test]
fn calculates_duration_proportional_layout() {
    let timeline = Timeline::parse("10 | A\n30 | B").unwrap();
    let config = progressbar_schema::ProjectConfig::default();
    let layout = Layout::calculate(&config, &timeline).unwrap();
    assert_eq!(layout.bar.x, 80.0);
    assert_eq!(layout.bar.width, 1760.0);
    assert_eq!(layout.segments.len(), 2);
    assert!((layout.segments[0].rect.width - 586.6667).abs() < 0.01);
    assert!((layout.segments[1].rect.width - 1173.3333).abs() < 0.01);
}

#[test]
fn derives_frame_count_from_final_end_time() {
    let timeline = Timeline::parse("2 | A").unwrap();
    assert_eq!(frame_count(timeline.duration_ms(), 60), 120);
    assert_eq!(frame_count(2_001, 60), 121);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-core`

Expected: FAIL with unresolved `Layout` and `frame_count`.

- [ ] **Step 3: Implement layout types and frame count**

Append to `crates/progressbar-core/src/lib.rs`:

```rust
use progressbar_schema::ProjectConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentLayout {
    pub segment_index: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub bar: Rect,
    pub segments: Vec<SegmentLayout>,
}

#[derive(Debug, Error, PartialEq)]
pub enum LayoutError {
    #[error("timeline duration must be greater than zero")]
    EmptyDuration,
}

impl Layout {
    pub fn calculate(config: &ProjectConfig, timeline: &Timeline) -> Result<Self, LayoutError> {
        let duration = timeline.duration_ms();
        if duration == 0 {
            return Err(LayoutError::EmptyDuration);
        }

        let x = config.bar.margin_x as f32;
        let width = (config.render.width - config.bar.margin_x * 2) as f32;
        let height = config.bar.height as f32;
        let y = (config.render.height - config.bar.margin_bottom - config.bar.height) as f32;
        let bar = Rect { x, y, width, height };

        let segments = timeline
            .segments
            .iter()
            .enumerate()
            .map(|(segment_index, segment)| {
                let start_ratio = segment.start_ms as f32 / duration as f32;
                let end_ratio = segment.end_ms as f32 / duration as f32;
                let segment_x = x + width * start_ratio;
                let segment_width = width * (end_ratio - start_ratio);
                SegmentLayout {
                    segment_index,
                    rect: Rect {
                        x: segment_x,
                        y,
                        width: segment_width,
                        height,
                    },
                }
            })
            .collect();

        Ok(Self { bar, segments })
    }
}

pub fn frame_count(duration_ms: TimeMs, fps: u32) -> u64 {
    if duration_ms == 0 || fps == 0 {
        return 0;
    }
    ((duration_ms as u128 * fps as u128 + 999) / 1000) as u64
}

pub fn frame_timestamp_ms(frame_index: u64, fps: u32) -> TimeMs {
    if fps == 0 {
        return 0;
    }
    ((frame_index as u128 * 1000) / fps as u128) as TimeMs
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-core`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-core
git commit -m "feat: calculate duration-based layout"
```

---

### Task 5: Text Overflow Strategy Selection

**Files:**
- Modify: `crates/progressbar-core/src/lib.rs`

- [ ] **Step 1: Write failing text strategy tests**

Add tests:

```rust
#[test]
fn auto_uses_scroll_only_after_min_font_size() {
    let decision = choose_text_strategy(TextStrategyInput {
        overflow: progressbar_schema::OverflowMode::Auto,
        text_width_px: 500.0,
        rect_width_px: 100.0,
        font_size: 28,
        min_font_size: 18,
        can_rotate: false,
    });
    assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 18 });
}

#[test]
fn auto_prefers_rotation_for_narrow_cells_when_allowed() {
    let decision = choose_text_strategy(TextStrategyInput {
        overflow: progressbar_schema::OverflowMode::Auto,
        text_width_px: 300.0,
        rect_width_px: 80.0,
        font_size: 28,
        min_font_size: 18,
        can_rotate: true,
    });
    assert_eq!(decision, TextStrategyDecision::Rotate { font_size: 18 });
}

#[test]
fn explicit_scroll_uses_configured_font_size() {
    let decision = choose_text_strategy(TextStrategyInput {
        overflow: progressbar_schema::OverflowMode::Scroll,
        text_width_px: 300.0,
        rect_width_px: 80.0,
        font_size: 28,
        min_font_size: 18,
        can_rotate: true,
    });
    assert_eq!(decision, TextStrategyDecision::Scroll { font_size: 28 });
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-core`

Expected: FAIL with unresolved text strategy types.

- [ ] **Step 3: Implement strategy selection**

Append:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStrategyInput {
    pub overflow: progressbar_schema::OverflowMode,
    pub text_width_px: f32,
    pub rect_width_px: f32,
    pub font_size: u32,
    pub min_font_size: u32,
    pub can_rotate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStrategyDecision {
    Normal { font_size: u32 },
    Shrink { font_size: u32 },
    Ellipsis { font_size: u32 },
    Rotate { font_size: u32 },
    Scroll { font_size: u32 },
}

pub fn choose_text_strategy(input: TextStrategyInput) -> TextStrategyDecision {
    use progressbar_schema::OverflowMode;

    if input.text_width_px <= input.rect_width_px {
        return TextStrategyDecision::Normal {
            font_size: input.font_size,
        };
    }

    match input.overflow {
        OverflowMode::Shrink => TextStrategyDecision::Shrink {
            font_size: input.min_font_size,
        },
        OverflowMode::Ellipsis => TextStrategyDecision::Ellipsis {
            font_size: input.font_size,
        },
        OverflowMode::Rotate => TextStrategyDecision::Rotate {
            font_size: input.font_size,
        },
        OverflowMode::Scroll => TextStrategyDecision::Scroll {
            font_size: input.font_size,
        },
        OverflowMode::Auto => {
            let shrink_ratio = input.rect_width_px / input.text_width_px;
            let shrunk_size = ((input.font_size as f32 * shrink_ratio).floor() as u32)
                .clamp(input.min_font_size, input.font_size);
            if shrunk_size > input.min_font_size {
                return TextStrategyDecision::Shrink {
                    font_size: shrunk_size,
                };
            }
            if input.text_width_px <= input.rect_width_px * 1.8 {
                return TextStrategyDecision::Ellipsis {
                    font_size: input.min_font_size,
                };
            }
            if input.can_rotate {
                return TextStrategyDecision::Rotate {
                    font_size: input.min_font_size,
                };
            }
            TextStrategyDecision::Scroll {
                font_size: input.min_font_size,
            }
        }
    }
}

pub fn scroll_offset_px(
    timestamp_ms: TimeMs,
    segment: &Segment,
    text_width_px: f32,
    rect_width_px: f32,
) -> f32 {
    if text_width_px <= rect_width_px || segment.end_ms <= segment.start_ms {
        return 0.0;
    }
    let elapsed = timestamp_ms.saturating_sub(segment.start_ms).min(segment.end_ms - segment.start_ms);
    let ratio = elapsed as f32 / (segment.end_ms - segment.start_ms) as f32;
    -(text_width_px - rect_width_px) * ratio
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-core`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-core
git commit -m "feat: select text overflow strategies"
```

---

### Task 6: Renderer Transparent Bar and Playback Overlay

**Files:**
- Modify: `crates/progressbar-renderer/Cargo.toml`
- Replace: `crates/progressbar-renderer/src/lib.rs`

- [ ] **Step 1: Write failing renderer tests**

Add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use progressbar_core::Timeline;
    use progressbar_schema::ProjectConfig;

    #[test]
    fn renders_transparent_background_pixels() {
        let config = ProjectConfig::default();
        let timeline = Timeline::parse("2 | 开场").unwrap();
        let frame = render_frame(&config, &timeline, 0).unwrap();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.pixel_rgba(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn playback_overlay_draws_above_bar_when_enabled() {
        let mut config = ProjectConfig::default();
        config.render.width = 320;
        config.render.height = 180;
        config.bar.margin_x = 20;
        config.bar.margin_bottom = 20;
        config.bar.height = 30;
        config.playback_progress.enabled = true;
        config.playback_progress.height = 6;
        config.playback_progress.thumb_enabled = false;
        let timeline = Timeline::parse("2 | A").unwrap();
        let frame = render_frame(&config, &timeline, 1_000).unwrap();
        let bar_y = 180 - 20 - 30;
        let sample = frame.pixel_rgba(160, bar_y + 15);
        assert!(sample[3] > 0);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-renderer`

Expected: FAIL with unresolved `render_frame`.

- [ ] **Step 3: Implement RGBA frame and primitive rendering**

Replace `crates/progressbar-renderer/src/lib.rs` with:

```rust
use progressbar_core::{Layout, TimeMs, Timeline};
use progressbar_schema::ProjectConfig;
use thiserror::Error;
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("failed to allocate RGBA frame")]
    Allocation,
    #[error("layout failed: {0}")]
    Layout(String),
    #[error("invalid color `{0}`")]
    Color(String),
}

pub struct RenderedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RenderedFrame {
    pub fn pixel_rgba(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ]
    }
}

pub fn render_frame(
    config: &ProjectConfig,
    timeline: &Timeline,
    timestamp_ms: TimeMs,
) -> Result<RenderedFrame, RenderError> {
    let layout = Layout::calculate(config, timeline).map_err(|error| RenderError::Layout(error.to_string()))?;
    let mut pixmap = Pixmap::new(config.render.width, config.render.height).ok_or(RenderError::Allocation)?;

    fill_rect(&mut pixmap, layout.bar, &config.bar.track_color)?;
    for segment in &layout.segments {
        fill_rect(&mut pixmap, segment.rect, &config.bar.fill_color)?;
        let divider = progressbar_core::Rect {
            x: segment.rect.x,
            y: segment.rect.y,
            width: 1.0,
            height: segment.rect.height,
        };
        fill_rect(&mut pixmap, divider, &config.bar.divider_color)?;
    }

    if config.playback_progress.enabled {
        draw_playback_progress(&mut pixmap, config, &layout, timeline.duration_ms(), timestamp_ms)?;
    }

    Ok(RenderedFrame {
        width: config.render.width,
        height: config.render.height,
        rgba: pixmap.take(),
    })
}

fn draw_playback_progress(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    layout: &Layout,
    duration_ms: TimeMs,
    timestamp_ms: TimeMs,
) -> Result<(), RenderError> {
    if duration_ms == 0 {
        return Ok(());
    }
    let ratio = (timestamp_ms.min(duration_ms) as f32 / duration_ms as f32).clamp(0.0, 1.0);
    let y = layout.bar.y + layout.bar.height / 2.0
        - config.playback_progress.height as f32 / 2.0
        + config.playback_progress.offset_y as f32;
    let track = progressbar_core::Rect {
        x: layout.bar.x,
        y,
        width: layout.bar.width,
        height: config.playback_progress.height as f32,
    };
    fill_rect(pixmap, track, &config.playback_progress.track_color)?;
    let elapsed = progressbar_core::Rect {
        x: layout.bar.x,
        y,
        width: layout.bar.width * ratio,
        height: config.playback_progress.height as f32,
    };
    fill_rect(pixmap, elapsed, &config.playback_progress.color)?;
    if config.playback_progress.thumb_enabled {
        draw_circle(
            pixmap,
            layout.bar.x + layout.bar.width * ratio,
            y + config.playback_progress.height as f32 / 2.0,
            config.playback_progress.thumb_radius as f32,
            &config.playback_progress.thumb_color,
        )?;
    }
    Ok(())
}

fn fill_rect(pixmap: &mut Pixmap, rect: progressbar_core::Rect, color: &str) -> Result<(), RenderError> {
    let Some(rect) = Rect::from_xywh(rect.x, rect.y, rect.width.max(0.0), rect.height.max(0.0)) else {
        return Ok(());
    };
    let mut paint = Paint::default();
    paint.set_color(parse_color(color)?);
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    Ok(())
}

fn draw_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, color: &str) -> Result<(), RenderError> {
    let mut path = tiny_skia::PathBuilder::new();
    path.push_circle(cx, cy, radius);
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color(parse_color(color)?);
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
    Ok(())
}

fn parse_color(value: &str) -> Result<Color, RenderError> {
    let hex = value.strip_prefix('#').ok_or_else(|| RenderError::Color(value.to_string()))?;
    let parse = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| RenderError::Color(value.to_string()))
    };
    let r = parse(0..2)?;
    let g = parse(2..4)?;
    let b = parse(4..6)?;
    let a = if hex.len() == 8 { parse(6..8)? } else { 255 };
    Ok(Color::from_rgba8(r, g, b, a))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-renderer`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-renderer
git commit -m "feat: render transparent progress frames"
```

---

### Task 7: Renderer Text Spike and Clipped Labels

**Files:**
- Modify: `crates/progressbar-renderer/src/lib.rs`

- [ ] **Step 1: Write failing text rendering test**

Add test:

```rust
#[test]
fn renders_label_pixels_inside_segment_area() {
    let mut config = ProjectConfig::default();
    config.render.width = 640;
    config.render.height = 360;
    config.bar.margin_x = 40;
    config.bar.margin_bottom = 30;
    config.bar.height = 60;
    config.text.font_size = 24;
    config.text.min_font_size = 16;
    let timeline = Timeline::parse("2 | 开场").unwrap();
    let frame = render_frame(&config, &timeline, 500).unwrap();
    let bar_y = 360 - 30 - 60;
        let has_text_alpha = (bar_y..bar_y + 60).any(|y| {
        (40..600).any(|x| frame.pixel_rgba(x, y)[3] > 0 && frame.pixel_rgba(x, y) != [77, 163, 255, 255])
    });
    assert!(has_text_alpha);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-renderer renders_label_pixels_inside_segment_area`

Expected: FAIL because no label text is drawn.

- [ ] **Step 3: Add label drawing through `cosmic-text`**

Add imports:

```rust
use cosmic_text::{Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache};
```

Add functions to `crates/progressbar-renderer/src/lib.rs` and call `draw_labels` after playback overlay:

```rust
fn draw_labels(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    timeline: &Timeline,
    layout: &Layout,
    timestamp_ms: TimeMs,
) -> Result<(), RenderError> {
    let active = timeline.active_segment_index(timestamp_ms);
    for segment_layout in &layout.segments {
        let segment = &timeline.segments[segment_layout.segment_index];
        let should_draw = match config.text.display_mode {
            progressbar_schema::TextDisplayMode::AllSegments => true,
            progressbar_schema::TextDisplayMode::ActiveOnly => Some(segment_layout.segment_index) == active,
            progressbar_schema::TextDisplayMode::PastCurrent => active
                .map(|active_index| segment_layout.segment_index <= active_index)
                .unwrap_or(false),
            progressbar_schema::TextDisplayMode::None => false,
        };
        if should_draw {
            draw_label_text(pixmap, config, segment_layout.rect, &segment.label)?;
        }
    }
    Ok(())
}

fn draw_label_text(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    label: &str,
) -> Result<(), RenderError> {
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();
    let font_size = config.text.font_size as f32;
    let metrics = Metrics::new(font_size, (font_size * 1.2).max(font_size + 2.0));
    let mut buffer = Buffer::new(&mut font_system, metrics);
    let attrs = Attrs::new().family(Family::Name(&config.text.font_family));
    {
        let mut borrowed = buffer.borrow_with(&mut font_system);
        borrowed.set_size(Some(rect.width.max(1.0)), Some(rect.height.max(1.0)));
        borrowed.set_text(label, &attrs, Shaping::Advanced, None);
    }

    let text_rgba = parse_color_components(&config.text.color)?;
    let text_color = TextColor::rgba(text_rgba[0], text_rgba[1], text_rgba[2], text_rgba[3]);
    let clip_left = rect.x.max(0.0) as i32;
    let clip_top = rect.y.max(0.0) as i32;
    let clip_right = (rect.x + rect.width).min(pixmap.width() as f32) as i32;
    let clip_bottom = (rect.y + rect.height).min(pixmap.height() as f32) as i32;
    let baseline_y = rect.y + (rect.height - font_size) / 2.0;

    let mut borrowed = buffer.borrow_with(&mut font_system);
    borrowed.draw(&mut swash_cache, text_color, |x, y, width, height, color| {
        let [r, g, b, a] = color.as_rgba();
        for dy in 0..height as i32 {
            for dx in 0..width as i32 {
                let px = rect.x as i32 + x + dx;
                let py = baseline_y as i32 + y + dy;
                if px >= clip_left && px < clip_right && py >= clip_top && py < clip_bottom {
                    blend_pixel(pixmap, px as u32, py as u32, [r, g, b, a]);
                }
            }
        }
    });
    Ok(())
}

fn parse_color_components(value: &str) -> Result<[u8; 4], RenderError> {
    let color = parse_color(value)?;
    Ok([color.red(), color.green(), color.blue(), color.alpha()])
}

fn blend_pixel(pixmap: &mut Pixmap, x: u32, y: u32, src: [u8; 4]) {
    let width = pixmap.width();
    let index = ((y * width + x) * 4) as usize;
    let data = pixmap.data_mut();
    let src_a = src[3] as f32 / 255.0;
    let dst_a = data[index + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let src_c = src[channel] as f32 / 255.0;
        let dst_c = data[index + channel] as f32 / 255.0;
        data[index + channel] = (((src_c * src_a + dst_c * dst_a * (1.0 - src_a)) / out_a) * 255.0) as u8;
    }
    data[index + 3] = (out_a * 255.0) as u8;
}
```

Also update `render_frame`:

```rust
    if config.playback_progress.enabled {
        draw_playback_progress(&mut pixmap, config, &layout, timeline.duration_ms(), timestamp_ms)?;
    }
    draw_labels(&mut pixmap, config, timeline, &layout, timestamp_ms)?;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-renderer`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-renderer
git commit -m "feat: render clipped label layer"
```

---

### Task 8: PNG Writing and Preview API

**Files:**
- Modify: `crates/progressbar-renderer/src/lib.rs`
- Modify: `crates/progressbar-api/Cargo.toml`
- Replace: `crates/progressbar-api/src/lib.rs`

- [ ] **Step 1: Write failing API preview test**

Add test in `crates/progressbar-api/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn writes_preview_png_with_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let segments_path = dir.path().join("segments.txt");
        let output_path = dir.path().join("preview.png");
        fs::write(
            &config_path,
            r##"
[render]
width = 320
height = 180
fps = 30

[output]
format = "png-sequence"
path = "out/progress"
"##,
        )
        .unwrap();
        fs::write(&segments_path, "2 | 开场").unwrap();

        preview_frame(PreviewFrameRequest {
            config_path,
            segments_path,
            output_path: output_path.clone(),
            timestamp_ms: 500,
        })
        .unwrap();

        let bytes = fs::read(output_path).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-api`

Expected: FAIL with unresolved API types.

- [ ] **Step 3: Implement PNG writer in renderer**

Add to `crates/progressbar-renderer/src/lib.rs`:

```rust
use std::io::Write;

pub fn write_png<W: Write>(frame: &RenderedFrame, writer: W) -> Result<(), RenderError> {
    let mut encoder = png::Encoder::new(writer, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder
        .write_header()
        .map_err(|error| RenderError::Layout(error.to_string()))?;
    png_writer
        .write_image_data(&frame.rgba)
        .map_err(|error| RenderError::Layout(error.to_string()))?;
    Ok(())
}
```

- [ ] **Step 4: Implement preview API**

Replace `crates/progressbar-api/src/lib.rs`:

```rust
use progressbar_core::Timeline;
use progressbar_renderer::{render_frame, write_png};
use progressbar_schema::ProjectConfig;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct PreviewFrameRequest {
    pub config_path: PathBuf,
    pub segments_path: PathBuf,
    pub output_path: PathBuf,
    pub timestamp_ms: u64,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("failed to read `{path}`: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write `{path}`: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config error: {0}")]
    Config(String),
    #[error("segment error: {0}")]
    Segment(String),
    #[error("render error: {0}")]
    Render(String),
}

pub fn validate_project(config_path: PathBuf, segments_path: PathBuf) -> Result<(), ApiError> {
    let _ = load_project(config_path, segments_path)?;
    Ok(())
}

pub fn preview_frame(request: PreviewFrameRequest) -> Result<(), ApiError> {
    let (config, timeline) = load_project(request.config_path, request.segments_path)?;
    let frame = render_frame(&config, &timeline, request.timestamp_ms)
        .map_err(|error| ApiError::Render(error.to_string()))?;
    if let Some(parent) = request.output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| ApiError::WriteFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let file = fs::File::create(&request.output_path).map_err(|source| ApiError::WriteFile {
        path: request.output_path.clone(),
        source,
    })?;
    write_png(&frame, file).map_err(|error| ApiError::Render(error.to_string()))
}

fn load_project(config_path: PathBuf, segments_path: PathBuf) -> Result<(ProjectConfig, Timeline), ApiError> {
    let config_text = fs::read_to_string(&config_path).map_err(|source| ApiError::ReadFile {
        path: config_path.clone(),
        source,
    })?;
    let segments_text = fs::read_to_string(&segments_path).map_err(|source| ApiError::ReadFile {
        path: segments_path.clone(),
        source,
    })?;
    let config = ProjectConfig::from_toml_str(&config_text).map_err(|error| ApiError::Config(error.to_string()))?;
    let timeline = Timeline::parse(&segments_text).map_err(|error| ApiError::Segment(error.to_string()))?;
    Ok((config, timeline))
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p progressbar-api`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/progressbar-renderer crates/progressbar-api
git commit -m "feat: add preview frame API"
```

---

### Task 9: PNG Sequence Encoder

**Files:**
- Replace: `crates/progressbar-encoder/src/lib.rs`
- Modify: `crates/progressbar-api/src/lib.rs`

- [ ] **Step 1: Write failing encoder test**

Add test in `crates/progressbar-encoder/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use progressbar_core::Timeline;
    use progressbar_schema::ProjectConfig;

    #[test]
    fn writes_frame_count_from_duration_and_fps() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::default();
        config.render.width = 160;
        config.render.height = 90;
        config.render.fps = 2;
        config.output.path = dir.path().join("frames");
        let timeline = Timeline::parse("2 | A").unwrap();
        let mut seen = Vec::new();
        render_png_sequence(&config, &timeline, |event| seen.push(event)).unwrap();
        assert!(dir.path().join("frames/frame_000000.png").exists());
        assert!(dir.path().join("frames/frame_000003.png").exists());
        assert_eq!(seen.last().unwrap().completed_frames, 4);
        assert_eq!(seen.last().unwrap().total_frames, 4);
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-encoder`

Expected: FAIL with unresolved `render_png_sequence`.

- [ ] **Step 3: Implement streaming PNG sequence encoder**

Replace `crates/progressbar-encoder/src/lib.rs`:

```rust
use progressbar_core::{frame_count, frame_timestamp_ms, Timeline};
use progressbar_renderer::{render_frame, write_png};
use progressbar_schema::ProjectConfig;
use std::fs;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderProgress {
    pub completed_frames: u64,
    pub total_frames: u64,
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("failed to create output directory: {0}")]
    CreateDir(std::io::Error),
    #[error("failed to create frame file: {0}")]
    CreateFrame(std::io::Error),
    #[error("render error: {0}")]
    Render(String),
}

pub fn render_png_sequence<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    let total_frames = frame_count(timeline.duration_ms(), config.render.fps);
    fs::create_dir_all(&config.output.path).map_err(EncodeError::CreateDir)?;

    for frame_index in 0..total_frames {
        let timestamp_ms = frame_timestamp_ms(frame_index, config.render.fps);
        let frame = render_frame(config, timeline, timestamp_ms)
            .map_err(|error| EncodeError::Render(error.to_string()))?;
        let path = config.output.path.join(format!("frame_{frame_index:06}.png"));
        let file = fs::File::create(path).map_err(EncodeError::CreateFrame)?;
        write_png(&frame, file).map_err(|error| EncodeError::Render(error.to_string()))?;
        on_progress(RenderProgress {
            completed_frames: frame_index + 1,
            total_frames,
        });
    }

    Ok(())
}
```

- [ ] **Step 4: Add API render function**

Add to `crates/progressbar-api/src/lib.rs`:

```rust
#[derive(Debug, Clone)]
pub struct RenderOverlayRequest {
    pub config_path: PathBuf,
    pub segments_path: PathBuf,
}

pub fn render_overlay<F>(request: RenderOverlayRequest, on_progress: F) -> Result<(), ApiError>
where
    F: FnMut(progressbar_encoder::RenderProgress),
{
    let (config, timeline) = load_project(request.config_path, request.segments_path)?;
    progressbar_encoder::render_png_sequence(&config, &timeline, on_progress)
        .map_err(|error| ApiError::Render(error.to_string()))
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p progressbar-encoder -p progressbar-api`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/progressbar-encoder crates/progressbar-api
git commit -m "feat: render png sequence overlays"
```

---

### Task 10: CLI Commands

**Files:**
- Modify: `apps/cli/Cargo.toml`
- Replace: `apps/cli/src/main.rs`

- [ ] **Step 1: Write failing CLI integration tests**

Create `apps/cli/tests/cli.rs`:

```rust
use assert_cmd::Command;
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
        .success();
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p progressbar-cli`

Expected: FAIL because CLI commands are not implemented.

- [ ] **Step 3: Implement CLI**

Replace `apps/cli/src/main.rs`:

```rust
use clap::{Parser, Subcommand};
use progressbar_api::{preview_frame, render_overlay, validate_project, PreviewFrameRequest, RenderOverlayRequest};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "progressbar2video")]
#[command(about = "Generate transparent progress-bar overlay assets for video editing.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
    },
    PreviewFrame {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 0)]
        timestamp_ms: u64,
    },
    Render {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), progressbar_api::ApiError> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { config, segments } => {
            validate_project(config, segments)?;
            println!("Project is valid.");
        }
        Command::PreviewFrame {
            config,
            segments,
            output,
            timestamp_ms,
        } => {
            preview_frame(PreviewFrameRequest {
                config_path: config,
                segments_path: segments,
                output_path: output,
                timestamp_ms,
            })?;
            println!("Preview frame written.");
        }
        Command::Render { config, segments } => {
            render_overlay(
                RenderOverlayRequest {
                    config_path: config,
                    segments_path: segments,
                },
                |progress| {
                    println!(
                        "Rendered {}/{} frames",
                        progress.completed_frames, progress.total_frames
                    );
                },
            )?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p progressbar-cli`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/cli
git commit -m "feat: add progressbar2video cli"
```

---

### Task 11: Examples and End-to-End Verification

**Files:**
- Create: `examples/basic/config.toml`
- Create: `examples/basic/segments.txt`
- Modify: `README.md`

- [ ] **Step 1: Add basic example**

`examples/basic/segments.txt`:

```txt
00:00:02.000 | 开场
00:00:04.000 | 背景介绍
00:00:06.000 | 核心演示
```

`examples/basic/config.toml`:

```toml
[render]
width = 640
height = 360
fps = 10
background = "transparent"

[bar]
position = "bottom"
height = 48
margin_x = 40
margin_bottom = 24
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
font_size = 20
min_font_size = 14
color = "#FFFFFF"
display_mode = "all-segments"
overflow = "auto"

[output]
format = "png-sequence"
path = "examples/basic/out/progress"
```

- [ ] **Step 2: Update README usage**

Add:

````markdown
## MVP Usage

Validate an overlay project:

```powershell
cargo run -p progressbar-cli -- validate --config examples/basic/config.toml --segments examples/basic/segments.txt
```

Render one transparent preview frame:

```powershell
cargo run -p progressbar-cli -- preview-frame --config examples/basic/config.toml --segments examples/basic/segments.txt --output examples/basic/out/preview.png --timestamp-ms 1000
```

Render a PNG sequence:

```powershell
cargo run -p progressbar-cli -- render --config examples/basic/config.toml --segments examples/basic/segments.txt
```
````

- [ ] **Step 3: Run full verification**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo run -p progressbar-cli -- validate --config examples/basic/config.toml --segments examples/basic/segments.txt
cargo run -p progressbar-cli -- preview-frame --config examples/basic/config.toml --segments examples/basic/segments.txt --output examples/basic/out/preview.png --timestamp-ms 1000
cargo run -p progressbar-cli -- render --config examples/basic/config.toml --segments examples/basic/segments.txt
```

Expected:

- Format check passes.
- Workspace tests pass.
- Validation prints `Project is valid.`
- Preview PNG exists at `examples/basic/out/preview.png`.
- PNG sequence contains 60 frames because the final segment ends at 6 seconds and fps is 10.

- [ ] **Step 4: Commit**

```bash
git add README.md examples
git commit -m "docs: add basic rendering example"
```

---

### Task 12: MVP Completion Review

**Files:**
- Modify: `README.md` if verification exposes missing usage notes.

- [ ] **Step 1: Run final verification**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo run -p progressbar-cli -- validate --config examples/basic/config.toml --segments examples/basic/segments.txt
```

Expected: all commands pass.

- [ ] **Step 2: Check git status**

Run: `git status --short --branch`

Expected: clean working tree on `master`, or only generated files under ignored `examples/basic/out/`.

- [ ] **Step 3: Summarize residual gaps**

Record in the final response:

- Text glyph rendering uses the `cosmic-text` label layer from Task 7.
- APNG, FFmpeg profiles, and Tauri GUI are outside this MVP plan.
- The current output baseline is transparent PNG sequence.

---

## Self-Review Notes

Spec coverage:

- Transparent overlay default: Task 2 and Task 6.
- Segment text file parsing: Task 3.
- Last segment end time defines duration: Task 3, Task 4, Task 9, Task 11.
- Duration-proportional splitting: Task 4 and Task 6.
- Config defaults and no hard-coded renderer defaults: Task 2.
- `overflow = "auto"` and `min_font_size`: Task 2 and Task 5.
- No scroll speed setting: Task 5 derives scroll offset from segment duration.
- Optional playback-progress overlay layer: Task 6.
- PNG sequence output: Task 9.
- CLI is thin over shared API: Task 8, Task 9, Task 10.

Type consistency:

- Config type: `ProjectConfig`.
- Timeline type: `Timeline`.
- Main API calls: `validate_project`, `preview_frame`, `render_overlay`.
- CLI package name: `progressbar-cli`, binary name: `progressbar2video`.
