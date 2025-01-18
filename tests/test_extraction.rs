use eyre::Result;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

// Bring in your enumerator & extractor from the mkv-subtitle-extractor code.
// Adjust the path/imports as needed for your project structure.
use mkv_subtitle_extractor::enumerate_subtitle_tracks;
use mkv_subtitle_extractor::extract_subtitle_track;

#[tokio::test]
async fn test_extract_subtitles_and_compare() -> Result<()> {
    // 1) Our target test file with two embedded subtitle streams: one SRT and one ASS
    let mkv_path = PathBuf::from("resources/output_with_subs.mkv");

    // 2) Enumerate all subtitle tracks using your function
    let tracks = enumerate_subtitle_tracks(&mkv_path).await?;
    println!("Found tracks: {tracks:#?}");

    // 3) We expect two tracks: one subrip, one ass
    let subrip_track = tracks
        .iter()
        .find(|t| t.format == "subrip")
        .ok_or_else(|| eyre::eyre!("No subrip track found"))?;

    let ass_track = tracks
        .iter()
        .find(|t| t.format == "ass")
        .ok_or_else(|| eyre::eyre!("No ass track found"))?;

    // Remove old files
    let _ = tokio::fs::remove_file("resources/output_with_subs.0.srt").await;
    let _ = tokio::fs::remove_file("resources/output_with_subs.1.ass").await;

    // 4) Extract the subrip track
    let extracted_srt = extract_subtitle_track(&mkv_path, subrip_track)
        .await?
        .ok_or_else(|| eyre::eyre!("Skipping SRT track extraction"))?;
    println!("Extracted SRT to: {}", extracted_srt.display());

    // 5) Extract the ASS track
    let extracted_ass = extract_subtitle_track(&mkv_path, ass_track)
        .await?
        .ok_or_else(|| eyre::eyre!("Skipping ASS track extraction"))?;
    println!("Extracted ASS to: {}", extracted_ass.display());

    // 6) Compare extracted files to your reference `test.srt` and `test.ass`.
    //    We'll do a direct textual comparison. If your extraction is raw copy,
    //    we expect them to match exactly.
    let reference_srt = "resources/test.srt";
    let reference_ass = "resources/test.ass";

    compare_files_text(&extracted_srt, &PathBuf::from(reference_srt))?;
    compare_files_text(&extracted_ass, &PathBuf::from(reference_ass))?;

    
    // Remove old files
    tokio::fs::remove_file("resources/output_with_subs.0.srt").await?;
    tokio::fs::remove_file("resources/output_with_subs.1.ass").await?;

    Ok(())
}

/// Simple helper to compare two files line by line.
/// Panics if they differ, with a clear message.
fn compare_files_text<P: AsRef<Path>>(path_a: P, path_b: P) -> Result<()> {
    let text_a = fs::read_to_string(&path_a)?.replace("\r","");
    let text_a = text_a.trim();
    let text_b = fs::read_to_string(&path_b)?.replace("\r","");
    let text_b = text_b.trim();

    if text_a != text_b {
        eyre::bail!(
            "Files differ:\n  A = {}\n  B = {}\n---\nFile A:\n```\n{}\n```\n---\nFile B:\n```\n{}\n```",
            path_a.as_ref().display(),
            path_b.as_ref().display(),
            text_a,
            text_b
        );
    }

    Ok(())
}
