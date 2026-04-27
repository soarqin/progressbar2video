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

Preview long text overflow handling:

```powershell
cargo run -p progressbar-cli -- preview-frame --config examples/long-text/config.toml --segments examples/long-text/segments.txt --output examples/long-text/out/preview.png --timestamp-ms 1500
```
