# v4.0.2

Maintenance release optimizing memory allocation for static builds.

## 🚀 Improvements

*   **Static Build Optimization**: Added `mimalloc` as the global allocator for `musl` targets. This significantly improves performance and memory usage when running statically linked binaries (e.g., in Alpine Linux containers).

# v4.0.1

Maintenance release focusing on documentation clarity, error handling improvements, and output consistency.

## 🚀 Improvements

*   **Summary Enhancements**: The final summary box now explicitly displays the thread count used (or "Default (N)" if auto-detected).
*   **Realtime Error Handling**: Errors encountered in Realtime mode are now correctly emitted to `stderr` instead of `stdout`. This ensures that piping standard output does not mix valid comparison data with error messages.
*   **Documentation**:
    *   Clarified `--depth` behavior (0 = root only, 1 = immediate children).
    *   Clarified `--symlinks compare` behavior (compares link paths, not target contents).
    *   Explicitly documented BLAKE3 internal threading thresholds.

## 🛠 Fixes & Internal

*   **Refactor**: Centralized output formatting logic to reduce code duplication between Realtime and Batch reporting modes.

# v4.0.0

Major release focusing on scriptability, improved defaults, and handling of complex directory structures (symlinks, permissions).

## 🚨 Breaking Changes

*   **Recursive by Default**: The `-s/--subfolders` flag has been removed. `cmpf` now recurses into subdirectories by default. Use `--no-recursive` or `--depth <N>` to limit traversal.
*   **Standard Exit Codes**: The tool now returns meaningful exit codes for CI/CD integration:
    *   `0`: Success (Folders are identical).
    *   `1`: Success (Differences found).
    *   `2`: Runtime Error (I/O, Permissions, Invalid Args).
*   **JSON Schema**: JSON output keys are now strictly `snake_case` (e.g., `Realtime` -> `realtime`) for better compatibility with parsers like `jq`.

## ✨ New Features

*   **Symlink Support**: Added `--symlinks <MODE>` to control symbolic link handling.
    *   `ignore` (default): Skip symlinks.
    *   `follow`: Transparently follow links.
    *   `compare`: Check if symlink targets match (path content).
*   **Recursion Control**: New `--depth <N>` and `--no-recursive` flags.
*   **Error Reporting**: Permission denied errors during file walking are now captured and reported in the final summary instead of being silently ignored.

## 🚀 Improvements

*   **Threading Heuristics**: Disabled internal threading for the `blake3` hasher on files smaller than 128MB. This prevents thread pool saturation when comparing thousands of small/medium files, significantly improving performance on high-core-count machines.
*   **TTY Awareness**: ANSI colors and progress bars are now automatically disabled when output is piped or redirected (e.g., `cmpf f1 f2 > report.txt`), ensuring clean text output.
*   **Clean Output**: Realtime mode now prints errors immediately as they occur.

## 📦 Maintenance

*   Updated dependencies to latest versions.
*   Codebase audit for Rust 2024 edition compliance.
