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

use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::cli::{AotArgs, BuildArgs, PackageArgs};
use crate::commands::aot;
use crate::commands::build::{self, CrossVerb, built_artifact_path};
use crate::profiles::{board_profile, board_type_cli_name};
use crate::workspace;

const MARKER: &str = ".cargo-teeny-package";
const MARKER_VERSION: u32 = 1;

pub fn run(args: PackageArgs) -> Result<()> {
    // Either one or more `--bin <name>` (repeatable), or a single `--example <name>` —
    // clap's `conflicts_with` guarantees these aren't mixed.
    let names: Vec<String> = if !args.bin.is_empty() {
        args.bin.clone()
    } else if let Some(ref name) = args.example {
        vec![name.clone()]
    } else {
        bail!("`cargo teeny package` requires --bin <name> (repeatable) or --example <name>");
    };
    let is_bin = !args.bin.is_empty();
    let release = !args.no_release;
    let profile = board_profile(args.target);
    let (device, options) = aot_device_options(
        profile.device,
        profile.options,
        args.device.as_deref(),
        args.options.as_deref(),
    );

    // Canonicalize `--dest` before deriving `cache_dir`: the host AOT-compile step (run via
    // plain `cargo run`, so it inherits our cwd) hands `--cache-dir` down into the `teenyc`
    // compiler pipeline, which internally changes its working directory while compiling
    // kernels — a *relative* cache dir silently breaks once that happens (rustc then fails
    // to find `.rs` sources it just wrote, under the now-stale relative path), while an
    // absolute one keeps resolving correctly throughout.
    fs::create_dir_all(&args.dest).with_context(|| format!("create {}", args.dest.display()))?;
    let dest = args
        .dest
        .canonicalize()
        .with_context(|| format!("canonicalize {}", args.dest.display()))?;

    let bin_dir = dest.join("bin");
    let cache_dir = dest.join("cache");
    let conf_dir = dest.join("conf");
    let data_dir = dest.join("data");
    for dir in [&bin_dir, &cache_dir, &conf_dir, &data_dir] {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }

    // `cross build` writes into `target/` under the *workspace* root, not necessarily next
    // to this crate's own manifest (e.g. a demo that's a workspace member) — resolve the same
    // root `build::run` used. Same root for every name in `names`, so resolve once.
    let cwd = env::current_dir().context("get current directory")?;
    let (_manifest, cross_root, _manifest_rel) = workspace::resolve(&cwd, args.package.as_deref())?;

    for name in &names {
        // ── 1. Cross-compile the deployment binary ──────────────────────────
        build::run(
            BuildArgs {
                target: args.target,
                package: args.package.clone(),
                cuda_path: args.cuda_path.clone(),
                no_release: args.no_release,
                features: args.features.clone(),
                examples: false,
                example: if is_bin { None } else { Some(name.clone()) },
                bin: if is_bin { Some(name.clone()) } else { None },
                extra: Vec::new(),
            },
            CrossVerb::Build,
        )?;

        let built_path =
            built_artifact_path(&cross_root, profile.cross_triple, release, is_bin, name);
        let dest_bin = bin_dir.join(name);
        fs::copy(&built_path, &dest_bin)
            .with_context(|| format!("copy {} -> {}", built_path.display(), dest_bin.display()))?;
        make_executable(&dest_bin)?;

        // ── 2. AOT-compile kernels on the host into <dest>/cache ────────────
        aot::run(AotArgs {
            package: args.package.clone(),
            bin: if is_bin { Some(name.clone()) } else { None },
            example: if is_bin { None } else { Some(name.clone()) },
            no_release: args.no_release,
            features: args.features.clone(),
            device: device.clone(),
            options: options.clone(),
            cache_dir: Some(cache_dir.clone()),
            force: args.force,
        })?;
    }

    // ── 3. Provenance marker ─────────────────────────────────────────────────
    write_marker(
        &conf_dir,
        &args,
        is_bin,
        &names,
        &device,
        options.as_deref(),
    )?;

    eprintln!("cargo-teeny: package assembled at {}", args.dest.display());
    Ok(())
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

/// Resolves the AOT `--device`/`--options`: explicit values win; otherwise the board profile's
/// defaults. The profile's options only apply to its own device — an explicitly different
/// `--device` gets no default options, since they wouldn't parse for another backend.
fn aot_device_options(
    profile_device: &str,
    profile_options: &str,
    device: Option<&str>,
    options: Option<&str>,
) -> (String, Option<String>) {
    let device = device.unwrap_or(profile_device);
    let options = match options {
        Some(options) => Some(options),
        None if device == profile_device => Some(profile_options),
        None => None,
    };
    (device.to_owned(), options.map(str::to_owned))
}

fn write_marker(
    conf_dir: &Path,
    args: &PackageArgs,
    is_bin: bool,
    names: &[String],
    device: &str,
    options: Option<&str>,
) -> Result<()> {
    let kind = if is_bin { "BIN" } else { "EXAMPLE" };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let commit = git_commit().unwrap_or_else(|| "unknown".to_string());
    let names_joined = names.join(",");

    let body = marker_body(
        &board_type_cli_name(args.target),
        kind,
        &names_joined,
        device,
        options.unwrap_or(""),
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

    #[test]
    fn aot_device_options_default_from_profile() {
        let (device, options) = aot_device_options("riscv", "capability=spacemit-k3", None, None);
        assert_eq!(device, "riscv");
        assert_eq!(options.as_deref(), Some("capability=spacemit-k3"));
    }

    #[test]
    fn aot_device_options_explicit_options_win() {
        let (device, options) =
            aot_device_options("cuda", "capability=sm_87", None, Some("capability=sm_90"));
        assert_eq!(device, "cuda");
        assert_eq!(options.as_deref(), Some("capability=sm_90"));
    }

    #[test]
    fn aot_device_options_other_device_gets_no_default_options() {
        let (device, options) = aot_device_options("cuda", "capability=sm_87", Some("riscv"), None);
        assert_eq!(device, "riscv");
        assert_eq!(options, None);
    }
}
