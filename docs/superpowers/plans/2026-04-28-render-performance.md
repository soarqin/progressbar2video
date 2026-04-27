# Render Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse renderer state across frame loops so all output profiles render faster without changing output semantics.

**Architecture:** Add `FrameRenderer` to `progressbar-renderer` as a reusable render session that caches layout, base bar pixels, static label pixels, and text shaping caches. Update `progressbar-encoder` to create one session per render job and call it for each timestamp.

**Tech Stack:** Rust 2021, existing `tiny-skia`, `cosmic-text`, `progressbar-core`, `progressbar-renderer`, and `progressbar-encoder`.

---

## File Structure

- Modify `crates/progressbar-renderer/src/lib.rs`: add `FrameRenderer`, cached label plans, cached static layers, and tests.
- Modify `crates/progressbar-encoder/src/lib.rs`: reuse `FrameRenderer` in PNG sequence, APNG, and FFmpeg loops.

---

### Task 1: Renderer Session API

**Files:**
- Modify: `crates/progressbar-renderer/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Add renderer tests:

```rust
#[test]
fn frame_renderer_matches_one_shot_render_frame() {
    let mut config = ProjectConfig::default();
    config.render.width = 320;
    config.render.height = 180;
    config.bar.margin_x = 20;
    config.bar.margin_bottom = 16;
    config.bar.height = 36;
    let timeline = Timeline::parse("1 | A\n2 | very very long label").unwrap();
    let mut renderer = FrameRenderer::new(&config, &timeline).unwrap();
    let cached = renderer.render_frame(1_500).unwrap();
    let one_shot = render_frame(&config, &timeline, 1_500).unwrap();
    assert_eq!(cached.rgba, one_shot.rgba);
}

#[test]
fn frame_renderer_keeps_static_frames_identical() {
    let mut config = ProjectConfig::default();
    config.render.width = 320;
    config.render.height = 180;
    config.bar.margin_x = 20;
    config.bar.margin_bottom = 16;
    config.bar.height = 36;
    config.playback_progress.enabled = false;
    config.text.overflow = progressbar_schema::OverflowMode::Ellipsis;
    let timeline = Timeline::parse("1 | very very long label\n2 | B").unwrap();
    let mut renderer = FrameRenderer::new(&config, &timeline).unwrap();
    let first = renderer.render_frame(100).unwrap();
    let second = renderer.render_frame(1_900).unwrap();
    assert_eq!(first.rgba, second.rgba);
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
cargo test -p progressbar-renderer frame_renderer_matches_one_shot_render_frame
```

Expected: compile failure because `FrameRenderer` does not exist.

- [ ] **Step 3: Implement `FrameRenderer`**

Add a public session type:

```rust
pub struct FrameRenderer {
    config: ProjectConfig,
    timeline: Timeline,
    layout: Layout,
    base_rgba: Vec<u8>,
    static_label_rgba: Option<Vec<u8>>,
    labels: Vec<CachedLabel>,
    font_system: FontSystem,
    swash_cache: SwashCache,
}
```

Add cached label types:

```rust
#[derive(Debug, Clone)]
struct CachedLabel {
    segment_index: usize,
    rect: progressbar_core::Rect,
    text: String,
    font_size: u32,
    mode: CachedLabelMode,
    start_ms: TimeMs,
    end_ms: TimeMs,
    measured_width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CachedLabelMode {
    Normal,
    Rotate,
    Scroll,
}
```

Implement:

```rust
impl FrameRenderer {
    pub fn new(config: &ProjectConfig, timeline: &Timeline) -> Result<Self, RenderError> { ... }
    pub fn render_frame(&mut self, timestamp_ms: TimeMs) -> Result<RenderedFrame, RenderError> { ... }
}
```

`new` should calculate layout once, render the static bar layer into `base_rgba`, precompute labels, and pre-render static labels only when `display_mode = AllSegments` and the label mode is not `Scroll`.

Change the existing free function:

```rust
pub fn render_frame(config: &ProjectConfig, timeline: &Timeline, timestamp_ms: TimeMs) -> Result<RenderedFrame, RenderError> {
    FrameRenderer::new(config, timeline)?.render_frame(timestamp_ms)
}
```

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-renderer
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/progressbar-renderer/src/lib.rs
git commit -m "feat: cache renderer state across frames"
```

---

### Task 2: Encoder Reuse

**Files:**
- Modify: `crates/progressbar-encoder/src/lib.rs`

- [ ] **Step 1: Write failing encoder test**

Add:

```rust
#[test]
fn png_sequence_uses_reusable_renderer_and_preserves_frame_count() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = ProjectConfig::default();
    config.render.width = 160;
    config.render.height = 90;
    config.render.fps = 2;
    config.bar.margin_x = 10;
    config.bar.margin_bottom = 10;
    config.bar.height = 24;
    config.output.path = dir.path().join("frames");
    let timeline = Timeline::parse("2 | A").unwrap();
    let mut seen = Vec::new();
    render_png_sequence(&config, &timeline, |event| seen.push(event)).unwrap();
    assert!(dir.path().join("frames/frame_000000.png").exists());
    assert!(dir.path().join("frames/frame_000003.png").exists());
    assert_eq!(seen.last().unwrap().completed_frames, 4);
}
```

This is initially equivalent to an existing behavior test, then implementation will route through `FrameRenderer`.

- [ ] **Step 2: Implement encoder session reuse**

Import:

```rust
use progressbar_renderer::{render_frame, write_png, FrameRenderer, RenderedFrame};
```

In `render_png_sequence`, `write_apng`, and `render_ffmpeg`, create:

```rust
let mut renderer = FrameRenderer::new(config, timeline)
    .map_err(|error| EncodeError::Render(error.to_string()))?;
```

Then replace per-frame calls to `render_frame(config, timeline, timestamp_ms)` with:

```rust
let frame = renderer
    .render_frame(timestamp_ms)
    .map_err(|error| EncodeError::Render(error.to_string()))?;
```

Keep the free `render_frame` import only if tests still need it; otherwise remove it.

- [ ] **Step 3: Verify GREEN**

Run:

```powershell
cargo test -p progressbar-encoder
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/progressbar-encoder/src/lib.rs
git commit -m "feat: reuse renderer session in encoders"
```

---

### Task 3: Verification and Timing

**Files:**
- No source changes expected.

- [ ] **Step 1: Run correctness checks**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 2: Run release timing checks**

Run:

```powershell
cargo build --release
$elapsed = Measure-Command { target\release\progressbar2video.exe render --config examples\basic\config.toml --segments examples\basic\segments.txt | Out-Null }; "release_basic_png_sequence_ms=$([int]$elapsed.TotalMilliseconds)"
$elapsed = Measure-Command { target\release\progressbar2video.exe render --config examples\encoder-profiles\apng.toml --segments examples\encoder-profiles\segments.txt | Out-Null }; "release_encoder_apng_ms=$([int]$elapsed.TotalMilliseconds)"
```

Compare against baseline noted in the design doc: about 6.4s for basic PNG sequence and 1.7s for APNG on this machine.

- [ ] **Step 3: Merge locally**

After tests and timing checks pass:

```powershell
git merge --ff-only feature/render-performance
git worktree remove .worktrees\render-performance
git branch -d feature/render-performance
```

---

## Self-Review Notes

- Public config stays unchanged.
- Existing `render_frame` remains available.
- Playback-progress remains above bar and below text because static label RGBA is blended after playback.
- Dynamic scroll labels are still drawn per timestamp.
- Encoders continue to stream frames; no full-video buffering is introduced.
