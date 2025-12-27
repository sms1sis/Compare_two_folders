use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use chrono::{DateTime, Local};
use ignore::WalkBuilder;
use globset::{Glob, GlobSetBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum HashAlgo {
    Sha256,
    Blake3,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Txt,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    /// Processes files sequentially and prints results as they happen.
    Realtime,
    /// Processes files in parallel and prints a report at the end.
    Batch,
    /// Compare file size and modification time to skip cryptographic hashing.
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
enum SymlinkMode {
    #[default]
    Ignore,
    Follow,
    Compare,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Config {
    /// First folder to compare
    folder1: PathBuf,
    /// Second folder to compare
    folder2: PathBuf,

    #[arg(short, long, value_enum, default_value_t = Mode::Batch)]
    mode: Mode,

    #[arg(short, long, value_enum, default_value_t = HashAlgo::Blake3)]
    algo: HashAlgo,

    /// (Batch mode only) Folder to save the report file
    #[arg(short, long)]
    output_folder: Option<PathBuf>,

    /// (Batch mode only) Format for the output report
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Txt)]
    output_format: OutputFormat,

    /// Maximum recursion depth (default: infinite)
    #[arg(long)]
    depth: Option<usize>,

    /// Disable recursive comparison (equivalent to --depth 1)
    #[arg(long, conflicts_with = "depth")]
    no_recursive: bool,

    /// Handling strategy for symbolic links
    #[arg(long, value_enum, default_value_t = SymlinkMode::Ignore)]
    symlinks: SymlinkMode,

    /// Show hash values for matched and different files
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Include hidden files and folders in the comparison
    #[arg(short = 'H', long, default_value_t = false)]
    hidden: bool,

    /// File extensions to include in the comparison (e.g., .txt, .jpg)
    #[arg(short = 't', long = "type", action = clap::ArgAction::Append)]
    types: Option<Vec<String>>,

    /// A gitignore-style pattern to ignore. Can be used multiple times.
    #[arg(short = 'i', long, action = clap::ArgAction::Append)]
    ignore: Option<Vec<String>>,

    /// Number of threads to use for parallel processing (default: number of CPU cores)
    #[arg(short = 'j', long, value_name = "COUNT")]
    threads: Option<usize>,

    /// Disable alphabetical sorting of the output (improves performance)
    #[arg(short = 'n', long, default_value_t = false)]
    no_sort: bool,
}

#[derive(Debug, Clone, Serialize)]
struct HashResult {
    sha256: Option<String>,
    blake3: Option<String>,
}

#[derive(Debug, Clone)]
struct FileEntry {
    path: PathBuf,
    size: u64,
    modified: Option<std::time::SystemTime>,
    symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorEntry {
    path: PathBuf,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
struct ComparisonResult {
    file: PathBuf,
    status: String,
    hash1: Option<HashResult>,
    hash2: Option<HashResult>,
    size1: Option<u64>,
    size2: Option<u64>,
    modified1: Option<String>,
    modified2: Option<String>,
    symlink1: Option<String>,
    symlink2: Option<String>,
}

#[derive(PartialEq)]
enum ExitStatus {
    Success,
    Diff,
    Error,
}

fn main() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();

    // TTY Detection
    if !io::stdout().is_terminal() {
        colored::control::set_override(false);
    }
    
    match run() {
        Ok(status) => match status {
            ExitStatus::Success => std::process::exit(0),
            ExitStatus::Diff => std::process::exit(1),
            ExitStatus::Error => std::process::exit(2),
        },
        Err(e) => {
            eprintln!("Error: {:#}", e);
            std::process::exit(2);
        }
    }
}

fn run() -> Result<ExitStatus> {
    let start_time = Instant::now();
    let config = Config::parse();

    if let Some(num_threads) = config.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .context("Failed to set Rayon thread pool size")?;
    }

    match config.mode {
        Mode::Realtime => run_realtime(&config, start_time),
        Mode::Batch | Mode::Metadata => run_batch(&config, start_time),
    }
}

//=============================================================================
// Core Comparison Logic
//=============================================================================

fn compare_files(
    rel_path: PathBuf,
    entry1: &FileEntry,
    entry2: &FileEntry,
    config: &Config,
) -> Result<ComparisonResult> {
    let size1 = Some(entry1.size);
    let size2 = Some(entry2.size);
    let mut time1_str = None;
    let mut time2_str = None;

    if let Some(t1) = entry1.modified {
        time1_str = Some(DateTime::<Local>::from(t1).format("%Y-%m-%d %H:%M:%S").to_string());
    }
    if let Some(t2) = entry2.modified {
        time2_str = Some(DateTime::<Local>::from(t2).format("%Y-%m-%d %H:%M:%S").to_string());
    }

    // 1. Symlink Comparison
    if config.symlinks == SymlinkMode::Compare {
        let s1 = entry1.symlink_target.as_deref();
        let s2 = entry2.symlink_target.as_deref();
        
        // If both are symlinks, compare targets
        if s1.is_some() && s2.is_some() {
            let matches = s1 == s2;
            return Ok(ComparisonResult {
                file: rel_path,
                status: if matches { "MATCH".to_string() } else { "DIFF".to_string() },
                hash1: None, hash2: None,
                size1: None, size2: None, // Sizes irrelevant for symlink structure check usually
                modified1: time1_str, modified2: time2_str,
                symlink1: entry1.symlink_target.clone(),
                symlink2: entry2.symlink_target.clone(),
            });
        }
        // If one is symlink and other is file, it's a diff (type mismatch)
        if s1.is_some() != s2.is_some() {
            return Ok(ComparisonResult {
                file: rel_path,
                status: "DIFF".to_string(), // Type mismatch
                hash1: None, hash2: None,
                size1: None, size2: None,
                modified1: time1_str, modified2: time2_str,
                symlink1: entry1.symlink_target.clone(),
                symlink2: entry2.symlink_target.clone(),
            });
        }
    }

    // 2. Metadata/Size Short-circuit
    if entry1.size != entry2.size {
        return Ok(ComparisonResult {
            file: rel_path,
            status: "DIFF".to_string(),
            hash1: None, hash2: None,
            size1, size2,
            modified1: time1_str, modified2: time2_str,
            symlink1: None, symlink2: None,
        });
    } else if config.mode == Mode::Metadata {
        if entry1.modified != entry2.modified {
            return Ok(ComparisonResult {
                file: rel_path,
                status: "DIFF".to_string(),
                hash1: None, hash2: None,
                size1, size2,
                modified1: time1_str, modified2: time2_str,
                symlink1: None, symlink2: None,
            });
        }
        return Ok(ComparisonResult {
            file: rel_path,
            status: "MATCH".to_string(),
            hash1: None, hash2: None,
            size1, size2,
            modified1: time1_str, modified2: time2_str,
            symlink1: None, symlink2: None,
        });
    }

    // 3. Compute hashes
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
            (if is_match { "MATCH" } else { "DIFF" }, Some(h1), Some(h2))
        }
        _ => ("ERROR", None, None),
    };

    Ok(ComparisonResult {
        file: rel_path,
        status: status.to_string(),
        hash1: h1, hash2: h2,
        size1, size2,
        modified1: time1_str, modified2: time2_str,
        symlink1: None, symlink2: None,
    })
}

