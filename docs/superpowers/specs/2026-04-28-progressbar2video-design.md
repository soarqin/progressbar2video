# ProgressBar2Video Design

## Goal

ProgressBar2Video generates transparent overlay assets for video editing. The first product surface is a reliable local tool that reads timeline text plus flexible style configuration, then renders a bottom progress bar with segment labels into lossless or editing-friendly outputs.

The output is designed to be composited over an existing video in editing software. Transparent background is the default, not an optional afterthought.

## Product Scope

The project will support these core workflows:

- Read segment definitions from a plain text file.
- Read render, style, text, and output settings from a structured config file.
- Preview a representative frame or short preview range.
- Render a complete transparent progress-bar overlay whose duration matches the last segment end time.
- Export lossless or near-editing-master outputs for downstream video editing.

The first implementation should avoid a large GUI runtime. It should also avoid making the command line the internal integration boundary. CLI support is useful for batch jobs, but GUI and CLI must call the same core API.

## Recommended Stack

Use a Rust-first architecture with a WebView desktop shell:

- Rust for configuration validation, timeline parsing, layout, frame rendering, and encoding orchestration.
- Tauri 2 for the future desktop GUI because it uses system WebView technology instead of bundling Chromium.
- Svelte with Vite for the GUI surface, keeping frontend assets small and simple.
- Optional CLI as a thin wrapper around the same Rust crates.
- FFmpeg as an internal encoder dependency for video outputs that require professional container or codec support.

Tauri is preferred over Electron because distribution size matters. On Windows it uses WebView2; on macOS it uses WKWebView; on Linux it uses WebKitGTK. Wails is a reasonable alternative, but Go is less attractive here than Rust for image buffers, alpha-aware rendering, future WASM reuse, and low-level encoding integration.

## Repository Shape

```txt
ProgressBar2Video/
  crates/
    progressbar-schema/
    progressbar-core/
    progressbar-renderer/
    progressbar-encoder/
    progressbar-api/
  apps/
    desktop-tauri/
    cli/
    web-preview/
  examples/
    basic/
    long-text/
    dense-segments/
  docs/
    superpowers/
      specs/
      plans/
```

### `progressbar-schema`

Owns the public configuration model, default values, validation, and schema export.

Responsibilities:

- Define strongly typed config structs with `serde`.
- Merge user config with defaults.
- Validate invalid combinations before rendering starts.
- Export JSON Schema for the web UI so forms and core validation stay aligned.
- Require a resolved `min_font_size` whenever automatic text overflow may fall back to scrolling.

### `progressbar-core`

Owns domain logic that does not need pixels.

Responsibilities:

- Parse segment text files.
- Normalize times into milliseconds or frame-accurate rational timestamps.
- Convert segment end points into `[start, end, label]` ranges.
- Treat the final segment end time as the full overlay duration.
- Compute progress state for any timestamp.
- Compute duration-proportional bar and label layout rectangles.
- Select text overflow behavior for each segment.

### `progressbar-renderer`

Owns drawing transparent RGBA frames.

Responsibilities:

- Render one frame from resolved config plus timeline state.
- Draw the segmented bar track, segment fills, dividers, labels, and optional active segment styling.
- Draw an optional playback-progress overlay above the segmented progress bar and below text.
- Respect transparent canvas defaults.
- Clip text to segment bounds to prevent overlap.
- Support long-text strategies: shrink, ellipsis, rotate, scroll, and auto.

Initial implementation will use `tiny-skia` for 2D vector-style drawing and `cosmic-text` for text shaping, font discovery, and glyph rasterization. The first implementation task must include a small rendering spike that verifies Chinese text quality, font fallback, clipping, rotation, and transparent RGBA output before broader renderer work continues. If that spike fails, the implementation plan should replace the text stack before implementing the remaining renderer.

### `progressbar-encoder`

Owns output formats and long-running export jobs.

Responsibilities:

- Write PNG sequence output.
- Write APNG output in the encoder milestone after PNG sequence output is stable.
- Pipe or stage RGBA/PNG frames into FFmpeg for FFV1, ProRes 4444, or other configured formats.
- Report deterministic progress events.
- Avoid loading the whole render into memory for long videos.

### `progressbar-api`

Owns stable application-facing commands used by both GUI and CLI.

Responsibilities:

- Provide functions such as `validate_project`, `preview_frame`, `render_overlay`, and `cancel_render`.
- Return structured errors, not formatted terminal strings.
- Emit progress events suitable for GUI progress bars.
- Keep path handling and file access policy in one place.

## Desktop GUI Strategy

