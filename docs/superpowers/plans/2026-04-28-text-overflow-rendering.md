# Text Overflow Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `shrink`, `ellipsis`, `rotate`, `scroll`, and `auto` text overflow modes affect actual rendered frames.

**Architecture:** Keep public config unchanged. Add renderer-local text measurement and render planning that feeds `progressbar-core::choose_text_strategy`, then draw labels according to the selected plan while clipping every pixel to the segment rectangle. Add examples that exercise narrow segments and long Chinese labels.

**Tech Stack:** Rust 2021, existing `progressbar-core`, `progressbar-renderer`, `cosmic-text`, `tiny-skia`, PNG sequence CLI.

---

## File Structure

- Modify `crates/progressbar-renderer/src/lib.rs`: add text measurement, ellipsis truncation, scroll offsets, rotated pixel mapping, and focused renderer tests.
- Add `examples/long-text/config.toml`: a small transparent overlay config with `overflow = "auto"`.
- Add `examples/long-text/segments.txt`: dense segments with long Chinese labels.
- Modify `README.md`: add long-text preview command.

---

### Task 1: Renderer Text Planning Helpers

**Files:**
- Modify: `crates/progressbar-renderer/src/lib.rs`

- [ ] **Step 1: Write failing tests for text planning**

Add tests inside the renderer test module:

```rust
#[test]
fn ellipsis_plan_shortens_text_to_fit_rect() {
    let mut config = ProjectConfig::default();
    config.text.overflow = progressbar_schema::OverflowMode::Ellipsis;
    config.text.font_size = 24;
    config.text.min_font_size = 14;
    let rect = progressbar_core::Rect { x: 0.0, y: 0.0, width: 80.0, height: 40.0 };
    let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 0, 0, 2_000);
    assert!(matches!(plan.mode, LabelRenderMode::Normal));
    assert!(plan.text.ends_with("..."));
    assert!(estimate_text_width(&plan.text, plan.font_size) <= rect.width);
}

#[test]
fn auto_scroll_plan_uses_min_font_size_when_rotation_is_not_available() {
    let mut config = ProjectConfig::default();
    config.text.overflow = progressbar_schema::OverflowMode::Auto;
    config.text.font_size = 28;
    config.text.min_font_size = 16;
    let rect = progressbar_core::Rect { x: 0.0, y: 0.0, width: 70.0, height: 12.0 };
    let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 1_000, 0, 2_000);
    assert_eq!(plan.font_size, 16);
    assert!(matches!(plan.mode, LabelRenderMode::Scroll { .. }));
}

#[test]
fn auto_rotate_plan_uses_min_font_size_for_narrow_tall_cells() {
    let mut config = ProjectConfig::default();
    config.text.overflow = progressbar_schema::OverflowMode::Auto;
    config.text.font_size = 28;
    config.text.min_font_size = 16;
    let rect = progressbar_core::Rect { x: 0.0, y: 0.0, width: 70.0, height: 64.0 };
    let plan = plan_label_render(&config, rect, "这是一个非常非常长的标题", 1_000, 0, 2_000);
    assert_eq!(plan.font_size, 16);
    assert!(matches!(plan.mode, LabelRenderMode::Rotate));
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p progressbar-renderer ellipsis_plan_shortens_text_to_fit_rect`

Expected: FAIL because `plan_label_render`, `LabelRenderMode`, and `estimate_text_width` do not exist.

- [ ] **Step 3: Implement planning helpers**

Add:

