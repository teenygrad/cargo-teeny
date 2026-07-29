//! Assemble a self-contained deployment package (`cargo teeny package`).
//!
//! Combines two things `build` and `aot` already do independently:
//!   1. Cross-compile the target binary/example for the board — delegates to
//!      `commands::build::run`.
//!   2. AOT-compile its kernels on the host for the given `--device`/`--options`,
//!      writing straight into `<dest>/cache` — delegates to `commands::aot::run`
//!      with `--cache-dir` forced to `<dest>/cache` so the package is
//!      self-contained (nothing outside `<dest>` is needed at runtime).
//!
//! The cross-compiled binary is then copied into `<dest>/bin`, and a
//! provenance marker is written to `<dest>/conf`. `<dest>/data` is scaffolded
//! empty — populating it with models/datasets is a separate concern.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::cli::{AotArgs, BuildArgs, PackageArgs};
use crate::commands::aot;
use crate::commands::build::{self, CrossVerb};
use crate::profiles::{board_profile, board_type_cli_name};

const MARKER: &str = ".cargo-teeny-package";
const MARKER_VERSION: u32 = 1;

pub fn run(args: PackageArgs) -> Result<()> {
    let name = match (&args.bin, &args.example) {
        (Some(name), None) => name.clone(),
        (None, Some(name)) => name.clone(),
        (None, None) => bail!("`cargo teeny package` requires --bin <name> or --example <name>"),
        (Some(_), Some(_)) => {
            unreachable!("clap enforces --bin/--example are mutually exclusive")
        }
    };
    let is_bin = args.bin.is_some();
    let release = !args.no_release;

    let bin_dir = args.dest.join("bin");
    let cache_dir = args.dest.join("cache");
    let conf_dir = args.dest.join("conf");
    let data_dir = args.dest.join("data");
    for dir in [&bin_dir, &cache_dir, &conf_dir, &data_dir] {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }

    // ── 1. Cross-compile the deployment binary ──────────────────────────────
    build::run(
        BuildArgs {
            target: args.target,
            cuda_path: args.cuda_path.clone(),
            no_release: args.no_release,
            examples: false,
            example: if is_bin { None } else { Some(name.clone()) },
            bin: if is_bin { Some(name.clone()) } else { None },
            extra: Vec::new(),
        },
        CrossVerb::Build,
    )?;

    let profile = board_profile(args.target);
    let built_path = built_artifact_path(profile.cross_triple, release, is_bin, &name);
    let dest_bin = bin_dir.join(&name);
    fs::copy(&built_path, &dest_bin)
        .with_context(|| format!("copy {} -> {}", built_path.display(), dest_bin.display()))?;
    make_executable(&dest_bin)?;

    // ── 2. AOT-compile kernels on the host into <dest>/cache ────────────────
    aot::run(AotArgs {
        bin: args.bin.clone(),
        example: args.example.clone(),
        no_release: args.no_release,
        device: args.device.clone(),
        options: args.options.clone(),
        cache_dir: Some(cache_dir.clone()),
        force: args.force,
    })?;

    // ── 3. Provenance marker ─────────────────────────────────────────────────
    write_marker(&conf_dir, &args, is_bin, &name)?;

    eprintln!("cargo-teeny: package assembled at {}", args.dest.display());
    Ok(())
}

/// Where `cross build` writes the artifact, relative to the crate root
/// (assumes no custom `CARGO_TARGET_DIR`, matching `build`/`aot`).
fn built_artifact_path(cross_triple: &str, release: bool, is_bin: bool, name: &str) -> PathBuf {
    let profile_dir = if release { "release" } else { "debug" };
    let mut p = PathBuf::from("target").join(cross_triple).join(profile_dir);
    if !is_bin {
        p = p.join("examples");
    }
    p.join(name)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms).with_context(|| format!("chmod {}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn write_marker(conf_dir: &Path, args: &PackageArgs, is_bin: bool, name: &str) -> Result<()> {
    let kind = if is_bin { "BIN" } else { "EXAMPLE" };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let commit = git_commit().unwrap_or_else(|| "unknown".to_string());

    let body = marker_body(
        &board_type_cli_name(args.target),
        kind,
        name,
        &args.device,
        args.options.as_deref().unwrap_or(""),
        ts,
        &commit,
    );

    let marker_path = conf_dir.join(MARKER);
    fs::write(&marker_path, body).with_context(|| format!("write {}", marker_path.display()))
}

#[allow(clippy::too_many_arguments)]
fn marker_body(
    target: &str,
    kind: &str,
    name: &str,
    device: &str,
    options: &str,
    build_unix_time: u64,
    source_commit: &str,
) -> String {
    format!(
        "{MARKER_VERSION}\n\
         TARGET={target}\n\
         {kind}={name}\n\
         DEVICE={device}\n\
         OPTIONS={options}\n\
         BUILD_UNIX_TIME={build_unix_time}\n\
         SOURCE_COMMIT={source_commit}\n"
    )
}

fn git_commit() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_artifact_path_example_release() {
        let p = built_artifact_path("aarch64-unknown-linux-gnu", true, false, "yolo26");
        assert_eq!(
            p,
            PathBuf::from("target/aarch64-unknown-linux-gnu/release/examples/yolo26")
        );
    }

    #[test]
    fn built_artifact_path_bin_debug() {
        let p = built_artifact_path("aarch64-unknown-linux-gnu", false, true, "tllm");
        assert_eq!(
            p,
            PathBuf::from("target/aarch64-unknown-linux-gnu/debug/tllm")
        );
    }

    #[test]
    fn marker_body_contains_expected_fields() {
        let body = marker_body(
            "jetson-orin-nano",
            "EXAMPLE",
            "yolo26",
            "cuda",
            "capability=sm_87",
            1700000000,
            "deadbeef",
        );
        assert!(body.starts_with(&format!("{MARKER_VERSION}\n")));
        assert!(body.contains("TARGET=jetson-orin-nano"));
        assert!(body.contains("EXAMPLE=yolo26"));
        assert!(body.contains("DEVICE=cuda"));
        assert!(body.contains("OPTIONS=capability=sm_87"));
        assert!(body.contains("BUILD_UNIX_TIME=1700000000"));
        assert!(body.contains("SOURCE_COMMIT=deadbeef"));
    }
}
