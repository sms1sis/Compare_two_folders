#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod models;
mod utils;
mod report;
mod compare;
mod snapshot;
mod sync;

use std::path::PathBuf;
use std::io::IsTerminal;
use clap::{Parser, Subcommand};
use colored::control;
use anyhow::Result;

use crate::models::{HashAlgo, OutputFormat, Mode, SymlinkMode};
use crate::compare::{run_compare, CompareConfig, ExitStatus};
use crate::snapshot::{create_snapshot, verify_snapshot, SnapshotConfig, VerifyConfig};
use crate::sync::{run_sync, SyncConfig};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// First folder to compare (legacy mode)
    #[arg(help_heading = "Legacy Mode")]
    folder1: Option<PathBuf>,
    /// Second folder to compare (legacy mode)
    #[arg(help_heading = "Legacy Mode")]
    folder2: Option<PathBuf>,

    #[arg(short, long, value_enum, default_value_t = Mode::Batch, global = true)]
    mode: Mode,
    #[arg(short, long, value_enum, default_value_t = HashAlgo::Blake3, global = true)]
    algo: HashAlgo,
    #[arg(short, long, global = true)]
    output_folder: Option<PathBuf>,
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Txt, global = true)]
    output_format: OutputFormat,
    #[arg(long, global = true)]
    depth: Option<usize>,
    #[arg(long, conflicts_with = "depth", global = true)]
    no_recursive: bool,
    #[arg(long, value_enum, default_value_t = SymlinkMode::Ignore, global = true)]
    symlinks: SymlinkMode,
    #[arg(short, long, default_value_t = false, global = true)]
    verbose: bool,
    #[arg(short = 'H', long, default_value_t = false, global = true)]
    hidden: bool,
    #[arg(short = 't', long = "type", action = clap::ArgAction::Append, global = true)]
    types: Option<Vec<String>>,
    #[arg(short = 'i', long, action = clap::ArgAction::Append, global = true)]
    ignore: Option<Vec<String>>,
    #[arg(short = 'j', long, value_name = "COUNT", global = true)]
    threads: Option<usize>,
    #[arg(short = 'n', long, default_value_t = false, global = true)]
    no_sort: bool,
    /// Command to use for external diff (e.g., "code --diff", "vimdiff")
    #[arg(long, value_name = "COMMAND", global = true)]
    diff_cmd: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Standard comparison between two folders
    Compare {
        folder1: PathBuf,
        folder2: PathBuf,
    },
    /// Create a snapshot of a folder's state
    Snapshot {
        folder: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verify a folder against a previously created snapshot
    Verify {
        folder: PathBuf,
        snapshot: PathBuf,
    },
    /// Sync changes from source to destination
    Sync {
        /// Source folder
        source: PathBuf,
        /// Destination folder
        destination: PathBuf,
        /// Perform a dry run without making any changes
        #[arg(long, default_value_t = true)]
        dry_run: bool,
        /// Delete extraneous files in the destination that are not in the source
        #[arg(long, default_value_t = false)]
        delete_extraneous: bool,
        /// Do not delete files, only copy
        #[arg(long, conflicts_with = "delete_extraneous")]
        no_delete: bool,
    },
}

fn main() {
    #[cfg(windows)]
    control::set_virtual_terminal(true).ok();

    if !std::io::stdout().is_terminal() {
        control::set_override(false);
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
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Compare { folder1, folder2 }) => {
            run_compare(CompareConfig {
                folder1, folder2, mode: cli.mode, algo: cli.algo, output_folder: cli.output_folder,
                output_format: cli.output_format, depth: cli.depth, no_recursive: cli.no_recursive,
                symlinks: cli.symlinks, verbose: cli.verbose, hidden: cli.hidden,
                types: cli.types, ignore: cli.ignore, threads: cli.threads, no_sort: cli.no_sort,
                diff_cmd: cli.diff_cmd,
            })
        },
        Some(Commands::Snapshot { folder, output }) => {
            create_snapshot(SnapshotConfig {
                folder, output, algo: cli.algo, depth: cli.depth, no_recursive: cli.no_recursive,
                hidden: cli.hidden, types: cli.types, ignore: cli.ignore, symlinks: cli.symlinks,
                threads: cli.threads
            })?;
            Ok(ExitStatus::Success)
        },
        Some(Commands::Verify { folder, snapshot }) => {
            verify_snapshot(VerifyConfig {
                folder, snapshot_path: snapshot, threads: cli.threads, 
                output_format: cli.output_format, verbose: cli.verbose
            })
        },
        Some(Commands::Sync { source, destination, dry_run, delete_extraneous, no_delete }) => {
            run_sync(SyncConfig {
                source, destination, dry_run, delete_extraneous, no_delete,
                algo: cli.algo, depth: cli.depth, no_recursive: cli.no_recursive,
                symlinks: cli.symlinks, hidden: cli.hidden, types: cli.types,
                ignore: cli.ignore, threads: cli.threads,
            })
        },
        None => {
            // Default to Compare with legacy args
            if let (Some(f1), Some(f2)) = (cli.folder1, cli.folder2) {
                run_compare(CompareConfig {
                    folder1: f1, folder2: f2,
                    mode: cli.mode, algo: cli.algo, output_folder: cli.output_folder,
                    output_format: cli.output_format, depth: cli.depth, no_recursive: cli.no_recursive,
                    symlinks: cli.symlinks, verbose: cli.verbose, hidden: cli.hidden,
                    types: cli.types, ignore: cli.ignore, threads: cli.threads, no_sort: cli.no_sort,
                    diff_cmd: cli.diff_cmd,                })
            } else {
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                cmd.print_help()?;
                Ok(ExitStatus::Error)
            }
        }
    }
}
