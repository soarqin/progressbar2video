use clap::{Parser, Subcommand};
use progressbar_api::{
    preview_frame, render_overlay, validate_project, PreviewFrameRequest, RenderOverlayRequest,
};
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
            render_overlay(
                RenderOverlayRequest {
                    config_path: config,
                    segments_path: segments,
                },
                |progress| {
                    println!(
                        "Rendered {}/{} frames",
                        progress.completed_frames, progress.total_frames
                    );
                },
            )?;
        }
    }
    Ok(())
}
