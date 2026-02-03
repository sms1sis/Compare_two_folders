# cmpf: Folder File Comparison Utility

A high-performance command-line utility implemented in **Rust** for efficiently comparing files across two directories. `cmpf` helps developers, system administrators, and anyone dealing with file synchronization or verification tasks to quickly identify matches, differences, missing, and extra files based on their names and cryptographic hashes.

---

## ✨ Features

*   **Dual Directory Comparison**: Compare files present in two specified directories.
*   **Flexible Hashing Algorithms**: Utilize robust cryptographic hashing for content comparison:
    *   **Blake3 (Default)**: A modern, extremely fast, and highly secure cryptographic hash function.
    *   **Sha256**: A widely-used, secure cryptographic hash function.
    *   **Both**: Compare files using both Blake3 and Sha256 for maximum integrity verification.
*   **Optimized Comparison Modes**:
    *   **Batch Mode (Default)**: Leverages parallel processing for significantly faster comparisons, ideal for large datasets. A comprehensive report is generated upon completion. Includes a dynamic progress bar for tracking.
    *   **Realtime Mode**: Processes files sequentially, providing immediate feedback as each file is compared. Suitable for smaller directories or when instant updates are preferred. Errors encountered during collection are emitted immediately to `stderr` (important for piping).
    *   **Metadata Mode**: Skips cryptographic hashing and compares files based on their size and modification time. This is extremely fast and improves accuracy over size-only checks.
*   **High-Speed Optimizations**: Includes smart short-circuiting and optimized I/O strategies for handling massive directory trees (e.g., kernel sources) with minimal overhead.
*   **Advanced File Filtering**:
    *   **Ignore Patterns**: Automatically respects `.gitignore` rules and supports custom ignore patterns (`--ignore`) to exclude specific files or directories.
    *   **Hidden Files**: By default, hidden files (those starting with a `.`) are ignored. Use the `--hidden` flag to include them.
    *   **File Types**: Filter the comparison to include only specific file extensions (e.g., `.txt`, `.jpg`).
*   **Symlink Support**: Configurable handling for symbolic links: `ignore`, `follow` (compare target contents), or `compare` (compare link paths).
*   **Parallelization Control**: Manually set the number of threads to use in batch mode for fine-grained performance tuning. Includes an internal threshold (128MB) that triggers multi-threaded hashing for individual large files (applies to BLAKE3 internal threading only, not Rayon task parallelism).
*   **Sorted Output**: All file lists in the output are alphabetically sorted by default for consistent and easy-to-read results. This can be disabled using the `--no-sort` flag for maximum performance.
*   **Verbose Output**: Option to display the actual cryptographic hash values, exact file sizes, or formatted timestamps for matched and differing files.
*   **Recursion Control**: Recursively compares subfolders by default. Depth can be limited via `--depth` or disabled with `--no-recursive`.
*   **Colorized Terminal Output**: Intuitive color-coding (green for matches, red for differences, blue for missing/extra files) enhances readability in real-time feedback and final reports. Colors are automatically disabled in non-interactive terminals.
*   **Script-Friendly**: 
    *   **Exit Codes**: Returns `0` (Match), `1` (Diff), or `2` (Error).
    *   **Stable JSON**: Snake-case JSON keys for easy parsing by external tools.
    *   **Exportable Reports**: Save comparison results in `JSON` or `TXT` formats (Batch mode only).
    *   **Standard Error**: Critical errors and file-level errors in realtime mode are emitted to `stderr`, allowing `stdout` to be piped or redirected cleanly.

---

## (✍️) Notes

