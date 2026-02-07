use std::path::PathBuf;
use std::fs;
use std::io::{self, IsTerminal};
use std::time::Instant;
use std::collections::{HashMap, HashSet};
use anyhow::{Context, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::models::{FileEntry, ComparisonResult, HashAlgo, Mode, SymlinkMode};
use crate::utils::{collect_files, compute_hashes};
use crate::compare::ExitStatus;
use crate::report::{generate_summary_text, print_error_entry, ReportConfig};

pub struct SyncConfig {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub dry_run: bool,
    pub delete_extraneous: bool,
    pub no_delete: bool, // Conflicts with delete_extraneous
    pub algo: HashAlgo,
    pub depth: Option<usize>,
    pub no_recursive: bool,
    pub symlinks: SymlinkMode,
    pub hidden: bool,
    pub types: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub threads: Option<usize>,
}

pub fn run_sync(config: SyncConfig) -> Result<ExitStatus> {
    let start_time = Instant::now();

    if let Some(num_threads) = config.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .context("Failed to set Rayon thread pool size")?;
    }

    if io::stdout().is_terminal() {
        println!("{}", "==============================================".bright_blue());
        println!("  Folder Synchronization Utility");
        println!("{}", "==============================================".bright_blue());
        if config.dry_run {
            println!("{} {}", "DRY RUN: No changes will be made.".yellow().bold(), "(Remove --dry-run to apply changes)".dimmed());
        }
        println!();
    }

    // 1. Collect files from source and destination
    let (source_files, source_errors) = collect_files(
        &config.source,
        config.depth,
        config.no_recursive,
        config.hidden,
        &config.types,
        &config.ignore,
        config.symlinks,
    )?;

    let (dest_files, dest_errors) = collect_files(
        &config.destination,
        config.depth,
        config.no_recursive,
        config.hidden,
        &config.types,
        &config.ignore,
        config.symlinks,
    )?;

    for e in &source_errors {
        print_error_entry(e, "source");
    }
    for e in &dest_errors {
        print_error_entry(e, "destination");
    }

    let total_errors = source_errors.len() + dest_errors.len();

    let source_map: HashMap<PathBuf, FileEntry> = source_files
        .into_par_iter()
        .map(|f| (f.path.strip_prefix(&config.source).unwrap().to_path_buf(), f))
        .collect();
    let dest_map: HashMap<PathBuf, FileEntry> = dest_files
        .into_par_iter()
        .map(|f| (f.path.strip_prefix(&config.destination).unwrap().to_path_buf(), f))
        .collect();

    let source_paths: HashSet<PathBuf> = source_map.keys().cloned().collect();
    let dest_paths: HashSet<PathBuf> = dest_map.keys().cloned().collect();

    let common_paths: Vec<PathBuf> = source_paths
        .intersection(&dest_paths)
        .cloned()
        .collect();

    let pb = if io::stderr().is_terminal() {
        let pb = ProgressBar::new(common_paths.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [Elap>{elapsed_precise}] [ {bar:40.cyan/blue} ] {pos}/{len} (Rema>{eta})")?
                .progress_chars("#>- ")
        );
        pb.set_draw_target(indicatif::ProgressDrawTarget::stderr_with_hz(10));
        Some(pb)
    } else {
        None
    };

    let mut sync_actions: Vec<ComparisonResult> = common_paths
        .par_iter()
        .filter_map(|rel_path| {
            if let Some(ref p) = pb { p.inc(1); }
            let source_entry = source_map.get(rel_path).unwrap();
            let dest_entry = dest_map.get(rel_path).unwrap();

            // Compare files
            // For sync, we only care about if they are different or not.
            // If same size and mod time, skip hash for Metadata mode equivalent.
            if source_entry.size == dest_entry.size && config.algo == HashAlgo::Both && source_entry.modified == dest_entry.modified {
                return None; // No action needed
            }

            let (h_source_res, h_dest_res) = rayon::join(
                || compute_hashes(&source_entry.path, config.algo),
                || compute_hashes(&dest_entry.path, config.algo),
            );

            let is_diff = match (h_source_res, h_dest_res) {
                (Ok(h_source), Ok(h_dest)) => {
                    match config.algo {
                        HashAlgo::Sha256 => h_source.sha256 != h_dest.sha256,
                        HashAlgo::Blake3 => h_source.blake3 != h_dest.blake3,
                        HashAlgo::Both => h_source.sha256 != h_dest.sha256 || h_source.blake3 != h_dest.blake3,
                    }
                },
                _ => true, // Treat hashing errors as differences
            };

            if is_diff {
                Some(ComparisonResult {
                    file: rel_path.clone(),
                    status: "DIFF".to_string(), // Will be "UPDATE"
                    hash1: None, hash2: None,
                    size1: Some(source_entry.size), size2: Some(dest_entry.size),
                    modified1: None, modified2: None,
                    symlink1: source_entry.symlink_target.clone(),
                    symlink2: dest_entry.symlink_target.clone(),
                })
            } else {
                None // No action needed
            }
        })
        .collect();

    if let Some(ref p) = pb { p.finish_with_message("Comparison complete for common files"); }

    // Identify MISSING (in dest, but not in source) for deletion or EXTRA (in source, not in dest) for creation
    let mut actions: Vec<ComparisonResult> = Vec::new();
    let mut created_count = 0;
    let mut updated_count = 0;
    let mut deleted_count = 0;

    // Files only in source (CREATE in destination)
    for rel_path in source_paths.difference(&dest_paths) {
        actions.push(ComparisonResult {
            file: rel_path.clone(),
            status: "CREATE".to_string(),
            hash1: None, hash2: None,
            size1: None, size2: None,
            modified1: None, modified2: None,
            symlink1: None, symlink2: None,
        });
    }

    // Files only in destination (DELETE from destination)
    if config.delete_extraneous {
        for rel_path in dest_paths.difference(&source_paths) {
            actions.push(ComparisonResult {
                file: rel_path.clone(),
                status: "DELETE".to_string(),
                hash1: None, hash2: None,
                size1: None, size2: None,
                modified1: None, modified2: None,
                symlink1: None, symlink2: None,
            });
        }
    }

    // Add identified differences (UPDATE in destination)
    for mut res in sync_actions {
        res.status = "UPDATE".to_string();
        actions.push(res);
    }

    actions.sort_by(|a, b| a.file.cmp(&b.file));

    // Apply actions
    if io::stdout().is_terminal() {
        println!("
Applying synchronization actions...");
    }
    
    let action_pb = if io::stderr().is_terminal() {
        let pb = ProgressBar::new(actions.len() as u64);
        pb.set_style(ProgressStyle::default_bar().template("{spinner:.green} [Elap>{elapsed_precise}] {msg}: {bar:40.cyan/blue} {pos}/{len} ({eta})")?);
        Some(pb)
    } else { None };

    for action in actions {
        if let Some(ref p) = action_pb { p.inc(1); p.set_message(format!("Processing {}", action.file.display())); }
        let source_path = config.source.join(&action.file);
        let dest_path = config.destination.join(&action.file);

        if config.dry_run {
            match action.status.as_str() {
                "CREATE" => println!("{} (Dry Run): Will create {}", "CREATE".green().bold(), dest_path.display()),
                "UPDATE" => println!("{} (Dry Run): Will update {}", "UPDATE".yellow().bold(), dest_path.display()),
                "DELETE" => println!("{} (Dry Run): Will delete {}", "DELETE".red().bold(), dest_path.display()),
                _ => {}
            }
        } else {
            match action.status.as_str() {
                "CREATE" | "UPDATE" => {
                    let parent = dest_path.parent().context("Failed to get parent directory")?;
                    fs::create_dir_all(parent)?;
                    fs::copy(&source_path, &dest_path)?;
                    if action.status == "CREATE" {
                        created_count += 1;
                        println!("{} {}", "CREATED".green(), dest_path.display());
                    } else {
                        updated_count += 1;
                        println!("{} {}", "UPDATED".yellow(), dest_path.display());
                    }
                },
                "DELETE" => {
                    fs::remove_file(&dest_path)?;
                    deleted_count += 1;
                    println!("{} {}", "DELETED".red(), dest_path.display());
                },
                _ => {}
            }
        }
    }
    if let Some(ref p) = action_pb { p.finish_with_message("Actions applied"); }


    let elapsed = start_time.elapsed();
    let total_actions = created_count + updated_count + deleted_count;

    // Report Summary
    let report_conf = ReportConfig {
        mode: Mode::Batch, // Sync operates like batch internally
        algo: config.algo,
        output_format: crate::models::OutputFormat::Txt, // Always text for sync summary
        output_folder: None,
        no_sort: false,
        threads: config.threads,
        verbose: false, // Sync summary is not verbose on hashes
    };

    let summary_lines = generate_summary_text(
        total_actions,
        0, // Matches are not counted as actions
        updated_count, // Diffs are updates
        created_count, // Missing are creations
        deleted_count, // Extra are deletions
        total_errors,
        elapsed,
        &report_conf
    );

    for line in summary_lines {
        println!("{}", line);
    }

    if total_errors > 0 {
        Ok(ExitStatus::Error)
    } else if created_count > 0 || updated_count > 0 || deleted_count > 0 {
        Ok(ExitStatus::Diff) // Indicate changes were made
    } else {
        Ok(ExitStatus::Success)
    }
}
