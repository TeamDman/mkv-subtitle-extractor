#![feature(try_blocks)]
use clap::Parser;
use cloud_terrastodon_core_user_input::prelude::pick;
use cloud_terrastodon_core_user_input::prelude::pick_many;
use cloud_terrastodon_core_user_input::prelude::Choice;
use cloud_terrastodon_core_user_input::prelude::FzfArgs;
use eyre::bail;
use eyre::eyre;
use eyre::Context;
use itertools::Itertools;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::fs;
use tokio::process::Command;
use tracing::debug;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

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
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
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

async fn gather_mkv_files() -> eyre::Result<Vec<PathBuf>> {
    let cwd = PathBuf::from("./");
    let mut entries = fs::read_dir(cwd).await?;
    let mut candidates = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = PathBuf::from(entry.file_name());
        if let Some(ext) = file_name.extension() {
            if ext == "mkv" {
                candidates.push(file_name);
            }
        }
    }
    Ok(candidates)
}

async fn pick_subtitle_tracks(path: &PathBuf) -> eyre::Result<Vec<SubtitleTrack>> {
    let tracks = enumerate_subtitle_tracks(&path).await?;
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

#[derive(Debug)]
struct SubtitleTrack {
    stream: u32,
    index: u32,
    lang: Option<String>,
    name: String,
}
impl std::fmt::Display for SubtitleTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Stream #{}:{}", self.stream, self.index))?;
        if let Some(lang) = &self.lang {
            f.write_fmt(format_args!("({}) {}", lang, self.name))?;
        } else {
            f.write_fmt(format_args!(" Subtitle {}", self.name))?;
        }
        Ok(())
    }
}
impl FromStr for SubtitleTrack {
    type Err = eyre::Error;

    fn from_str(s: &str) -> eyre::Result<Self> {
        // "  Stream #0:2(eng): Subtitle: subrip (default)",
        // "  Stream #0:3: Subtitle: hdmv_pgs_subtitle, 1920x1080",
        debug!("Parsing subtitle track: {:?}", s);
        let mut chunks = s.split(':').collect::<VecDeque<_>>();
        debug!("chunks: {:#?}", chunks);
        if chunks.len() < 3 {
            bail!("Invalid subtitle track: {:?}", s);
        }

        let Some(stream) = chunks.pop_front() else {
            bail!("Invalid subtitle track: {:?}", s);
        };
        let stream = stream
            .trim_start()
            .strip_prefix("Stream #")
            .ok_or_else(|| {
                eyre!(
                    "Invalid subtitle track: {:?}, did not begin with \"Stream #\"",
                    s
                )
            })?;
        let stream = stream.parse::<u32>()?;
        debug!("stream={stream}, chunks: {chunks:#?}");

        let Some(index_and_lang) = chunks.pop_front() else {
            bail!("Invalid subtitle track: {:?}", s);
        };
        let (index, lang) = match index_and_lang.split_once('(') {
            Some((index, lang)) => {
                let index = index.parse::<u32>()?;
                let Some((lang, _)) = lang.split_once(")") else {
                    bail!("Invalid subtitle track: {:?}", s);
                };
                let lang = Some(lang.to_string()).filter(|x| !x.is_empty());
                (index, lang)
            }
            None => (index_and_lang.parse::<u32>()?, None),
        };
        debug!("index={index}, lang={lang:?}, chunks: {chunks:#?}");

        let Some(subtitle_literal) = chunks.pop_front() else {
            bail!("Invalid subtitle track: {:?}", s);
        };
        if subtitle_literal.trim_start() != "Subtitle" {
            bail!("Invalid subtitle track: {:?}", s);
        }

        let name = chunks.into_iter().join(":").trim().to_string();

        Ok(SubtitleTrack {
            stream,
            index,
            lang,
            name,
        })
    }
}
async fn enumerate_subtitle_tracks(path: &PathBuf) -> eyre::Result<Vec<SubtitleTrack>> {
    info!("Enumerating subtitle tracks");
    debug!("Running command `ffmpeg -i $path`");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-i".as_ref(), path.as_os_str()]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let output = cmd.output().await?;
    // don't check success since it fails since we didn't specify an output file, we are just reading the display
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    debug!("stdout: {}", stdout);
    debug!("stderr: {}", stderr);

    let mut tracks: Vec<SubtitleTrack> = Vec::new();
    for line in stderr.lines() {
        if line.trim_start().starts_with("Stream #") && line.contains("Subtitle") {
            tracks.push(line.parse()?);
        }
    }

    // Patch indices
    let tracks = tracks
        .into_iter()
        .enumerate()
        .map(|(i, mut track)| {
            track.index = i as u32;
            track
        })
        .collect_vec();

    Ok(tracks)
}

async fn extract_subtitle_track(
    path: &PathBuf,
    track: &SubtitleTrack,
) -> eyre::Result<Option<PathBuf>> {
    info!("Extracting subtitle track: {}", track);
    let mut extension = format!("{}", track.index);
    if let Some(lang) = track.lang.as_ref() {
        extension.push_str(&format!(".{}", lang));
    }
    extension.push_str(&format!(".{}.srt", track.name));

    let extension = sanitize_to_windows_path_characters(&extension);
    let output_path = path.with_extension(extension);
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

    let temp_path = path.with_file_name("output.srt");
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
        }).map_err(|e| eyre!(e))?.value;
        if !proceed {
            bail!("Temp file already exists: {}", temp_path.display());
        }
        fs::remove_file(&temp_path).await?;
    }

    let mut cmd = Command::new("ffmpeg");
    cmd.current_dir(path.parent().unwrap());
    let selector = format!("0:s:{}", track.stream);
    let args = [
        "-i".as_ref(),
        path.file_name()
            .ok_or(eyre!("No file name on input path"))?,
        "-map".as_ref(),
        selector.as_ref(),
        "-c".as_ref(),
        "copy".as_ref(),
        temp_path
            .file_name()
            .ok_or(eyre!("No file name on output path"))?,
    ];
    cmd.args(args);
    debug!(
        "Running command `ffmpeg {}`",
        args.join(" ".as_ref()).to_string_lossy()
    );
    let x: eyre::Result<()> = try {
        let output = cmd.output().await?;
        if !output.status.success() {
            Err(eyre!(
                "Failed to extract subtitle track: {}",
                String::from_utf8(output.stderr)?
            ))?;
            unreachable!();
        }
        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;
        debug!("stdout: {}", stdout);
        debug!("stderr: {}", stderr);
        ()
    };
    x.wrap_err(format!(
        "ffmpeg {}",
        args.join(" ".as_ref()).to_string_lossy()
    ))?;

    // move
    fs::rename(&temp_path, &output_path).await?;

    Ok(Some(output_path))
}

fn sanitize_to_windows_path_characters(segment: &str) -> String {
    // replace all characters that aren't valid path characters with underscores
    segment
        .chars()
        .filter(|c| !['/', '\\', ':', '*', '"', '<', '>', '|'].contains(c))
        .collect()
}