//=============================================================================
// Real-time (Sequential) Mode
//=============================================================================

fn run_realtime(config: &Config, start_time: Instant) -> Result<ExitStatus> {
    if io::stdout().is_terminal() {
        println!("{}", "==============================================".bright_blue());
        println!("  Folder Comparison Utility (Real-time Mode)");
        println!("{}", "==============================================".bright_blue());
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
    
    // Print errors immediately in realtime mode
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

    // Sort if not disabled
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
            let result = compare_files(rel_path.clone(), entry1, &entry2, config)?;

            match result.status.as_str() {
                "MATCH" => matches += 1,
                "DIFF" => diffs += 1,
                _ => (),
            }

            print_realtime_result(&result, config)?;
        } else {
            missing += 1;
            print_realtime_missing("MISSING", &rel_path, config.verbose)?;
        }
    }

    let extra = files2_map.len();
    let mut sorted_extra: Vec<_> = files2_map.into_keys().collect();
    if !config.no_sort {
        sorted_extra.sort();
    }

    for rel_path in sorted_extra {
        print_realtime_missing("EXTRA", &rel_path, config.verbose)?;
    }

    let elapsed = start_time.elapsed();
    let total = files1.len() + extra;
    let total_errors = errors1.len() + errors2.len();

    print_summary(total, matches, diffs, missing, extra, total_errors, elapsed, config)?;

    if total_errors > 0 {
        Ok(ExitStatus::Error)
    } else if diffs > 0 || missing > 0 || extra > 0 {
        Ok(ExitStatus::Diff)
    } else {
        Ok(ExitStatus::Success)
    }
}

