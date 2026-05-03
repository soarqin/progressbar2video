use clap::{Parser, Subcommand};
use progressbar_api::{
    preview_frame, render_overlay, validate_project, PreviewFrameRequest, RenderOverlayRequest,
};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "progressbar2video")]
#[command(about = "Generate transparent progress-bar overlay assets for video editing.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
    },
    PreviewFrame {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 0)]
        timestamp_ms: u64,
    },
    Render {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        segments: PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), progressbar_api::ApiError> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { config, segments } => {
            validate_project(config, segments)?;
            println!("Project is valid.");
        }
        Command::PreviewFrame {
            config,
            segments,
            output,
            timestamp_ms,
        } => {
            preview_frame(PreviewFrameRequest {
                config_path: config,
                segments_path: segments,
                output_path: output,
                timestamp_ms,
            })?;
            println!("Preview frame written.");
        }
        Command::Render { config, segments } => {
            // Throttle progress prints to roughly one update per second of
            // rendered video time and use `\r` so the line is rewritten
            // in-place instead of producing a per-frame log spam. The final
            // frame always prints (followed by a newline) so the terminal
            // is left clean.
            let mut last_printed_frame = 0u64;
            render_overlay(
                RenderOverlayRequest {
                    config_path: config,
                    segments_path: segments,
                },
                |progress| {
                    let interval = (progress.fps as u64).max(1);
                    let is_final = progress.completed_frames == progress.total_frames;
                    let should_print = is_final
                        || progress.completed_frames == 1
                        || progress.completed_frames - last_printed_frame >= interval;
                    if !should_print {
                        return;
                    }
                    last_printed_frame = progress.completed_frames;
                    let percent = if progress.total_frames > 0 {
                        progress.completed_frames as f64 / progress.total_frames as f64 * 100.0
                    } else {
                        0.0
                    };
                    let mut stdout = std::io::stdout().lock();
                    let _ = write!(
                        stdout,
                        "\rRendered {}/{} frames ({:.1}%)",
                        progress.completed_frames, progress.total_frames, percent,
                    );
                    if is_final {
                        let _ = writeln!(stdout);
                    }
                    let _ = stdout.flush();
                },
            )?;
        }
    }
    Ok(())
}
