use anyhow::{Context, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Instant;

use crate::compare::ExitStatus;
use crate::models::{ComparisonResult, FileEntry, HashAlgo, Mode, Status, SymlinkMode};
use crate::report::{ReportConfig, SummaryData, generate_summary_text, print_error_entry};
use crate::utils::{collect_files, compute_hashes};

pub struct SyncConfig {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub dry_run: bool,
    pub delete_extraneous: bool,
    pub no_delete: bool,
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

    // Fix #5: silently ignore if global pool is already initialised
    if let Some(num_threads) = config.threads {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global();
    }

    if io::stdout().is_terminal() {
        println!(
            "{}",
            "==============================================".bright_blue()
        );
        println!("  Folder Synchronization Utility");
        println!(
            "{}",
            "==============================================".bright_blue()
        );
        if config.dry_run {
            println!(
                "{} {}",
                "DRY RUN: No changes will be made.".yellow().bold(),
                "(Remove --dry-run to apply changes)".dimmed()
            );
        }
        println!();
    }

    // Fix #1: collect both folders in parallel (was sequential in original)
    let (res_source, res_dest) = rayon::join(
        || {
            collect_files(
                &config.source,
                config.depth,
                config.no_recursive,
                config.hidden,
                &config.types,
                &config.ignore,
                config.symlinks,
            )
        },
        || {
            collect_files(
                &config.destination,
                config.depth,
                config.no_recursive,
                config.hidden,
                &config.types,
                &config.ignore,
                config.symlinks,
            )
        },
    );
    let (source_files, source_errors) = res_source?;
    let (dest_files, dest_errors) = res_dest?;

    for e in &source_errors {
        print_error_entry(e, "source");
    }
    for e in &dest_errors {
        print_error_entry(e, "destination");
    }

    let total_errors = source_errors.len() + dest_errors.len();

    let source_map: HashMap<PathBuf, FileEntry> = source_files
        .into_par_iter()
        .map(|f| {
            (
                f.path.strip_prefix(&config.source).unwrap().to_path_buf(),
                f,
            )
        })
        .collect();
    let dest_map: HashMap<PathBuf, FileEntry> = dest_files
        .into_par_iter()
        .map(|f| {
            (
                f.path
                    .strip_prefix(&config.destination)
                    .unwrap()
                    .to_path_buf(),
                f,
            )
        })
        .collect();

    let source_paths: HashSet<&PathBuf> = source_map.keys().collect();
    let dest_paths:   HashSet<&PathBuf> = dest_map.keys().collect();

    let common_paths: Vec<PathBuf> = source_paths
        .intersection(&dest_paths)
        .map(|p| (*p).clone())
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

    let sync_actions: Vec<ComparisonResult> = common_paths
        .par_iter()
        .filter_map(|rel_path| {
            if let Some(ref p) = pb {
                p.inc(1);
            }
            let source_entry = source_map.get(rel_path).unwrap();
            let dest_entry   = dest_map.get(rel_path).unwrap();

            // Fix #2: fast-path size check applies regardless of algorithm —
            //   if sizes differ we already know it's a DIFF, no hashing needed.
            if source_entry.size != dest_entry.size {
                return Some(Ok(ComparisonResult {
                    file: rel_path.clone(),
                    status: Status::Diff,
                    hash1: None,
                    hash2: None,
                    size1: Some(source_entry.size),
                    size2: Some(dest_entry.size),
                    modified1: None,
                    modified2: None,
                    symlink1: source_entry.symlink_target.clone(),
                    symlink2: dest_entry.symlink_target.clone(),
                }));
            }

            // Fix #7: metadata fast-path now applies for *every* algorithm, not
            //   only HashAlgo::Both. (Original code had `&& config.algo == HashAlgo::Both`
            //   which meant Sha256 and Blake3 modes always hashed even on matching metadata.)
            if source_entry.modified == dest_entry.modified {
                return None; // Same size + same mtime → skip hashing
            }

            let (h_source_res, h_dest_res) = rayon::join(
                || compute_hashes(&source_entry.path, config.algo),
                || compute_hashes(&dest_entry.path, config.algo),
            );

            let result = match (h_source_res, h_dest_res) {
                (Ok(h_source), Ok(h_dest)) => {
                    let is_diff = match config.algo {
                        HashAlgo::Sha256 => h_source.sha256 != h_dest.sha256,
                        HashAlgo::Blake3 => h_source.blake3 != h_dest.blake3,
                        HashAlgo::Both   => {
                            h_source.sha256 != h_dest.sha256
                                || h_source.blake3 != h_dest.blake3
                        }
                    };
                    is_diff
                }
                _ => true, // Treat hashing errors as differences
            };

            if result {
                Some(Ok(ComparisonResult {
                    file: rel_path.clone(),
                    status: Status::Diff,
                    hash1: None,
                    hash2: None,
                    size1: Some(source_entry.size),
                    size2: Some(dest_entry.size),
                    modified1: None,
                    modified2: None,
                    symlink1: source_entry.symlink_target.clone(),
                    symlink2: dest_entry.symlink_target.clone(),
                }))
            } else {
                None
            }
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(ref p) = pb {
        p.finish_with_message("Comparison complete for common files");
    }

    let mut actions: Vec<ComparisonResult> = Vec::new();
    let mut created_count = 0;
    let mut updated_count = 0;
    let mut deleted_count = 0;

    // Files only in source → CREATE in destination
    // Fix #12: use constructor helpers
    for rel_path in source_paths.difference(&dest_paths) {
        let mut r = ComparisonResult::missing((*rel_path).clone());
        r.status = Status::Create;
        actions.push(r);
    }

    // Files only in destination → DELETE from destination
    if config.delete_extraneous && !config.no_delete {
        for rel_path in dest_paths.difference(&source_paths) {
            let mut r = ComparisonResult::extra((*rel_path).clone());
            r.status = Status::Delete;
            actions.push(r);
        }
    }

    // Common files that differ → UPDATE in destination
    for mut res in sync_actions {
        res.status = Status::Update;
        actions.push(res);
    }

    actions.sort_by(|a, b| a.file.cmp(&b.file));

    if io::stdout().is_terminal() {
        println!("\nApplying synchronization actions...");
    }

    let action_pb = if io::stderr().is_terminal() {
        let pb = ProgressBar::new(actions.len() as u64);
        pb.set_style(ProgressStyle::default_bar().template(
            "{spinner:.green} [Elap>{elapsed_precise}] {msg}: {bar:40.cyan/blue} {pos}/{len} ({eta})",
        )?);
        Some(pb)
    } else {
        None
    };

    for action in actions {
        if let Some(ref p) = action_pb {
            p.inc(1);
            p.set_message(format!("Processing {}", action.file.display()));
        }
        let source_path = config.source.join(&action.file);
        let dest_path   = config.destination.join(&action.file);

        if config.dry_run {
            match action.status {
                Status::Create => println!(
                    "{} (Dry Run): Will create {}",
                    "CREATE".green().bold(),
                    dest_path.display()
                ),
                Status::Update => println!(
                    "{} (Dry Run): Will update {}",
                    "UPDATE".yellow().bold(),
                    dest_path.display()
                ),
                Status::Delete => println!(
                    "{} (Dry Run): Will delete {}",
                    "DELETE".red().bold(),
                    dest_path.display()
                ),
                _ => {}
            }
        } else {
            match action.status {
                Status::Create | Status::Update => {
                    let parent = dest_path
                        .parent()
                        .context("Failed to get parent directory")?;
                    fs::create_dir_all(parent)?;
                    fs::copy(&source_path, &dest_path)?;
                    if action.status == Status::Create {
                        created_count += 1;
                        println!("{} {}", "CREATED".green(), dest_path.display());
                    } else {
                        updated_count += 1;
                        println!("{} {}", "UPDATED".yellow(), dest_path.display());
                    }
                }
                Status::Delete => {
                    fs::remove_file(&dest_path)?;
                    deleted_count += 1;
                    println!("{} {}", "DELETED".red(), dest_path.display());
                }
                _ => {}
            }
        }
    }

    if let Some(ref p) = action_pb {
        p.finish_with_message("Actions applied");
    }

    let elapsed = start_time.elapsed();
    let total_actions = created_count + updated_count + deleted_count;

    let report_conf = ReportConfig {
        mode: Mode::Batch,
        algo: config.algo,
        threads: config.threads,
        verbose: false,
    };

    let summary_data = SummaryData {
        total:   total_actions,
        matches: 0,
        diffs:   updated_count,
        missing: created_count,
        extra:   deleted_count,
        errors:  total_errors,
        elapsed,
    };

    let summary_lines = generate_summary_text(&summary_data, &report_conf);
    for line in summary_lines {
        println!("{}", line);
    }

    if total_errors > 0 {
        Ok(ExitStatus::Error)
    } else if created_count > 0 || updated_count > 0 || deleted_count > 0 {
        Ok(ExitStatus::Diff)
    } else {
        Ok(ExitStatus::Success)
    }
}
