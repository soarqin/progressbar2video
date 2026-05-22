# ProgressBar2Video

ProgressBar2Video generates transparent progress-bar overlay assets for video editing.

The MVP reads a TOML config and a segment text file, then writes a transparent preview PNG or a PNG sequence whose duration matches the final segment end time.

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

Output profiles:

- `png-sequence`: directory of transparent PNG frames.
- `apng`: single transparent animated PNG, best for short overlays.
- `ffv1-mkv`: FFmpeg-backed mathematically lossless alpha video.
- `prores4444-mov`: FFmpeg-backed editing intermediate with alpha.

For long overlays, enable the alpha strip mode on `apng`, `ffv1-mkv`, or
`prores4444-mov` to output only the transparent progress-bar band instead of a
full-frame canvas:

```toml
[output]
format = "prores4444-mov"
path = "out/progress-strip.mov"

[output.strip]
enabled = true
padding_top = 16
padding_bottom = 16
```

APNG outputs also write dirty-rectangle animation frames, so unchanged transparent
or static areas are not repeated in every APNG frame.

Render APNG:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/apng.toml --segments examples/encoder-profiles/segments.txt
```

Render FFV1 MKV:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/ffv1.toml --segments examples/encoder-profiles/segments.txt
```

Render ProRes 4444 MOV:

```powershell
cargo run -p progressbar-cli -- render --config examples/encoder-profiles/prores4444.toml --segments examples/encoder-profiles/segments.txt
```

Preview long text overflow handling:

```powershell
cargo run -p progressbar-cli -- preview-frame --config examples/long-text/config.toml --segments examples/long-text/segments.txt --output examples/long-text/out/preview.png --timestamp-ms 1500
```
