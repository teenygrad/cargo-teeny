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
    /// Root directory for the sysroot (created if it does not exist).
    pub path: PathBuf,
}
