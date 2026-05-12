//! Create a minimal FHS-style tree suitable for `--sysroot` with GCC/Clang-style cross links.

use std::fs;
use std::io::Write;

use anyhow::{Context, Result};

use crate::cli::SysrootArgs;

const MARKER: &str = ".cargo-teeny-sysroot";
const MARKER_VERSION: u32 = 2;

/// Standard directories created under the sysroot root (before host-specific paths).
const SYSROOT_DIRS: &[&str] = &[
    "usr/include",
    "usr/lib",
    "lib",
    "bin",
    "etc",
];

fn validate_host(host: &str) -> Result<()> {
    anyhow::ensure!(
        !host.is_empty(),
        "--host must not be empty"
    );
    anyhow::ensure!(
        !host.contains('/') && !host.contains('\\'),
        "--host must not contain path separators (got {host:?})"
    );
    Ok(())
}

pub fn run(args: SysrootArgs) -> Result<()> {
    validate_host(&args.host)?;

    let root = &args.path;
    fs::create_dir_all(root).with_context(|| format!("create sysroot root {}", root.display()))?;

    for rel in SYSROOT_DIRS {
        let dir = root.join(rel);
        fs::create_dir_all(&dir)
            .with_context(|| format!("create {}", dir.display()))?;
    }

    let host_lib = format!("usr/lib/{}", args.host);
    let host_lib_dir = root.join(&host_lib);
    fs::create_dir_all(&host_lib_dir)
        .with_context(|| format!("create {}", host_lib_dir.display()))?;

    let marker_path = root.join(MARKER);
    let marker_body = format!("{MARKER_VERSION}\nHOST={}\n", args.host);
    let mut marker = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&marker_path)
        .with_context(|| format!("write {}", marker_path.display()))?;
    marker
        .write_all(marker_body.as_bytes())
        .with_context(|| format!("write contents of {}", marker_path.display()))?;

    eprintln!(
        "sysroot scaffold at {} (host {})\n\
         base directories: {}\n\
         host lib directory: {}",
        root.display(),
        args.host,
        SYSROOT_DIRS.join(", "),
        host_lib
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::cli::SysrootArgs;

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
        })
        .unwrap();
        for rel in SYSROOT_DIRS {
            assert!(tmp.join(rel).is_dir(), "{rel} missing");
        }
        assert!(tmp.join("usr/lib").join(host).is_dir());
        assert!(tmp.join(MARKER).is_file());
        let marker = fs::read_to_string(tmp.join(MARKER)).unwrap();
        assert!(marker.contains("HOST=aarch64-unknown-linux-gnu"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
