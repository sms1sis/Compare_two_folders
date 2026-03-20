use anyhow::Result;
use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;
use memmap2::Mmap;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::models::{ErrorEntry, FileEntry, HashAlgo, HashResult, SymlinkMode};

pub fn compute_hashes(path: &Path, algo: HashAlgo) -> io::Result<HashResult> {
    let metadata = fs::metadata(path)?;
    let len = metadata.len();

    const MMAP_THRESHOLD: u64 = 32 * 1024;
    const RAYON_THRESHOLD: u64 = 128 * 1024 * 1024;

    let mut sha256_hasher = if matches!(algo, HashAlgo::Sha256 | HashAlgo::Both) {
        Some(Sha256::new())
    } else {
        None
    };
    let mut blake3_hasher = if matches!(algo, HashAlgo::Blake3 | HashAlgo::Both) {
        Some(blake3::Hasher::new())
    } else {
        None
    };

    if len == 0 {
        return Ok(HashResult {
            // Fix #3: pre-allocate a 64-char buffer instead of one String-per-byte
            sha256: sha256_hasher.map(|h| bytes_to_hex(&h.finalize())),
            blake3: blake3_hasher.map(|h| h.finalize().to_hex().to_string()),
        });
    }

    if len < MMAP_THRESHOLD {
        let data = fs::read(path)?;
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&data);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            bh.update(&data);
        }
    } else {
        let f = File::open(path)?;
        let mmap = unsafe { Mmap::map(&f)? };

        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&mmap);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            if len > RAYON_THRESHOLD {
                bh.update_rayon(&mmap);
            } else {
                bh.update(&mmap);
            }
        }
    }

    // Fix #3: use pre-allocated hex encoding (64 bytes, no per-byte alloc)
    let sha256 = sha256_hasher.map(|h| bytes_to_hex(&h.finalize()));
    let blake3 = blake3_hasher.map(|h| h.finalize().to_hex().to_string());

    Ok(HashResult { sha256, blake3 })
}

/// Encode a byte slice to lowercase hex with a single pre-allocated String.
/// This replaces the old `.iter().map(|b| format!("{:02x}", b)).collect()` pattern
/// that allocated one String per byte (32 allocations for SHA-256). (Fix #3)
#[inline]
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{:02x}", b).expect("write to String is infallible");
    }
    s
}

pub fn collect_files(
    dir: &Path,
    depth: Option<usize>,
    no_recursive: bool,
    hidden: bool,
    types: &Option<Vec<String>>,
    ignore_patterns: &Option<Vec<String>>,
    symlink_mode: SymlinkMode,
) -> Result<(Vec<FileEntry>, Vec<ErrorEntry>)> {
    let mut walk_builder = WalkBuilder::new(dir);
    walk_builder.hidden(!hidden);

    if no_recursive {
        walk_builder.max_depth(Some(1));
    } else if let Some(d) = depth {
        walk_builder.max_depth(Some(d));
    }

    match symlink_mode {
        SymlinkMode::Follow => {
            walk_builder.follow_links(true);
        }
        _ => {
            walk_builder.follow_links(false);
        }
    }

    let custom_ignore_set = if let Some(patterns) = ignore_patterns {
        let mut builder = GlobSetBuilder::new();
        for p in patterns {
            builder.add(Glob::new(p)?);
        }
        Some(builder.build()?)
    } else {
        None
    };

    let type_filter: Option<HashSet<String>> = types.as_ref().map(|exts| {
        exts.iter()
            .map(|ext| ext.trim_start_matches('.').to_lowercase())
            .collect()
    });

    let (tx, rx) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();

    let walker = walk_builder.build_parallel();

    std::thread::spawn(move || {
        walker.run(|| {
            let tx = tx.clone();
            let tx_err = tx_err.clone();
            let type_filter = type_filter.clone();
            let custom_ignore_set = custom_ignore_set.clone();

            Box::new(move |result| {
                let entry = match result {
                    Ok(e) => e,
                    Err(err) => {
                        let _ = tx_err.send(ErrorEntry {
                            path: PathBuf::from("?"),
                            error: err.to_string(),
                        });
                        return ignore::WalkState::Continue;
                    }
                };

                if let Some(ref set) = custom_ignore_set
                    && set.is_match(entry.path())
                {
                    return ignore::WalkState::Continue;
                }

                let ft = match entry.file_type() {
                    Some(ft) => ft,
                    None => return ignore::WalkState::Continue,
                };

                let is_symlink = ft.is_symlink();
                let is_file = ft.is_file();

                let should_include = match symlink_mode {
                    SymlinkMode::Ignore => is_file,
                    SymlinkMode::Follow => is_file,
                    SymlinkMode::Compare => is_file || is_symlink,
                };

                if !should_include {
                    return ignore::WalkState::Continue;
                }

                if let Some(ref exts) = type_filter
                    && !entry
                        .path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| exts.contains(&s.to_lowercase()))
                {
                    return ignore::WalkState::Continue;
                }

                let mut symlink_target = None;
                if is_symlink
                    && matches!(symlink_mode, SymlinkMode::Compare)
                    && let Ok(target) = fs::read_link(entry.path())
                {
                    symlink_target = Some(target.to_string_lossy().to_string());
                }

                if let Ok(meta) = entry.metadata() {
                    let entry_data = FileEntry {
                        path: entry.path().to_path_buf(),
                        size: meta.len(),
                        modified: meta.modified().ok(),
                        symlink_target,
                    };
                    let _ = tx.send(entry_data);
                }

                ignore::WalkState::Continue
            })
        });
    });

    let final_files: Vec<FileEntry> = rx.into_iter().collect();
    let final_errors: Vec<ErrorEntry> = rx_err.into_iter().collect();
    Ok((final_files, final_errors))
}
