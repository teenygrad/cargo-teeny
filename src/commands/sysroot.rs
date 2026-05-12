//! Create a minimal FHS-style tree suitable for `--sysroot` with GCC/Clang-style cross links.

use std::fs;
use std::io::Write;

use anyhow::{Context, Result};

use crate::cli::SysrootArgs;

const MARKER: &str = ".cargo-teeny-sysroot";
const MARKER_VERSION: &str = "1\n";

/// Standard directories created under the sysroot root.
const SYSROOT_DIRS: &[&str] = &[
    "usr/include",
    "usr/lib",
    "lib",
    "bin",
    "etc",
];

pub fn run(args: SysrootArgs) -> Result<()> {
    let root = &args.path;
    fs::create_dir_all(root).with_context(|| format!("create sysroot root {}", root.display()))?;

    for rel in SYSROOT_DIRS {
        let dir = root.join(rel);
        fs::create_dir_all(&dir)
            .with_context(|| format!("create {}", dir.display()))?;
    }

    let marker_path = root.join(MARKER);
    let mut marker = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&marker_path)
        .with_context(|| format!("write {}", marker_path.display()))?;
    marker
        .write_all(MARKER_VERSION.as_bytes())
        .with_context(|| format!("write contents of {}", marker_path.display()))?;

    eprintln!(
        "sysroot scaffold at {}\n\
         directories: {}",
        root.display(),
        SYSROOT_DIRS.join(", ")
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
        run(SysrootArgs { path: tmp.clone() }).unwrap();
        for rel in SYSROOT_DIRS {
            assert!(tmp.join(rel).is_dir(), "{rel} missing");
        }
        assert!(tmp.join(MARKER).is_file());
        let _ = fs::remove_dir_all(&tmp);
    }
}
