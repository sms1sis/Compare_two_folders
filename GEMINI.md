# GEMINI.md

## Project Overview

This project is a command-line utility named `cmpf` written in Rust. Its purpose is to compare files in two different folders, offering robust comparison based on names and cryptographic hashes. It's designed for efficiency and flexibility, suitable for various file synchronization and verification tasks.

## Building and Running

To build the project, you need to have Rust and Cargo installed. Refer to the `README.md` for detailed installation instructions.

**Build:**
```sh
cargo build --release
```

**Run:**
```sh
# From the project root folder
./target/release/cmpf <folder1> <folder2> [options]
```

## Command-line Flags Overview

The tool supports several command-line arguments to customize its behavior. For full details and examples, please refer to the `README.md` file.

*   `-m, --mode <MODE>`: Comparison mode (`batch` or `realtime`).
*   `-a, --algo <ALGORITHM>`: Hashing algorithm (`blake3`, `sha256`, or `both`).
*   `-o, --output-folder <OUTPUT_FOLDER>`: Output directory for reports (Batch mode only).
*   `-f, --output-format <FORMAT>`: Report format (`txt` or `json`, Batch mode only).
*   `-s, --subfolders`: Enable recursive comparison in subfolders.
*   `-v, --verbose`: Show hash values in the output.
*   `-H, --hidden`: Include hidden files and folders in the comparison. By default, hidden files are ignored.
*   `-t, --type <EXTENSION>`: Filter comparison to include only files with specified extensions (e.g., `.txt`, `png`). Can be used multiple times.
*   `-i, --ignore <PATTERN>`: Specify a gitignore-style pattern to ignore files or directories. Can be used multiple times.
*   `-j, --threads <COUNT>`: Set the number of threads for parallel processing in batch mode. Defaults to the number of CPU cores.
*   `-S, --size-only`: Compare only file sizes to skip cryptographic hashing for maximum speed.

## Development Conventions

The codebase is structured following standard Rust best practices, utilizing a modular design for clarity and maintainability. Key functionalities, such as file collection, hashing, and report generation, are encapsulated within dedicated functions. The code is commented where necessary to explain complex logic.

## Git Workflow

*   **Proactive Commits**: After completing a task (e.g., adding a feature, fixing a bug, or refactoring) and verifying it with builds/tests, proactively gather git information (`status`, `diff`, `log`) and propose a concise commit message to the user.
*   **Atomic Commits**: Prefer small, focused commits that address a single change or related set of changes.

## Recent Enhancements

*   **Version 3.3.0 Metadata Mode & Architectural Cleanup**:
    *   **Metadata Mode**: Replaced legacy `-S` flag with a robust `-m metadata` mode (checks size + time).
    *   **Refactored Core Logic**: Unified comparison logic into a single internal function, ensuring identical behavior across Batch and Realtime modes.
    *   **Sorting Optimization**: Implemented conditional sorting. The tool now defaults to alphabetical output but allows users to disable it for maximum performance.
    *   **New Flag**: Added `--no-sort` to skip CPU-heavy sorting operations in large directory trees.
    *   **Enhanced Verbose Output**: The `-v` flag now explicitly details size/time differences.
    *   **Chrono Integration**: Added `chrono` for cross-platform timestamp formatting.

*   **Version 3.2.0 Performance Overhaul**:
    *   **Size-Only Mode**: Added `-S` flag for ultra-fast comparison when cryptographic security is not required.
    *   **Smart I/O**: Small files (<10MB) are now read entirely into memory to reduce syscall overhead, while larger files use efficient streaming.
    *   **UI Throttling**: The progress bar now updates at a fixed 10Hz rate, preventing terminal output from becoming a bottleneck during high-speed comparisons (e.g., kernel trees).
    *   **Short-Circuit Logic**: Files with differing sizes are immediately flagged as different without computing hashes.
    *   **Parallel Hashing**: Enabled `rayon` support for `blake3` to utilize multi-threading for hashing individual large files.

*   **Alphabetical Output**: All output file lists are now consistently sorted alphabetically for improved readability.
*   **Hidden File Control**: Introduced `-H` flag to override the new default behavior of ignoring hidden files.
*   **File Type Filtering**: Added `-t` flag to allow users to compare only specific file types/extensions.
*   **Ignore Files/Patterns**: Implemented support for `.gitignore` files and a new `-i` flag for custom ignore patterns.
