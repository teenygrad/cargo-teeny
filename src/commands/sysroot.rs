//! Create a minimal FHS-style tree suitable for `--sysroot` with GCC/Clang-style cross links.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::ValueEnum;

use crate::cli::{BoardType, SysrootArgs};

const MARKER: &str = ".cargo-teeny-sysroot";
const MARKER_VERSION: u32 = 5;

/// Remote directory to mirror into the sysroot at the same relative path (leading `/` stripped).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SysrootRsyncFolder {
    /// Absolute directory path on the remote host.
    pub remote_path: &'static str,
}

/// Directories to `rsync` from a donor machine into the sysroot for the given profile.
///
/// Each `remote_path` is mirrored under `sysroot_root` at `sysroot_root/<path-without-leading-slash>`.
pub fn sysroot_rsync_folders(kind: BoardType) -> &'static [SysrootRsyncFolder] {
    match kind {
        BoardType::JetsonOrinNano => &[
            SysrootRsyncFolder {
                remote_path: "/usr/local/cuda/include",
            },
            SysrootRsyncFolder {
                remote_path: "/usr/local/cuda/lib",
            },
            SysrootRsyncFolder {
                remote_path: "/usr/lib/aarch64-linux-gnu",
            },
        ],
    }
}

fn sysroot_type_cli_name(t: BoardType) -> String {
    t.to_possible_value()
        .expect("BoardType maps to a clap PossibleValue")
        .get_name()
        .to_owned()
}

/// Standard directories created under the sysroot root (before host-specific paths).
const SYSROOT_DIRS: &[&str] = &["usr/include", "usr/lib", "lib", "bin", "etc"];

fn validate_host(host: &str) -> Result<()> {
    anyhow::ensure!(!host.is_empty(), "--host must not be empty");
    anyhow::ensure!(
        !host.contains('/') && !host.contains('\\'),
        "--host must not contain path separators (got {host:?})"
    );
    Ok(())
}

fn validate_remote_abs_path(label: &str, path: &str) -> Result<()> {
    anyhow::ensure!(
        path.starts_with('/'),
        "{label} must be an absolute Unix path (got {path:?})"
    );
    anyhow::ensure!(!path.contains('\0'), "{label} must not contain NUL bytes");
    Ok(())
}

fn validate_rsync_peer(peer: &str) -> Result<()> {
    anyhow::ensure!(!peer.is_empty(), "--rsync-from must not be empty");
    anyhow::ensure!(
        peer.contains('@'),
        "--rsync-from must look like user@host (got {peer:?})"
    );
    anyhow::ensure!(
        !peer.contains(':'),
        "--rsync-from must be user@host only without :path (got {peer:?})"
    );
    Ok(())
}

fn local_mirror_under_sysroot(sysroot: &Path, remote_abs: &str) -> PathBuf {
    sysroot.join(remote_abs.trim_start_matches('/'))
}

/// `remote_abs` is an absolute directory on the remote (no trailing slash). A trailing slash is
/// added for rsync “copy contents” semantics. `local_dest` is the destination directory root.
fn rsync_remote_dir(
    rsync_ssh: &str,
    peer: &str,
    remote_abs: &str,
    local_dest: &Path,
) -> Result<()> {
    validate_remote_abs_path("remote rsync path", remote_abs)?;
    fs::create_dir_all(local_dest).with_context(|| format!("create {}", local_dest.display()))?;

    let src = format!("{peer}:{remote_abs}/");
    let dest = format!("{}/", local_dest.display());

    let status = Command::new("rsync")
        .args(["-a", "-e", rsync_ssh, &src, &dest])
        .status()
        .with_context(|| format!("spawn rsync from {src} to {dest}"))?;

    anyhow::ensure!(
        status.success(),
        "rsync from {src} to {dest} exited with {status}"
    );
    Ok(())
}

