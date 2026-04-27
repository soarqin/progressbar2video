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
        let path = config
            .output
            .path
            .join(format!("frame_{frame_index:06}.png"));
        let file = fs::File::create(path).map_err(EncodeError::CreateFrame)?;
        write_png(&frame, file).map_err(|error| EncodeError::Render(error.to_string()))?;
        on_progress(RenderProgress {
            completed_frames: frame_index + 1,
            total_frames,
        });
    }

    Ok(())
}

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
        assert_eq!(seen.last().unwrap().total_frames, 4);
    }
}
