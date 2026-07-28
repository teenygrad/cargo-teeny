//! Install the custom teenyc rustup toolchain from the spinorml CDN.
//!
//! `rustup toolchain install <name>` can never work for a custom channel name — rustup's own
//! grammar only accepts `stable`/`beta`/`nightly` or a bare version number, checked client-side
//! before any network call. So instead this downloads the package tarball named in the channel
//! manifest directly, verifies its sha256, extracts it, and registers it with
//! `rustup toolchain link` — the mechanism rustup itself documents for arbitrarily-named local
//! toolchains.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::cli::InstallToolchainArgs;

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    anyhow::ensure!(!value.is_empty(), "{label} must not be empty");
    anyhow::ensure!(
        value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "{label} must be alphanumeric/-/_ only (got {value:?})"
    );
    Ok(())
}

fn validate_dist_server(dist_server: &str) -> Result<()> {
    anyhow::ensure!(
        dist_server.starts_with("https://") || dist_server.starts_with("http://"),
        "--dist-server must be an http(s) URL (got {dist_server:?})"
    );
    anyhow::ensure!(
        !dist_server.ends_with('/'),
        "--dist-server must not have a trailing slash (got {dist_server:?})"
    );
    Ok(())
}

/// `rustup toolchain link` itself only rejects `none` and path separators — verified empirically,
/// it happily accepts a name that looks exactly like an official channel (e.g. `stable-teenyc`,
/// or even `nightly-x86_64-unknown-linux-gnu`). The one real footgun is linking over the name of
/// a genuine official toolchain you have installed for *this* host via `rustup toolchain install`
/// (`stable`/`beta`/`nightly`/`stable-<host>`/etc.), which would silently redirect it. `host` is
/// this machine's real target triple (see `detect_host_triple`), so a name like `stable-teenyc`
/// — whose suffix isn't a real triple — is never confused with `stable-<host>` and is allowed.
fn validate_toolchain_name(name: &str, host: &str) -> Result<()> {
    anyhow::ensure!(!name.is_empty(), "toolchain name must not be empty");
    anyhow::ensure!(name != "none", "toolchain name must not be 'none'");
    anyhow::ensure!(
        !name.contains('/') && !name.contains('\\'),
        "toolchain name must not contain path separators (got {name:?})"
    );
    for channel in ["stable", "beta", "nightly"] {
        anyhow::ensure!(
            name != channel && name != format!("{channel}-{host}"),
            "toolchain name must not look like your real, official '{channel}' rustup toolchain \
             for this host (got {name:?}); linking over it would shadow it"
        );
    }
    Ok(())
}

struct PackageTarget {
    url: String,
    sha256: String,
}

/// Extract the (preferring `.tar.xz`) download URL and sha256 for `package`'s `host` target
/// out of a channel manifest TOML.
fn parse_target(manifest_toml: &str, package: &str, host: &str) -> Result<PackageTarget> {
    let doc: toml::Value = manifest_toml
        .parse()
        .context("parse channel manifest TOML")?;
    let target = doc
        .get("pkg")
        .and_then(|p| p.get(package))
        .and_then(|p| p.get("target"))
        .and_then(|t| t.get(host))
        .with_context(|| format!("manifest has no pkg.{package}.target.{host} entry"))?;

    let available = target
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    anyhow::ensure!(
        available,
        "package '{package}' is not available for target '{host}' in this channel manifest"
    );

    if let (Some(url), Some(hash)) = (
        target.get("xz_url").and_then(|v| v.as_str()),
        target.get("xz_hash").and_then(|v| v.as_str()),
    ) {
        return Ok(PackageTarget {
            url: url.to_string(),
            sha256: hash.to_string(),
        });
    }

    let url = target
        .get("url")
        .and_then(|v| v.as_str())
        .with_context(|| format!("pkg.{package}.target.{host} has neither xz_url nor url"))?;
    let hash = target
        .get("hash")
        .and_then(|v| v.as_str())
        .with_context(|| format!("pkg.{package}.target.{host} has neither xz_hash nor hash"))?;
    Ok(PackageTarget {
        url: url.to_string(),
        sha256: hash.to_string(),
    })
}

