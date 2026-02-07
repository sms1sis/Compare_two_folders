use std::path::PathBuf;
use std::fs::File;
use std::io::{self, Write, IsTerminal};
use std::time::Instant;
use std::collections::HashMap;
use anyhow::{Context, Result};
use colored::*;
use serde::{Serialize, Deserialize};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::models::{HashResult, HashAlgo, FileEntry, ComparisonResult, SymlinkMode, OutputFormat, Mode};
use crate::utils::{collect_files, compute_hashes};
use crate::compare::ExitStatus;
use crate::report::{generate_text_report, generate_json_report, ReportConfig};

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub created_at: String,
    pub root_path: String,
    pub files: Vec<SnapshotEntry>,
    pub algo: HashAlgo,
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
    if let Some(num_threads) = config.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .context("Failed to set Rayon thread pool size")?;
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
        pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")?);
        Some(pb)
    } else {
        None
    };

    let entries: Vec<SnapshotEntry> = files.par_iter().map(|f| {
        if let Some(ref p) = pb { p.inc(1); }
        let h = compute_hashes(&f.path, config.algo).unwrap_or(HashResult { sha256: None, blake3: None });
        let rel = f.path.strip_prefix(&config.folder).unwrap_or(&f.path).to_path_buf();
        SnapshotEntry {
            rel_path: rel,
            size: f.size,
            modified: f.modified,
            hashes: h,
            symlink_target: f.symlink_target.clone(),
        }
    }).collect();

    if let Some(ref p) = pb { p.finish_with_message("Snapshot complete"); }

    let snapshot = Snapshot {
        created_at: chrono::Local::now().to_rfc3339(),
        root_path: config.folder.to_string_lossy().to_string(),
        files: entries,
        algo: config.algo,
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
     if let Some(num_threads) = config.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .context("Failed to set Rayon thread pool size")?;
    }

    let start_time = Instant::now();
    let snapshot_file = File::open(&config.snapshot_path)?;
    let snapshot: Snapshot = serde_json::from_reader(snapshot_file)?;
    
    println!("Verifying against snapshot created at: {}", snapshot.created_at.cyan());

    // Collect current state
    let (current_files, current_errors) = collect_files(
        &config.folder,
        None, // Default to full recursive as snapshot implies state
        false,
        true, // Assume snapshot might cover hidden files, or should we pass flags?
              // Ideally snapshot stores what it saw.
        &None, &None,
        SymlinkMode::Compare, // Usually we want to compare what we see
    )?;

    let current_map: HashMap<PathBuf, FileEntry> = current_files.into_iter()
        .map(|f| (f.path.strip_prefix(&config.folder).unwrap_or(&f.path).to_path_buf(), f))
        .collect();

    let snapshot_map: HashMap<PathBuf, SnapshotEntry> = snapshot.files.into_iter()
        .map(|f| (f.rel_path.clone(), f))
        .collect();

    let pb = if io::stderr().is_terminal() {
         let pb = ProgressBar::new(snapshot_map.len() as u64);
         pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} Verifying {bar:40.cyan/blue} {pos}/{len}")?);
         Some(pb)
    } else { None };

    // Check files in snapshot
    let snapshot_keys: Vec<PathBuf> = snapshot_map.keys().cloned().collect();
    
    // We iterate over snapshot keys to find MATCH/DIFF/MISSING
    let mut checked_paths = std::collections::HashSet::new();

    let mut results: Vec<ComparisonResult> = snapshot_keys.par_iter().map(|rel_path| {
        if let Some(ref p) = pb { p.inc(1); }
        let snap_entry = snapshot_map.get(rel_path).unwrap();
        
        if let Some(curr_entry) = current_map.get(rel_path) {
            // Compare
            let h = compute_hashes(&curr_entry.path, snapshot.algo).unwrap_or(HashResult { sha256: None, blake3: None });
            
            let status = match snapshot.algo {
                HashAlgo::Sha256 => if h.sha256 == snap_entry.hashes.sha256 { "MATCH" } else { "DIFF" },
                HashAlgo::Blake3 => if h.blake3 == snap_entry.hashes.blake3 { "MATCH" } else { "DIFF" },
                HashAlgo::Both => if h.sha256 == snap_entry.hashes.sha256 && h.blake3 == snap_entry.hashes.blake3 { "MATCH" } else { "DIFF" },
            };

            ComparisonResult {
                file: rel_path.clone(),
                status: status.to_string(),
                hash1: Some(snap_entry.hashes.clone()),
                hash2: Some(h),
                size1: Some(snap_entry.size),
                size2: Some(curr_entry.size),
                modified1: None, modified2: None, // Could format if needed
                symlink1: snap_entry.symlink_target.clone(),
                symlink2: curr_entry.symlink_target.clone(),
            }
        } else {
            ComparisonResult {
                file: rel_path.clone(),
                status: "MISSING".to_string(), // Missing in current folder
                hash1: Some(snap_entry.hashes.clone()), hash2: None,
                size1: Some(snap_entry.size), size2: None,
                modified1: None, modified2: None,
                symlink1: snap_entry.symlink_target.clone(), symlink2: None,
            }
        }
    }).collect();

    if let Some(ref p) = pb { p.finish_with_message("Verification complete"); }
    
    for r in &results {
        checked_paths.insert(r.file.clone());
    }

    // Check for EXTRA files (in current but not in snapshot)
    for (rel_path, curr_entry) in &current_map {
        if !checked_paths.contains(rel_path) {
            results.push(ComparisonResult {
                file: rel_path.clone(),
                status: "EXTRA".to_string(),
                hash1: None, hash2: None,
                size1: None, size2: Some(curr_entry.size),
                modified1: None, modified2: None,
                symlink1: None, symlink2: curr_entry.symlink_target.clone(),
            });
        }
    }

    results.sort_by(|a, b| a.file.cmp(&b.file));

    // Generate Report
    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;
    let mut extra = 0;
    for r in &results {
        match r.status.as_str() {
            "MATCH" => matches += 1,
            "DIFF" => diffs += 1,
            "MISSING" => missing += 1,
            "EXTRA" => extra += 1,
            _ => (),
        }
    }
    
    let report_conf = ReportConfig {
        mode: Mode::Batch,
        algo: snapshot.algo,
        output_format: config.output_format,
        output_folder: None,
        no_sort: false,
        threads: config.threads,
        verbose: config.verbose,
    };
    
    let report = match config.output_format {
        OutputFormat::Txt => generate_text_report(
            &results, 
            &[], // Snapshot errors?
            &current_errors, 
            results.len(), matches, diffs, missing, extra, current_errors.len(), 
            start_time.elapsed(), 
            &report_conf
        )?,
        OutputFormat::Json => generate_json_report(
            &results, &[], &current_errors, results.len(), matches, diffs, missing, extra, current_errors.len(), start_time.elapsed()
        )?,
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
