//! Cross-compile and run a binary under QEMU user-mode emulation (`cargo teeny run`).
//!
//! Builds the binary/example for a RISC-V board — delegating to `commands::build::run` — then
//! runs the result with `qemu-riscv64 -cpu <model> -L <sysroot>`. `-L` is where QEMU finds the
//! RISC-V dynamic linker and libc: the Debian/Ubuntu cross libc (`libc6-dev-riscv64-cross`,
//! `/usr/riscv64-linux-gnu`) by default, or a board sysroot from `cargo teeny sysroot`. stdio is
//! inherited and the binary's exit code is propagated.

use std::env;
use std::path::Path;
use std::process::{self, Command};

use anyhow::{Context, Result, bail};

use crate::cli::{BuildArgs, RunArgs};
use crate::commands::build::{self, CrossVerb, built_artifact_path};
use crate::profiles::{board_profile, board_type_cli_name};
use crate::workspace;

/// The dynamic linker `riscv64gc-unknown-linux-gnu` binaries request (`PT_INTERP`), relative to
/// the sysroot root.
const RISCV_DYNAMIC_LINKER: &str = "lib/ld-linux-riscv64-lp64d.so.1";

pub fn run(args: RunArgs) -> Result<()> {
    let profile = board_profile(args.target);
    let Some(qemu_cpu) = profile.qemu_cpu else {
        bail!(
            "`cargo teeny run` only supports RISC-V boards (got --target {})",
            board_type_cli_name(args.target)
        );
    };

    let (name, is_bin) = match (&args.bin, &args.example) {
        (Some(name), None) => (name.clone(), true),
        (None, Some(name)) => (name.clone(), false),
        (None, None) => bail!("`cargo teeny run` requires --bin <name> or --example <name>"),
        (Some(_), Some(_)) => {
            unreachable!("clap enforces --bin/--example are mutually exclusive")
        }
    };

    // Check the sysroot up front so a missing cross libc fails before a (slow) cross build.
    validate_sysroot(&args.sysroot)?;

    build::run(
        BuildArgs {
            target: args.target,
            package: args.package.clone(),
            cuda_path: None,
            no_release: args.no_release,
            examples: false,
            example: if is_bin { None } else { Some(name.clone()) },
            bin: if is_bin { Some(name.clone()) } else { None },
            features: args.features.clone(),
            extra: Vec::new(),
        },
        CrossVerb::Build,
    )?;

    let cwd = env::current_dir().context("get current directory")?;
    let (_manifest, cross_root, _manifest_rel) = workspace::resolve(&cwd, args.package.as_deref())?;
    let binary = built_artifact_path(
        &cross_root,
        profile.cross_triple,
        !args.no_release,
        is_bin,
        &name,
    );

    let mut cmd = Command::new(&args.qemu);
    cmd.args(qemu_args(qemu_cpu, &args.sysroot, &binary));
    cmd.args(&args.args);

    eprintln!(
        "cargo-teeny: running {} under {} (-cpu {qemu_cpu}, -L {})",
        binary.display(),
        args.qemu,
        args.sysroot.display()
    );

    let status = cmd.status().with_context(|| {
        format!(
            "spawn `{}` (install it with e.g. `apt install qemu-user`)",
            args.qemu
        )
    })?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn qemu_args(cpu: &str, sysroot: &Path, binary: &Path) -> Vec<String> {
    vec![
        "-cpu".into(),
        cpu.into(),
        "-L".into(),
        sysroot.display().to_string(),
        binary.display().to_string(),
    ]
}

fn validate_sysroot(sysroot: &Path) -> Result<()> {
    anyhow::ensure!(
        sysroot.is_dir(),
        "--sysroot {} is not a directory (install the RISC-V cross libc, e.g. \
         `apt install libc6-dev-riscv64-cross`, or create one with `cargo teeny sysroot \
         --rsync-from`)",
        sysroot.display()
    );
    // `exists` follows symlinks, so a dangling loader link (sysroot scaffolded without
    // `--rsync-from`) is reported here too.
    anyhow::ensure!(
        sysroot.join(RISCV_DYNAMIC_LINKER).exists(),
        "--sysroot {} has no RISC-V dynamic linker at {RISCV_DYNAMIC_LINKER} — populate it \
         with `cargo teeny sysroot --rsync-from <user@board>`",
        sysroot.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn qemu_args_format() {
        let args = qemu_args(
            "max",
            Path::new("/usr/riscv64-linux-gnu"),
            &PathBuf::from("/repo/target/riscv64gc-unknown-linux-gnu/release/demo"),
        );
        assert_eq!(
            args,
            vec![
                "-cpu",
                "max",
                "-L",
                "/usr/riscv64-linux-gnu",
                "/repo/target/riscv64gc-unknown-linux-gnu/release/demo",
            ]
        );
    }

    #[test]
    fn validate_sysroot_rejects_missing_dir() {
        assert!(validate_sysroot(Path::new("/nonexistent/cargo-teeny/sysroot")).is_err());
    }
}
