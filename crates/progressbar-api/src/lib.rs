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

#[derive(Debug, Clone)]
pub struct RenderOverlayRequest {
    pub config_path: PathBuf,
    pub segments_path: PathBuf,
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

pub fn render_overlay<F>(request: RenderOverlayRequest, on_progress: F) -> Result<(), ApiError>
where
    F: FnMut(progressbar_encoder::RenderProgress),
{
    let (config, timeline) = load_project(request.config_path, request.segments_path)?;
    progressbar_encoder::render_overlay(&config, &timeline, on_progress)
        .map_err(|error| ApiError::Render(error.to_string()))
}

fn load_project(
    config_path: PathBuf,
    segments_path: PathBuf,
) -> Result<(ProjectConfig, Timeline), ApiError> {
    let config_text = fs::read_to_string(&config_path).map_err(|source| ApiError::ReadFile {
        path: config_path.clone(),
        source,
    })?;
    let segments_text =
        fs::read_to_string(&segments_path).map_err(|source| ApiError::ReadFile {
            path: segments_path.clone(),
            source,
        })?;
    let config = ProjectConfig::from_toml_str(&config_text)
        .map_err(|error| ApiError::Config(error.to_string()))?;
    let timeline =
        Timeline::parse(&segments_text).map_err(|error| ApiError::Segment(error.to_string()))?;
    Ok((config, timeline))
}

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
