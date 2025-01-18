#![feature(try_blocks)]

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

/// Prompt user (via FZF) to pick an MKV file in current directory
pub async fn pick_mkv_file() -> eyre::Result<PathBuf> {
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
pub async fn gather_mkv_files() -> eyre::Result<Vec<PathBuf>> {
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
pub struct SubtitleTrack {
    /// This is the "N" in `Stream #0:N` (the real ffmpeg index).
    pub stream_index: u32,

    /// We parse the language from `(eng)` or similar if present.
    pub lang: Option<String>,

    /// The recognized format, e.g. `subrip`, `ass`, `hdmv_pgs_subtitle`.
    pub format: String,

    /// A "title" if found in subsequent metadata lines, e.g. "English subs".
    pub title: Option<String>,
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
pub async fn pick_subtitle_tracks(path: &Path) -> eyre::Result<Vec<SubtitleTrack>> {
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
pub fn extension_for_format(fmt: &str) -> &str {
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
pub async fn enumerate_subtitle_tracks(path: &Path) -> eyre::Result<Vec<SubtitleTrack>> {
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
        if line.trim_start().starts_with("Stream #") && line.contains("Subtitle") {
            // Finalize the previous track if it exists
            if let Some(t) = current.take() {
                result.push(t);
            }

            // Example line:
            //   "Stream #0:1: Subtitle: subrip (default)"
            //   "Stream #0:2(eng): Subtitle: subrip (default)"
            //   "Stream #0:3: Subtitle: hdmv_pgs_subtitle, 1920x1080"
            //   "Stream #0:0: Subtitle: subrip (default)"

            let line_trim = line.trim_start();
            // Remove "Stream #"
            let after_stream = &line_trim["Stream #".len()..].trim();

            // We expect "0:1:" or "0:2(eng):" etc. before the words "Subtitle:"
            // Split by "Subtitle:"
            let (mut stream_part, after_subtitle) = match after_stream.split_once("Subtitle:") {
                Some((left, right)) => (left.trim(), right.trim()),
                None => {
                    // Shouldn’t happen if line.contains("Subtitle"), but just in case:
                    return Err(eyre!("No 'Subtitle:' in line: {}", line));
                }
            };

            // stream_part might be "0:1:" or "0:1(eng):" or "0:2(eng):"
            // remove trailing colons
            stream_part = stream_part.trim_end_matches(':').trim();

            // Extract optional (lang). We'll do:
            //   - If we find '(' => parse everything up to '(' as e.g. "0:1"
            //   - Then parse what's inside '(...)' as the language
            let (numeric_part, lang) = if let Some(open_paren) = stream_part.find('(') {
                // e.g. "0:2(eng)"
                let inside = &stream_part[open_paren + 1..]; // "eng)"
                let close_paren = inside
                    .find(')')
                    .ok_or_else(|| eyre!("Unclosed parenthesis in line: {}", line))?;
                let lang_str = Some(inside[..close_paren].to_string());

                // numeric_str is everything up to '('
                let numeric_str = stream_part[..open_paren].trim_end_matches(':').trim();
                // e.g. "0:2" or "0:1"
                let idx = numeric_str
                    .rfind(':')
                    .ok_or_else(|| eyre!("No colon found in numeric_str: '{}'", numeric_str))?;
                let int_str = &numeric_str[idx + 1..];
                let num = int_str.parse::<u32>()?;
                (num, lang_str)
            } else {
                // No parentheses => no language
                // e.g. "0:1" or "0:2"
                let trimmed = stream_part.trim_end_matches(':').trim();
                // If there's still a trailing colon, remove it again
                let idx = trimmed
                    .rfind(':')
                    .ok_or_else(|| eyre!("No colon found in stream portion: '{}'", trimmed))?;
                let int_str = &trimmed[idx + 1..];
                let num = int_str.parse::<u32>()?;
                (num, None)
            };

            // Now parse the format from after_subtitle
            // e.g. "subrip (default)" => "subrip"
            //      "hdmv_pgs_subtitle, 1920x1080" => "hdmv_pgs_subtitle"
            let format_str = after_subtitle
                .split(|c: char| c.is_whitespace() || c == ',' || c == '(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            let track = SubtitleTrack {
                stream_index: numeric_part,
                lang,
                format: format_str,
                title: None,
            };

            current = Some(track);
        } else {
            // Possibly a metadata line if `current` is Some
            if let Some(current_track) = current.as_mut() {
                if line.trim_start().starts_with("title") {
                    // e.g. "title           : English subs"
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

    // Update the indices to be 0-based
    for (i, track) in result.iter_mut().enumerate() {
        track.stream_index = i as u32;
    }

    Ok(result)
}

/// Actually run ffmpeg to copy a track to a new file with the correct extension
pub async fn extract_subtitle_track(
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

    cmd.args([
        "-i",
        path.file_name().ok_or(eyre!("No file name"))?.to_string_lossy().as_ref(),
        "-map",
        &selector,
        "-c",
        "copy",
    ]);

    // Decide container format for text-based subtitles
    let container_format = match track.format.as_str() {
        "subrip" => "srt",
        "ass" => "ass",
        "hdmv_pgs_subtitle" | "pgssub" => "sup",
        _ => "",
    };

    if !container_format.is_empty() {
        cmd.arg("-f").arg(container_format);
    }

    // Finally, specify the output file (temp_path)
    cmd.arg(
        temp_path
            .file_name()
            .ok_or(eyre!("No file name on output"))?,
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
