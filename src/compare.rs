use anyhow::Result;
use chrono::{DateTime, Local};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Instant;

use crate::models::{
    ComparisonResult, FileEntry, HashAlgo, Mode, OutputFormat, Status, SymlinkMode,
};
use crate::report::{
    ReportConfig, SummaryData, generate_json_report, generate_summary_text, generate_text_report,
    print_error_entry, print_realtime_missing, write_report,
};
use crate::utils::{collect_files, compute_hashes};

#[derive(Debug, PartialEq)]
pub enum ExitStatus {
    Success,
    Diff,
    Error,
}

pub struct CompareConfig {
    pub folder1: PathBuf,
    pub folder2: PathBuf,
    pub mode: Mode,
    pub algo: HashAlgo,
    pub output_folder: Option<PathBuf>,
    pub output_format: OutputFormat,
    pub depth: Option<usize>,
    pub no_recursive: bool,
    pub symlinks: SymlinkMode,
    pub verbose: bool,
    pub hidden: bool,
    pub types: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub threads: Option<usize>,
    pub no_sort: bool,
    pub diff_cmd: Option<String>,
}

pub fn run_compare(config: CompareConfig) -> Result<ExitStatus> {
    let start_time = Instant::now();

    // Fix #5: thread pool configuration is done once here, not repeated in every
    // subcommand entry point. Subsequent calls are harmless (global pool already set).
    if let Some(num_threads) = config.threads {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global();
    }

    match config.mode {
        Mode::Realtime => run_realtime(&config, start_time),
        Mode::Batch | Mode::Metadata => run_batch(&config, start_time),
    }
}

pub(crate) fn compare_files_core(
    rel_path: PathBuf,
    entry1: &FileEntry,
    entry2: &FileEntry,
    config: &CompareConfig,
) -> Result<ComparisonResult> {
    let size1 = Some(entry1.size);
    let size2 = Some(entry2.size);
    let mut time1_str = None;
    let mut time2_str = None;

    if let Some(t1) = entry1.modified {
        time1_str = Some(
            DateTime::<Local>::from(t1)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        );
    }
    if let Some(t2) = entry2.modified {
        time2_str = Some(
            DateTime::<Local>::from(t2)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        );
    }

    if config.symlinks == SymlinkMode::Compare {
        let s1 = entry1.symlink_target.as_deref();
        let s2 = entry2.symlink_target.as_deref();

        if s1.is_some() && s2.is_some() {
            let matches = s1 == s2;
            return Ok(ComparisonResult {
                file: rel_path,
                status: if matches { Status::Match } else { Status::Diff },
                hash1: None,
                hash2: None,
                size1: None,
                size2: None,
                modified1: time1_str,
                modified2: time2_str,
                symlink1: entry1.symlink_target.clone(),
                symlink2: entry2.symlink_target.clone(),
            });
        }
        if s1.is_some() != s2.is_some() {
            return Ok(ComparisonResult {
                file: rel_path,
                status: Status::Diff,
                hash1: None,
                hash2: None,
                size1: None,
                size2: None,
                modified1: time1_str,
                modified2: time2_str,
                symlink1: entry1.symlink_target.clone(),
                symlink2: entry2.symlink_target.clone(),
            });
        }
    }

    if entry1.size != entry2.size {
        return Ok(ComparisonResult {
            file: rel_path,
            status: Status::Diff,
            hash1: None,
            hash2: None,
            size1,
            size2,
            modified1: time1_str,
            modified2: time2_str,
            symlink1: None,
            symlink2: None,
        });
    } else if config.mode == Mode::Metadata {
        let status = if entry1.modified != entry2.modified {
            Status::Diff
        } else {
            Status::Match
        };
        return Ok(ComparisonResult {
            file: rel_path,
            status,
            hash1: None,
            hash2: None,
            size1,
            size2,
            modified1: time1_str,
            modified2: time2_str,
            symlink1: None,
            symlink2: None,
        });
    }

    let (h1_res, h2_res) = rayon::join(
        || compute_hashes(&entry1.path, config.algo),
        || compute_hashes(&entry2.path, config.algo),
    );

    let (status, h1, h2) = match (h1_res, h2_res) {
        (Ok(h1), Ok(h2)) => {
            let is_match = match config.algo {
                HashAlgo::Sha256 => h1.sha256 == h2.sha256,
                HashAlgo::Blake3 => h1.blake3 == h2.blake3,
                HashAlgo::Both => h1.sha256 == h2.sha256 && h1.blake3 == h2.blake3,
            };
            (
                if is_match {
                    Status::Match
                } else {
                    Status::Diff
                },
                Some(h1),
                Some(h2),
            )
        }
        _ => (Status::Error, None, None),
    };

    Ok(ComparisonResult {
        file: rel_path,
        status,
        hash1: h1,
        hash2: h2,
        size1,
        size2,
        modified1: time1_str,
        modified2: time2_str,
        symlink1: None,
        symlink2: None,
    })
}