fn run_rsyncs(args: &SysrootArgs, root: &Path) -> Result<()> {
    let peer = args
        .rsync_from
        .as_ref()
        .expect("run_rsyncs only when rsync-from is set");
    validate_rsync_peer(peer)?;

    let folders = sysroot_rsync_folders(args.sysroot_type);
    anyhow::ensure!(
        !folders.is_empty(),
        "no rsync folders are defined for sysroot type {:?}",
        args.sysroot_type
    );

    for folder in folders {
        validate_remote_abs_path("rsync folder remote_path", folder.remote_path)?;
        let local = local_mirror_under_sysroot(root, folder.remote_path);
        eprintln!(
            "rsync: {peer}:{}/ -> {}",
            folder.remote_path.trim_end_matches('/'),
            local.display()
        );
        rsync_remote_dir(&args.rsync_ssh, peer, folder.remote_path, &local)?;
    }

    Ok(())
}

pub fn run(args: SysrootArgs) -> Result<()> {
    validate_host(&args.host)?;

    let root = &args.path;
    fs::create_dir_all(root).with_context(|| format!("create sysroot root {}", root.display()))?;

    for rel in SYSROOT_DIRS {
        let dir = root.join(rel);
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    }

    let host_lib = format!("usr/lib/{}", args.host);
    let host_lib_dir = root.join(&host_lib);
    fs::create_dir_all(&host_lib_dir)
        .with_context(|| format!("create {}", host_lib_dir.display()))?;

    let type_str = sysroot_type_cli_name(args.sysroot_type);
    let mut marker_body = format!("{MARKER_VERSION}\nHOST={}\nTYPE={type_str}\n", args.host);

    if let Some(peer) = args.rsync_from.as_ref() {
        marker_body.push_str(&format!("RSYNC_FROM={peer}\n"));
        let dirs = sysroot_rsync_folders(args.sysroot_type)
            .iter()
            .map(|f| f.remote_path)
            .collect::<Vec<_>>()
            .join(",");
        marker_body.push_str(&format!("RSYNC_DIRS={dirs}\n"));
    }

    eprintln!(
        "sysroot scaffold at {} (host {}, type {type_str})\n\
         base directories: {}\n\
         host lib directory: {}",
        root.display(),
        args.host,
        SYSROOT_DIRS.join(", "),
        host_lib
    );

    if let Some(peer) = args.rsync_from.as_ref() {
        let n = sysroot_rsync_folders(args.sysroot_type).len();
        eprintln!(
            "rsync from {peer} ({n} director{} for type {type_str})",
            if n == 1 { "y" } else { "ies" }
        );
        run_rsyncs(&args, root)?;
    }

    let marker_path = root.join(MARKER);
    let mut marker = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&marker_path)
        .with_context(|| format!("write {}", marker_path.display()))?;
    marker
        .write_all(marker_body.as_bytes())
        .with_context(|| format!("write contents of {}", marker_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::cli::{BoardType, SysrootArgs};

    #[test]
    fn jetson_orin_nano_rsync_folders() {
        let folders = sysroot_rsync_folders(BoardType::JetsonOrinNano);
        let paths: Vec<_> = folders.iter().map(|f| f.remote_path).collect();
        assert_eq!(
            paths,
            vec![
                "/usr/local/cuda/include",
                "/usr/local/cuda/lib",
                "/usr/lib/aarch64-linux-gnu",
            ]
        );
    }

    #[test]
    fn creates_expected_tree() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp = std::env::temp_dir().join(format!("cargo-teeny-sysroot-test-{suffix}"));
        let _ = fs::remove_dir_all(&tmp);
        let host = "aarch64-unknown-linux-gnu";
        run(SysrootArgs {
            host: host.into(),
            path: tmp.clone(),
            sysroot_type: BoardType::JetsonOrinNano,
            rsync_from: None,
            rsync_ssh: "ssh".into(),
        })
        .unwrap();
        for rel in SYSROOT_DIRS {
            assert!(tmp.join(rel).is_dir(), "{rel} missing");
        }
        assert!(tmp.join("usr/lib").join(host).is_dir());
        assert!(tmp.join(MARKER).is_file());
        let marker = fs::read_to_string(tmp.join(MARKER)).unwrap();
        assert!(marker.starts_with(&format!("{MARKER_VERSION}\n")));
        assert!(marker.contains("HOST=aarch64-unknown-linux-gnu"));
        assert!(marker.contains("TYPE=jetson-orin-nano"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
