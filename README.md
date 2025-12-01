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
    *   **Realtime Mode**: Processes files sequentially, providing immediate feedback as each file is compared. Suitable for smaller directories or when instant updates are preferred.
*   **Verbose Output**: Option to display the actual cryptographic hash values for matched and differing files.
*   **Subfolder Traversal**: Control whether the comparison should include files within subdirectories recursively or only operate on the top-level files.
*   **Colorized Terminal Output**: Intuitive color-coding (green for matches, red for differences, blue for missing/extra files) enhances readability in real-time feedback and final reports.
*   **Exportable Reports**: Save comparison results in `JSON` or `TXT` formats for further analysis or record-keeping (Batch mode only).

---

## 🚀 Getting Started

These instructions will get you a copy of the project up and running on your local machine.

### 📦 Prerequisites

*   **Rust**: `cmpf` is built with Rust. If you don't have Rust and Cargo installed, you can get them from [rustup.rs](https://www.rust-lang.org/tools/install).

### ⚙️ Installation & Building

1.  **Clone the repository:**
    ```sh
    git clone https://github.com/sms1sis/cmpf.git
    cd cmpf
    ```
2.  **Build the project in release mode:**
    ```sh
    cargo build --release
    ```
    The executable will be located at `./target/release/cmpf`.

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
*   `-a, --algo <ALGORITHM>`: Choose the hashing algorithm for content comparison.
    *   `blake3` (default): Uses the high-performance Blake3 algorithm.
    *   `sha256`: Uses the SHA-256 algorithm.
    *   `both`: Uses both Blake3 and SHA-256 for comparison.
*   `-o, --output-folder <OUTPUT_FOLDER>`: (Batch mode only) Specify a folder to save the comparison report. If omitted, the report is printed to stdout.
*   `-f, --output-format <FORMAT>`: (Batch mode only) Define the format for the output report.
    *   `txt` (default)
    *   `json`
*   `-s, --subfolders`: Enable file comparison in subfolders (recursive traversal). By default, only files in the top-level directories are compared.
*   `-v, --verbose`: Show hash values for matched and different files in the output.

### Examples

1.  **Basic Comparison (Batch Mode, Blake3, Top-Level Only):**
    ```sh
    ./target/release/cmpf ./my_folder_a ./my_folder_b
    ```

2.  **Realtime Comparison, including Subfolders:**
    ```sh
    ./target/release/cmpf ./my_project_v1 ./my_project_v2 -m realtime -s
    ```

3.  **Batch Comparison with SHA-256, Verbose Output, Save to JSON:**
    ```sh
    ./target/release/cmpf /path/to/backup /path/to/current -a sha256 -v -o ./reports -f json
    ```

4.  **Batch Comparison with Both Algorithms, Saving Text Report:**
    ```sh
    ./target/release/cmpf ./source_dir ./target_dir -a both -o ./comparison_logs -f txt
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

This project is licensed under the MIT License - see the `LICENSE` file for details.

---

## 🙏 Acknowledgments

*   Built with the power of Rust and its fantastic ecosystem of crates.
*   Special thanks to the developers of `clap`, `rayon`, `colored`, `indicatif`, `blake3`, `sha2`, `serde`, and `walkdir`.