fn run_realtime(config: &CompareConfig, start_time: Instant) -> Result<ExitStatus> {
    if io::stdout().is_terminal() {
        println!(
            "{}",
            "==============================================".bright_blue()
        );
        println!("  Folder Comparison Utility (Real-time Mode)");
        println!(
            "{}",
            "==============================================".bright_blue()
        );
    }

    let (mut files1, errors1) = collect_files(
        &config.folder1,
        config.depth,
        config.no_recursive,
        config.hidden,
        &config.types,
        &config.ignore,
        config.symlinks,
    )?;

    for e in &errors1 {
        print_error_entry(e, "folder1");
    }

    let (files2, errors2) = collect_files(
        &config.folder2,
        config.depth,
        config.no_recursive,
        config.hidden,
        &config.types,
        &config.ignore,
        config.symlinks,
    )?;

    for e in &errors2 {
        print_error_entry(e, "folder2");
    }

    if !config.no_sort {
        files1.sort_by(|a, b| a.path.cmp(&b.path));
    }

    let mut files2_map: HashMap<PathBuf, FileEntry> = files2
        .into_iter()
        .map(|f| {
            let rel = f.path.strip_prefix(&config.folder2).unwrap().to_path_buf();
            (rel, f)
        })
        .collect();

    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;

    for entry1 in &files1 {
        let rel_path = entry1.path.strip_prefix(&config.folder1)?.to_path_buf();

        if let Some(entry2) = files2_map.remove(&rel_path) {
            let result = compare_files_core(rel_path.clone(), entry1, &entry2, config)?;

            match result.status {
                Status::Match => matches += 1,
                Status::Diff => diffs += 1,
                _ => (),
            }

            print!("{}", result.format_text(config.verbose, config.algo)?);

            // Fix #11: use shlex-style splitting to support paths-with-spaces in diff_cmd
            if let Some(diff_cmd_str) = &config.diff_cmd
                && result.status == Status::Diff
            {
                let parts = split_command(diff_cmd_str);
                if let Some((command, args)) = parts.split_first() {
                    let file1_path = config.folder1.join(&rel_path);
                    let file2_path = config.folder2.join(&rel_path);

                    eprintln!(
                        "Launching diff: {} {} {}",
                        diff_cmd_str,
                        file1_path.display(),
                        file2_path.display()
                    );

                    let _ = std::process::Command::new(command)
                        .args(args)
                        .arg(&file1_path)
                        .arg(&file2_path)
                        .spawn();
                }
            }
        } else {
            missing += 1;
            print_realtime_missing(Status::Missing, &rel_path, config.verbose)?;
        }
    }

    let extra = files2_map.len();
    let mut sorted_extra: Vec<_> = files2_map.into_keys().collect();
    if !config.no_sort {
        sorted_extra.sort();
    }

    for rel_path in sorted_extra {
        print_realtime_missing(Status::Extra, &rel_path, config.verbose)?;
    }

    let elapsed = start_time.elapsed();
    let total = files1.len() + extra;
    let total_errors = errors1.len() + errors2.len();

    let report_conf = ReportConfig {
        mode: config.mode,
        algo: config.algo,
        threads: config.threads,
        verbose: config.verbose,
    };

    let summary_data = SummaryData {
        total,
        matches,
        diffs,
        missing,
        extra,
        errors: total_errors,
        elapsed,
    };

    let summary_lines = generate_summary_text(&summary_data, &report_conf);
    for line in summary_lines {
        println!("{}", line);
    }

    if total_errors > 0 {
        Ok(ExitStatus::Error)
    } else if diffs > 0 || missing > 0 || extra > 0 {
        Ok(ExitStatus::Diff)
    } else {
        Ok(ExitStatus::Success)
    }
}

