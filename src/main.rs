use clap::Parser;
use eyre::bail;
use mkv_subtitle_extractor::extract_subtitle_track;
use mkv_subtitle_extractor::pick_mkv_file;
use mkv_subtitle_extractor::pick_subtitle_tracks;
use std::path::PathBuf;
use tokio::fs;
use tracing::info;
use tracing::Level;
use tracing_subscriber::EnvFilter;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(version, about = "Extract subtitles from MKV files")]
struct Args {
    /// If set, enable debug logging
    #[arg(long)]
    debug: bool,

    /// Path to MKV to extract from
    #[arg(long)]
    file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();

    // Setup logging
    let log_level = if args.debug {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let env_filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
    color_eyre::install()?;

    info!("Ahoy!");

    // Get file path
    let file_path = match args.file {
        Some(x) => x,
        None => pick_mkv_file().await?,
    };

    info!("Extracting subtitles from {}", file_path.display());
    if !fs::try_exists(&file_path).await? {
        bail!("File does not exist: {}", file_path.display());
    }

    // Enumerate subtitle tracks
    let tracks = pick_subtitle_tracks(&file_path).await?;

    // Write subtitle tracks
    for track in tracks {
        extract_subtitle_track(&file_path, &track).await?;
    }

    Ok(())
}
