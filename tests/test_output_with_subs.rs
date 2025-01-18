mod test_extraction;

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::str;

    /// Validates that `output_with_subs.mkv` has:
    /// 1) Exactly two subtitle streams
    /// 2) One stream is subrip (SRT) and one stream is ass (ASS)
    #[test]
    fn test_output_with_subs() -> Result<(), Box<dyn std::error::Error>> {
        // Adjust the path as needed; here we assume the file is in `resources/`
        let test_file = "resources/output_with_subs.mkv";

        // Invoke ffprobe with a format that’s easy to parse:
        // - show_entries stream=index,codec_name,codec_type
        // - of csv=p=0 => plain CSV output with no headers
        // - -v error => suppress extra logs
        let output = Command::new("ffprobe")
            .args([
                "-i",
                test_file,
                "-v",
                "error",
                "-show_entries",
                "stream=index,codec_name,codec_type",
                "-of",
                "csv=p=0",
            ])
            .output()?;

        // If ffprobe failed to run or the file doesn't exist, we get an error here
        if !output.status.success() {
            let stderr = str::from_utf8(&output.stderr)?;
            panic!("ffprobe failed: {stderr}");
        }

        // Parse the CSV lines: index,codec_name,codec_type
        // e.g. "0,h264,video"
        //      "2,subrip,subtitle"
        //      "3,ass,subtitle"
        let stdout = str::from_utf8(&output.stdout)?;
        let mut subtitles = Vec::new();

        for line in stdout.lines() {
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() == 3 {
                let index = fields[0].to_string();
                let codec_name = fields[1].to_string();
                let codec_type = fields[2].to_string();

                if codec_type == "subtitle" {
                    subtitles.push((index, codec_name));
                }
            }
        }

        // Check we have exactly 2 subtitle streams
        assert_eq!(
            subtitles.len(),
            2,
            "Expected 2 subtitle streams, found {}: {subtitles:?}",
            subtitles.len()
        );

        // Confirm that one is subrip (SRT) and the other is ass
        let has_srt = subtitles.iter().any(|(_, codec)| codec == "subrip");
        let has_ass = subtitles.iter().any(|(_, codec)| codec == "ass");

        assert!(
            has_srt,
            "No subrip (SRT) stream found among: {subtitles:?}"
        );
        assert!(
            has_ass,
            "No ass stream found among: {subtitles:?}"
        );

        Ok(())
    }
}
