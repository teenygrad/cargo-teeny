//! Cross-compilation via `cross build / check / clippy`.

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};

use crate::cli::BuildArgs;
use crate::profiles::board_profile;
use crate::workspace;

#[derive(Clone, Copy)]
pub enum CrossVerb {
    Build,
    Check,
    Clippy,
}

impl CrossVerb {
    fn as_str(self) -> &'static str {
        match self {
            CrossVerb::Build => "build",
            CrossVerb::Check => "check",
            CrossVerb::Clippy => "clippy",
        }
    }
}

pub fn run(args: BuildArgs, verb: CrossVerb) -> Result<()> {
    let cwd = env::current_dir().context("get current directory")?;
    let (_manifest, cross_root, manifest_arg) = workspace::resolve(&cwd, args.package.as_deref())?;

    // `[patch.crates-io]` is only valid in the workspace root manifest — read it from
    // `cross_root`, not from `manifest`, which may be a workspace member with no patches
    // of its own (e.g. a demo crate patched only via the root `Cargo.toml`). Absent entirely
    // (a published crate, no local teenygrad checkout) or pointing at a path that doesn't
    // exist on disk both mean "nothing to mount" rather than an error — see
    // `workspace::teenygrad_mount`.
    let teenygrad = workspace::teenygrad_mount(&cross_root.join("Cargo.toml"))?;

    let profile = board_profile(args.target);
    let cuda_host = args
        .cuda_path
        .unwrap_or_else(|| PathBuf::from(profile.cuda_host_path));

    let container_opts =
        build_container_opts(teenygrad.as_deref(), &cuda_host, profile.cuda_container_path);

    // Prepend any existing CROSS_CONTAINER_OPTS so callers can inject extra mounts.
    let merged_opts = match env::var("CROSS_CONTAINER_OPTS")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        Some(existing) => format!("{existing} {container_opts}"),
        None => container_opts,
    };

    let mut cmd = process::Command::new("cross");
    cmd.current_dir(&cross_root);
    cmd.arg(verb.as_str());
    cmd.args(["--manifest-path", &manifest_arg.display().to_string()]);
    cmd.args(["--target", profile.cross_triple]);

    if !args.no_release {
        cmd.arg("--release");
    }

    if let Some(ref features) = args.features {
        cmd.args(["--features", features]);
    }

    if args.examples {
        cmd.arg("--examples");
    } else if let Some(ref name) = args.example {
        cmd.args(["--example", name]);
    } else if let Some(ref name) = args.bin {
        cmd.args(["--bin", name]);
    }

    if !args.extra.is_empty() {
        cmd.arg("--");
        cmd.args(&args.extra);
    }

    cmd.env("CROSS_CONTAINER_OPTS", &merged_opts);

    match &teenygrad {
        Some(root) => eprintln!("cargo-teeny: teenygrad root  {}", root.display()),
        None => eprintln!("cargo-teeny: teenygrad root  (not mounted — no local checkout)"),
    }
    eprintln!(
        "cargo-teeny: cross root      {} (--manifest-path {})",
        cross_root.display(),
        manifest_arg.display()
    );
    eprintln!("cargo-teeny: CROSS_CONTAINER_OPTS={merged_opts}");

    let status = cmd.status().context("spawn `cross`")?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn build_container_opts(teenygrad: Option<&Path>, cuda_host: &Path, cuda_container: &str) -> String {
    let cuda_opt = format!("-v {}:{}", cuda_host.display(), cuda_container);
    match teenygrad {
        Some(teenygrad) => format!("-v {t}:{t} {cuda_opt}", t = teenygrad.display()),
        None => cuda_opt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_opts_format() {
        let teenygrad = PathBuf::from("/home/user/teenygrad");
        let cuda_host = PathBuf::from("/usr/local/cuda-12.6/targets/aarch64-linux");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(Some(&teenygrad), &cuda_host, cuda_container);
        assert_eq!(
            opts,
            "-v /home/user/teenygrad:/home/user/teenygrad \
             -v /usr/local/cuda-12.6/targets/aarch64-linux:/usr/local/cuda-12.6/targets/aarch64-linux"
        );
    }

    #[test]
    fn container_opts_custom_cuda_host() {
        let teenygrad = PathBuf::from("/repos/teenygrad");
        let cuda_host = PathBuf::from("/opt/cuda-12.8/aarch64");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(Some(&teenygrad), &cuda_host, cuda_container);
        assert!(
            opts.contains("-v /opt/cuda-12.8/aarch64:/usr/local/cuda-12.6/targets/aarch64-linux")
        );
    }

    #[test]
    fn container_opts_without_teenygrad() {
        let cuda_host = PathBuf::from("/usr/local/cuda-12.6/targets/aarch64-linux");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(None, &cuda_host, cuda_container);
        assert_eq!(
            opts,
            "-v /usr/local/cuda-12.6/targets/aarch64-linux:/usr/local/cuda-12.6/targets/aarch64-linux"
        );
    }
}
