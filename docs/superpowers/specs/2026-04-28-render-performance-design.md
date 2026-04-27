# Render Performance Design

## Goal

Improve render throughput without changing visible output, config format, or the shared API surface that CLI and future GUI use.

## Current Bottlenecks

The current render path calls `render_frame(config, timeline, timestamp)` independently for every frame. Each call recalculates layout, reparses colors, redraws the static bar, creates a new `FontSystem` and `SwashCache`, reshapes every label, and then returns a full RGBA buffer. Encoder profiles repeat that same work for PNG sequence, APNG, FFV1, and ProRes.

Release-mode rough baseline on the current examples:

- `examples/basic` PNG sequence: about 6.4 seconds for 60 frames.
- `examples/encoder-profiles/apng.toml`: about 1.7 seconds for 20 frames.

## Design

Add a reusable `FrameRenderer` session in `progressbar-renderer`.

`FrameRenderer::new(config, timeline)` will:

- Clone the config and timeline for a stable render session.
- Calculate `Layout` once.
- Pre-render the static bar layer once.
- Reuse one `FontSystem` and one `SwashCache`.
- Pre-plan segment labels once.
- Pre-render non-scrolling `all-segments` labels into a transparent text layer.

`FrameRenderer::render_frame(timestamp_ms)` will:

- Start from the cached static bar RGBA buffer.
- Draw timestamp-dependent playback-progress overlay.
- Blend the cached static text layer above playback-progress.
- Draw dynamic labels only when needed, including scrolling labels and timestamp-dependent display modes.

The existing free function `render_frame(config, timeline, timestamp)` remains available and internally creates a one-shot `FrameRenderer` for compatibility.

## Encoder Integration

`progressbar-encoder` will create one `FrameRenderer` per render job and reuse it inside every frame loop:

- `render_png_sequence`
- `render_apng`
- `render_ffmpeg`

This keeps the optimization shared by every output profile and avoids making the CLI or GUI aware of renderer internals.

## Correctness

Tests will verify that:

- The session renderer produces the same RGBA output as the compatibility path for representative frames.
- Static-only frames remain identical across timestamps.
- Encoder outputs still produce the expected frame counts and APNG chunks.

Manual verification will compare release-mode timings before and after the change on the same example commands.

## Scope

This phase does not add parallel rendering or change PNG compression settings. Those are separate knobs with different trade-offs around disk throughput, CPU pressure, and output determinism.
