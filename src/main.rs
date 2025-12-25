use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle, ParallelProgressIterator};
use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use chrono::{DateTime, Local};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Mode {
    /// Processes files sequentially and prints results as they happen. Slower.
    Realtime,
    /// Processes files in parallel and prints a report at the end. Faster.
    Batch,
    /// Compare file size and modification time to skip cryptographic hashing.
    Metadata,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, help_template = "{before-help}{name} {version}\n{author-with-newline}{about-with-newline}\n{usage-heading} {usage} \n\n {all-args} {after-help}")]
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

    /// Enable file comparison in subfolders
    #[arg(short, long, default_value_t = false)]
    subfolders: bool,

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
}

fn main() -> Result<()> {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
    
    let start_time = Instant::now();
    let config = Config::parse();

    if let Some(num_threads) = config.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .context("Failed to set Rayon thread pool size")?;
    }

    match config.mode {
        Mode::Realtime => run_realtime(&config, start_time)?,
        Mode::Batch | Mode::Metadata => run_batch(&config, start_time)?,
    }

    Ok(())
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

    // Short-circuit: Check file sizes and times first using cached metadata
    if entry1.size != entry2.size {
        return Ok(ComparisonResult {
            file: rel_path,
            status: "DIFF".to_string(),
            hash1: None,
            hash2: None,
            size1,
            size2,
            modified1: time1_str,
            modified2: time2_str,
        });
    } else if config.mode == Mode::Metadata {
        if entry1.modified != entry2.modified {
            return Ok(ComparisonResult {
                file: rel_path,
                status: "DIFF".to_string(),
                hash1: None,
                hash2: None,
                size1,
                size2,
                modified1: time1_str,
                modified2: time2_str,
            });
        }
        // Sizes and times match
        return Ok(ComparisonResult {
            file: rel_path,
            status: "MATCH".to_string(),
            hash1: None,
            hash2: None,
            size1,
            size2,
            modified1: time1_str,
            modified2: time2_str,
        });
    }

    // Compute hashes for the pair of files in parallel
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
        hash1: h1,
        hash2: h2,
        size1,
        size2,
        modified1: time1_str,
        modified2: time2_str,
    })
}

//=============================================================================
// Real-time (Sequential) Mode
//=============================================================================

fn run_realtime(config: &Config, start_time: Instant) -> Result<()> {
    println!("{}", "==============================================".bright_blue());
    println!("  Folder Comparison Utility (Real-time Mode)");
    println!("{}", "==============================================".bright_blue());

    let mut files1 = collect_files(
        &config.folder1,
        config.subfolders,
        config.hidden,
        &config.types,
        &config.ignore,
    )?;
    let files2 = collect_files(
        &config.folder2,
        config.subfolders,
        config.hidden,
        &config.types,
        &config.ignore,
    )?;

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

            print_realtime_result(
                &result.status,
                &rel_path,
                result.hash1.as_ref(),
                result.hash2.as_ref(),
                result.size1,
                result.size2,
                result.modified1.as_deref(),
                result.modified2.as_deref(),
                config.algo,
                config.verbose
            )?;
        } else {
            missing += 1;
            print_realtime_result("MISSING", &rel_path, None, None, None, None, None, None, config.algo, config.verbose)?;
        }
    }

    let extra = files2_map.len();
    let mut sorted_extra: Vec<_> = files2_map.into_keys().collect();
    if !config.no_sort {
        sorted_extra.sort();
    }

    for rel_path in sorted_extra {
        print_realtime_result("EXTRA", &rel_path, None, None, None, None, None, None, config.algo, config.verbose)?;
    }

    let elapsed = start_time.elapsed();
    let total = files1.len() + extra;

    print_summary(total, matches, diffs, missing, extra, elapsed, config)?;

    Ok(())
}