The future desktop app should use Tauri 2:

- Frontend: Svelte + Vite web UI.
- Backend: Rust commands exposed through Tauri `invoke`.
- User chooses segment file, config file, output path, and optional source video dimensions.
- GUI calls Rust functions directly through typed command wrappers.
- No GUI feature should depend on parsing CLI output.

Example conceptual call:

```ts
await invoke("render_overlay", {
  request: {
    configPath,
    segmentsPath,
    outputPath
  }
});
```

The app can still include a CLI package later for automation:

```powershell
progressbar2video render --config config.toml --segments segments.txt --output out/progress.mov
```

Both entry points call `progressbar-api`; neither owns rendering behavior.

## Input Formats

### Segment Text

The default format is one segment end point per line:

```txt
00:00:12.500 | 开场
00:01:05.000 | 背景介绍
00:03:20.000 | 核心演示
```

Rules:

- The first segment starts at `0`.
- Each line defines the end time and text for the current segment.
- Times must be strictly increasing.
- Blank lines and comment lines beginning with `#` are ignored.
- Supported time formats include `SS.mmm`, `MM:SS.mmm`, and `HH:MM:SS.mmm`.

### Config File

Use TOML for hand-edited project files because it is readable and supports sections cleanly.

Example:

```toml
[render]
width = 1920
height = 1080
fps = 60
background = "transparent"

[bar]
position = "bottom"
height = 72
margin_x = 80
margin_bottom = 36
corner_radius = 8
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
```

Defaults live in `progressbar-schema`; renderer code should receive resolved values and should not hard-code visual defaults.

## Rendering Model

The renderer produces frames at a configured width, height, and fps.

The overlay duration is derived from the last segment end time. There is no separate duration setting in the first version. If the final segment ends at `00:03:20.000`, rendering at 60 fps produces a 200-second overlay with 12,000 frames.

Default composition:

- Full frame is transparent RGBA.
- Progress bar sits near the bottom edge.
- Track spans the configured horizontal margins.
- Segment fill is drawn as a duration-proportional static band according to bar styling.
- Segment widths and divider positions are always proportional to segment duration.
- Segment dividers mark duration-based segment boundaries.
- Optional playback-progress overlay shows the current timestamp above the segmented progress bar.
- Text is drawn inside each segment area or according to the selected text display mode.

Render order:

- Transparent frame background.
- Segmented progress bar track, fills, and dividers.
- Optional playback-progress overlay.
- Text labels and label-specific effects.

Text display modes are configured through `text.display_mode`:

- `all-segments`: show labels for all segments.
- `active-only`: show only the current segment label.
- `past-current`: show labels up to the current segment.
- `none`: render only the visual bar and dividers.

Segment splitting is fixed to duration-proportional layout. There is no segment split mode setting in the first version. Equal-width splitting is intentionally out of scope because the overlay should match the source video's real time.

### Playback Progress Overlay

The optional playback-progress overlay behaves like the progress indicator commonly seen in video players. It shows the current timestamp's absolute position within the full overlay duration.

Configuration controls:

- `enabled`: turn the overlay on or off.
- `height`: progress overlay thickness.
- `offset_y`: vertical offset relative to the segmented progress bar centerline.
- `color`: elapsed progress color.
- `track_color`: optional remaining-track color.
- `thumb_enabled`, `thumb_radius`, and `thumb_color`: optional current-time handle.

This overlay is visual-only. It does not change segment boundaries, segment labels, or output duration.

## Long Text Handling

Every label receives a layout rectangle. Text rendering must be clipped to that rectangle before drawing.

Overflow strategies:

- `shrink`: reduce font size down to a configured minimum.
- `ellipsis`: truncate with an ellipsis.
- `rotate`: rotate text 90 degrees when the segment is narrow but bar height can fit the vertical label.
- `scroll`: horizontally scroll text inside its clipped region over the segment's own time range.
- `auto`: choose a strategy in this order: normal, shrink down to `min_font_size`, ellipsis for mildly long labels, rotate for narrow tall-enough cells, scroll for labels that still cannot fit.

When `overflow = "auto"`, scrolling is only allowed after the font reaches `min_font_size`. The resolved configuration must include `min_font_size`; otherwise validation fails before rendering. There is no `scroll_speed_px_per_sec` setting. Scroll offset is derived from the current timestamp, segment start time, segment end time, measured text width, and label rectangle width so previewing and rendering the same time produces the same frame.

## Output Formats

Initial supported outputs:

