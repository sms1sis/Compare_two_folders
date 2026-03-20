use clap::ValueEnum;
use colored::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    Sha256,
    Blake3,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Txt,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Realtime,
    Batch,
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkMode {
    #[default]
    Ignore,
    Follow,
    Compare,
}

// Fix #8: Status is now a typed enum instead of a plain String,
// eliminating silent fall-through bugs on typos/missing arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Match,
    Diff,
    Missing,
    Extra,
    Error,
    /// Used by sync subcommand
    Create,
    /// Used by sync subcommand
    Update,
    /// Used by sync subcommand
    Delete,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Match => write!(f, "MATCH"),
            Status::Diff => write!(f, "DIFF"),
            Status::Missing => write!(f, "MISSING"),
            Status::Extra => write!(f, "EXTRA"),
            Status::Error => write!(f, "ERROR"),
            Status::Create => write!(f, "CREATE"),
            Status::Update => write!(f, "UPDATE"),
            Status::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashResult {
    pub sha256: Option<String>,
    pub blake3: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEntry {
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub file: PathBuf,
    pub status: Status,
    pub hash1: Option<HashResult>,
    pub hash2: Option<HashResult>,
    pub size1: Option<u64>,
    pub size2: Option<u64>,
    pub modified1: Option<String>,
    pub modified2: Option<String>,
    pub symlink1: Option<String>,
    pub symlink2: Option<String>,
}

// Fix #12: Constructor helpers to eliminate boilerplate None-filled struct construction.
impl ComparisonResult {
    pub fn missing(path: PathBuf) -> Self {
        Self {
            file: path,
            status: Status::Missing,
            hash1: None,
            hash2: None,
            size1: None,
            size2: None,
            modified1: None,
            modified2: None,
            symlink1: None,
            symlink2: None,
        }
    }

    pub fn extra(path: PathBuf) -> Self {
        Self {
            file: path,
            status: Status::Extra,
            hash1: None,
            hash2: None,
            size1: None,
            size2: None,
            modified1: None,
            modified2: None,
            symlink1: None,
            symlink2: None,
        }
    }

    pub fn format_text(&self, verbose: bool, algo: HashAlgo) -> anyhow::Result<String> {
        let mut output = String::new();
        let (status_colored, file_color) = match self.status {
            Status::Match => ("MATCH".green(), Color::Green),
            Status::Diff => ("DIFF".red(), Color::Red),
            Status::Missing => ("MISSING".blue(), Color::Blue),
            Status::Extra => ("EXTRA".blue(), Color::Blue),
            Status::Error => ("ERROR".red().on_white(), Color::Red),
            Status::Create => ("CREATE".green(), Color::Green),
            Status::Update => ("UPDATE".yellow(), Color::Yellow),
            Status::Delete => ("DELETE".red(), Color::Red),
        };

        let file_name = self.file.to_str().unwrap_or("Invalid Name");
        output.push_str(&format!(
            "[{}]  {}\n",
            status_colored,
            file_name.color(file_color)
        ));

        if verbose {
            if self.status == Status::Diff {
                if let (Some(h1), Some(h2)) = (&self.hash1, &self.hash2) {
                    output.push_str(&format!(
                        "    {}: {}\n",
                        "folder1".dimmed(),
                        self.format_hashres(h1, algo)?
                    ));
                    output.push_str(&format!(
                        "    {}: {}\n",
                        "folder2".dimmed(),
                        self.format_hashres(h2, algo)?
                    ));
                } else if let (Some(s1), Some(s2)) = (self.size1, self.size2) {
                    if s1 != s2 {
                        output.push_str(&format!(
                            "    {}: {}\n",
                            "folder1".dimmed(),
                            format!("{} bytes", s1).cyan()
                        ));
                        output.push_str(&format!(
                            "    {}: {}\n",
                            "folder2".dimmed(),
                            format!("{} bytes", s2).cyan()
                        ));
                    } else if let (Some(t1), Some(t2)) = (&self.modified1, &self.modified2) {
                        if t1 != t2 {
                            output.push_str(&format!(
                                "    {}: {}\n",
                                "folder1".dimmed(),
                                t1.cyan()
                            ));
                            output.push_str(&format!(
                                "    {}: {}\n",
                                "folder2".dimmed(),
                                t2.cyan()
                            ));
                        }
                    } else if let (Some(sym1), Some(sym2)) = (&self.symlink1, &self.symlink2)
                        && sym1 != sym2
                    {
                        output.push_str(&format!(
                            "    {}: -> {}\n",
                            "folder1".dimmed(),
                            sym1.cyan()
                        ));
                        output.push_str(&format!(
                            "    {}: -> {}\n",
                            "folder2".dimmed(),
                            sym2.cyan()
                        ));
                    }
                }
            } else if self.status == Status::Match
                && let Some(h1) = &self.hash1
            {
                output.push_str(&format!(
                    "    {}: {}\n",
                    "in_both".dimmed(),
                    self.format_hashres(h1, algo)?
                ));
            }
        }
        Ok(output)
    }

    fn format_hashres(&self, h: &HashResult, algo: HashAlgo) -> anyhow::Result<String> {
        use anyhow::Context;
        match algo {
            HashAlgo::Sha256 => Ok(h
                .sha256
                .as_ref()
                .context("SHA256 hash not computed")?
                .color(Color::Cyan)
                .to_string()),
            HashAlgo::Blake3 => Ok(h
                .blake3
                .as_ref()
                .context("BLAKE3 hash not computed")?
                .color(Color::Cyan)
                .to_string()),
            HashAlgo::Both => Ok(format!(
                "sha256:{}\n            blake3:{}",
                h.sha256
                    .as_ref()
                    .context("SHA256 hash not computed")?
                    .color(Color::Cyan),
                h.blake3
                    .as_ref()
                    .context("BLAKE3 hash not computed")?
                    .color(Color::Cyan)
            )),
        }
    }
}