fn print_realtime_result(
    status: &str,
    file: &Path,
    h1: Option<&HashResult>,
    h2: Option<&HashResult>,
    size1: Option<u64>,
    size2: Option<u64>,
    time1: Option<&str>,
    time2: Option<&str>,
    algo: HashAlgo,
    verbose: bool,
) -> Result<()> {
    let (status_colored, file_color) = match status {
        "MATCH" => ("MATCH".green(), Color::Green),
        "DIFF" => ("DIFF".red(), Color::Red),
        "MISSING" => ("MISSING".blue(), Color::Blue),
        "EXTRA" => ("EXTRA".blue(), Color::Blue),
        "ERROR" => ("ERROR".red().on_white(), Color::Red),
        _ => (status.normal(), Color::White),
    };

    let file_name = file.to_str().context("Invalid file name")?;
    println!(
        "[{}]  {}",
        status_colored,
        file_name.color(file_color)
    );

    if verbose {
        if status == "DIFF" {
            if let (Some(h1), Some(h2)) = (h1, h2) {
                println!("    {}: {}", "folder1".dimmed(), format_hashres(h1, algo)?);
                println!("    {}: {}", "folder2".dimmed(), format_hashres(h2, algo)?);
            } else if let (Some(s1), Some(s2)) = (size1, size2) {
                if s1 != s2 {
                    println!("    {}: {}", "folder1".dimmed(), format!("{} bytes", s1).cyan());
                    println!("    {}: {}", "folder2".dimmed(), format!("{} bytes", s2).cyan());
                } else if let (Some(t1), Some(t2)) = (time1, time2) {
                    // Sizes match, so it must be timestamps
                    if t1 != t2 {
                         println!("    {}: {}", "folder1".dimmed(), t1.cyan());
                         println!("    {}: {}", "folder2".dimmed(), t2.cyan());
                    }
                }
            }
        } else if status == "MATCH" {
            if let Some(h) = h1 {
                println!("    {}: {}", "in_both".dimmed(), format_hashres(h, algo)?);
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
    elapsed: std::time::Duration,
    config: &Config,
) -> Result<()> {
    let summary_lines = generate_summary_text(total, matches, diffs, missing, extra, elapsed, config);
    for line in summary_lines {
        println!("{}", line);
    }

    Ok(())
}

//=============================================================================
// Batch (Parallel) Mode
//=============================================================================

fn run_batch(config: &Config, start_time: Instant) -> Result<()> {
    println!("{}", "==============================================".bright_blue());
    println!("  Folder File Comparison Utility (Batch Mode)");
    println!("{}", "==============================================".bright_blue());
    println!(); // Empty line after banner
    // 1. Collect files from both directories in parallel
    let (files1, files2) = rayon::join(
        || {
            collect_files(
                &config.folder1,
                config.subfolders,
                config.hidden,
                &config.types,
                &config.ignore,
            )
        },
        || {
            collect_files(
                &config.folder2,
                config.subfolders,
                config.hidden,
                &config.types,
                &config.ignore,
            )
        },
    );
    let files1 = files1?;
    let files2 = files2?;

    // Create maps from relative path -> FileEntry for easy lookup later
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

    // 2. Identify files that exist in both directories (potential MATCH or DIFF)
    let common_paths: Vec<PathBuf> = set1_paths
        .intersection(&set2_paths)
        .cloned()
        .collect();

    let pb = ProgressBar::new(common_paths.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [Elap>{elapsed_precise}] [ {bar:40.cyan/blue} ] {pos}/{len} (Rema>{eta})")?
            .progress_chars("#>- ")
    );
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr_with_hz(10));

    // 3. Process common files in parallel (the only ones that need hashing)
    let mut all_results: Vec<ComparisonResult> = common_paths
        .par_iter()
        .progress_with(pb.clone())
        .map(|rel_path| {
            let entry1 = files1_map.get(rel_path).unwrap();
            let entry2 = files2_map.get(rel_path).unwrap();
            compare_files(rel_path.clone(), entry1, entry2, config)
        })
        .collect::<Result<Vec<_>>>()?;

    pb.finish_with_message("Comparison complete");

    // 4. Add MISSING files (from set1 but not set2) - no hashing needed
    for rel_path in set1_paths.difference(&set2_paths) {
        all_results.push(ComparisonResult {
            file: rel_path.clone(),
            status: "MISSING".to_string(),
            hash1: None,
            hash2: None,
            size1: None,
            size2: None,
            modified1: None,
            modified2: None,
        });
    }

    // 5. Add EXTRA files (from set2 but not set1) - no hashing needed
    for rel_path in set2_paths.difference(&set1_paths) {
        all_results.push(ComparisonResult {
            file: rel_path.clone(),
            status: "EXTRA".to_string(),
            hash1: None,
            hash2: None,
            size1: None,
            size2: None,
            modified1: None,
            modified2: None,
        });
    }

    // 6. Sort results alphabetically by file path (unless no_sort is set)
    if !config.no_sort {
        all_results.sort_by(|a, b| a.file.cmp(&b.file));
    }

    // 7. Count results
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

    // 8. Generate report (no changes needed here)
    match config.output_format {
        OutputFormat::Txt => {
            let output = generate_text_report(
                &all_results,
                total,
                matches,
                diffs,
                missing,
                extra,
                elapsed,
                config.algo,
                config,
            )?;
            write_report(output, &config.output_folder, "report.txt")?;
        }
        OutputFormat::Json => {
            let output = generate_json_report(
                &all_results,
                total,
                matches,
                diffs,
                missing,
                extra,
                elapsed,
            )?;
            write_report(output, &config.output_folder, "report.json")?;
        }
    }

    Ok(())
}

fn generate_text_report(
    results: &[ComparisonResult],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    elapsed: std::time::Duration,
    algo: HashAlgo,
    config: &Config,
) -> Result<String> {
    let mut output = String::new();

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
                    let line1 = format!("    {}: {}\n", "folder1".dimmed(), format_hashres(h1, algo)?);
                    let line2 = format!("    {}: {}\n", "folder2".dimmed(), format_hashres(h2, algo)?);
                    output.push_str(&line1);
                    output.push_str(&line2);
                } else if let (Some(s1), Some(s2)) = (result.size1, result.size2) {
                     if s1 != s2 {
                        let line1 = format!("    {}: {}\n", "folder1".dimmed(), format!("{} bytes", s1).cyan());
                        let line2 = format!("    {}: {}\n", "folder2".dimmed(), format!("{} bytes", s2).cyan());
                        output.push_str(&line1);
                        output.push_str(&line2);
                     } else if let (Some(t1), Some(t2)) = (&result.modified1, &result.modified2) {
                        // Sizes match, check timestamps
                        if t1 != t2 {
                             let line1 = format!("    {}: {}\n", "folder1".dimmed(), t1.cyan());
                             let line2 = format!("    {}: {}\n", "folder2".dimmed(), t2.cyan());
                             output.push_str(&line1);
                             output.push_str(&line2);
                        }
                     }
                }
            } else if result.status == "MATCH" {
                if let Some(h1) = &result.hash1 {
                    let line = format!("    {}: {}\n", "in_both".dimmed(), format_hashres(h1, algo)?);
                    output.push_str(&line);
                }
            }
        }
        output.push_str("\n");
    }

    let summary_text = generate_summary_text(total, matches, diffs, missing, extra, elapsed, config);
    output.push_str(&summary_text.join("\n"));

    Ok(output)
}

