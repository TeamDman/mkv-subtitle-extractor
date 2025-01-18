#![feature(try_blocks)]

use clap::Parser;
use cloud_terrastodon_core_user_input::prelude::pick;
use cloud_terrastodon_core_user_input::prelude::pick_many;
use cloud_terrastodon_core_user_input::prelude::Choice;
use cloud_terrastodon_core_user_input::prelude::FzfArgs;
use eyre::bail;
use eyre::eyre;
use itertools::Itertools;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use tokio::process::Command;
use tracing::debug;
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

/// Prompt user (via FZF) to pick an MKV file in current directory
async fn pick_mkv_file() -> eyre::Result<PathBuf> {
    let mkv_files = gather_mkv_files().await?;
    info!("Found {} mkv files", mkv_files.len());
    if mkv_files.is_empty() {
        bail!("No MKV files found in current directory");
    }
    let chosen = pick(FzfArgs {
        choices: mkv_files
            .into_iter()
            .map(|x| Choice {
                key: x.display().to_string(),
                value: x,
            })
            .collect_vec(),
        header: Some("Choose an MKV file to extract subtitles from".to_string()),
        prompt: None,
    })
    .map_err(|e| eyre!(e))?
    .value;
    info!("You chose: {}", chosen.display());
    Ok(chosen)
}

/// Gather all MKV files in the current directory
async fn gather_mkv_files() -> eyre::Result<Vec<PathBuf>> {
    let cwd = PathBuf::from("./");
    let mut entries = fs::read_dir(cwd).await?;
    let mut candidates = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        if let Some(ext) = PathBuf::from(&file_name).extension() {
            if ext.eq_ignore_ascii_case("mkv") {
                candidates.push(PathBuf::from(file_name));
            }
        }
    }
    Ok(candidates)
}

/// A struct describing each found subtitle track
#[derive(Debug)]
struct SubtitleTrack {
    /// This is the "N" in `Stream #0:N` (the real ffmpeg index).
    stream_index: u32,

    /// We parse the language from `(eng)` or similar if present.
    lang: Option<String>,

    /// The recognized format, e.g. `subrip`, `ass`, `hdmv_pgs_subtitle`.
    format: String,

    /// A "title" if found in subsequent metadata lines, e.g. "English subs".
    title: Option<String>,
}

impl std::fmt::Display for SubtitleTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lang_part = if let Some(lang) = &self.lang {
            format!("({lang}) ")
        } else {
            "".to_string()
        };
        let title_part = if let Some(t) = &self.title {
            format!(" \"{t}\"")
        } else {
            "".to_string()
        };
        write!(
            f,
            "Stream #0:{} {}{:?}{}",
            self.stream_index, lang_part, self.format, title_part
        )
    }
}

/// Let the user pick which subtitle tracks to extract
async fn pick_subtitle_tracks(path: &Path) -> eyre::Result<Vec<SubtitleTrack>> {
    let tracks = enumerate_subtitle_tracks(path).await?;
    info!("Found {} subtitle tracks", tracks.len());
    if tracks.is_empty() {
        bail!("No subtitle tracks found in {}", path.display());
    }
    let tracks = pick_many(FzfArgs {
        choices: tracks,
        header: Some("Select subtitle tracks to extract".to_string()),
        prompt: None,
    })
    .map_err(|e| eyre!(e))?;

    info!("You chose: {:#?}", tracks);
    Ok(tracks)
}

/// Detect the subtitle format -> extension
fn extension_for_format(fmt: &str) -> &str {
    match fmt {
        "subrip" => "srt",
        "ass" => "ass",
        "hdmv_pgs_subtitle" | "pgssub" => "sup", // PGS typically .sup
        other => {
            // fallback
            debug!("Unknown subtitle format: {} => .sub", other);
            "sub"
        }
    }
}

