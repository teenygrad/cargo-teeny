use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Cargo integration for the teeny compiler (`cargo teeny …`).
#[derive(Parser)]
#[command(
    name = "cargo-teeny",
    version,
    about,
    disable_help_subcommand = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Lay out an empty sysroot tree for a cross toolchain (`--sysroot=…`).
    Sysroot(SysrootArgs),
}

#[derive(Parser)]
pub struct SysrootArgs {
    /// Host or target triple this sysroot is for (e.g. `aarch64-unknown-linux-gnu`).
    #[arg(long)]
    pub host: String,

    /// Root directory for the sysroot (created if it does not exist).
    #[arg(long)]
    pub path: PathBuf,
}