fn generate_summary_text(total: usize, matches: usize, diffs: usize, missing: usize, extra: usize, elapsed: std::time::Duration, config: &Config) -> Vec<String> {
    let mode_str = format!("{:?}", config.mode);
    let algo_str = if config.mode == Mode::Metadata {
        "Metadata".to_string()
    } else {
        format!("{:?}", config.algo)
    };
    let elapsed_str = format!("{:.2?}", elapsed);

    // The total width of the content area INSIDE the box borders
    let content_width = 47;
    let mut output = Vec::new();

    // --- Top border ---
    output.push(format!("{}{}{}", "╔".bright_blue(), "═".repeat(content_width).bright_blue(), "╗".bright_blue()));

    // --- Title ---
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

    // --- Separator ---
    output.push(format!("{}{}{}", "╠".bright_blue(), "═".repeat(content_width).bright_blue(), "╣".bright_blue()));

    // --- Helper for content lines ---
    let add_line = |vec: &mut Vec<String>, label: &str, value: &str, label_color: Color, value_color: Color| {
        // Create the colored parts with a 2-space left margin
        let colored_line = format!("  {} : {}",
            format!("{:<22}", label).bold().color(label_color),
            value.bold().color(value_color)
        );

        // Calculate padding based on the UNCOLORED length to fill the remaining space
        let uncolored_len = 2 + 22 + 3 + value.len(); // 2-margin + 22-label + 3-" : " + value
        let padding = " ".repeat(content_width.saturating_sub(uncolored_len));

        // Assemble the full line with borders
        vec.push(format!("{}{}{}{}",
            "║".bright_blue(),
            colored_line,
            padding,
            "║".bright_blue()
        ));
    };

    // --- Add all lines ---
    add_line(&mut output, "Mode", &mode_str, Color::Cyan, Color::Magenta);
    add_line(&mut output, "Algorithm", &algo_str, Color::Cyan, Color::Magenta);
    add_line(&mut output, "Total files checked", &total.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Missing in Folder2", &missing.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Extra in Folder2", &extra.to_string(), Color::Cyan, Color::Blue);
    add_line(&mut output, "Matches", &matches.to_string(), Color::Cyan, Color::Green);
    add_line(&mut output, "Differences", &diffs.to_string(), Color::Cyan, Color::Red);
    add_line(&mut output, "Time taken", &elapsed_str, Color::Cyan, Color::Yellow);

    // --- Bottom border ---
    output.push(format!("{}{}{}", "╚".bright_blue(), "═".repeat(content_width).bright_blue(), "╝".bright_blue()));

    output
}

