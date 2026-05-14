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
        if i == 1 && arg == "teeny" { None } else { Some(arg) }
    });
    let cli = cli::Cli::parse_from(args);
    match cli.command {
        cli::Command::Sysroot(args) => commands::sysroot::run(args),
        cli::Command::Build(args) => commands::build::run(args, CrossVerb::Build),
        cli::Command::Check(args) => commands::build::run(args, CrossVerb::Check),
        cli::Command::Clippy(args) => commands::build::run(args, CrossVerb::Clippy),
    }
}
