//! Push a package directory (from `cargo teeny package`) to a remote host via
//! `rsync` (`cargo teeny deploy`).
//!
//! Uses `rsync -a -e <ssh> <package>/ <host>:<dest>/` so interactive password
//! prompts (when no key-based auth is configured) pass straight through —
//! stdio is inherited from this process, never piped or captured.
//!
//! By default `--ignore-existing` is passed so re-running a deploy only adds
//! files that aren't on the remote yet; `--overwrite` drops that flag so
//! everything syncs normally (existing files get updated/replaced).

use std::path::Path;
use std::process::{self, Command};

use anyhow::{Context, Result};

use crate::cli::DeployArgs;

/// Marker written by `cargo teeny package`; presence sanity-checks `--package`.
const PACKAGE_MARKER: &str = "conf/.cargo-teeny-package";

pub fn run(args: DeployArgs) -> Result<()> {
    validate_package(&args.package)?;
    validate_host(&args.host)?;
    validate_dest(&args.dest)?;

    // Trailing slash on the source means "sync contents, not the directory
    // itself" (same rsync convention already used by `sysroot --rsync-from`).
    let src = format!("{}/", args.package.display());
    let dest_spec = format!("{}:{}/", args.host, args.dest.trim_end_matches('/'));

    let mut cmd = Command::new("rsync");
    cmd.args(["-a", "-e", &args.ssh]);
    if !args.overwrite {
        cmd.arg("--ignore-existing");
    }
    cmd.arg(&src);
    cmd.arg(&dest_spec);

    eprintln!(
        "cargo-teeny: deploying {} -> {} (overwrite={})",
        args.package.display(),
        dest_spec,
        args.overwrite
    );

    let status = cmd.status().context("spawn `rsync`")?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn validate_package(package: &Path) -> Result<()> {
    anyhow::ensure!(
        package.is_dir(),
        "--package {} is not a directory",
        package.display()
    );
    let marker = package.join(PACKAGE_MARKER);
    anyhow::ensure!(
        marker.is_file(),
        "--package {} does not look like a cargo-teeny package (missing {PACKAGE_MARKER} — \
         run `cargo teeny package` first)",
        package.display()
    );
    Ok(())
}

fn validate_host(host: &str) -> Result<()> {
    anyhow::ensure!(!host.is_empty(), "--host must not be empty");
    anyhow::ensure!(
        !host.chars().any(char::is_whitespace),
        "--host must not contain whitespace (got {host:?})"
    );
    anyhow::ensure!(
        !host.contains(':'),
        "--host must not contain ':' (got {host:?}) — for a custom port use \
         --ssh \"ssh -p <port>\" instead"
    );
    Ok(())
}

fn validate_dest(dest: &str) -> Result<()> {
    anyhow::ensure!(!dest.is_empty(), "--dest must not be empty");
    anyhow::ensure!(!dest.contains('\0'), "--dest must not contain NUL bytes");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cargo-teeny-deploy-test-{name}-{suffix}"))
    }

    #[test]
    fn validate_package_requires_marker() {
        let dir = tmp_dir("nopkg");
        fs::create_dir_all(&dir).unwrap();
        assert!(validate_package(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_package_accepts_marker() {
        let dir = tmp_dir("pkg");
        fs::create_dir_all(dir.join("conf")).unwrap();
        fs::write(dir.join(PACKAGE_MARKER), "1\n").unwrap();
        assert!(validate_package(&dir).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_host_accepts_bare_and_user_at_host() {
        assert!(validate_host("xyzhost").is_ok());
        assert!(validate_host("arshadm@xyzhost").is_ok());
    }

    #[test]
    fn validate_host_rejects_colon_whitespace_and_empty() {
        assert!(validate_host("host:2222").is_err());
        assert!(validate_host("host name").is_err());
        assert!(validate_host("").is_err());
    }

    #[test]
    fn validate_dest_accepts_absolute_and_home_relative() {
        assert!(validate_dest("/opt/app").is_ok());
        assert!(validate_dest("~/app").is_ok());
    }

    #[test]
    fn validate_dest_rejects_empty() {
        assert!(validate_dest("").is_err());
    }
}
