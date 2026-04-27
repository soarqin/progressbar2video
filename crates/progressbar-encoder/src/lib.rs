use crc32fast::Hasher;
use flate2::{write::ZlibEncoder, Compression};
use progressbar_core::{frame_count, frame_timestamp_ms, Timeline};
use progressbar_renderer::{render_frame, write_png, RenderedFrame};
use progressbar_schema::ProjectConfig;
use std::fs;
use std::io::Write;
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
    #[error("failed to create output file: {0}")]
    CreateOutput(std::io::Error),
    #[error("failed to write output file: {0}")]
    WriteOutput(std::io::Error),
    #[error("failed to create frame file: {0}")]
    CreateFrame(std::io::Error),
    #[error("render error: {0}")]
    Render(String),
    #[error("APNG frame rate {fps} is too high for APNG frame delay")]
    UnsupportedApngFps { fps: u32 },
    #[error("APNG frame count {total_frames} exceeds the format limit")]
    TooManyApngFrames { total_frames: u64 },
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

pub fn render_apng<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    let total_frames = frame_count(timeline.duration_ms(), config.render.fps);
    let total_frames_u32 = u32::try_from(total_frames)
        .map_err(|_| EncodeError::TooManyApngFrames { total_frames })?;
    let delay_den = u16::try_from(config.render.fps)
        .map_err(|_| EncodeError::UnsupportedApngFps {
            fps: config.render.fps,
        })?;

    if let Some(parent) = config.output.path.parent() {
        fs::create_dir_all(parent).map_err(EncodeError::CreateDir)?;
    }
    let file = fs::File::create(&config.output.path).map_err(EncodeError::CreateOutput)?;
    write_apng(
        config,
        timeline,
        file,
        total_frames,
        total_frames_u32,
        delay_den,
        &mut on_progress,
    )
}

fn write_apng<W, F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut writer: W,
    total_frames: u64,
    total_frames_u32: u32,
    delay_den: u16,
    on_progress: &mut F,
) -> Result<(), EncodeError>
where
    W: Write,
    F: FnMut(RenderProgress),
{
    writer
        .write_all(b"\x89PNG\r\n\x1a\n")
        .map_err(EncodeError::WriteOutput)?;

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&config.render.width.to_be_bytes());
    ihdr.extend_from_slice(&config.render.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_png_chunk(&mut writer, b"IHDR", &ihdr)?;

    let mut actl = Vec::with_capacity(8);
    actl.extend_from_slice(&total_frames_u32.to_be_bytes());
    actl.extend_from_slice(&0_u32.to_be_bytes());
    write_png_chunk(&mut writer, b"acTL", &actl)?;

    let mut sequence_number = 0_u32;
    for frame_index in 0..total_frames {
        let timestamp_ms = frame_timestamp_ms(frame_index, config.render.fps);
        let frame = render_frame(config, timeline, timestamp_ms)
            .map_err(|error| EncodeError::Render(error.to_string()))?;
        let fctl = apng_frame_control(&frame, sequence_number, delay_den);
        sequence_number += 1;
        write_png_chunk(&mut writer, b"fcTL", &fctl)?;

        let compressed = compress_rgba_frame(&frame)?;
        if frame_index == 0 {
            write_png_chunk(&mut writer, b"IDAT", &compressed)?;
        } else {
            let mut fdat = Vec::with_capacity(compressed.len() + 4);
            fdat.extend_from_slice(&sequence_number.to_be_bytes());
            sequence_number += 1;
            fdat.extend_from_slice(&compressed);
            write_png_chunk(&mut writer, b"fdAT", &fdat)?;
        }

        on_progress(RenderProgress {
            completed_frames: frame_index + 1,
            total_frames,
        });
    }

    write_png_chunk(&mut writer, b"IEND", &[])?;
    Ok(())
}

fn apng_frame_control(frame: &RenderedFrame, sequence_number: u32, delay_den: u16) -> Vec<u8> {
    let mut data = Vec::with_capacity(26);
    data.extend_from_slice(&sequence_number.to_be_bytes());
    data.extend_from_slice(&frame.width.to_be_bytes());
    data.extend_from_slice(&frame.height.to_be_bytes());
    data.extend_from_slice(&0_u32.to_be_bytes());
    data.extend_from_slice(&0_u32.to_be_bytes());
    data.extend_from_slice(&1_u16.to_be_bytes());
    data.extend_from_slice(&delay_den.to_be_bytes());
    data.push(0);
    data.push(0);
    data
}

fn compress_rgba_frame(frame: &RenderedFrame) -> Result<Vec<u8>, EncodeError> {
    let stride = frame.width as usize * 4;
    let mut filtered = Vec::with_capacity(frame.rgba.len() + frame.height as usize);
    for row in frame.rgba.chunks_exact(stride) {
        filtered.push(0);
        filtered.extend_from_slice(row);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&filtered)
        .map_err(EncodeError::WriteOutput)?;
    encoder.finish().map_err(EncodeError::WriteOutput)
}

fn write_png_chunk<W: Write>(
    writer: &mut W,
    chunk_type: &[u8; 4],
    data: &[u8],
) -> Result<(), EncodeError> {
    writer
        .write_all(&(data.len() as u32).to_be_bytes())
        .map_err(EncodeError::WriteOutput)?;
    writer
        .write_all(chunk_type)
        .map_err(EncodeError::WriteOutput)?;
    writer.write_all(data).map_err(EncodeError::WriteOutput)?;

    let mut hasher = Hasher::new();
    hasher.update(chunk_type);
    hasher.update(data);
    writer
        .write_all(&hasher.finalize().to_be_bytes())
        .map_err(EncodeError::WriteOutput)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressbar_core::Timeline;
    use progressbar_schema::ProjectConfig;

    fn png_chunk_names(bytes: &[u8]) -> Vec<String> {
        let mut names = Vec::new();
        let mut offset = 8;
        while offset + 12 <= bytes.len() {
            let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            let name = std::str::from_utf8(&bytes[offset + 4..offset + 8])
                .unwrap()
                .to_string();
            names.push(name);
            offset += 12 + length;
        }
        names
    }

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

    #[test]
    fn writes_apng_with_animation_chunks_and_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::default();
        config.render.width = 96;
        config.render.height = 54;
        config.render.fps = 2;
        config.bar.margin_x = 8;
        config.bar.margin_bottom = 6;
        config.bar.height = 16;
        config.output.format = progressbar_schema::OutputFormat::Apng;
        config.output.path = dir.path().join("progress.apng");
        let timeline = Timeline::parse("1 | A").unwrap();
        let mut seen = Vec::new();

        render_apng(&config, &timeline, |event| seen.push(event)).unwrap();

        let bytes = std::fs::read(&config.output.path).unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let chunks = png_chunk_names(&bytes);
        assert!(chunks.contains(&"acTL".to_string()));
        assert_eq!(chunks.iter().filter(|name| *name == "fcTL").count(), 2);
        assert!(chunks.contains(&"IDAT".to_string()));
        assert!(chunks.contains(&"fdAT".to_string()));
        assert_eq!(seen.last().unwrap().completed_frames, 2);
        assert_eq!(seen.last().unwrap().total_frames, 2);
    }
}
