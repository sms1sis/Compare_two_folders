use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use colored::*;
use serde::{Serialize};
use sha2::{Digest, Sha256};
use blake3;

const BUF_SIZE: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HashAlgo {
    Sha256,
    Blake3,
    Both,
}

impl std::str::FromStr for HashAlgo {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "sha256" => Ok(HashAlgo::Sha256),
            "blake3" => Ok(HashAlgo::Blake3),
            "both" => Ok(HashAlgo::Both),
            other => Err(format!("unknown algo `{}`. allowed: sha256, blake3, both", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Txt,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "txt" => Ok(OutputFormat::Txt),
            "json" => Ok(OutputFormat::Json),
            other => Err(format!("unknown format `{}`. allowed: txt, json", other)),
        }
    }
}

#[derive(Debug, Serialize)]
struct HashResult {
    sha256: Option<String>,
    blake3: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComparisonResult {
    file: PathBuf,
    status: String,
    hash1: Option<HashResult>,
    hash2: Option<HashResult>,
}
// Color palette to cycle through for file names
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

// Helper to pick a color for a file based on its hash or index
fn color_for_file(name: &str) -> Color {
    let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    FILE_COLORS[hash % FILE_COLORS.len()]
}

fn compute_hashes(path: &Path, algo: HashAlgo) -> io::Result<HashResult> {
    // Open file
    let mut f = File::open(path)?;
    let mut buf = [0u8; BUF_SIZE];

    // prepare hashers
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
        if n == 0 { break; }
        if let Some(h) = sha256_hasher.as_mut() {
            h.update(&buf[..n]);
        }
        if let Some(bh) = blake3_hasher.as_mut() {
            bh.update(&buf[..n]);
        }
    }

    let sha256 = sha256_hasher.map(|h| {
        let result = h.finalize();
        hex::encode(result) // hex crate not necessary if you prefer format("{:x}", ...)
    });

    let blake3 = blake3_hasher.map(|h| {
        let out = h.finalize();
        out.to_hex().to_string()
    });

    Ok(HashResult { sha256, blake3 })
}

fn format_hashres(h: &HashResult, algo: HashAlgo) -> String {
    match algo {
        HashAlgo::Sha256 => h.sha256.as_ref().unwrap().clone(),
        HashAlgo::Blake3 => h.blake3.as_ref().unwrap().clone(),
        HashAlgo::Both => format!(
            "sha256:{}
 blake3:{}",
            h.sha256.as_ref().unwrap(),
            h.blake3.as_ref().unwrap()
        ),
    }
}