/// Resolves `target.url`'s file inside a local staging directory laid out like
/// `publish-teenyc-runtime.sh` produces: `<local_dir>/dist/<date>/<basename of url>`. The date
/// comes from the manifest's own top-level `date` field, so this doesn't depend on `target.url`
/// being a real, fetchable URL at all (it never is one served locally — it's the CDN URL baked
/// in at `build-manifest` time).
fn local_package_path(local_dir: &Path, manifest_toml: &str, url: &str) -> Result<PathBuf> {
    let doc: toml::Value = manifest_toml
        .parse()
        .context("parse channel manifest TOML")?;
    let date = doc
        .get("date")
        .and_then(|v| v.as_str())
        .context("manifest has no top-level 'date' field")?;
    let filename = Path::new(url)
        .file_name()
        .with_context(|| format!("package url {url:?} has no filename"))?;
    Ok(local_dir.join("dist").join(date).join(filename))
}

fn default_dest(name: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home)
        .join(".cargo-teeny")
        .join("toolchains")
        .join(name))
}

fn detect_host_triple() -> Result<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .context("spawn `rustc -vV` to detect the host triple")?;
    anyhow::ensure!(
        output.status.success(),
        "rustc -vV exited with {}",
        output.status
    );
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|host| host.trim().to_string())
        .context("could not find a 'host:' line in `rustc -vV` output")
}