fn print_error_entry(e: &ErrorEntry, source: &str) {
    eprintln!(
        "[{}]{} ({}: {})",
        "ERROR".red().on_white(),
        e.path.display(),
        source,
        e.error
    );
}

fn print_realtime_missing(status: &str, file: &Path, _verbose: bool) -> Result<()> {
    let (status_colored, file_color) = match status {
        "MISSING" => ("MISSING".blue(), Color::Blue),
        "EXTRA" => ("EXTRA".blue(), Color::Blue),
         _ => (status.normal(), Color::White),
    };
    println!("[{}]  {}", status_colored, file.to_str().unwrap_or("???").color(file_color));
    Ok(())
}

fn print_realtime_result(
    r: &ComparisonResult,
    config: &Config,
) -> Result<()> {
    let (status_colored, file_color) = match r.status.as_str() {
        "MATCH" => ("MATCH".green(), Color::Green),
        "DIFF" => ("DIFF".red(), Color::Red),
        "ERROR" => ("ERROR".red().on_white(), Color::Red),
        _ => (r.status.as_str().normal(), Color::White),
    };

    println!(
        "[{}]  {}",
        status_colored,
        r.file.to_str().context("Invalid file name")?.color(file_color)
    );

    if config.verbose {
        if r.status == "DIFF" {
             if let (Some(h1), Some(h2)) = (&r.hash1, &r.hash2) {
                println!("    {}: {}", "folder1".dimmed(), format_hashres(h1, config.algo)?);
                println!("    {}: {}", "folder2".dimmed(), format_hashres(h2, config.algo)?);
            } else if let (Some(s1), Some(s2)) = (r.size1, r.size2) {
                if s1 != s2 {
                    println!("    {}: {}", "folder1".dimmed(), format!("{} bytes", s1).cyan());
                    println!("    {}: {}", "folder2".dimmed(), format!("{} bytes", s2).cyan());
                } else if let (Some(t1), Some(t2)) = (&r.modified1, &r.modified2) {
                     if t1 != t2 {
                         println!("    {}: {}", "folder1".dimmed(), t1.cyan());
                         println!("    {}: {}", "folder2".dimmed(), t2.cyan());
                     }
                } else if let (Some(sym1), Some(sym2)) = (&r.symlink1, &r.symlink2) {
                    if sym1 != sym2 {
                        println!("    {}: -> {}", "folder1".dimmed(), sym1.cyan());
                        println!("    {}: -> {}", "folder2".dimmed(), sym2.cyan());
                    }
                }
            }
        } else if r.status == "MATCH" {
             if let Some(h) = &r.hash1 {
                println!("    {}: {}", "in_both".dimmed(), format_hashres(h, config.algo)?);
            }
        }
    }
    println!();
    Ok(())
}

fn print_summary(
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    errors: usize,
    elapsed: std::time::Duration,
    config: &Config,
) -> Result<()> {
    // If we are not a terminal, maybe skip the fancy box?
    // But for now, we keep the box but rely on colors being disabled if needed.
    
    let summary_lines = generate_summary_text(total, matches, diffs, missing, extra, errors, elapsed, config);
    for line in summary_lines {
        println!("{}", line);
    }

    Ok(())
}

//=============================================================================
// Batch (Parallel) Mode
//=============================================================================