```rust
#[derive(Debug, Clone, PartialEq)]
struct LabelRenderPlan {
    text: String,
    font_size: u32,
    mode: LabelRenderMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum LabelRenderMode {
    Normal,
    Rotate,
    Scroll { offset_px: f32 },
}

fn estimate_text_width(text: &str, font_size: u32) -> f32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { 0.58 } else { 1.0 })
        .sum::<f32>()
        * font_size as f32
}

fn can_rotate_label(rect: progressbar_core::Rect, min_font_size: u32) -> bool {
    rect.height >= min_font_size as f32 * 2.5
}

fn plan_label_render(
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    label: &str,
    timestamp_ms: TimeMs,
    segment_start_ms: TimeMs,
    segment_end_ms: TimeMs,
) -> LabelRenderPlan {
    let text_width = estimate_text_width(label, config.text.font_size);
    let decision = progressbar_core::choose_text_strategy(progressbar_core::TextStrategyInput {
        overflow: config.text.overflow.clone(),
        text_width_px: text_width,
        rect_width_px: rect.width.max(1.0),
        font_size: config.text.font_size,
        min_font_size: config.text.min_font_size,
        can_rotate: can_rotate_label(rect, config.text.min_font_size),
    });

    match decision {
        progressbar_core::TextStrategyDecision::Normal { font_size }
        | progressbar_core::TextStrategyDecision::Shrink { font_size } => LabelRenderPlan {
            text: label.to_string(),
            font_size,
            mode: LabelRenderMode::Normal,
        },
        progressbar_core::TextStrategyDecision::Ellipsis { font_size } => LabelRenderPlan {
            text: ellipsize_to_width(label, font_size, rect.width.max(1.0)),
            font_size,
            mode: LabelRenderMode::Normal,
        },
        progressbar_core::TextStrategyDecision::Rotate { font_size } => LabelRenderPlan {
            text: label.to_string(),
            font_size,
            mode: LabelRenderMode::Rotate,
        },
        progressbar_core::TextStrategyDecision::Scroll { font_size } => {
            let segment = progressbar_core::Segment {
                start_ms: segment_start_ms,
                end_ms: segment_end_ms,
                label: label.to_string(),
            };
            let measured = estimate_text_width(label, font_size);
            LabelRenderPlan {
                text: label.to_string(),
                font_size,
                mode: LabelRenderMode::Scroll {
                    offset_px: progressbar_core::scroll_offset_px(timestamp_ms, &segment, measured, rect.width),
                },
            }
        }
    }
}

fn ellipsize_to_width(label: &str, font_size: u32, max_width: f32) -> String {
    if estimate_text_width(label, font_size) <= max_width {
        return label.to_string();
    }
    let ellipsis = "...";
    let mut output = String::new();
    for ch in label.chars() {
        let candidate = format!("{output}{ch}{ellipsis}");
        if estimate_text_width(&candidate, font_size) > max_width {
            break;
        }
        output.push(ch);
    }
    if output.is_empty() {
        ellipsis.to_string()
    } else {
        format!("{output}{ellipsis}")
    }
}
```

- [ ] **Step 4: Run renderer tests**

Run: `cargo test -p progressbar-renderer`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-renderer/src/lib.rs
git commit -m "feat: plan text overflow rendering"
```

---

### Task 2: Apply Text Plans During Rendering

**Files:**
- Modify: `crates/progressbar-renderer/src/lib.rs`

- [ ] **Step 1: Write failing rendering tests**

Add tests:

```rust
#[test]
fn scroll_mode_changes_visible_pixels_over_segment_time() {
    let mut config = ProjectConfig::default();
    config.render.width = 240;
    config.render.height = 120;
    config.bar.margin_x = 20;
    config.bar.margin_bottom = 16;
    config.bar.height = 32;
    config.playback_progress.enabled = false;
    config.text.overflow = progressbar_schema::OverflowMode::Scroll;
    config.text.font_size = 22;
    let timeline = Timeline::parse("2 | very very very long label").unwrap();
    let start = render_frame(&config, &timeline, 0).unwrap();
    let later = render_frame(&config, &timeline, 1_800).unwrap();
    assert_ne!(sample_bar_region(&start), sample_bar_region(&later));
}

#[test]
fn rotate_mode_renders_pixels_in_narrow_segment() {
    let mut config = ProjectConfig::default();
    config.render.width = 220;
    config.render.height = 140;
    config.bar.margin_x = 20;
    config.bar.margin_bottom = 16;
    config.bar.height = 72;
    config.playback_progress.enabled = false;
    config.text.overflow = progressbar_schema::OverflowMode::Rotate;
    config.text.font_size = 20;
    let timeline = Timeline::parse("2 | rotate label").unwrap();
    let frame = render_frame(&config, &timeline, 500).unwrap();
    assert!(count_non_fill_pixels(&frame) > 0);
}
```

Add test helpers:

```rust
fn sample_bar_region(frame: &RenderedFrame) -> Vec<[u8; 4]> {
    (60..90)
        .flat_map(|y| (30..210).step_by(3).map(move |x| frame.pixel_rgba(x, y)))
        .collect()
}