fn fetch_text(url: &str) -> Result<String> {
    let output = Command::new("curl")
        .args(["-fsSL", url])
        .output()
        .context("spawn `curl` (is curl installed and on PATH?)")?;
    anyhow::ensure!(
        output.status.success(),
        "curl {url} exited with {}",
        output.status
    );
    String::from_utf8(output.stdout).context("manifest response was not valid UTF-8")
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .context("spawn `curl` (is curl installed and on PATH?)")?;
    anyhow::ensure!(status.success(), "curl {url} exited with {status}");
    Ok(())
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<()> {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .context("spawn `sha256sum`")?;
    anyhow::ensure!(
        output.status.success(),
        "sha256sum exited with {}",
        output.status
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let actual = text.split_whitespace().next().unwrap_or_default();
    anyhow::ensure!(
        actual.eq_ignore_ascii_case(expected_hex),
        "sha256 mismatch for {}: expected {expected_hex}, got {actual}",
        path.display()
    );
    Ok(())
}

fn extract_tar(tarball: &Path, dest_dir: &Path) -> Result<()> {
    fs::create_dir_all(dest_dir).with_context(|| format!("create {}", dest_dir.display()))?;
    let status = Command::new("tar")
        .arg("-xf")
        .arg(tarball)
        .arg("-C")
        .arg(dest_dir)
        .status()
        .context("spawn `tar`")?;
    anyhow::ensure!(
        status.success(),
        "tar extraction of {} exited with {status}",
        tarball.display()
    );
    Ok(())
}

/// The extracted tarball layout is `<top>/components` (component dir names, one per line — this
/// package always has exactly one) plus `<top>/<component>/{bin,lib,...}`. Returns the path to
/// that `<component>` directory, which is what `rustup toolchain link` should point at.
fn find_component_root(extract_dir: &Path) -> Result<PathBuf> {
    let mut top_dirs: Vec<PathBuf> = fs::read_dir(extract_dir)
        .with_context(|| format!("read {}", extract_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    anyhow::ensure!(
        top_dirs.len() == 1,
        "expected exactly one top-level directory in the extracted tarball, found {}",
        top_dirs.len()
    );
    let top = top_dirs.remove(0);

    let components_file = top.join("components");
    let components = fs::read_to_string(&components_file)
        .with_context(|| format!("read {}", components_file.display()))?;
    let component = components
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .with_context(|| format!("{} is empty", components_file.display()))?;

    let component_root = top.join(component);
    anyhow::ensure!(
        component_root.is_dir(),
        "{} names component '{component}' but {} does not exist",
        components_file.display(),
        component_root.display()
    );
    Ok(component_root)
}

pub fn run(args: InstallToolchainArgs) -> Result<()> {
    validate_identifier("--channel", &args.channel)?;
    validate_identifier("--package", &args.package)?;
    let host = detect_host_triple()?;
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| format!("{}-{host}", args.channel));
    validate_toolchain_name(&name, &host)?;

    let manifest_toml = if let Some(local_dir) = &args.local_dir {
        let manifest_path = local_dir
            .join("dist")
            .join(format!("channel-rust-{}.toml", args.channel));
        eprintln!(
            "cargo-teeny: reading local manifest {}",
            manifest_path.display()
        );
        fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?
    } else {
        validate_dist_server(&args.dist_server)?;
        let manifest_url = format!(
            "{}/dist/channel-rust-{}.toml",
            args.dist_server, args.channel
        );
        eprintln!("cargo-teeny: fetching manifest {manifest_url}");
        fetch_text(&manifest_url)?
    };

    eprintln!(
        "cargo-teeny: resolving package '{}' for host {host}",
        args.package
    );
    let target = parse_target(&manifest_toml, &args.package, &host)?;

    let dest = match &args.dest {
        Some(d) => d.clone(),
        None => default_dest(&name)?,
    };
    let dest_parent = dest
        .parent()
        .with_context(|| format!("{} has no parent directory", dest.display()))?;
    fs::create_dir_all(dest_parent).with_context(|| format!("create {}", dest_parent.display()))?;

    // Stage in a sibling of `dest` (not the system tmpdir) so the final `fs::rename` below stays
    // on one filesystem — cross-filesystem renames fail, and dest's parent may not be on the
    // same filesystem as e.g. /tmp.
    let staging = dest_parent.join(format!(".cargo-teeny-tmp-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).with_context(|| format!("create {}", staging.display()))?;
    let cleanup = |staging: &Path| {
        let _ = fs::remove_dir_all(staging);
    };

    let tarball_path = staging.join("package.tar");
    if let Some(local_dir) = &args.local_dir {
        let result = local_package_path(local_dir, &manifest_toml, &target.url).and_then(|src| {
            eprintln!("cargo-teeny: copying {}", src.display());
            fs::copy(&src, &tarball_path)
                .with_context(|| format!("copy {} to {}", src.display(), tarball_path.display()))
                .map(|_| ())
        });
        if let Err(e) = result {
            cleanup(&staging);
            return Err(e);
        }
    } else {
        eprintln!("cargo-teeny: downloading {}", target.url);
        if let Err(e) = download_file(&target.url, &tarball_path) {
            cleanup(&staging);
            return Err(e);
        }
    }

    eprintln!("cargo-teeny: verifying sha256");
    if let Err(e) = verify_sha256(&tarball_path, &target.sha256) {
        cleanup(&staging);
        return Err(e);
    }

    let extract_dir = staging.join("extracted");
    eprintln!("cargo-teeny: extracting");
    if let Err(e) = extract_tar(&tarball_path, &extract_dir) {
        cleanup(&staging);
        return Err(e);
    }

    let component_root = match find_component_root(&extract_dir) {
        Ok(root) => root,
        Err(e) => {
            cleanup(&staging);
            return Err(e);
        }
    };

    if dest.exists() {
        eprintln!("cargo-teeny: removing existing {}", dest.display());
        fs::remove_dir_all(&dest).with_context(|| format!("remove {}", dest.display()))?;
    }
    fs::rename(&component_root, &dest)
        .with_context(|| format!("move {} to {}", component_root.display(), dest.display()))?;
    cleanup(&staging);

    eprintln!(
        "cargo-teeny: linking rustup toolchain '{name}' -> {}",
        dest.display()
    );
    let status = Command::new("rustup")
        .args(["toolchain", "link", &name])
        .arg(&dest)
        .status()
        .context("spawn `rustup toolchain link`")?;
    anyhow::ensure!(
        status.success(),
        "rustup toolchain link exited with {status}"
    );

    if args.default {
        eprintln!("cargo-teeny: setting '{name}' as your rustup default");
        let status = Command::new("rustup")
            .args(["default", &name])
            .status()
            .context("spawn `rustup default`")?;
        anyhow::ensure!(status.success(), "rustup default exited with {status}");
    } else {
        eprintln!(
            "cargo-teeny: '{name}' linked but not set as default. Use `rustup override set \
             {name}` in a project directory, `cargo +{name} …` for one-off invocations, or \
             re-run with --default to make it global."
        );
    }

    if let Ok(output) = Command::new("rustup")
        .args(["which", "--toolchain", &name, "teenyc"])
        .output()
        && output.status.success()
    {
        let path = String::from_utf8_lossy(&output.stdout);
        eprintln!("cargo-teeny: teenyc binary at {}", path.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MANIFEST: &str = r#"
manifest-version = "2"
date = "2026-07-25"

[pkg.teenyc]
version = "1.94.0-dev"

[pkg.teenyc.target.x86_64-unknown-linux-gnu]
available = true
url = "https://cdn.spinorml.com/teenyc/dist/2026-07-25/teenyc-1.94.0-dev-x86_64-unknown-linux-gnu.tar.gz"
hash = "aaaa"
xz_url = "https://cdn.spinorml.com/teenyc/dist/2026-07-25/teenyc-1.94.0-dev-x86_64-unknown-linux-gnu.tar.xz"
xz_hash = "bbbb"

[pkg.teenyc.target.aarch64-apple-darwin]
available = false
"#;

    #[test]
    fn parse_target_prefers_xz() {
        let t = parse_target(SAMPLE_MANIFEST, "teenyc", "x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(t.sha256, "bbbb");
        assert!(t.url.ends_with(".tar.xz"));
    }

    #[test]
    fn parse_target_rejects_unavailable() {
        assert!(parse_target(SAMPLE_MANIFEST, "teenyc", "aarch64-apple-darwin").is_err());
    }

    #[test]
    fn parse_target_rejects_missing_entry() {
        assert!(parse_target(SAMPLE_MANIFEST, "cargo", "x86_64-unknown-linux-gnu").is_err());
    }

    #[test]
    fn validate_identifier_rejects_empty() {
        assert!(validate_identifier("--channel", "").is_err());
    }

    #[test]
    fn validate_identifier_rejects_path_separators() {
        assert!(validate_identifier("--channel", "../evil").is_err());
        assert!(validate_identifier("--channel", "foo/bar").is_err());
    }

    #[test]
    fn validate_identifier_accepts_teenyc() {
        assert!(validate_identifier("--channel", "teenyc").is_ok());
    }

    #[test]
    fn validate_dist_server_requires_scheme() {
        assert!(validate_dist_server("cdn.spinorml.com/teenyc").is_err());
        assert!(validate_dist_server("https://cdn.spinorml.com/teenyc").is_ok());
    }

    #[test]
    fn validate_dist_server_rejects_trailing_slash() {
        assert!(validate_dist_server("https://cdn.spinorml.com/teenyc/").is_err());
    }

    const HOST: &str = "x86_64-unknown-linux-gnu";

    #[test]
    fn validate_toolchain_name_accepts_teenyc() {
        assert!(validate_toolchain_name("teenyc", HOST).is_ok());
    }

    #[test]
    fn validate_toolchain_name_accepts_stable_teenyc() {
        // Not a real collision: "teenyc" isn't a target triple, so this can't shadow a genuine
        // official `stable-<host>` toolchain.
        assert!(validate_toolchain_name("stable-teenyc", HOST).is_ok());
    }

    #[test]
    fn validate_toolchain_name_rejects_bare_standard_channels() {
        assert!(validate_toolchain_name("stable", HOST).is_err());
        assert!(validate_toolchain_name("beta", HOST).is_err());
        assert!(validate_toolchain_name("nightly", HOST).is_err());
    }

    #[test]
    fn validate_toolchain_name_rejects_real_official_names_for_this_host() {
        assert!(validate_toolchain_name(&format!("stable-{HOST}"), HOST).is_err());
        assert!(validate_toolchain_name(&format!("nightly-{HOST}"), HOST).is_err());
    }

    #[test]
    fn validate_toolchain_name_allows_official_name_for_a_different_host() {
        // Only collides with *this* host's real toolchain slot.
        assert!(validate_toolchain_name("stable-aarch64-apple-darwin", HOST).is_ok());
    }

    #[test]
    fn validate_toolchain_name_rejects_none_and_slashes() {
        assert!(validate_toolchain_name("none", HOST).is_err());
        assert!(validate_toolchain_name("foo/bar", HOST).is_err());
    }

    #[test]
    fn find_component_root_reads_components_file() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cargo-teeny-component-root-test-{suffix}"));
        let _ = fs::remove_dir_all(&dir);
        let top = dir.join("teenyc-1.94.0-dev-x86_64-unknown-linux-gnu");
        let payload = top.join("rustc");
        fs::create_dir_all(&payload).unwrap();
        fs::write(top.join("components"), "rustc\n").unwrap();

        let root = find_component_root(&dir).unwrap();
        assert_eq!(root, payload);

        let _ = fs::remove_dir_all(&dir);
    }
}