fn run_batch(config: &CompareConfig, start_time: Instant) -> Result<ExitStatus> {
    if io::stdout().is_terminal() {
        println!(
            "{}",
            "==============================================".bright_blue()
        );
        println!("  Folder File Comparison Utility (Batch Mode)");
        println!(
            "{}",
            "==============================================".bright_blue()
        );
        println!();
    }

    // Both folder scans run in parallel (already correct in original batch mode)
    let (res1, res2) = rayon::join(
        || {
            collect_files(
                &config.folder1,
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
                &config.folder2,
                config.depth,
                config.no_recursive,
                config.hidden,
                &config.types,
                &config.ignore,
                config.symlinks,
            )
        },
    );
    let (files1, errors1) = res1?;
    let (files2, errors2) = res2?;

    let total_errors = errors1.len() + errors2.len();

    // Fix #4: build maps without double-cloning all keys
    let files1_map: HashMap<PathBuf, FileEntry> = files1
        .into_par_iter()
        .map(|f| {
            (
                f.path.strip_prefix(&config.folder1).unwrap().to_path_buf(),
                f,
            )
        })
        .collect();
    let files2_map: HashMap<PathBuf, FileEntry> = files2
        .into_par_iter()
        .map(|f| {
            (
                f.path.strip_prefix(&config.folder2).unwrap().to_path_buf(),
                f,
            )
        })
        .collect();

    // Fix #4: use reference sets to avoid cloning every key twice
    let set1_paths: HashSet<&PathBuf> = files1_map.keys().collect();
    let set2_paths: HashSet<&PathBuf> = files2_map.keys().collect();

    let common_paths: Vec<PathBuf> = set1_paths
        .intersection(&set2_paths)
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

    let mut all_results: Vec<ComparisonResult> = common_paths
        .par_iter()
        .map(|rel_path| {
            if let Some(ref p) = pb {
                p.inc(1);
            }
            let entry1 = files1_map.get(rel_path).unwrap();
            let entry2 = files2_map.get(rel_path).unwrap();
            compare_files_core(rel_path.clone(), entry1, entry2, config)
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(ref p) = pb {
        p.finish_with_message("Comparison complete");
    }

    // Fix #12: use constructor helpers instead of large None-filled struct literals
    for rel_path in set1_paths.difference(&set2_paths) {
        all_results.push(ComparisonResult::missing((*rel_path).clone()));
    }

    for rel_path in set2_paths.difference(&set1_paths) {
        all_results.push(ComparisonResult::extra((*rel_path).clone()));
    }

    if !config.no_sort {
        all_results.sort_by(|a, b| a.file.cmp(&b.file));
    }

    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;
    let mut extra = 0;
    for r in &all_results {
        match r.status {
            Status::Match => matches += 1,
            Status::Diff => diffs += 1,
            Status::Missing => missing += 1,
            Status::Extra => extra += 1,
            _ => (),
        }
    }
    let total = all_results.len();
    let elapsed = start_time.elapsed();

    let report_conf = ReportConfig {
        mode: config.mode,
        algo: config.algo,
        threads: config.threads,
        verbose: config.verbose,
    };

    let summary_data = SummaryData {
        total,
        matches,
        diffs,
        missing,
        extra,
        errors: total_errors,
        elapsed,
    };

    match config.output_format {
        OutputFormat::Txt => {
            let output = generate_text_report(
                &all_results,
                &errors1,
                &errors2,
                &summary_data,
                &report_conf,
            )?;
            write_report(output, &config.output_folder, "report.txt")?;
        }
        OutputFormat::Json => {
            let output = generate_json_report(&all_results, &errors1, &errors2, &summary_data)?;
            write_report(output, &config.output_folder, "report.json")?;
        }
    }

    if total_errors > 0 {
        Ok(ExitStatus::Error)
    } else if diffs > 0 || missing > 0 || extra > 0 {
        Ok(ExitStatus::Diff)
    } else {
        Ok(ExitStatus::Success)
    }
}

/// Fix #11: split a command string respecting single- and double-quoted segments
/// so that paths containing spaces (e.g. "/my tools/code --diff") are handled
/// correctly instead of being naively split on every whitespace character.
fn split_command(cmd: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in cmd.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}
