# MKV Subtitle Extractor

**MKV Subtitle Extractor** is a command-line tool written in Rust that allows users to extract subtitle tracks from MKV (Matroska) video files. Leveraging `ffmpeg` for processing, this utility provides an interactive selection interface using FZF, enabling users to choose specific subtitle tracks and export them in their respective formats.

## 📦 Features

- **Interactive Selection**: Use FZF-based prompts to select MKV files and choose one or multiple subtitle tracks for extraction.
- **Format Detection**: Automatically detects the subtitle format (e.g., SRT, ASS, SUP) and assigns the appropriate file extension.
- **Metadata Handling**: Extracts and utilizes subtitle metadata, such as language and title, to generate descriptive output filenames.
- **Safe File Operations**: Checks for existing output files and prompts users to overwrite or skip, ensuring no accidental data loss.
- **Cross-Platform**: Designed to work seamlessly on Windows, macOS, and Linux systems.
- **Debug Logging**: Enable detailed debug logs to troubleshoot and understand the extraction process.

## 🚀 Installation

### Prerequisites

- **Rust**: Ensure you have Rust installed. If not, install it from [rustup.rs](https://rustup.rs/).
- **FFmpeg**: The tool relies on `ffmpeg` being installed and accessible in your system's PATH. Download it from [ffmpeg.org](https://ffmpeg.org/download.html).

### Building from Source

1. **Clone the Repository:**
   ```bash
   git clone https://github.com/yourusername/mkv-subtitle-extractor.git
   cd mkv-subtitle-extractor
   ```

2. **Build the Project:**
   ```bash
   cargo build --release
   ```

3. **Run the Executable:**
   The compiled binary will be located in `target/release/`.
   ```bash
   ./target/release/mkv-subtitle-extractor --help
   ```

## 📝 Usage

### Command-Line Arguments

- `--debug`: Enable debug logging for detailed output.
- `--file <PATH>`: Specify the path to the MKV file from which to extract subtitles. If omitted, the tool will prompt you to select an MKV file from the current directory.

### Basic Usage

```bash
mkv-subtitle-extractor --file "/path/to/video.mkv"
```

### Enabling Debug Logs

For verbose output useful during troubleshooting:

```bash
mkv-subtitle-extractor --file "/path/to/video.mkv" --debug
```

### Interactive File Selection

If you don't specify the `--file` argument, the tool will present an interactive FZF prompt to choose an MKV file from the current directory.

```bash
mkv-subtitle-extractor
```

### Selecting Subtitle Tracks

After selecting the MKV file, you'll be prompted to select one or more subtitle tracks to extract. Use the arrow keys to navigate and spacebar to select multiple tracks.

## 🔍 Examples

### Extracting a Single Subtitle Track

```bash
mkv-subtitle-extractor --file "Blade.Runner.2049.2017.mkv"
```

- **Output**: `Blade.Runner.2049.2017.2.eng.srt` (assuming stream index 2 with English subtitles in SRT format)

### Extracting Multiple Subtitle Tracks

```bash
mkv-subtitle-extractor --file "Jujutsu.Kaisen.44.mkv"
```

- **Output**:
  - `Jujutsu.Kaisen.2.jpn.ass` (Japanese subtitles in ASS format)
  - `Jujutsu.Kaisen.3.eng.srt` (English subtitles in SRT format)

## 🛠️ Development

### Running Clippy

Ensure your code adheres to Rust's best practices by running Clippy:

```bash
cargo clippy
```

### Testing

Currently, the project does not include automated tests. Contributions adding tests are welcome!

## 🤝 Contributing

Contributions are welcome! Whether it's reporting bugs, suggesting features, or submitting pull requests, your input helps improve the project.

1. **Fork the Repository**
2. **Create a Feature Branch**
   ```bash
   git checkout -b feature/YourFeature
   ```
3. **Commit Your Changes**
   ```bash
   git commit -m "Add your feature"
   ```
4. **Push to the Branch**
   ```bash
   git push origin feature/YourFeature
   ```
5. **Open a Pull Request**

Please ensure your code follows the project's coding standards and includes necessary documentation.

---

**Enjoy extracting your MKV subtitles effortlessly!** 🎬✨