// Example small helper to walk a directory and collect files with relative paths
fn collect_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn main() -> io::Result<()> {
    let start_time = Instant::now();
    // Simple CLI parsing: <dir1> <dir2> [--algo=<sha256|blake3|both>] [--output-folder=<path>] [--output-format=<txt|json>]
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <folder1> <folder2> [--algo=sha256|blake3|both] [--output-folder=<path>] [--output-format=<txt|json>]",
            args[0]
        );
        std::process::exit(2);
    }
    let folder1 = Path::new(&args[1]);
    let folder2 = Path::new(&args[2]);

    // parse optional flags
    let mut algo = HashAlgo::Both;
    let mut output_folder: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Txt;

    for a in &args[3..] {
        if let Some(rest) = a.strip_prefix("--algo=") {
            match rest.parse::<HashAlgo>() {
                Ok(parsed) => algo = parsed,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(2);
                }
            }
        } else if let Some(rest) = a.strip_prefix("--output-folder=") {
            output_folder = Some(PathBuf::from(rest));
        } else if let Some(rest) = a.strip_prefix("--output-format=") {
            match rest.parse::<OutputFormat>() {
                Ok(parsed) => output_format = parsed,
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(2);
                }
            }
        }
    }

    // Collect files (relative paths)
    let files1 = collect_files(folder1)?;
    let files2 = collect_files(folder2)?;

    use std::collections::HashMap;
    let mut map1: HashMap<PathBuf, HashResult> = HashMap::new();
    let mut map2: HashMap<PathBuf, HashResult> = HashMap::new();

    // Normalize paths to relative paths from the folder root so comparisons match.
    for f in files1 {
        let rel = f.strip_prefix(folder1).unwrap().to_path_buf();
        let h = compute_hashes(&f, algo)?;
        map1.insert(rel, h);
    }
    for f in files2 {
        let rel = f.strip_prefix(folder2).unwrap().to_path_buf();
        let h = compute_hashes(&f, algo)?;
        map2.insert(rel, h);
    }

    // Compare keys (filenames)
    let mut results: Vec<ComparisonResult> = Vec::new();
    let mut total = 0usize;
    let mut matches = 0usize;
    let mut diffs = 0usize;
    let mut missing = 0usize;
    let mut extra = 0usize;

    // For stable listing, gather union of keys
    let mut all_keys: Vec<_> = map1.keys().chain(map2.keys()).collect();
    all_keys.sort();
    all_keys.dedup();

    for key in all_keys {
        total += 1;
        let in1 = map1.get(key);
        let in2 = map2.get(key);

        let (status, h1, h2) = match (in1, in2) {
            (Some(h1), Some(h2)) => {
                let is_match = match algo {
                    HashAlgo::Sha256 => h1.sha256 == h2.sha256,
                    HashAlgo::Blake3 => h1.blake3 == h2.blake3,
                    HashAlgo::Both => h1.sha256 == h2.sha256 && h1.blake3 == h2.blake3,
                };
                if is_match {
                    matches += 1;
                    ("MATCH", Some(h1), Some(h2))
                } else {
                    diffs += 1;
                    ("DIFF", Some(h1), Some(h2))
                }
            }
            (Some(h1), None) => {
                missing += 1;
                ("MISSING", Some(h1), None)
            }
            (None, Some(h2)) => {
                extra += 1;
                ("EXTRA", None, Some(h2))
            }
            _ => continue, // Should not happen
        };

        results.push(ComparisonResult {
            file: key.clone(),
            status: status.to_string(),
            hash1: h1.map(|h| HashResult { sha256: h.sha256.clone(), blake3: h.blake3.clone() }),
            hash2: h2.map(|h| HashResult { sha256: h.sha256.clone(), blake3: h.blake3.clone() }),
        });
    }

    let elapsed = start_time.elapsed();

    match output_format {
        OutputFormat::Txt => {
            if let Some(output_folder) = output_folder {
                let mut output = String::new();
                output.push_str(&format!("{}\n", "==============================================".bright_blue()));
                output.push_str("   Folder File Comparison Utility\n");
                output.push_str(&format!("{}\n", "==============================================".bright_blue()));

                for res in &results {
                    let status_colored = match res.status.as_str() {
                        "MATCH" => "MATCH".green(),
                        "DIFF" => "DIFF".yellow(),
                        "MISSING" => "MISSING".red(),
                        "EXTRA" => "EXTRA".cyan(),
                        _ => res.status.normal(),
                    };

                    output.push_str(&format!(
                        "[{}]  {}\n",
                        status_colored,
                        res.file.display().to_string().color(color_for_file(res.file.to_str().unwrap()))
                    ));

                    if res.status == "DIFF" {
                        if let (Some(h1), Some(h2)) = (&res.hash1, &res.hash2) {
                            output.push_str(&format!("    {}: {}\n", "folder1".dimmed(), format_hashres(h1, algo)));
                            output.push_str(&format!("    {}: {}\n", "folder2".dimmed(), format_hashres(h2, algo)));
                        }
                    } else if res.status == "MATCH" {
                        if let Some(h1) = &res.hash1 {
                            output.push_str(&format!("    {}: {}\n", "in_both".dimmed(), format_hashres(h1, algo)));
                        }
                    }
                    output.push_str("\n");
                }

                output.push_str(&format!("{}\n", "-----------------------------------------------".bright_blue()));
                let total_width = 47; // same width as your dash lines
                let title = "Summary";
                let padding = (total_width - title.len()) / 2;
                output.push_str(&format!("{:padding$}{}\n", "", title.bold().white().on_bright_black(), padding = padding));
                output.push_str(&format!("{}\n", "-----------------------------------------------".bright_blue()));
                output.push_str(&format!("{} {}\n", "Total files checked  :".bold().cyan(), total.to_string().bold().white()));
                output.push_str(&format!("{} {}\n", "Matches              :".bold().green(), matches.to_string().bold().green()));
                output.push_str(&format!("{} {}\n", "Differences          :".bold().yellow(), diffs.to_string().bold().yellow()));
                output.push_str(&format!("{} {}\n", "Missing in Folder2   :".bold().red(), missing.to_string().bold().red()));
                output.push_str(&format!("{} {}\n", "Extra in Folder2     :".bold().magenta(), extra.to_string().bold().magenta()));
                output.push_str(&format!("{} {:.2?}\n", "Time taken           :".bold().blue(), elapsed));
                output.push_str(&format!("{}\n", "==============================================".bright_blue()));

                fs::create_dir_all(&output_folder)?;
                let report_path = output_folder.join("report.txt");
                let mut file = File::create(report_path)?;
                file.write_all(output.as_bytes())?;
            } else {
                println!("{}", "==============================================".bright_blue());
                println!("   Folder File Comparison Utility");
                println!("{}", "==============================================".bright_blue());

                for res in &results {
                    let status_colored = match res.status.as_str() {
                        "MATCH" => "MATCH".green(),
                        "DIFF" => "DIFF".yellow(),
                        "MISSING" => "MISSING".red(),
                        "EXTRA" => "EXTRA".cyan(),
                        _ => res.status.normal(),
                    };

                    println!(
                        "[{}]  {}",
                        status_colored,
                        res.file.display().to_string().color(color_for_file(res.file.to_str().unwrap()))
                    );

                    if res.status == "DIFF" {
                        if let (Some(h1), Some(h2)) = (&res.hash1, &res.hash2) {
                            println!("    {}: {}", "folder1".dimmed(), format_hashres(h1, algo));
                            println!("    {}: {}", "folder2".dimmed(), format_hashres(h2, algo));
                        }
                    } else if res.status == "MATCH" {
                        if let Some(h1) = &res.hash1 {
                            println!("    {}: {}", "in_both".dimmed(), format_hashres(h1, algo));
                        }
                    }
                    println!();
                }

                println!("{}", "-----------------------------------------------".bright_blue());
                let total_width = 47; // same width as your dash lines
                let title = "Summary";
                let padding = (total_width - title.len()) / 2;
                println!("{:padding$}{}", "", title.bold().white().on_bright_black(), padding = padding);
                println!("{}", "-----------------------------------------------".bright_blue());
                println!("{} {}", "Total files checked  :".bold().cyan(), total.to_string().bold().white());
                println!("{} {}", "Matches              :".bold().green(), matches.to_string().bold().green());
                println!("{} {}", "Differences          :".bold().yellow(), diffs.to_string().bold().yellow());
                println!("{} {}", "Missing in Folder2   :".bold().red(), missing.to_string().bold().red());
                println!("{} {}", "Extra in Folder2     :".bold().magenta(), extra.to_string().bold().magenta());
                println!("{} {:.2?}", "Time taken           :".bold().blue(), elapsed);
                println!("{}", "==============================================".bright_blue());
            }
        }
        OutputFormat::Json => {
            let json_summary = serde_json::json!({
                "total_files_checked": total,
                "matches": matches,
                "differences": diffs,
                "missing_in_folder2": missing,
                "extra_in_folder2": extra,
                "time_taken": format!("{:.2?}", elapsed),
            });

            let json_output = serde_json::json!({
                "summary": json_summary,
                "results": results,
            });

            let output = serde_json::to_string_pretty(&json_output)?;

            if let Some(output_folder) = output_folder {
                fs::create_dir_all(&output_folder)?;
                let report_path = output_folder.join("report.json");
                let mut file = File::create(report_path)?;
                file.write_all(output.as_bytes())?;
            } else {
                println!("{}", output);
            }
        }
    }

    Ok(())
}
