use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::{self, Write, IsTerminal};
use std::time::Duration;
use colored::*;
use anyhow::Result;

use crate::models::{ComparisonResult, ErrorEntry, OutputFormat, Mode, HashAlgo};

pub fn print_realtime_missing(status: &str, file: &Path, _verbose: bool) -> Result<()> {
    let (status_colored, file_color) = match status {
        "MISSING" => ("MISSING".blue(), Color::Blue),
        "EXTRA" => ("EXTRA".blue(), Color::Blue),
         _ => (status.normal(), Color::White),
    };
    println!("[{}]  {}", status_colored, file.to_str().unwrap_or("???").color(file_color));
    Ok(())
}

pub fn print_error_entry(e: &ErrorEntry, source: &str) {
    eprintln!(
        "[{}]{} ({}: {})",
        "ERROR".red().on_white(),
        e.path.display(),
        source,
        e.error
    );
}

pub struct ReportConfig {
    pub mode: Mode,
    pub algo: HashAlgo,
    pub output_format: OutputFormat,
    pub output_folder: Option<PathBuf>,
    pub no_sort: bool,
    pub threads: Option<usize>,
    pub verbose: bool,
}

pub fn generate_summary_text(
    total: usize, matches: usize, diffs: usize, missing: usize, extra: usize, errors: usize, 
    elapsed: Duration, config: &ReportConfig
) -> Vec<String> {
    let mode_str = format!("{:?}", config.mode);
    let algo_str = if config.mode == Mode::Metadata {
        "Metadata".to_string()
    } else {
        format!("{:?}", config.algo)
    };
    let threads_str = if let Some(t) = config.threads {
        t.to_string()
    } else {
        format!("Default ({})", rayon::current_num_threads())
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
    add_line(&mut output, "Threads", &threads_str, Color::Cyan, Color::Magenta);
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

pub fn generate_text_report(
    results: &[ComparisonResult],
    errors1: &[ErrorEntry],
    errors2: &[ErrorEntry],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    errors: usize,
    elapsed: Duration,
    config: &ReportConfig,
) -> Result<String> {
    let mut output = String::new();

    for e in errors1 {
        output.push_str(&format!("[{}] {} (folder1: {})
", "ERROR".red().on_white(), e.path.display(), e.error));
    }
    for e in errors2 {
        output.push_str(&format!("[{}] {} (folder2: {})
", "ERROR".red().on_white(), e.path.display(), e.error));
    }

    for result in results {
        output.push_str(&result.format_text(config.verbose, config.algo)?);
    }
    
    output.push_str("
");
    let summary_text = generate_summary_text(total, matches, diffs, missing, extra, errors, elapsed, config);
    output.push_str(&summary_text.join("
"));

    Ok(output)
}

pub fn generate_json_report(
    results: &[ComparisonResult],
    errors1: &[ErrorEntry],
    errors2: &[ErrorEntry],
    total: usize,
    matches: usize,
    diffs: usize,
    missing: usize,
    extra: usize,
    errors: usize,
    elapsed: Duration,
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

pub fn write_report(output: String, output_folder: &Option<PathBuf>, filename: &str) -> Result<()> {
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
