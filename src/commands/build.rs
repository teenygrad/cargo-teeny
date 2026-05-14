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
    let manifest = workspace::find_cargo_toml(&cwd)?;
    let teenygrad = workspace::teenygrad_root_from_patches(&manifest)?;

    let profile = board_profile(args.target);
    let cuda_host = args
        .cuda_path
        .unwrap_or_else(|| PathBuf::from(profile.cuda_host_path));

    let container_opts = build_container_opts(&teenygrad, &cuda_host, profile.cuda_container_path);

    // Prepend any existing CROSS_CONTAINER_OPTS so callers can inject extra mounts.
    let merged_opts = match env::var("CROSS_CONTAINER_OPTS")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        Some(existing) => format!("{existing} {container_opts}"),
        None => container_opts,
    };

    let mut cmd = process::Command::new("cross");
    cmd.arg(verb.as_str());
    cmd.args(["--target", profile.cross_triple]);

    if !args.no_release {
        cmd.arg("--release");
    }

    if args.examples {
        cmd.arg("--examples");
    } else if let Some(ref name) = args.example {
        cmd.args(["--example", name]);
    }

    if !args.extra.is_empty() {
        cmd.arg("--");
        cmd.args(&args.extra);
    }

    cmd.env("CROSS_CONTAINER_OPTS", &merged_opts);

    eprintln!("cargo-teeny: teenygrad root  {}", teenygrad.display());
    eprintln!("cargo-teeny: CROSS_CONTAINER_OPTS={merged_opts}");

    let status = cmd.status().context("spawn `cross`")?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn build_container_opts(teenygrad: &Path, cuda_host: &Path, cuda_container: &str) -> String {
    format!(
        "-v {teenygrad}:{teenygrad} -v {cuda_host}:{cuda_container}",
        teenygrad = teenygrad.display(),
        cuda_host = cuda_host.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_opts_format() {
        let teenygrad = PathBuf::from("/home/user/teenygrad");
        let cuda_host = PathBuf::from("/usr/local/cuda-12.6/targets/aarch64-linux");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(&teenygrad, &cuda_host, cuda_container);
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
        let opts = build_container_opts(&teenygrad, &cuda_host, cuda_container);
        assert!(
            opts.contains("-v /opt/cuda-12.8/aarch64:/usr/local/cuda-12.6/targets/aarch64-linux")
        );
    }
}
