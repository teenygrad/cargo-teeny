//! Host-arch build + run for AOT kernel compilation (`cargo teeny aot`).
//!
//! Unlike `build`/`check`/`clippy` (which cross-compile via `cross` for a
//! *target* board), this builds for the *host* triple via plain `cargo run`
//! and forwards `--device`/`--options`/`--cache-dir`/`--force` to the
//! resulting binary verbatim. cargo-teeny never parses those itself — the
//! binary (typically linking `teeny-cli`) does.

use std::process;

use anyhow::{Context, Result, bail};

use crate::cli::AotArgs;

pub fn run(args: AotArgs) -> Result<()> {
    let mut cmd = process::Command::new("cargo");
    cmd.arg("run");

    if !args.no_release {
        cmd.arg("--release");
    }

    match (&args.bin, &args.example) {
        (Some(name), None) => {
            cmd.args(["--bin", name]);
        }
        (None, Some(name)) => {
            cmd.args(["--example", name]);
        }
        (None, None) => bail!("`cargo teeny aot` requires --bin <name> or --example <name>"),
        (Some(_), Some(_)) => {
            unreachable!("clap enforces --bin/--example are mutually exclusive")
        }
    }

    cmd.arg("--");
    cmd.args(["--device", &args.device]);
    if let Some(ref options) = args.options {
        cmd.args(["--options", options]);
    }
    if let Some(ref cache_dir) = args.cache_dir {
        cmd.args(["--cache-dir", &cache_dir.display().to_string()]);
    }
    if args.force {
        cmd.arg("--force");
    }

    let status = cmd.status().context("spawn `cargo run`")?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
