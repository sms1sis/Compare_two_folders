use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle, ParallelProgressIterator};
use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};

const BUF_SIZE: usize = 8 * 1024;

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
}

#[derive(Debug, Clone, Serialize)]
struct HashResult {
    sha256: Option<String>,
    blake3: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ComparisonResult {
    file: PathBuf,
    status: String,
    hash1: Option<HashResult>,
    hash2: Option<HashResult>,
}

fn main() -> Result<()> {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();
    
    let start_time = Instant::now();
    let config = Config::parse();

    match config.mode {
        Mode::Realtime => run_realtime(&config, start_time)?,
        Mode::Batch => run_batch(&config, start_time)?,
    }

    Ok(())
}

//=============================================================================
// Real-time (Sequential) Mode
//=============================================================================

fn run_realtime(config: &Config, start_time: Instant) -> Result<()> {
    println!("{}", "==============================================".bright_blue());
    println!("  Folder Comparison Utility (Real-time Mode)");
    println!("{}", "==============================================".bright_blue());

    let files1 = collect_files(&config.folder1)?;
    let files2 = collect_files(&config.folder2)?;

    let mut files2_relative: HashSet<PathBuf> = files2
        .iter()
        .map(|f| f.strip_prefix(&config.folder2).map(|p| p.to_path_buf()))
        .collect::<Result<_, _>>()?;

    let mut matches = 0;
    let mut diffs = 0;
    let mut missing = 0;

    for f1_abs in &files1 {
        let rel_path = f1_abs.strip_prefix(&config.folder1)?;
        let f2_abs = config.folder2.join(rel_path);

        if f2_abs.exists() {
            let h1 = compute_hashes(f1_abs, config.algo)?;
            let h2 = compute_hashes(&f2_abs, config.algo)?;

            let is_match = match config.algo {
                HashAlgo::Sha256 => h1.sha256 == h2.sha256,
                HashAlgo::Blake3 => h1.blake3 == h2.blake3,
                HashAlgo::Both => h1.sha256 == h2.sha256 && h1.blake3 == h2.blake3,
            };

            if is_match {
                matches += 1;
                print_realtime_result("MATCH", rel_path, Some(&h1), None, config.algo)?;
            } else {
                diffs += 1;
                print_realtime_result("DIFF", rel_path, Some(&h1), Some(&h2), config.algo)?;
            }
            files2_relative.remove(rel_path);
        } else {
            missing += 1;
            print_realtime_result("MISSING", rel_path, None, None, config.algo)?;
        }
    }

    let extra = files2_relative.len();
    for rel_path in &files2_relative {
        print_realtime_result("EXTRA", rel_path, None, None, config.algo)?;
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
    algo: HashAlgo,
) -> Result<()> {
    let status_colored = match status {
        "MATCH" => "MATCH".green(),
        "DIFF" => "DIFF".yellow(),
        "MISSING" => "MISSING".red(),
        "EXTRA" => "EXTRA".cyan(),
        _ => status.normal(),
    };

    let file_name = file.to_str().context("Invalid file name")?;
    println!(
        "[{}]  {}",
        status_colored,
        file_name.color(color_for_file(file_name))
    );

    if status == "DIFF" {
        if let (Some(h1), Some(h2)) = (h1, h2) {
            println!("    {}: {}", "folder1".dimmed(), format_hashres(h1, algo)?);
            println!("    {}: {}", "folder2".dimmed(), format_hashres(h2, algo)?);
        }
    } else if status == "MATCH" {
        if let Some(h) = h1 {
            println!("    {}: {}", "in_both".dimmed(), format_hashres(h, algo)?);
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
    let (files1, files2) = rayon::join(
        || collect_files(&config.folder1),
        || collect_files(&config.folder2),
    );

    let files1 = files1?;
    let files2 = files2?;

    let pb = ProgressBar::new((files1.len() + files2.len()) as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
            .progress_chars("#>-"),
    );

    let map2_hashes = create_hash_map(&files2, &config.folder2, config.algo, &pb)?;

    let mut all_results = compare_files_batch(
        &files1,
        &config.folder1,
        &map2_hashes,
        config.algo,
        &pb,
    )?;

    pb.finish_with_message("Comparison complete");

    let (matches, diffs, missing) = count_results(&all_results);
    let extra = map2_hashes.len() - (matches + diffs);
    let total = all_results.len() + extra;

    handle_extra_files(&mut all_results, &map2_hashes, &files1, &config.folder1)?;

    let elapsed = start_time.elapsed();

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

fn create_hash_map(
    files: &[PathBuf],
    base_dir: &Path,
    algo: HashAlgo,
    pb: &ProgressBar,
) -> Result<HashMap<PathBuf, HashResult>> {
    let hashes = files
        .par_iter()
        .progress_with(pb.clone())
        .map(|f| {
            let rel_path = f
                .strip_prefix(base_dir)
                .with_context(|| format!("Failed to strip prefix '{:?}' from '{:?}'", base_dir, f))?;
            let h = compute_hashes(f, algo)?;
            Ok((rel_path.to_path_buf(), h))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(hashes.into_iter().collect())
}

fn compare_files_batch(
    files1: &[PathBuf],
    folder1: &Path,
    map2_hashes: &HashMap<PathBuf, HashResult>,
    algo: HashAlgo,
    pb: &ProgressBar,
) -> Result<Vec<ComparisonResult>> {
    files1
        .par_iter()
        .progress_with(pb.clone())
        .map(|f1_abs| {
            let rel_path = f1_abs
                .strip_prefix(folder1)
                .with_context(|| format!("Failed to strip prefix '{:?}' from '{:?}'", folder1, f1_abs))?;
            let h1 = compute_hashes(f1_abs, algo)?;

            let (status, h2_result_option) = match map2_hashes.get(rel_path) {
                Some(h2) => {
                    let is_match = match algo {
                        HashAlgo::Sha256 => h1.sha256 == h2.sha256,
                        HashAlgo::Blake3 => h1.blake3 == h2.blake3,
                        HashAlgo::Both => h1.sha256 == h2.sha256 && h1.blake3 == h2.blake3,
                    };
                    if is_match {
                        ("MATCH", Some(h2.clone()))
                    } else {
                        ("DIFF", Some(h2.clone()))
                    }
                }
                None => ("MISSING", None),
            };

            Ok(ComparisonResult {
                file: rel_path.to_path_buf(),
                status: status.to_string(),
                hash1: Some(h1),
                hash2: h2_result_option,
            })
        })
        .collect()
}

fn count_results(results: &[ComparisonResult]) -> (usize, usize, usize) {
    results.iter().fold((0, 0, 0), |(m, d, s), r| {
        match r.status.as_str() {
            "MATCH" => (m + 1, d, s),
            "DIFF" => (m, d + 1, s),
            "MISSING" => (m, d, s + 1),
            _ => (m, d, s),
        }
    })
}

fn handle_extra_files(
    all_results: &mut Vec<ComparisonResult>,
    map2_hashes: &HashMap<PathBuf, HashResult>,
    files1: &[PathBuf],
    folder1: &Path,
) -> Result<()> {
    let files1_set: HashSet<PathBuf> = files1
        .par_iter()
        .map(|f| {
            Ok(f.strip_prefix(folder1)
                .with_context(|| format!("Failed to strip prefix '{:?}' from '{:?}'", folder1, f))?
                .to_path_buf())
        })
        .collect::<Result<_>>()?;

    for (rel_path, h2) in map2_hashes {
        if !files1_set.contains(rel_path) {
            all_results.push(ComparisonResult {
                file: rel_path.clone(),
                status: "EXTRA".to_string(),
                hash1: None,
                hash2: Some(h2.clone()),
            });
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
        println!("{}", "==============================================".bright_blue());
        println!("  Folder File Comparison Utility (Batch Mode)");
        println!("{}", "==============================================".bright_blue());

    for result in results {
        let status_colored = match result.status.as_str() {
            "MATCH" => "MATCH".green(),
            "DIFF" => "DIFF".yellow(),
            "MISSING" => "MISSING".red(),
            "EXTRA" => "EXTRA".cyan(),
            _ => result.status.as_str().normal(),
        };

        let file_name = result.file.to_str().context("Invalid file name")?;
        let line = format!(
            "[{}]  {}
",
            status_colored,
            file_name.color(color_for_file(file_name))
        );
        output.push_str(&line);

        if result.status == "DIFF" {
            if let (Some(h1), Some(h2)) = (&result.hash1, &result.hash2) {
                let line1 = format!("    {}: {}
", "folder1".dimmed(), format_hashres(h1, algo)?);
                let line2 = format!("    {}: {}
", "folder2".dimmed(), format_hashres(h2, algo)?);
                output.push_str(&line1);
                output.push_str(&line2);
            }
        } else if result.status == "MATCH" {
            if let Some(h1) = &result.hash1 {
                let line = format!("    {}: {}
", "in_both".dimmed(), format_hashres(h1, algo)?);
                output.push_str(&line);
            }
        }
        output.push_str("
");
    }

    let summary_text = generate_summary_text(total, matches, diffs, missing, extra, elapsed, config);
    output.push_str(&summary_text.join("\n"));

    Ok(output)
}

fn generate_summary_text(total: usize, matches: usize, diffs: usize, missing: usize, extra: usize, elapsed: std::time::Duration, config: &Config) -> Vec<String> {
    let mode_str = format!("{:?}", config.mode);
    let algo_str = format!("{:?}", config.algo);
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
        println!("{}", output);
    }
    Ok(())
}

//=============================================================================
// Common Helper Functions
//=============================================================================

fn collect_files(dir: &Path) -> Result<Vec<PathBuf>> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| Ok(e.path().to_path_buf()))
        .collect()
}

fn compute_hashes(path: &Path, algo: HashAlgo) -> io::Result<HashResult> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; BUF_SIZE];

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

    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&buf[..n]);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            bh.update(&buf[..n]);
        }
    }

    let sha256 = sha256_hasher.map(|h| format!("{:x}", h.finalize()));
    let blake3 = blake3_hasher.map(|h| h.finalize().to_hex().to_string());

    Ok(HashResult { sha256, blake3 })
}

fn format_hashres(h: &HashResult, algo: HashAlgo) -> Result<String> {
    match algo {
        HashAlgo::Sha256 => Ok(h.sha256.as_ref().context("SHA256 hash not computed")?.clone()),
        HashAlgo::Blake3 => Ok(h.blake3.as_ref().context("BLAKE3 hash not computed")?.clone()),
        HashAlgo::Both => Ok(format!(
            "sha256:{} blake3:{}",
            h.sha256.as_ref().context("SHA256 hash not computed")?,
            h.blake3.as_ref().context("BLAKE3 hash not computed")?
        )),
    }
}

const FILE_COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::BrightCyan,
    Color::BrightGreen,
    Color::BrightYellow,
    Color::BrightMagenta,
];

fn color_for_file(name: &str) -> Color {
    let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    FILE_COLORS[hash % FILE_COLORS.len()]
}