> [!IMPORTANT]
> **Windows Performance Note: Antivirus "I/O Tax"**
>
> On Windows, background security features (such as Real-time Protection) intercept every file access request to perform scans. For I/O-intensive tasks involving tens of thousands of files, this can degrade performance by **1,000% to 2,000%**.
>
> **To achieve maximum performance (~7s vs 200s):**
> * **Add a Process Exclusion:** Add `cmpf.exe` to your security software's exclusion list.
> * **Add a Path Exclusion:** Exclude the specific directories you are comparing.
> * **Temporary Disable:** Follow the [manual steps](https://docs.sisense.com/win/SisenseWin/disabling-windows-defender-real-time-protection.htm) below to disable monitoring during the run.

---

## 🚀 Getting Started

These instructions will get you a copy of the project up and running on your local machine.

### 📦 Prerequisites

*   **Rust**: `cmpf` is built with Rust. If you don't have Rust and Cargo installed, you can get them from [rustup.rs](https://www.rust-lang.org/tools/install).

### ⚙️ Installation & Building

1.  **Clone the repository:**
    ```sh
    git clone https://github.com/sms1sis/Compare_two_folders.git
    cd Compare_two_folders
    ```
2.  **Build the project in release mode:**
    ```sh
    cargo build --release
    ```
    The executable will be located at `./target/release/cmpf`.

---

## 🔐 Verification

To ensure the integrity of your download, we provide a `SHA256SUMS.txt` file for every release. You can use this to verify that the binary you downloaded has not been corrupted or tampered with.

> [!IMPORTANT]
> Make sure you download both the binary and the `SHA256SUMS.txt` file to the same directory before running these commands.

### Linux (Standard Distros)
For Ubuntu, Arch, Termux, etc.:
```sh
sha256sum -c SHA256SUMS.txt --ignore-missing
```

### Linux (Minimal/Alpine/BusyBox)
For environments where `sha256sum` lacks extended flags:
```sh
sha256sum -c SHA256SUMS.txt
```
*Note: This may print error messages for missing files (the other binaries you didn't download). Look for "OK" next to your downloaded file.*

### macOS
```sh
shasum -a 256 -c SHA256SUMS.txt
```

### Windows

**PowerShell:**
```powershell
Get-FileHash .\cmpf.exe -Algorithm SHA256
```
*Compare the output hash manually with the entry in `SHA256SUMS.txt`.*

**Command Prompt:**
```cmd
certutil -hashfile cmpf.exe SHA256
```
*Compare the output hash manually with the entry in `SHA256SUMS.txt`.*

---

## 💡 Usage

The `cmpf` utility is run from the command line, requiring two folder paths as primary arguments.

```sh
./target/release/cmpf <FOLDER1_PATH> <FOLDER2_PATH> [OPTIONS]
```

### Arguments

*   `<FOLDER1_PATH>`: The path to the first directory for comparison.
*   `<FOLDER2_PATH>`: The path to the second directory for comparison.

### Options

*   `-m, --mode <MODE>`: Specify the comparison mode.
    *   `batch` (default): Processes files in parallel, generating a report at the end.
    *   `realtime`: Processes files sequentially, providing immediate output.
    *   `metadata`: Compare file size and modification time to skip cryptographic hashing for maximum speed.
*   `-a, --algo <ALGORITHM>`: Choose the hashing algorithm for content comparison.
    *   `blake3` (default): Uses the high-performance Blake3 algorithm.
    *   `sha256`: Uses the SHA-256 algorithm.
    *   `both`: Uses both Blake3 and SHA-256 for comparison.
*   `-o, --output-folder <OUTPUT_FOLDER>`: (Batch mode only) Specify a folder to save the comparison report. If omitted, the report is printed to stdout.
*   `-f, --output-format <FORMAT>`: (Batch mode only) Define the format for the output report.
    *   `txt` (default)
    *   `json`
*   `--depth <DEPTH>`: Maximum recursion depth. Default is infinite.
    *   `0`: Compare only the root directory itself.
    *   `1`: Compare the root directory and its immediate children.
*   `--no-recursive`: Disable recursive comparison (equivalent to `--depth 1`).
*   `--symlinks <MODE>`: Handling strategy for symbolic links:
    *   `ignore` (default): Skip symbolic links.
    *   `follow`: Follow symbolic links and compare the target files.
    *   `compare`: Compare symlink targets (link path), not file contents. Prevents confusion about whether target file contents are hashed.
*   `-v, --verbose`: Show hash values, file sizes, or timestamps for differences in the output.
*   `-H, --hidden`: Include hidden files and directories in the comparison. By default, they are ignored.
*   `-t, --type <EXTENSION>`: Compare only files with the specified extension (e.g., `txt`, `.jpg`). This flag can be used multiple times.
*   `-i, --ignore <PATTERN>`: Specify a glob pattern to ignore files or directories. This flag can be used multiple times. Automatically respects `.gitignore` rules.
*   `-j, --threads <COUNT>`: Set the number of threads to use for parallel processing in batch mode. Defaults to the number of available CPU cores.
    *   *Note*: A size threshold (currently 128MB) determines when individual files use internal BLAKE3 threading. This threshold applies only to internal BLAKE3 threading (not Rayon task parallelism) and may be tuned in future releases.
*   `-n, --no-sort`: Disable alphabetical sorting of the output. Drastically improves performance on massive directory trees when order is not required.

### Exit Codes
*   `0`: Comparison successful, folders are identical.
*   `1`: Comparison successful, differences found.
*   `2`: Runtime error or file-level errors occurred (e.g., permission denied, I/O error). Any error encountered during processing triggers this code.

### Examples

1.  **Basic Comparison (Batch Mode, Blake3, Recursive):**
    ```sh
    ./target/release/cmpf ./my_folder_a ./my_folder_b
    ```

2.  **Rapid Metadata Comparison (Ideal for initial checks of large trees):**
    ```sh
    ./target/release/cmpf ./linux-kernel-v1 ./linux-kernel-v2 -m metadata
    ```

3.  **Realtime Comparison, including Hidden Files:**
    ```sh
    ./target/release/cmpf ./my_project_v1 ./my_project_v2 -m realtime -H
    ```

4.  **Batch Comparison, Only Comparing `.rs` and `.toml` Files:**
    ```sh
    ./target/release/cmpf ./src_v1 ./src_v2 -t rs -t toml
    ```

5.  **Non-Recursive Comparison:**
    ```sh
    ./target/release/cmpf ./folder1 ./folder2 --no-recursive
    ```

6.  **Comparison including Symlink Targets:**
    ```sh
    ./target/release/cmpf ./lib_v1 ./lib_v2 --symlinks compare
    ```

7.  **High-Performance Batch with Verbose Output, Saving to JSON:**
    ```sh
    ./target/release/cmpf /path/to/backup /path/to/current -a sha256 -v -o ./reports -f json
    ```

---

## 🤝 Contributing

Contributions are welcome! If you have suggestions for improvements or new features, please open an issue first to discuss them. For pull requests:

1.  Fork the repository.
2.  Create your feature branch (`git checkout -b feature/AmazingFeature`).
3.  Commit your changes (`git commit -m 'Add some AmazingFeature'`).
4.  Push to the branch (`git push origin feature/AmazingFeature`).
5.  Open a Pull Request.

---

## 📜 License

This project is licensed under the GPL-3.0 license - see the `LICENSE` file for details.

---

## 🙏 Acknowledgments

*   Built with the power of Rust and its fantastic ecosystem of crates.
*   Special thanks to the developers of `clap`, `rayon`, `colored`, `indicatif`, `blake3`, `sha2`, `serde`, `ignore`, `globset` and `chrono`.
