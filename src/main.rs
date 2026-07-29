#![warn(missing_docs)]

//! Cargo integration for the teeny cross-compiler toolchain (`cargo teeny …`).
//!
//! Provides subcommands for installing the custom `teenyc` rustup toolchain,
//! cross-compiling and packaging a project for a target board, ahead-of-time
//! compiling GPU kernels, and deploying the result over SSH. See
//! [`cli::Command`] for the full subcommand list, or run `cargo teeny --help`.

mod cli;
mod commands;
mod profiles;
mod workspace;

use anyhow::Result;
use clap::Parser;

use commands::build::CrossVerb;

fn main() -> Result<()> {
    // When invoked as `cargo teeny`, cargo prepends the subcommand name ("teeny") as
    // the first argument. Strip it so clap sees only our own subcommands.
    let args = std::env::args_os().enumerate().filter_map(|(i, arg)| {
        if i == 1 && arg == "teeny" {
            None
        } else {
            Some(arg)
        }
    });
    let cli = cli::Cli::parse_from(args);
    match cli.command {
        cli::Command::Sysroot(args) => commands::sysroot::run(args),
        cli::Command::Build(args) => commands::build::run(args, CrossVerb::Build),
        cli::Command::Check(args) => commands::build::run(args, CrossVerb::Check),
        cli::Command::Clippy(args) => commands::build::run(args, CrossVerb::Clippy),
        cli::Command::InstallToolchain(args) => commands::install_toolchain::run(args),
        cli::Command::Aot(args) => commands::aot::run(args),
        cli::Command::Package(args) => commands::package::run(args),
        cli::Command::Deploy(args) => commands::deploy::run(args),
    }
}