fn count_non_fill_pixels(frame: &RenderedFrame) -> usize {
    let fill = [77, 163, 255, 255];
    (0..frame.height)
        .flat_map(|y| (0..frame.width).map(move |x| frame.pixel_rgba(x, y)))
        .filter(|pixel| pixel[3] > 0 && *pixel != fill)
        .count()
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p progressbar-renderer scroll_mode_changes_visible_pixels_over_segment_time`

Expected: FAIL because current rendering ignores overflow mode and timestamp for labels.

- [ ] **Step 3: Refactor label drawing to use plans**

Change `draw_labels` to pass segment timing:

```rust
draw_label_text(
    pixmap,
    config,
    segment_layout.rect,
    &segment.label,
    timestamp_ms,
    segment.start_ms,
    segment.end_ms,
)?;
```

Change `draw_label_text` signature:

```rust
fn draw_label_text(
    pixmap: &mut Pixmap,
    config: &ProjectConfig,
    rect: progressbar_core::Rect,
    label: &str,
    timestamp_ms: TimeMs,
    segment_start_ms: TimeMs,
    segment_end_ms: TimeMs,
) -> Result<(), RenderError>
```

Inside it, call `plan_label_render`, then route to `draw_text_pixels`:

```rust
let plan = plan_label_render(config, rect, label, timestamp_ms, segment_start_ms, segment_end_ms);
draw_text_pixels(pixmap, config, rect, &plan)
```

Implement `draw_text_pixels` by moving the existing cosmic-text drawing body and applying:

- Normal: base x is `rect.x + 4.0`.
- Scroll: base x is `rect.x + 4.0 + offset_px`.
- Rotate: map source text pixels 90 degrees clockwise into the segment rectangle.
- All modes: clip to `rect`.

- [ ] **Step 4: Run renderer tests**

Run: `cargo test -p progressbar-renderer`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/progressbar-renderer/src/lib.rs
git commit -m "feat: apply text overflow render modes"
```

---

### Task 3: Long Text Example and Verification

**Files:**
- Create: `examples/long-text/config.toml`
- Create: `examples/long-text/segments.txt`
- Modify: `README.md`

- [ ] **Step 1: Add example files**

`examples/long-text/segments.txt`:

```txt
00:00:01.000 | 这是一个非常非常长的开场标题
00:00:02.000 | 中间段落标题也很长需要自动处理
00:00:03.000 | 结尾段落
```

`examples/long-text/config.toml`:

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

[output]
format = "png-sequence"
path = "examples/long-text/out/progress"
```

- [ ] **Step 2: Update README**

Add:

```markdown
Long text preview:

```powershell
cargo run -p progressbar-cli -- preview-frame --config examples/long-text/config.toml --segments examples/long-text/segments.txt --output examples/long-text/out/preview.png --timestamp-ms 1500
```
```

- [ ] **Step 3: Run verification**

Run:

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo run -p progressbar-cli -- preview-frame --config examples/long-text/config.toml --segments examples/long-text/segments.txt --output examples/long-text/out/preview.png --timestamp-ms 1500
```

Expected:

- Formatting passes.
- All tests pass.
- Preview PNG is created.

- [ ] **Step 4: Commit**

```bash
git add README.md examples/long-text
git commit -m "docs: add long text rendering example"
```

---

## Self-Review Notes

Spec coverage:

- `shrink`: renderer uses the planned smaller font size.
- `ellipsis`: renderer draws a shortened label that fits the segment width.
- `rotate`: renderer maps text pixels into a rotated narrow-cell layout.
- `scroll`: renderer derives offset from timestamp and segment duration.
- `auto`: renderer uses `choose_text_strategy`, including the `min_font_size` rule before scroll fallback.
- Clipping: every drawn text pixel is checked against the segment rectangle.

