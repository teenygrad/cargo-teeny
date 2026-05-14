mod cli;
mod commands;
mod profiles;
mod workspace;

use anyhow::Result;
use clap::Parser;

use commands::build::CrossVerb;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Sysroot(args) => commands::sysroot::run(args),
        cli::Command::Build(args) => commands::build::run(args, CrossVerb::Build),
        cli::Command::Check(args) => commands::build::run(args, CrossVerb::Check),
        cli::Command::Clippy(args) => commands::build::run(args, CrossVerb::Clippy),
    }
}
