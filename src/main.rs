use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use colored::*;
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

#[derive(Debug)]
struct HashResult {
    sha256: Option<String>,
    blake3: Option<String>,
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
        hex::encode(result) // hex crate not necessary if you prefer format!("{:x}", ...)
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
            "sha256:{} blake3:{}",
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
    // Simple CLI parsing: <dir1> <dir2> [--algo=<sha256|blake3|both>]
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <folder1> <folder2> [--algo=sha256|blake3|both]", args[0]);
        std::process::exit(2);
    }
    let folder1 = Path::new(&args[1]);
    let folder2 = Path::new(&args[2]);

    // parse optional algo flag
    let mut algo = HashAlgo::Both;
    for a in &args[3..] {
        if let Some(rest) = a.strip_prefix("--algo=") {
            match rest.parse::<HashAlgo>() {
                Ok(parsed) => algo = parsed,
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
    let mut total = 0usize;
    let mut matches = 0usize;
    let mut diffs = 0usize;
    let mut missing = 0usize;
    let mut extra = 0usize;

    // For stable listing, gather union of keys
    let mut all_keys: Vec<_> = map1.keys().chain(map2.keys()).collect();
    all_keys.sort();
    all_keys.dedup();

    println!("{}", "==============================================".bright_blue());
    println!("   Folder File Comparison Utility");
    println!("{}", "==============================================".bright_blue());

    for key in all_keys {
        total += 1;
        let in1 = map1.get(key);
        let in2 = map2.get(key);

        match (in1, in2) {
            (Some(h1), Some(h2)) => {
                // decide match logic: here, match only if the requested hashes equal
                let is_match = match algo {
                    HashAlgo::Sha256 => h1.sha256 == h2.sha256,
                    HashAlgo::Blake3 => h1.blake3 == h2.blake3,
                    HashAlgo::Both => h1.sha256 == h2.sha256 && h1.blake3 == h2.blake3,
                };
                if is_match {
                    matches += 1;
                    println!("[{}]  {}", "MATCH".green(), key.display());
                } else {
                    diffs += 1;
                    println!("[{}]   {}", "DIFF".yellow(), key.display());
                    println!("    {}: {}", "folder1".dimmed(), format_hashres(h1, algo));
                    println!("    {}: {}", "folder2".dimmed(), format_hashres(h2, algo));
                }
            }
            (Some(_), None) => {
                missing += 1;
                println!("[{}] {}", "MISSING".red(), key.display());
            }
            (None, Some(_)) => {
                extra += 1;
                println!("[{}] {}", "EXTRA".cyan(), key.display());
            }
            _ => {}
        }
    }

    println!("{}", "-----------------------------------------------".bright_blue());
    println!("{}", " Summary ".bold().white().on_bright_black());
    println!("{}", "-----------------------------------------------".bright_blue());
    println!("{} {}", "Total files checked  :".bold().cyan(), total.to_string().bold().white());
    println!("{} {}", "Matches              :".bold().green(), matches.to_string().bold().green());
    println!("{} {}", "Differences          :".bold().yellow(), diffs.to_string().bold().yellow());
    println!("{} {}", "Missing in Folder2   :".bold().red(), missing.to_string().bold().red());
    println!("{} {}", "Extra in Folder2     :".bold().magenta(), extra.to_string().bold().magenta());
    println!("{}", "==============================================".bright_blue());

    Ok(())
}
