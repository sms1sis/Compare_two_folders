use anyhow::{Context, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Instant;

use crate::compare::ExitStatus;
use crate::models::{
    ComparisonResult, FileEntry, HashAlgo, HashResult, Mode, OutputFormat, Status, SymlinkMode,
};
use crate::report::{ReportConfig, SummaryData, generate_json_report, generate_text_report};
use crate::utils::{collect_files, compute_hashes};

// Fix #6: store the scan parameters alongside the snapshot data so that
// verify_snapshot can reproduce the exact same walk instead of hardcoding
// hidden=true, depth=None, types=None, etc.
#[derive(Serialize, Deserialize)]
pub struct SnapshotScanParams {
    pub depth: Option<usize>,
    pub no_recursive: bool,
    pub hidden: bool,
    pub types: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub symlinks: SymlinkMode,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub created_at: String,
    pub root_path: String,
    pub files: Vec<SnapshotEntry>,
    pub algo: HashAlgo,
    /// Scan parameters recorded at snapshot creation time. (Fix #6)
    /// An absent field (old snapshot files) falls back to safe defaults.
    #[serde(default)]
    pub scan_params: Option<SnapshotScanParams>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnapshotEntry {
    pub rel_path: PathBuf,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub hashes: HashResult,
    pub symlink_target: Option<String>,
}

pub struct SnapshotConfig {
    pub folder: PathBuf,
    pub output: Option<PathBuf>,
    pub algo: HashAlgo,
    pub depth: Option<usize>,
    pub no_recursive: bool,
    pub hidden: bool,
    pub types: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub symlinks: SymlinkMode,
    pub threads: Option<usize>,
}

pub fn create_snapshot(config: SnapshotConfig) -> Result<()> {
    // Fix #5: silently ignore if global pool is already initialised
    if let Some(num_threads) = config.threads {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global();
    }

    if io::stdout().is_terminal() {
        println!("{}", "Creating Snapshot...".bright_cyan());
    }

    let (files, errors) = collect_files(
        &config.folder,
        config.depth,
        config.no_recursive,
        config.hidden,
        &config.types,
        &config.ignore,
        config.symlinks,
    )?;

    for e in &errors {
        eprintln!("[{}] {}", "ERROR".red(), e.error);
    }

    let pb = if io::stderr().is_terminal() {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::default_bar().template(
            "{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})",
        )?);
        Some(pb)
    } else {
        None
    };

    let entries: Vec<SnapshotEntry> = files
        .par_iter()
        .map(|f| {
            if let Some(ref p) = pb {
                p.inc(1);
            }
            // Fix #10: surface hash errors instead of silently storing None hashes.
            // We propagate the error so the snapshot is not saved with corrupt data.
            let h = compute_hashes(&f.path, config.algo)?;
            let rel = f
                .path
                .strip_prefix(&config.folder)
                .unwrap_or(&f.path)
                .to_path_buf();
            Ok(SnapshotEntry {
                rel_path: rel,
                size: f.size,
                modified: f.modified,
                hashes: h,
                symlink_target: f.symlink_target.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(ref p) = pb {
        p.finish_with_message("Snapshot complete");
    }

    // Fix #6: persist scan parameters so verify can reproduce the same walk.
    let scan_params = SnapshotScanParams {
        depth: config.depth,
        no_recursive: config.no_recursive,
        hidden: config.hidden,
        types: config.types.clone(),
        ignore: config.ignore.clone(),
        symlinks: config.symlinks,
    };

    let snapshot = Snapshot {
        created_at: chrono::Local::now().to_rfc3339(),
        root_path: config.folder.to_string_lossy().to_string(),
        files: entries,
        algo: config.algo,
        scan_params: Some(scan_params),
    };

    let json = serde_json::to_string_pretty(&snapshot)?;

    if let Some(out_path) = config.output {
        let mut f = File::create(&out_path)?;
        f.write_all(json.as_bytes())?;
        println!("Snapshot saved to {}", out_path.display());
    } else {
        println!("{}", json);
    }

    Ok(())
}

pub struct VerifyConfig {
    pub folder: PathBuf,
    pub snapshot_path: PathBuf,
    pub threads: Option<usize>,
    pub output_format: OutputFormat,
    pub verbose: bool,
}

pub fn verify_snapshot(config: VerifyConfig) -> Result<ExitStatus> {
    // Fix #5: silently ignore if global pool is already initialised
    if let Some(num_threads) = config.threads {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global();
    }

    let start_time = Instant::now();
    let snapshot_file = File::open(&config.snapshot_path)?;
    let snapshot: Snapshot = serde_json::from_reader(snapshot_file)?;

    println!(
        "Verifying against snapshot created at: {}",
        snapshot.created_at.cyan()
    );

    // Fix #6: reproduce the exact scan parameters used when the snapshot was created.
    // For old snapshots without scan_params, fall back to sensible defaults.
    let sp = snapshot.scan_params.as_ref();
    let depth          = sp.and_then(|p| p.depth);
    let no_recursive   = sp.map(|p| p.no_recursive).unwrap_or(false);
    let hidden         = sp.map(|p| p.hidden).unwrap_or(false);
    let types          = sp.and_then(|p| p.types.clone());
    let ignore         = sp.and_then(|p| p.ignore.clone());
    let symlink_mode   = sp.map(|p| p.symlinks).unwrap_or(SymlinkMode::Ignore);

    let (current_files, current_errors) = collect_files(
        &config.folder,
        depth,
        no_recursive,
        hidden,
        &types,
        &ignore,
        symlink_mode,
    )?;

    let current_map: HashMap<PathBuf, FileEntry> = current_files
        .into_iter()
        .map(|f| {
            (
                f.path
                    .strip_prefix(&config.folder)
                    .unwrap_or(&f.path)
                    .to_path_buf(),
                f,
            )
        })
        .collect();

    let snapshot_map: HashMap<PathBuf, SnapshotEntry> = snapshot
        .files
        .into_iter()
        .map(|f| (f.rel_path.clone(), f))
        .collect();

    let pb = if io::stderr().is_terminal() {
        let pb = ProgressBar::new(snapshot_map.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} Verifying {bar:40.cyan/blue} {pos}/{len}")?,
        );
        Some(pb)
    } else {
        None
    };

    let snapshot_keys: Vec<PathBuf> = snapshot_map.keys().cloned().collect();

    let mut results: Vec<ComparisonResult> = snapshot_keys
        .par_iter()
        .map(|rel_path| {
            if let Some(ref p) = pb {
                p.inc(1);
            }
            let snap_entry = snapshot_map.get(rel_path).unwrap();

            if let Some(curr_entry) = current_map.get(rel_path) {
                // Fix #10: propagate hashing errors instead of silently treating
                // them as DIFF (the old unwrap_or behaviour).
                let h = compute_hashes(&curr_entry.path, snapshot.algo)
                    .context("Failed to hash file during verification")?;

                let status = match snapshot.algo {
                    HashAlgo::Sha256 => {
                        if h.sha256 == snap_entry.hashes.sha256 { Status::Match } else { Status::Diff }
                    }
                    HashAlgo::Blake3 => {
                        if h.blake3 == snap_entry.hashes.blake3 { Status::Match } else { Status::Diff }
                    }
                    HashAlgo::Both => {
                        if h.sha256 == snap_entry.hashes.sha256
                            && h.blake3 == snap_entry.hashes.blake3
                        {
                            Status::Match
                        } else {
                            Status::Diff
                        }
                    }
                };

                Ok(ComparisonResult {
                    file: rel_path.clone(),
                    status,
                    hash1: Some(snap_entry.hashes.clone()),
                    hash2: Some(h),
                    size1: Some(snap_entry.size),
                    size2: Some(curr_entry.size),
                    modified1: None,
                    modified2: None,
                    symlink1: snap_entry.symlink_target.clone(),
                    symlink2: curr_entry.symlink_target.clone(),
                })
            } else {
                // Fix #12: use constructor helper
                let mut r = ComparisonResult::missing(rel_path.clone());
                r.hash1 = Some(snap_entry.hashes.clone());
                r.size1 = Some(snap_entry.size);
                r.symlink1 = snap_entry.symlink_target.clone();
                Ok(r)
            }
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(ref p) = pb {
        p.finish_with_message("Verification complete");
    }

    // Collect all paths that were checked in the snapshot loop.
    // Clone into an owned HashSet so the immutable borrow on `results`
    // is released before we push EXTRA entries. (Fix for E0502)
    let checked_paths: std::collections::HashSet<PathBuf> =
        results.iter().map(|r| r.file.clone()).collect();

    // Gather EXTRA entries separately so we avoid a simultaneous mut/immut borrow.
    let extras: Vec<ComparisonResult> = current_map
        .iter()
        .filter(|(rel_path, _)| !checked_paths.contains(*rel_path))
        .map(|(rel_path, curr_entry)| {
            let mut r = ComparisonResult::extra(rel_path.clone());
            r.size2 = Some(curr_entry.size);
            r.symlink2 = curr_entry.symlink_target.clone();
            r
        })
        .collect();
    results.extend(extras);

    results.sort_by(|a, b| a.file.cmp(&b.file));

    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;
    let mut extra = 0;
    for r in &results {
        match r.status {
            Status::Match   => matches += 1,
            Status::Diff    => diffs += 1,
            Status::Missing => missing += 1,
            Status::Extra   => extra += 1,
            _               => (),
        }
    }

    let report_conf = ReportConfig {
        mode: Mode::Batch,
        algo: snapshot.algo,
        threads: config.threads,
        verbose: config.verbose,
    };

    let summary_data = SummaryData {
        total: results.len(),
        matches,
        diffs,
        missing,
        extra,
        errors: current_errors.len(),
        elapsed: start_time.elapsed(),
    };

    let report = match config.output_format {
        OutputFormat::Txt => generate_text_report(
            &results,
            &[],
            &current_errors,
            &summary_data,
            &report_conf,
        )?,
        OutputFormat::Json => generate_json_report(&results, &[], &current_errors, &summary_data)?,
    };

    println!("{}", report);

    if !current_errors.is_empty() {
        Ok(ExitStatus::Error)
    } else if diffs > 0 || missing > 0 || extra > 0 {
        Ok(ExitStatus::Diff)
    } else {
        Ok(ExitStatus::Success)
    }
}
