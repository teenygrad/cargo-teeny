//! Cross-compilation via `cross build / check / clippy`.

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result, bail};

use crate::cli::BuildArgs;
use crate::profiles::{board_profile, board_type_cli_name};
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
    let cuda = match (&profile.cuda, args.cuda_path.clone()) {
        (Some(cuda), path) => Some((
            path.unwrap_or_else(|| PathBuf::from(cuda.host_path)),
            cuda.container_path,
        )),
        (None, None) => None,
        (None, Some(_)) => bail!(
            "--cuda-path is not supported for --target {} (the board has no CUDA)",
            board_type_cli_name(args.target)
        ),
    };

    let container_opts = build_container_opts(
        teenygrad.as_deref(),
        cuda.as_ref()
            .map(|(host, container)| (host.as_path(), *container)),
    );

    // Prepend any existing CROSS_CONTAINER_OPTS so callers can inject extra mounts.
    let merged_opts = [
        env::var("CROSS_CONTAINER_OPTS").unwrap_or_default(),
        container_opts,
    ]
    .into_iter()
    .filter(|s| !s.trim().is_empty())
    .collect::<Vec<_>>()
    .join(" ");

    let target_dir = cross_target_dir(&cross_root);
    let mut cmd = process::Command::new("cross");
    cmd.current_dir(&cross_root);
    cmd.args(cross_args(
        verb,
        &manifest_arg,
        profile.cross_triple,
        &target_dir,
        &args,
    ));
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
    eprintln!("cargo-teeny: target dir      {}", target_dir.display());
    eprintln!("cargo-teeny: CROSS_CONTAINER_OPTS={merged_opts}");

    let status = cmd.status().context("spawn `cross`")?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// The target dir for cross builds of the workspace at `root`: `target/cross`, apart from native
/// builds' `target/`. Build scripts always run on the build machine, which for a cross build is
/// the container, but cargo keeps them under the target dir's own `release/build/` with the same
/// hashes a native build uses. In a shared `target/`, a build script last built natively (linked
/// against the host's newer glibc) is reused in the container and fails there.
pub fn cross_target_dir(root: &Path) -> PathBuf {
    root.join("target").join("cross")
}

/// `cross` arguments for `verb`ing `manifest_arg` for `triple` into `target_dir`.
fn cross_args(
    verb: CrossVerb,
    manifest_arg: &Path,
    triple: &str,
    target_dir: &Path,
    args: &BuildArgs,
) -> Vec<String> {
    let mut out: Vec<String> = vec![
        verb.as_str().into(),
        "--manifest-path".into(),
        manifest_arg.display().to_string(),
        "--target".into(),
        triple.into(),
        "--target-dir".into(),
        target_dir.display().to_string(),
    ];
    if !args.no_release {
        out.push("--release".into());
    }
    if let Some(features) = &args.features {
        out.extend(["--features".into(), features.clone()]);
    }
    if args.no_default_features {
        out.push("--no-default-features".into());
    }
    if args.examples {
        out.push("--examples".into());
    } else if let Some(name) = &args.example {
        out.extend(["--example".into(), name.clone()]);
    } else if let Some(name) = &args.bin {
        out.extend(["--bin".into(), name.clone()]);
    }
    if !args.extra.is_empty() {
        out.push("--".into());
        out.extend(args.extra.iter().cloned());
    }
    out
}

/// Where `cross build` writes the artifact, under `root` (the workspace root, i.e. the
/// directory `run` invokes `cross` from), in [`cross_target_dir`].
pub fn built_artifact_path(
    root: &Path,
    cross_triple: &str,
    release: bool,
    is_bin: bool,
    name: &str,
) -> PathBuf {
    let profile_dir = if release { "release" } else { "debug" };
    let mut p = cross_target_dir(root).join(cross_triple).join(profile_dir);
    if !is_bin {
        p = p.join("examples");
    }
    p.join(name)
}