- `png-sequence`: highest reliability, alpha-preserving, large file size.
- `apng`: lossless animation with alpha for shorter overlays or compatible workflows.
- `ffv1-mkv`: mathematically lossless video with alpha support when FFmpeg build supports the selected pixel format.
- `prores4444-mov`: editing-friendly alpha output. Treat as high-quality intermediate rather than strict mathematical lossless.

The config should make output profiles explicit instead of hiding codec decisions:

```toml
[output]
format = "ffv1-mkv"
path = "out/progress.mkv"
```

Future output profiles can include WebM VP9 alpha or HEVC alpha where platform support is acceptable.

## Error Handling

Errors should be structured and user-actionable.

Examples:

- `ConfigParseError`: invalid TOML syntax.
- `ConfigValidationError`: width, height, fps, margins, or colors are invalid.
- `SegmentParseError`: line number plus invalid time or missing label.
- `TimelineError`: end times are not strictly increasing.
- `FontError`: requested font cannot be loaded and no fallback is available.
- `EncoderError`: FFmpeg missing, codec unsupported, or output path not writable.

GUI should display concise messages and optional details. CLI should print the same structured error in terminal-friendly form.

## Testing Strategy

Unit tests:

- Segment parsing and time normalization.
- Overlay duration derived from the final segment end time.
- Config default merging and validation.
- `overflow = "auto"` validation requires a resolved `min_font_size` before scroll fallback can be selected.
- Layout calculations for different segment densities.
- Text overflow strategy selection.

Snapshot or image tests:

- Render a deterministic preview frame and compare dimensions, alpha, and selected pixel regions.
- Verify transparent background pixels remain transparent.
- Verify playback-progress overlay pixels appear above the segmented bar and below text.
- Verify labels stay clipped within their segment rectangles.

Integration tests:

- Render a tiny 2-second PNG sequence.
- Verify rendered frame count matches `final_segment_end_time * fps`.
- Render a tiny APNG after the encoder milestone adds APNG support.
- Validate FFmpeg command construction without requiring a full long render.

Manual QA:

- 1920x1080, 60 fps, transparent overlay.
- Long Chinese labels.
- Many short segments.
- Mixed Latin and Chinese labels.
- Playback-progress overlay enabled and disabled.
- Very small and very large bar heights.

## Milestones

### Milestone 1: Core Spec, Playback Overlay, and Preview Frame

Build schema, segment parser, duration-proportional layout engine, optional playback-progress overlay, and one-frame transparent PNG preview.

Success criteria:

- A config and segment file can produce a transparent PNG preview.
- Default values are centralized.
- Invalid segment files produce line-specific errors.
- Segment widths are proportional to duration.
- Playback-progress overlay renders at the correct timestamp layer when enabled.

### Milestone 2: Full PNG Sequence Rendering

Render all frames as a PNG sequence without loading all frames into memory.

Success criteria:

- Progress events report frame count and percentage.
- Total frame count is derived from the final segment end time and fps.
- Output frames preserve alpha.
- Long videos stream frame-by-frame.

### Milestone 3: Text Overflow Modes

Add shrink, ellipsis, rotate, scroll, and auto text strategies.

Success criteria:

- Labels do not overlap neighboring segments.
- Labels do not overflow the frame.
- Auto overflow can only fall back to scrolling after shrinking to `min_font_size`.
- Scroll position is deterministic by timestamp and segment duration.

### Milestone 4: Encoder Profiles

Add APNG and FFmpeg-backed output profiles.

Success criteria:

- APNG preserves alpha and frame timing.
- FFV1 or ProRes 4444 export works through FFmpeg.
- Encoder errors are structured.

### Milestone 5: Tauri Desktop GUI

Create a lightweight desktop app over the same Rust API.

Success criteria:

- Users can pick files, edit config, preview frames, render output, and watch progress.
- GUI calls Rust commands directly.
- CLI remains optional and does not mediate GUI behavior.

## Non-Goals for First Version

- Full video editor timeline.
- Importing source video and automatically compositing final video.
- Cloud rendering.
- Plugin system.
- Complex animation editor beyond progress and text behavior.

These can be added later if the overlay generator proves stable.

## Fixed Implementation Choices

These choices are part of the first implementation plan:

- Renderer stack starts with `tiny-skia` plus `cosmic-text`, guarded by an early spike and test fixture.
- PNG sequence ships before APNG because it is the most reliable alpha-preserving baseline.
- APNG ships in the encoder milestone, not in the first preview milestone.
- The first GUI uses Svelte + Vite inside Tauri 2.
- FFmpeg is discovered from the system `PATH` first, with a configurable explicit binary path. Bundled FFmpeg is deferred so the first desktop package stays small.