/// Parse the output of `ffmpeg -i` and build a list of subtitles with metadata
async fn enumerate_subtitle_tracks(path: &Path) -> eyre::Result<Vec<SubtitleTrack>> {
    info!("Enumerating subtitle tracks");
    debug!("Running command `ffmpeg -i {}`", path.display());

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(path);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let output = cmd.output().await?;
    // ffmpeg -i fails (exit code != 0) because no output file is specified, but we only want the console output
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;

    debug!("stdout: {}", stdout);
    debug!("stderr: {}", stderr);

    let lines: Vec<&str> = stderr.lines().collect();

    let mut result = Vec::new();
    let mut current: Option<SubtitleTrack> = None;

    // Iterate over each line to find subtitle streams
    for line in lines.iter() {
        // Check if the line contains a subtitle stream
        if line.trim_start().starts_with("Stream #") && line.contains("Subtitle") {
            // Finalize the previous track if it exists
            if let Some(t) = current.take() {
                result.push(t);
            }

            // Parse the subtitle stream line
            // Example:
            // Stream #0:2(eng): Subtitle: subrip (default)
            // Stream #0:3: Subtitle: hdmv_pgs_subtitle, 1920x1080
            let line_trim = line.trim_start();
            let after_stream = &line_trim["Stream #".len()..];

            // Split at the first colon to separate stream info and stream details
            let colon_pos = after_stream
                .find(':')
                .ok_or_else(|| eyre!("Invalid stream line: {}", line))?;
            let (stream_portion, after_colon) = after_stream.split_at(colon_pos);
            let after_colon = after_colon[1..].trim(); // Remove the colon

            // Extract stream index and language (if any)
            let (numeric_part, lang) = if let Some(par_open) = stream_portion.find('(') {
                let after_paren = &stream_portion[par_open + 1..];
                let cl_par = after_paren
                    .find(')')
                    .ok_or_else(|| eyre!("Unclosed parenthesis in stream line: {}", line))?;
                let inside = &after_paren[..cl_par];
                let lang = Some(inside.to_string());

                // Extract the numeric part before the parenthesis
                let numeric_str = &stream_portion[..par_open];
                let colon_idx = numeric_str
                    .rfind(':')
                    .ok_or_else(|| eyre!("No colon found in stream portion: {}", numeric_str))?;
                let maybe_num = &numeric_str[colon_idx + 1..];
                let num = maybe_num.parse::<u32>()?;
                (Some(num), lang)
            } else {
                // No language specified
                let colon_idx = stream_portion
                    .rfind(':')
                    .ok_or_else(|| eyre!("No colon found in stream portion: {}", stream_portion))?;
                let maybe_num = &stream_portion[colon_idx + 1..];
                let num = maybe_num.parse::<u32>()?;
                (Some(num), None)
            };

            // Extract the subtitle format
            // Example after_colon: "Subtitle: subrip (default)"
            let subtitle_pos = after_colon
                .find("Subtitle:")
                .ok_or_else(|| eyre!("No 'Subtitle:' found in stream details: {}", after_colon))?;
            let after_subtitle = after_colon[subtitle_pos + "Subtitle:".len()..].trim();
            let format_str = after_subtitle
                .split(|c: char| c.is_whitespace() || c == ',' || c == '(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            // Initialize a new SubtitleTrack
            let track = SubtitleTrack {
                stream_index: numeric_part.unwrap_or(0),
                lang,
                format: format_str,
                title: None,
            };
            current = Some(track);
        } else {
            // Check for metadata lines related to the current subtitle track
            if let Some(current_track) = current.as_mut() {
                if line.trim_start().starts_with("title") {
                    // Example: title           : English subs
                    if let Some(colon_pos) = line.find(':') {
                        let after_colon = &line[colon_pos + 1..].trim();
                        if !after_colon.is_empty() {
                            current_track.title = Some(after_colon.to_string());
                        }
                    }
                }
            }
        }
    }

    // Finalize the last track if it exists
    if let Some(t) = current.take() {
        result.push(t);
    }

    Ok(result)
}

/// Actually run ffmpeg to copy a track to a new file with the correct extension
async fn extract_subtitle_track(
    path: &Path,
    track: &SubtitleTrack,
) -> eyre::Result<Option<PathBuf>> {
    info!("Extracting subtitle track: {}", track);

    // Determine the file extension based on subtitle format
    let ext = extension_for_format(&track.format);

    // Build the output file name
    // Example: "Blade Runner 2049.2.eng.srt" or "Jujutsu Kaisen.2.ass"
    let base_stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut fname = format!("{}.{}", base_stem, track.stream_index);
    if let Some(lang) = &track.lang {
        fname.push_str(&format!(".{}", lang));
    }
    if let Some(t) = &track.title {
        let sanitized = sanitize_to_windows_path_characters(t);
        if !sanitized.is_empty() {
            fname.push_str(&format!(".{}", sanitized));
        }
    }
    fname.push('.');
    fname.push_str(ext);

    let output_path = path.with_file_name(fname);
    if fs::try_exists(&output_path).await? {
        let proceed = pick(FzfArgs {
            choices: vec![
                Choice {
                    key: "Overwrite".to_string(),
                    value: true,
                },
                Choice {
                    key: "Skip".to_string(),
                    value: false,
                },
            ],
            header: Some(format!(
                "Output file already exists: {}",
                output_path.display()
            )),
            prompt: Some("Overwrite or skip?".to_string()),
        })
        .map_err(|e| eyre!(e))?
        .value;
        if !proceed {
            return Ok(None);
        }
    }

    // Temporarily write to "output.{ext}" in the same directory, then rename
    let temp_name = format!("output.{}", ext);
    let temp_path = path.with_file_name(temp_name);
    if fs::try_exists(&temp_path).await? {
        let proceed = pick(FzfArgs {
            choices: vec![
                Choice {
                    key: "Overwrite".to_string(),
                    value: true,
                },
                Choice {
                    key: "Abort".to_string(),
                    value: false,
                },
            ],
            header: Some(format!("Temp file already exists: {}", temp_path.display())),
            prompt: Some("Overwrite or abort?".to_string()),
        })
        .map_err(|e| eyre!(e))?
        .value;
        if !proceed {
            bail!("Temp file already exists: {}", temp_path.display());
        }
        fs::remove_file(&temp_path).await?;
    }

    // Build and run the ffmpeg command
    // Example: ffmpeg -i input.mkv -map 0:s:2 -c copy output.srt
    let mut cmd = Command::new("ffmpeg");
    cmd.current_dir(path.parent().unwrap_or_else(|| Path::new(".")));
    let selector = format!("0:s:{}", track.stream_index);
    let args = [
        "-i".as_ref(),
        path.file_name()
            .ok_or_else(|| eyre!("No file name on input path"))?,
        "-map".as_ref(),
        selector.as_ref(),
        "-c".as_ref(),
        "copy".as_ref(),
        temp_path
            .file_name()
            .ok_or_else(|| eyre!("No file name on output path"))?,
    ];
    cmd.args(args);
    debug!(
        "Running command `ffmpeg {}`",
        args.iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Execute the command and handle errors
    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to extract subtitle track: {}", stderr);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    debug!("stdout: {}", stdout);
    debug!("stderr: {}", stderr);

    // Rename the temporary file to the final output path
    fs::rename(&temp_path, &output_path).await?;

    Ok(Some(output_path))
}

/// Replace invalid Windows path characters with underscores
fn sanitize_to_windows_path_characters(segment: &str) -> String {
    segment
        .chars()
        .map(|c| {
            if ['/', '\\', ':', '*', '"', '<', '>', '|'].contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}