fn generate_json_report(
    results: &[ComparisonResult],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    elapsed: std::time::Duration,
) -> Result<String> {
    let summary = serde_json::json!({
        "total_files_checked": total,
        "matches": matches,
        "differences": diffs,
        "missing_in_folder2": missing,
        "extra_in_folder2": extra,
        "time_taken": format!("{:.2?}", elapsed),
    });

    let output = serde_json::json!({
        "summary": summary,
        "results": results,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

fn write_report(
    output: String,
    output_folder: &Option<PathBuf>,
    filename: &str,
) -> Result<()> {
    if let Some(output_folder) = output_folder {
        fs::create_dir_all(output_folder)?;
        let report_path = output_folder.join(filename);
        let mut file = File::create(&report_path)?;
        file.write_all(output.as_bytes())?;
        println!("Report saved to {}", report_path.display());
    } else {
        for line in output.lines() {
            println!("{}", line);
        }
    }
    Ok(())
}

//=============================================================================
// Common Helper Functions
//=============================================================================

use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;

fn collect_files(
    dir: &Path,
    subfolders: bool,
    hidden: bool,
    types: &Option<Vec<String>>,
    ignore_patterns: &Option<Vec<String>>,
) -> Result<Vec<FileEntry>> {
    let mut walk_builder = WalkBuilder::new(dir);
    walk_builder.hidden(!hidden);
    if !subfolders {
        walk_builder.max_depth(Some(1));
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
    let walker = walk_builder.build_parallel();

    std::thread::spawn(move || {
        walker.run(|| {
            let tx = tx.clone();
            let type_filter = type_filter.clone();
            let custom_ignore_set = custom_ignore_set.clone();

            Box::new(move |result| {
                let entry = match result {
                    Ok(e) => e,
                    Err(_) => return ignore::WalkState::Continue,
                };

                if let Some(ref set) = custom_ignore_set {
                    if set.is_match(entry.path()) {
                        return ignore::WalkState::Continue;
                    }
                }

                if !entry.file_type().map_or(false, |ft| ft.is_file()) {
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

                // Collect metadata during walk
                if let Ok(meta) = entry.metadata() {
                    let entry_data = FileEntry {
                        path: entry.path().to_path_buf(),
                        size: meta.len(),
                        modified: meta.modified().ok(),
                    };
                    let _ = tx.send(entry_data);
                }
                
                ignore::WalkState::Continue
            })
        });
    });

    let final_files: Vec<FileEntry> = rx.into_iter().collect();
    Ok(final_files)
}



fn compute_hashes(path: &Path, algo: HashAlgo) -> io::Result<HashResult> {
    let metadata = fs::metadata(path)?;
    let len = metadata.len();
    let small_file_threshold = 10 * 1024 * 1024; // 10 MB

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
        // Handle empty files - nothing to update
    } else if len < small_file_threshold {
        let data = fs::read(path)?;
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&data);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            bh.update_rayon(&data);
        }
    } else {
        let f = File::open(path)?;
        // SAFETY: We assume the file is not being modified/truncated externally during comparison.
        let mmap = unsafe { Mmap::map(&f)? };
        
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&mmap);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            bh.update_rayon(&mmap);
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
