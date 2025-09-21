use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use colored::*;
use sha2::{Sha256, Digest};
use terminal_size::{Width, terminal_size};

fn get_term_width() -> usize {
    if let Some((Width(w), _)) = terminal_size() {
        w as usize
    } else {
        80
    }
}

fn center(s: &str, width: usize) {
    let len = s.chars().count();
    if width > len {
        let pad = (width - len) / 2;
        println!("{:pad$}{}", "", s, pad=pad);
    } else {
        println!("{}", s);
    }
}

fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(folder: &str) -> io::Result<HashSet<String>> {
    let mut files = HashSet::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                files.insert(name.to_string());
            }
        }
    }
    Ok(files)
}

fn compute_max_filename_len(set1: &HashSet<String>, set2: &HashSet<String>) -> usize {
    set1.iter().chain(set2.iter()).map(|s| s.len()).max().unwrap_or(1)
}

fn print_status_line(
    color: &str, status: &str, filename: &str, suffix: &str,
    left_pad: usize, status_col_width: usize, filename_col_width: usize
) {
    if left_pad > 0 { print!("{:left_pad$}", "", left_pad=left_pad); }
    let status = match color {
        "green" => status.green(),
        "red" => status.red(),
        "yellow" => status.yellow(),
        "cyan" => status.cyan(),
        _ => status.normal(),
    };
    print!("{:<width$} ", status, width = status_col_width);
    print!("{:<width$}", filename, width = filename_col_width);
    if !suffix.is_empty() { print!("{}", suffix); }
    println!();
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && (args[1] == "-h" || args[1] == "--help") {
        println!("Folder File Comparison Utility");
        println!("Usage: {} <FOLDER1> <FOLDER2>", args[0]);
        println!("Compares files in FOLDER1 and FOLDER2 by SHA256 hash.");
        return Ok(());
    }
    if args.len() != 3 {
        eprintln!("Usage: {} <folder1> <folder2>", args[0]);
        eprintln!("Try '{} --help' for more information.", args[0]);
        std::process::exit(1);
    }
    let folder1 = &args[1];
    let folder2 = &args[2];

    let files1 = collect_files(folder1)?;
    let files2 = collect_files(folder2)?;

    let max_fname = compute_max_filename_len(&files1, &files2);
    let status_col_width = 11;
    let max_suffix_len = 20;
    let term_width = get_term_width();
    let content_width = status_col_width + 1 + max_fname + max_suffix_len;
    let left_pad = if term_width > content_width {
        (term_width - content_width) / 2
    } else {
        0
    };

    center("===============================================", term_width);
    center("Folder File Comparison Utility by sms1sis", term_width);
    center("===============================================", term_width);
    println!();
    center("Comparing files in folders:", term_width);
    center(&format!("Folder 1: {}", folder1), term_width);
    center(&format!("Folder 2: {}", folder2), term_width);
    center("-----------------------------------------------", term_width);
    println!();

    let mut total = 0;
    let mut match_count = 0;
    let mut diff = 0;
    let mut missing = 0;
    let mut extra = 0;

    for filename in &files1 {
        total += 1;
        let path1 = Path::new(folder1).join(filename);
        let path2 = Path::new(folder2).join(filename);
        if files2.contains(filename) {
            let h1 = file_sha256(&path1).unwrap_or_else(|_| "".to_string());
            let h2 = file_sha256(&path2).unwrap_or_else(|_| "".to_string());
            if !h1.is_empty() && !h2.is_empty() && h1 == h2 {
                print_status_line("green", "[MATCH]", filename, "", left_pad, status_col_width, max_fname);
                match_count += 1;
            } else {
                print_status_line("red", "[DIFF]", filename, "", left_pad, status_col_width, max_fname);
                diff += 1;
            }
        } else {
            print_status_line("yellow", "[MISSING]", filename, " not found in Folder2", left_pad, status_col_width, max_fname);
            missing += 1;
        }
    }
    for filename in &files2 {
        if !files1.contains(filename) {
            print_status_line("cyan", "[EXTRA]", filename, " only in Folder2", left_pad, status_col_width, max_fname);
            extra += 1;
        }
    }

    println!();
    center("-----------------------------------------------", term_width);
    center("Summary", term_width);
    center("-----------------------------------------------", term_width);

    let labels = [
        "Total files checked",
        "Matches",
        "Differences",
        "Missing in Folder2",
        "Extra in Folder2"
    ];
    let values = [total, match_count, diff, missing, extra];

    let label_width = labels.iter().map(|l| l.len()).max().unwrap();

    for (label, value) in labels.iter().zip(values.iter()) {
        let line = format!("{:<label_width$} : {}", label, value, label_width = label_width);
        center(&line, term_width);
    }

    center("===============================================", term_width);
    Ok(())
}