fn run_batch(config: &Config, start_time: Instant) -> Result<ExitStatus> {
    if io::stdout().is_terminal() {
        println!("{}", "==============================================".bright_blue());
        println!("  Folder File Comparison Utility (Batch Mode)");
        println!("{}", "==============================================".bright_blue());
        println!(); 
    }

    // 1. Collect files
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

    // Create maps
    let files1_map: HashMap<PathBuf, FileEntry> = files1
        .into_par_iter()
        .map(|f| (f.path.strip_prefix(&config.folder1).unwrap().to_path_buf(), f))
        .collect();
    let files2_map: HashMap<PathBuf, FileEntry> = files2
        .into_par_iter()
        .map(|f| (f.path.strip_prefix(&config.folder2).unwrap().to_path_buf(), f))
        .collect();

    let set1_paths: HashSet<PathBuf> = files1_map.keys().cloned().collect();
    let set2_paths: HashSet<PathBuf> = files2_map.keys().cloned().collect();

    // 2. Common files
    let common_paths: Vec<PathBuf> = set1_paths
        .intersection(&set2_paths)
        .cloned()
        .collect();

    // Progress Bar only if terminal
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

    // 3. Process common files
    let mut all_results: Vec<ComparisonResult> = common_paths
        .par_iter()
        .map(|rel_path| {
            if let Some(ref p) = pb { p.inc(1); }
            let entry1 = files1_map.get(rel_path).unwrap();
            let entry2 = files2_map.get(rel_path).unwrap();
            compare_files(rel_path.clone(), entry1, entry2, config)
        })
        .collect::<Result<Vec<_>>>()?;

    if let Some(ref p) = pb { p.finish_with_message("Comparison complete"); }

    // 4. Add MISSING
    for rel_path in set1_paths.difference(&set2_paths) {
        all_results.push(ComparisonResult {
            file: rel_path.clone(),
            status: "MISSING".to_string(),
            hash1: None, hash2: None,
            size1: None, size2: None,
            modified1: None, modified2: None,
            symlink1: None, symlink2: None,
        });
    }

    // 5. Add EXTRA
    for rel_path in set2_paths.difference(&set1_paths) {
        all_results.push(ComparisonResult {
            file: rel_path.clone(),
            status: "EXTRA".to_string(),
            hash1: None, hash2: None,
            size1: None, size2: None,
            modified1: None, modified2: None,
            symlink1: None, symlink2: None,
        });
    }

    // 6. Sort
    if !config.no_sort {
        all_results.sort_by(|a, b| a.file.cmp(&b.file));
    }

    // 7. Count
    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;
    let mut extra = 0;
    for r in &all_results {
        match r.status.as_str() {
            "MATCH" => matches += 1,
            "DIFF" => diffs += 1,
            "MISSING" => missing += 1,
            "EXTRA" => extra += 1,
            _ => (),
        }
    }
    let total = all_results.len();
    let elapsed = start_time.elapsed();

    // 8. Generate report
    match config.output_format {
        OutputFormat::Txt => {
            let output = generate_text_report(
                &all_results,
                &errors1,
                &errors2,
                total,
                matches,
                diffs,
                missing,
                extra,
                total_errors,
                elapsed,
                config,
            )?;
            write_report(output, &config.output_folder, "report.txt", "")?;
        }
        OutputFormat::Json => {
            let output = generate_json_report(
                &all_results,
                &errors1,
                &errors2,
                total,
                matches,
                diffs,
                missing,
                extra,
                total_errors,
                elapsed,
            )?;
            write_report(output, &config.output_folder, "report.json", "")?;
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

fn generate_text_report(
    results: &[ComparisonResult],
    errors1: &[ErrorEntry],
    errors2: &[ErrorEntry],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    errors: usize,
    elapsed: std::time::Duration,
    config: &Config,
) -> Result<String> {
    let mut output = String::new();

    // Print Errors First
    for e in errors1 {
        output.push_str(&format!("[{}] {} (folder1: {})\n", "ERROR".red().on_white(), e.path.display(), e.error));
    }
    for e in errors2 {
        output.push_str(&format!("[{}] {} (folder2: {})\n", "ERROR".red().on_white(), e.path.display(), e.error));
    }

    for result in results {
        let (status_colored, file_color) = match result.status.as_str() {
            "MATCH" => ("MATCH".green(), Color::Green),
            "DIFF" => ("DIFF".red(), Color::Red),
            "MISSING" => ("MISSING".blue(), Color::Blue),
            "EXTRA" => ("EXTRA".blue(), Color::Blue),
            "ERROR" => ("ERROR".red().on_white(), Color::Red),
            _ => (result.status.as_str().normal(), Color::White),
        };

        let file_name = result.file.to_str().context("Invalid file name")?;
        let line = format!(
            "[{}]  {}\n",
            status_colored,
            file_name.color(file_color)
        );
        output.push_str(&line);

        if config.verbose {
            if result.status == "DIFF" {
                 if let (Some(h1), Some(h2)) = (&result.hash1, &result.hash2) {
                    output.push_str(&format!("    {}: {}
", "folder1".dimmed(), format_hashres(h1, config.algo)?));
                    output.push_str(&format!("    {}: {}
", "folder2".dimmed(), format_hashres(h2, config.algo)?));
                } else if let (Some(s1), Some(s2)) = (result.size1, result.size2) {
                     if s1 != s2 {
                        output.push_str(&format!("    {}: {}
", "folder1".dimmed(), format!("{} bytes", s1).cyan()));
                        output.push_str(&format!("    {}: {}
", "folder2".dimmed(), format!("{} bytes", s2).cyan()));
                     } else if let (Some(t1), Some(t2)) = (&result.modified1, &result.modified2) {
                        if t1 != t2 {
                             output.push_str(&format!("    {}: {}
", "folder1".dimmed(), t1.cyan()));
                             output.push_str(&format!("    {}: {}
", "folder2".dimmed(), t2.cyan()));
                        }
                     } else if let (Some(sym1), Some(sym2)) = (&result.symlink1, &result.symlink2) {
                        if sym1 != sym2 {
                            output.push_str(&format!("    {}: -> {}
", "folder1".dimmed(), sym1.cyan()));
                            output.push_str(&format!("    {}: -> {}
", "folder2".dimmed(), sym2.cyan()));
                        }
                     }
                }
            } else if result.status == "MATCH" {
                if let Some(h1) = &result.hash1 {
                    output.push_str(&format!("    {}: {}
", "in_both".dimmed(), format_hashres(h1, config.algo)?));
                }
            }
        }
    }
    // Summary
    output.push_str("\n");
    let summary_text = generate_summary_text(total, matches, diffs, missing, extra, errors, elapsed, config);
    output.push_str(&summary_text.join("\n"));

    Ok(output)
}

fn generate_summary_text(
    total: usize, matches: usize, diffs: usize, missing: usize, extra: usize, errors: usize, 
    elapsed: std::time::Duration, config: &Config
) -> Vec<String> {
    let mode_str = format!("{:?}", config.mode);
    let algo_str = if config.mode == Mode::Metadata {
        "Metadata".to_string()
    } else {
        format!("{:?}", config.algo)
    };
    let elapsed_str = format!("{:.2?}", elapsed);

    let content_width = 47;
    let mut output = Vec::new();

    output.push(format!("{}{}{}", "╔".bright_blue(), "═".repeat(content_width).bright_blue(), "╗".bright_blue()));

    let title = "Summary";
    let padding_total = content_width.saturating_sub(title.len());
    let padding_start = padding_total / 2;
    let padding_end = padding_total - padding_start;
    output.push(format!("{}{}{}{}{}",
        "║".bright_blue(),
        " ".repeat(padding_start),
        title.bold().bright_yellow(),
        " ".repeat(padding_end),
        "║".bright_blue()
    ));

    output.push(format!("{}{}{}", "╠".bright_blue(), "═".repeat(content_width).bright_blue(), "╣".bright_blue()));

    let add_line = |vec: &mut Vec<String>, label: &str, value: &str, label_color: Color, value_color: Color| {
        let colored_line = format!("  {} : {}",
            format!("{:<22}", label).bold().color(label_color),
            value.bold().color(value_color)
        );
        let uncolored_len = 2 + 22 + 3 + value.len();
        let padding = " ".repeat(content_width.saturating_sub(uncolored_len));
        vec.push(format!("{}{}{}{}", "║".bright_blue(), colored_line, padding, "║".bright_blue()));
    };

    add_line(&mut output, "Mode", &mode_str, Color::Cyan, Color::Magenta);
    add_line(&mut output, "Algorithm", &algo_str, Color::Cyan, Color::Magenta);
    add_line(&mut output, "Total files checked", &total.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Missing in Folder2", &missing.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Extra in Folder2", &extra.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Matches", &matches.to_string(), Color::Cyan, Color::Green);
    add_line(&mut output, "Differences", &diffs.to_string(), Color::Cyan, Color::Red);
    if errors > 0 {
        add_line(&mut output, "Errors", &errors.to_string(), Color::Cyan, Color::Red);
    }
    add_line(&mut output, "Time taken", &elapsed_str, Color::Cyan, Color::Yellow);

    output.push(format!("{}{}{}", "╚".bright_blue(), "═".repeat(content_width).bright_blue(), "╝".bright_blue()));

    output
}

fn generate_json_report(
    results: &[ComparisonResult],
    errors1: &[ErrorEntry],
    errors2: &[ErrorEntry],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    errors: usize,
    elapsed: std::time::Duration,
) -> Result<String> {
    let summary = serde_json::json!({
        "total_files_checked": total,
        "matches": matches,
        "differences": diffs,
        "missing_in_folder2": missing,
        "extra_in_folder2": extra,
        "errors": errors,
        "time_taken": format!("{:.2?}", elapsed),
    });

    let output = serde_json::json!({
        "summary": summary,
        "folder1_errors": errors1,
        "folder2_errors": errors2,
        "results": results,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

fn write_report(output: String, output_folder: &Option<PathBuf>, filename: &str, _file_path: &str) -> Result<()> {
    if let Some(output_folder) = output_folder {
        fs::create_dir_all(output_folder)?;
        let report_path = output_folder.join(filename);
        let mut file = File::create(&report_path)?;
        file.write_all(output.as_bytes())?;
        if io::stdout().is_terminal() {
            println!("Report saved to {}", report_path.display());
        }
    } else {
        for line in output.lines() {
            println!("{}", line);
        }
    }
    Ok(())
}

fn collect_files(
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
    
    // Recursion logic
    if no_recursive {
        walk_builder.max_depth(Some(1));
    } else if let Some(d) = depth {
        walk_builder.max_depth(Some(d));
    }
    // Default is now infinite recursion (unless ignored by max_depth default of WalkBuilder? 
    // WalkBuilder default is recursive.

    // Symlink logic
    match symlink_mode {
        SymlinkMode::Follow => { walk_builder.follow_links(true); },
        _ => { walk_builder.follow_links(false); },
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
        exts.iter().map(|ext| ext.trim_start_matches('.').to_lowercase()).collect()
    });

    let (tx, rx) = mpsc::channel();
    // For errors
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
                        // Capture permission denied etc.
                        let _ = tx_err.send(ErrorEntry {
                             path: PathBuf::from("?"),
                             error: err.to_string(),
                         });
                        return ignore::WalkState::Continue;
                    },
                };

                if let Some(ref set) = custom_ignore_set {
                    if set.is_match(entry.path()) {
                        return ignore::WalkState::Continue;
                    }
                }

                let ft = match entry.file_type() {
                    Some(ft) => ft,
                    None => return ignore::WalkState::Continue,
                };

                let is_symlink = ft.is_symlink();
                let is_file = ft.is_file();

                // Logic based on symlink mode
                // If Ignore: skip symlinks
                // If Follow: symlinks that point to files come as is_file()=true (usually).
                // If Compare: we want the symlink itself.
                
                let should_include = match symlink_mode {
                    SymlinkMode::Ignore => is_file, // Skip symlinks
                    SymlinkMode::Follow => is_file, // Followed links appear as files
                    SymlinkMode::Compare => is_file || is_symlink,
                };

                if !should_include {
                     return ignore::WalkState::Continue;
                }

                if let Some(ref exts) = type_filter {
                    if !entry
                        .path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map_or(false, |s| exts.contains(&s.to_lowercase()))
                    {
                        return ignore::WalkState::Continue;
                    }
                }

                let mut symlink_target = None;
                if is_symlink && symlink_mode == SymlinkMode::Compare {
                    if let Ok(target) = fs::read_link(entry.path()) {
                        symlink_target = Some(target.to_string_lossy().to_string());
                    }
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

fn compute_hashes(path: &Path, algo: HashAlgo) -> io::Result<HashResult> {
    let metadata = fs::metadata(path)?;
    let len = metadata.len();
    
    const MMAP_THRESHOLD: u64 = 32 * 1024; 
    // Optimized threshold for threading: 128MB
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
            sha256: sha256_hasher.map(|h| format!("{:x}", h.finalize())),
            blake3: blake3_hasher.map(|h| h.finalize().to_hex().to_string()),
        });
    }

    if len < MMAP_THRESHOLD {
        let data = fs::read(path)?;
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&data);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            // Always single thread for small files
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

    let sha256 = sha256_hasher.map(|h| format!("{:x}", h.finalize()));
    let blake3 = blake3_hasher.map(|h| h.finalize().to_hex().to_string());

    Ok(HashResult { sha256, blake3 })
}

fn format_hashres(h: &HashResult, algo: HashAlgo) -> Result<String> {
    match algo {
        HashAlgo::Sha256 => Ok(h.sha256.as_ref().context("SHA256 hash not computed")?.color(Color::Cyan).to_string()),
        HashAlgo::Blake3 => Ok(h.blake3.as_ref().context("BLAKE3 hash not computed")?.color(Color::Cyan).to_string()),
        HashAlgo::Both => Ok(format!(
            "sha256:{}\n            blake3:{}",
            h.sha256.as_ref().context("SHA256 hash not computed")?.color(Color::Cyan),
            h.blake3.as_ref().context("BLAKE3 hash not computed")?.color(Color::Cyan)
        )),
    }
}