/// `-v` mounts for the cross container: the teenygrad checkout (if any) and the CUDA target
/// directory as `(host, container)` (if the board has CUDA).
fn build_container_opts(teenygrad: Option<&Path>, cuda: Option<(&Path, &str)>) -> String {
    let mut opts = Vec::new();
    if let Some(teenygrad) = teenygrad {
        opts.push(format!("-v {t}:{t}", t = teenygrad.display()));
    }
    if let Some((host, container)) = cuda {
        opts.push(format!("-v {}:{}", host.display(), container));
    }
    opts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::BoardType;

    #[test]
    fn container_opts_format() {
        let teenygrad = PathBuf::from("/home/user/teenygrad");
        let cuda_host = PathBuf::from("/usr/local/cuda-12.6/targets/aarch64-linux");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(Some(&teenygrad), Some((&cuda_host, cuda_container)));
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
        let opts = build_container_opts(Some(&teenygrad), Some((&cuda_host, cuda_container)));
        assert!(
            opts.contains("-v /opt/cuda-12.8/aarch64:/usr/local/cuda-12.6/targets/aarch64-linux")
        );
    }

    #[test]
    fn container_opts_without_teenygrad() {
        let cuda_host = PathBuf::from("/usr/local/cuda-12.6/targets/aarch64-linux");
        let cuda_container = "/usr/local/cuda-12.6/targets/aarch64-linux";
        let opts = build_container_opts(None, Some((&cuda_host, cuda_container)));
        assert_eq!(
            opts,
            "-v /usr/local/cuda-12.6/targets/aarch64-linux:/usr/local/cuda-12.6/targets/aarch64-linux"
        );
    }

    #[test]
    fn container_opts_without_cuda() {
        let teenygrad = PathBuf::from("/repos/teenygrad");
        let opts = build_container_opts(Some(&teenygrad), None);
        assert_eq!(opts, "-v /repos/teenygrad:/repos/teenygrad");
        assert_eq!(build_container_opts(None, None), "");
    }

    #[test]
    fn built_artifact_path_example_release() {
        let root = Path::new("/repo");
        let p = built_artifact_path(root, "aarch64-unknown-linux-gnu", true, false, "yolo26");
        assert_eq!(
            p,
            PathBuf::from("/repo/target/cross/aarch64-unknown-linux-gnu/release/examples/yolo26")
        );
    }

    #[test]
    fn built_artifact_path_bin_debug() {
        let root = Path::new("/repo");
        let p = built_artifact_path(root, "riscv64gc-unknown-linux-gnu", false, true, "tllm");
        assert_eq!(
            p,
            PathBuf::from("/repo/target/cross/riscv64gc-unknown-linux-gnu/debug/tllm")
        );
    }

    fn build_args(target: BoardType) -> BuildArgs {
        BuildArgs {
            target,
            package: None,
            cuda_path: None,
            no_release: false,
            examples: false,
            example: None,
            bin: Some("parking-garage-server".into()),
            features: Some("riscv".into()),
            no_default_features: true,
            extra: Vec::new(),
        }
    }

    // A cross build's build scripts run inside the container, but cargo keeps them under the
    // target dir's host `release/build/` with the same hashes a native build of the crate uses.
    // Sharing `target/` with native builds therefore runs host-linked build scripts against the
    // container's older glibc ("GLIBC_2.28 not found"), so cross builds get a target dir of
    // their own.
    #[test]
    fn cross_builds_use_a_target_dir_of_their_own() {
        let root = Path::new("/repo");
        assert_eq!(cross_target_dir(root), PathBuf::from("/repo/target/cross"));
        let args = cross_args(
            CrossVerb::Build,
            Path::new("demos/parking_garage/Cargo.toml"),
            "riscv64gc-unknown-linux-gnu",
            &cross_target_dir(root),
            &build_args(BoardType::SpacemitK3),
        );
        let target_dir = args.iter().position(|a| a == "--target-dir");
        assert_eq!(
            target_dir.map(|i| args[i + 1].as_str()),
            Some("/repo/target/cross"),
            "{args:?}"
        );
    }

    #[test]
    fn cross_args_forward_the_build_options() {
        let args = cross_args(
            CrossVerb::Build,
            Path::new("demos/parking_garage/Cargo.toml"),
            "riscv64gc-unknown-linux-gnu",
            Path::new("/repo/target/cross"),
            &build_args(BoardType::SpacemitK3),
        );
        assert_eq!(
            args,
            [
                "build",
                "--manifest-path",
                "demos/parking_garage/Cargo.toml",
                "--target",
                "riscv64gc-unknown-linux-gnu",
                "--target-dir",
                "/repo/target/cross",
                "--release",
                "--features",
                "riscv",
                "--no-default-features",
                "--bin",
                "parking-garage-server",
            ]
        );
    }
}
