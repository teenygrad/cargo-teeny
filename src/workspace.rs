use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

/// Walk up from `start` until `Cargo.toml` is found.
pub fn find_cargo_toml(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
        anyhow::ensure!(
            dir.pop(),
            "no Cargo.toml found at or above {}",
            start.display()
        );
    }
}

/// Parse `[patch.crates-io]` path entries and return the common ancestor directory —
/// the teenygrad workspace root.
///
/// Paths are normalized but not required to exist, so this works even before a full
/// checkout.
pub fn teenygrad_root_from_patches(manifest: &Path) -> Result<PathBuf> {
    let content = std::fs::read_to_string(manifest)
        .with_context(|| format!("read {}", manifest.display()))?;
    let doc: toml::Value = content
        .parse()
        .with_context(|| format!("parse TOML in {}", manifest.display()))?;

    let patches = doc
        .get("patch")
        .and_then(|p| p.get("crates-io"))
        .and_then(|c| c.as_table())
        .with_context(|| format!("no [patch.crates-io] section in {}", manifest.display()))?;

    let manifest_dir = manifest.parent().unwrap_or(Path::new("."));

    let roots: Vec<PathBuf> = patches
        .values()
        .filter_map(|entry| {
            entry
                .get("path")
                .and_then(|p| p.as_str())
                .map(|rel| normalize(manifest_dir.join(rel)))
        })
        .collect();

    anyhow::ensure!(
        !roots.is_empty(),
        "no path-based entries in [patch.crates-io] in {}",
        manifest.display()
    );

    let ancestor = common_ancestor(&roots);
    anyhow::ensure!(
        !ancestor.as_os_str().is_empty(),
        "patch paths have no common ancestor (all paths differ at the root)"
    );
    Ok(ancestor)
}

fn normalize(path: PathBuf) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out.iter().collect()
}

fn common_ancestor(paths: &[PathBuf]) -> PathBuf {
    let mut ancestor: Vec<Component<'_>> = paths[0].components().collect();
    for path in &paths[1..] {
        let comps: Vec<Component<'_>> = path.components().collect();
        let n = ancestor
            .iter()
            .zip(&comps)
            .take_while(|(a, b)| a == b)
            .count();
        ancestor.truncate(n);
    }
    ancestor.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn common_ancestor_two_siblings() {
        let paths = vec![
            PathBuf::from("/home/user/teenygrad/core/teeny-core"),
            PathBuf::from("/home/user/teenygrad/kernels/teeny-kernels"),
        ];
        assert_eq!(
            common_ancestor(&paths),
            PathBuf::from("/home/user/teenygrad")
        );
    }

    #[test]
    fn common_ancestor_identical() {
        let paths = vec![
            PathBuf::from("/home/user/teenygrad/core"),
            PathBuf::from("/home/user/teenygrad/core"),
        ];
        assert_eq!(
            common_ancestor(&paths),
            PathBuf::from("/home/user/teenygrad/core")
        );
    }

    #[test]
    fn teenygrad_root_from_patches_works() {
        let dir = std::env::temp_dir().join("cargo-teeny-ws-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest = dir.join("Cargo.toml");
        fs::write(
            &manifest,
            r#"
[package]
name = "test"
version = "0.1.0"
edition = "2021"

[patch.crates-io]
teeny-core    = { path = "../teenygrad/core/teeny-core" }
teeny-kernels = { path = "../teenygrad/kernels/teeny-kernels" }
"#,
        )
        .unwrap();

        let root = teenygrad_root_from_patches(&manifest).unwrap();
        // manifest_dir is `dir`; "../teenygrad/..." resolves to dir.parent()/teenygrad/...
        let expected = normalize(dir.join("../teenygrad"));
        assert_eq!(root, expected);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn teenygrad_root_no_patches_errors() {
        let dir = std::env::temp_dir().join("cargo-teeny-ws-test-nopatch");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest = dir.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        assert!(teenygrad_root_from_patches(&manifest).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
