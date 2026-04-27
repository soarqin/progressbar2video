# Agent Guide

This repository builds transparent progress-bar overlay assets for video
editing. Keep output alpha-friendly, deterministic, and suitable for use as
source material in an editor unless a task explicitly changes that behavior.

## Project Shape

- `Cargo.toml` defines the Rust workspace.
- `crates/progressbar-schema` owns config and serializable data structures.
- `crates/progressbar-core` owns timeline/config validation and frame planning.
- `crates/progressbar-renderer` owns pixel rendering and the cached
  `FrameRenderer` session API.
- `crates/progressbar-encoder` owns APNG, PNG sequence, FFV1 MKV, and ProRes
  4444 MOV output profiles.
- `crates/progressbar-api` is the stable programmatic entry point for future
  GUI/WebView integration. Prefer this API over driving behavior through CLI
  input/output when building app integrations.
- `apps/cli` owns the command-line interface.

## Development Notes

- Treat generated overlays as transparent source assets. Do not add opaque
  backgrounds unless the config requests it.
- Keep configuration flexible and avoid hard-coded dimensions, colors, paths,
  font choices, frame rates, or output profiles where a config field fits.
- Keep config changes backward compatible where practical.
- Render duration is based on the last segment end time; segments are cut by
  time, not by scroll speed.
- When `overflow = "auto"` can fall back to scrolling, preserve the configured
  minimum font size behavior.
- The optional playback-style progress overlay should render above the base
  progress bar and below text.
- Use `FrameRenderer` for frame loops so fonts, layout buffers, and static
  geometry are reused.
- Keep examples small and put generated example output under
  `examples/**/out/`.

## Common Commands

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo build --release
cargo run -p progressbar-cli -- validate --config examples/basic/config.toml --segments examples/basic/segments.txt
cargo run -p progressbar-cli -- preview-frame --config examples/basic/config.toml --segments examples/basic/segments.txt --output examples/basic/out/preview.png --timestamp-ms 1000
```

## Git Hygiene

- Stage explicit paths for commits.
- Do not commit generated files from `target/`, `.worktrees/`, `out/`, or
  `examples/**/out/`.
- Preserve user changes you did not make.
