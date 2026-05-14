use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Cargo integration for the teeny compiler (`cargo teeny …`).
#[derive(Parser)]
#[command(name = "cargo-teeny", version, about, disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Lay out an empty sysroot tree for a cross toolchain (`--sysroot=…`).
    Sysroot(SysrootArgs),
    /// Cross-compile the current crate using `cross build`.
    Build(BuildArgs),
    /// Type-check the current crate via `cross check`.
    Check(BuildArgs),
    /// Lint the current crate via `cross clippy`.
    Clippy(BuildArgs),
}

/// Board or environment profile shared by sysroot and build commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum BoardType {
    JetsonOrinNano,
}

#[derive(Parser)]
pub struct SysrootArgs {
    /// Host or target triple this sysroot is for (e.g. `aarch64-unknown-linux-gnu`).
    #[arg(long)]
    pub host: String,

    /// Root directory for the sysroot (created if it does not exist).
    #[arg(long)]
    pub path: PathBuf,

    /// Sysroot profile (fixed set; controls layout and marker metadata).
    #[arg(long = "type", value_enum)]
    pub sysroot_type: BoardType,

    /// When set (e.g. `ubuntu@jetson`), run `rsync` over SSH after scaffolding; remote paths
    /// depend on `--type` (see `sysroot_rsync_folders` in `commands/sysroot.rs`).
    #[arg(long)]
    pub rsync_from: Option<String>,

    /// Remote shell passed to `rsync -e` (e.g. `ssh` or `ssh -p 2222`).
    #[arg(long = "rsync-ssh", default_value = "ssh")]
    pub rsync_ssh: String,
}

#[derive(Parser)]
pub struct BuildArgs {
    /// Board profile — controls the Rust/cross target triple and default volume mounts.
    #[arg(long, value_enum)]
    pub target: BoardType,

    /// Host path to the CUDA aarch64 target directory (overrides the profile default).
    #[arg(long)]
    pub cuda_path: Option<PathBuf>,

    /// Build in debug mode (omits `--release`; default is release).
    #[arg(long)]
    pub no_release: bool,

    /// Build all examples.
    #[arg(long)]
    pub examples: bool,

    /// Build a specific example by name.
    #[arg(long, conflicts_with = "examples")]
    pub example: Option<String>,

    /// Extra arguments forwarded verbatim to `cross` after `--`.
    #[arg(last = true)]
    pub extra: Vec<String>,
}
