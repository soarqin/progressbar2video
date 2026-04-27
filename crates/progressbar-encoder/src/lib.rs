use crc32fast::Hasher;
use flate2::{write::ZlibEncoder, Compression};
use progressbar_core::{frame_count, frame_timestamp_ms, Timeline};
use progressbar_renderer::{render_frame, write_png, RenderedFrame};
use progressbar_schema::ProjectConfig;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
    #[error("output format is not an FFmpeg-backed profile")]
    UnsupportedFfmpegFormat,
    #[error("failed to start ffmpeg `{program}`: {source}")]
    FfmpegSpawn {
        program: String,
        source: std::io::Error,
    },
    #[error("failed to write raw frame data to ffmpeg: {0}")]
    FfmpegStdin(std::io::Error),
    #[error("ffmpeg exited unsuccessfully with status {status}")]
    FfmpegExit { status: String },
    #[error("APNG frame rate {fps} is too high for APNG frame delay")]
    UnsupportedApngFps { fps: u32 },
    #[error("APNG frame count {total_frames} exceeds the format limit")]
    TooManyApngFrames { total_frames: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegCommandPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

pub fn render_overlay<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    match config.output.format {
        progressbar_schema::OutputFormat::PngSequence => {
            render_png_sequence(config, timeline, on_progress)
        }
        progressbar_schema::OutputFormat::Apng => render_apng(config, timeline, on_progress),
        progressbar_schema::OutputFormat::Ffv1Mkv
        | progressbar_schema::OutputFormat::Prores4444Mov => {
            render_ffmpeg(config, timeline, on_progress)
        }
    }
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
    let total_frames_u32 =
        u32::try_from(total_frames).map_err(|_| EncodeError::TooManyApngFrames { total_frames })?;
    let delay_den =
        u16::try_from(config.render.fps).map_err(|_| EncodeError::UnsupportedApngFps {
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

pub fn ffmpeg_command_plan(
    config: &ProjectConfig,
    total_frames: u64,
) -> Result<FfmpegCommandPlan, EncodeError> {
    let program = config
        .output
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut args = vec![
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", config.render.width, config.render.height),
        "-r".to_string(),
        config.render.fps.to_string(),
        "-i".to_string(),
        "pipe:0".to_string(),
        "-frames:v".to_string(),
        total_frames.to_string(),
    ];

    match config.output.format {
        progressbar_schema::OutputFormat::Ffv1Mkv => {
            args.extend(["-c:v", "ffv1", "-level", "3", "-pix_fmt", "bgra"].map(String::from));
        }
        progressbar_schema::OutputFormat::Prores4444Mov => {
            args.extend(
                [
                    "-c:v",
                    "prores_ks",
                    "-profile:v",
                    "4444",
                    "-pix_fmt",
                    "yuva444p10le",
                ]
                .map(String::from),
            );
        }
        _ => return Err(EncodeError::UnsupportedFfmpegFormat),
    }

    args.push(config.output.path.to_string_lossy().to_string());
    Ok(FfmpegCommandPlan { program, args })
}

pub fn render_ffmpeg<F>(
    config: &ProjectConfig,
    timeline: &Timeline,
    mut on_progress: F,
) -> Result<(), EncodeError>
where
    F: FnMut(RenderProgress),
{
    let total_frames = frame_count(timeline.duration_ms(), config.render.fps);
    if let Some(parent) = config.output.path.parent() {
        fs::create_dir_all(parent).map_err(EncodeError::CreateDir)?;
    }
    let plan = ffmpeg_command_plan(config, total_frames)?;
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| EncodeError::FfmpegSpawn {
            program: plan.program.to_string_lossy().to_string(),
            source,
        })?;

    {
        let stdin = child.stdin.as_mut().expect("stdin was configured as piped");
        for frame_index in 0..total_frames {
            let timestamp_ms = frame_timestamp_ms(frame_index, config.render.fps);
            let frame = render_frame(config, timeline, timestamp_ms)
                .map_err(|error| EncodeError::Render(error.to_string()))?;
            stdin
                .write_all(&frame.rgba)
                .map_err(EncodeError::FfmpegStdin)?;
            on_progress(RenderProgress {
                completed_frames: frame_index + 1,
                total_frames,
            });
        }
    }

    let status = child.wait().map_err(EncodeError::FfmpegStdin)?;
    if status.success() {
        Ok(())
    } else {
        Err(EncodeError::FfmpegExit {
            status: status.to_string(),
        })
    }
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
    use std::path::PathBuf;

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

    #[test]
    fn builds_ffv1_command_with_alpha_pixel_format() {
        let mut config = ProjectConfig::default();
        config.render.width = 320;
        config.render.height = 180;
        config.render.fps = 30;
        config.output.format = progressbar_schema::OutputFormat::Ffv1Mkv;
        config.output.path = PathBuf::from("out/progress.mkv");
        let plan = ffmpeg_command_plan(&config, 60).unwrap();
        assert_eq!(plan.program, PathBuf::from("ffmpeg"));
        assert!(plan.args.contains(&"-f".to_string()));
        assert!(plan.args.contains(&"rawvideo".to_string()));
        assert!(plan.args.contains(&"rgba".to_string()));
        assert!(plan.args.contains(&"ffv1".to_string()));
        assert!(plan.args.contains(&"bgra".to_string()));
        assert_eq!(plan.args.last().unwrap(), "out/progress.mkv");
    }

    #[test]
    fn builds_prores_command_with_explicit_ffmpeg_path() {
        let mut config = ProjectConfig::default();
        config.output.format = progressbar_schema::OutputFormat::Prores4444Mov;
        config.output.path = PathBuf::from("out/progress.mov");
        config.output.ffmpeg_path = Some(PathBuf::from("tools/ffmpeg.exe"));
        let plan = ffmpeg_command_plan(&config, 12).unwrap();
        assert_eq!(plan.program, PathBuf::from("tools/ffmpeg.exe"));
        assert!(plan.args.contains(&"prores_ks".to_string()));
        assert!(plan.args.contains(&"4444".to_string()));
        assert!(plan.args.contains(&"yuva444p10le".to_string()));
        assert_eq!(plan.args.last().unwrap(), "out/progress.mov");
    }

    #[test]
    fn render_overlay_dispatches_apng_profile() {
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
        render_overlay(&config, &timeline, |_| {}).unwrap();
        let bytes = std::fs::read(&config.output.path).unwrap();
        assert!(png_chunk_names(&bytes).contains(&"acTL".to_string()));
    }
}
