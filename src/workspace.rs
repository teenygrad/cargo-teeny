use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

/// Parse `[patch.crates-io]` path entries and return the common ancestor directory — the
/// teenygrad workspace root — or `Ok(None)` if there's no `[patch.crates-io]` section (or no
/// path-based entries in it) at all. That's the expected state for anyone consuming the
/// published crates.io crates rather than developing against a local teenygrad checkout — the
/// patch section is dev-only and gets stripped before publishing (see the comment above
/// `[patch.crates-io]` in the root `Cargo.toml`), so its absence isn't an error here.
///
/// Paths are normalized but not required to exist, so this works even before a full
/// checkout — see [`teenygrad_mount`] for a wrapper that also checks existence.
pub fn teenygrad_root_from_patches(manifest: &Path) -> Result<Option<PathBuf>> {
    let content = std::fs::read_to_string(manifest)
        .with_context(|| format!("read {}", manifest.display()))?;
    let doc: toml::Value = content
        .parse()
        .with_context(|| format!("parse TOML in {}", manifest.display()))?;

    let Some(patches) = doc
        .get("patch")
        .and_then(|p| p.get("crates-io"))
        .and_then(|c| c.as_table())
    else {
        return Ok(None);
    };

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

    if roots.is_empty() {
        return Ok(None);
    }

    let ancestor = common_ancestor(&roots);
    anyhow::ensure!(
        !ancestor.as_os_str().is_empty(),
        "patch paths have no common ancestor (all paths differ at the root)"
    );
    Ok(Some(ancestor))
}

/// Like [`teenygrad_root_from_patches`], but also treats a resolved root that doesn't exist on
/// disk as "not needed" — a stray `[patch.crates-io]` entry pointing at a sibling teenygrad
/// checkout that isn't actually there. Docker would otherwise fail to bind-mount a missing host
/// path with a confusing error; skipping it here just means the real problem (cargo failing to
/// read the patched crate's manifest) surfaces on its own during dependency resolution instead.
pub fn teenygrad_mount(manifest: &Path) -> Result<Option<PathBuf>> {
    Ok(teenygrad_root_from_patches(manifest)?.filter(|root| root.is_dir()))
}

/// Confirm `dir` *is* the workspace root — i.e. `dir/Cross.toml` exists — rather than
/// searching upward for one.
///
/// `cargo teeny` only supports being invoked from the workspace root, not from within a
/// member's own directory: `cross` always mounts its own cwd (not any ancestor) at
/// `/project` inside the container, so a crate whose `path` dependencies escape its own
/// directory (e.g. a demo crate depending on `../..`, or patch entries several levels up)
/// can't resolve them there — anything above `/project` clamps to the container's root.
/// Invoking `cross` from the workspace root instead (with `--manifest-path` pointing at the
/// actual member — see [`resolve`]) mounts the whole repo tree, so intra-repo relative paths
/// resolve exactly as they do on the host. Use `--package`/`-p` to select a member instead of
/// `cd`-ing into its directory.
pub fn require_workspace_root(dir: &Path) -> Result<()> {
    anyhow::ensure!(
        dir.join("Cross.toml").is_file(),
        "cargo teeny must be run from the workspace root (no Cross.toml in {}). \
         Use --package/-p to select a workspace member instead of cd-ing into its directory.",
        dir.display()
    );
    Ok(())
}

/// Find a workspace member's manifest by its `[package].name`, within the workspace rooted
/// at `root` (already confirmed via [`require_workspace_root`]).
///
/// Checks the root package's own name, then each literal `[workspace.members]` entry — no
/// glob support, since this project only ever lists members as literal paths.
pub fn find_member_manifest(root: &Path, package: &str) -> Result<PathBuf> {
    let root_manifest = root.join("Cargo.toml");

    let root_doc = read_toml(&root_manifest)?;
    if package_name(&root_doc) == Some(package) {
        return Ok(root_manifest);
    }

    let members = root_doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    for member in members.iter().filter_map(|m| m.as_str()) {
        let manifest = root.join(member).join("Cargo.toml");
        let Ok(doc) = read_toml(&manifest) else {
            continue;
        };
        if package_name(&doc) == Some(package) {
            return Ok(manifest);
        }
    }

    anyhow::bail!(
        "no workspace member named `{package}` found under {}",
        root.display()
    )
}

fn read_toml(path: &Path) -> Result<toml::Value> {
    let content = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    content
        .parse()
        .with_context(|| format!("parse TOML in {}", path.display()))
}

fn package_name(doc: &toml::Value) -> Option<&str> {
    doc.get("package")?.get("name")?.as_str()
}

/// Resolve everything needed to invoke `cross`/`cargo`, requiring `cwd` to *be* the workspace
/// root (see [`require_workspace_root`]) rather than a member's own directory: the target
/// package's manifest — the root package itself when `package` is `None`, or a specific
/// member by name (see [`find_member_manifest`]) — plus `cwd` itself (the directory to run
/// `cross` from, so the whole repo tree is mounted) and the manifest path relative to it (for
/// `--manifest-path`).
///
/// `cwd` also doubles as where `cargo` places `target/`.
pub fn resolve(cwd: &Path, package: Option<&str>) -> Result<(PathBuf, PathBuf, PathBuf)> {
    require_workspace_root(cwd)?;
    let manifest = match package {
        Some(name) => find_member_manifest(cwd, name)?,
        None => cwd.join("Cargo.toml"),
    };
    let manifest_rel = manifest
        .strip_prefix(cwd)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| manifest.clone());
    Ok((manifest, cwd.to_path_buf(), manifest_rel))
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

        let root = teenygrad_root_from_patches(&manifest).unwrap().unwrap();
        // manifest_dir is `dir`; "../teenygrad/..." resolves to dir.parent()/teenygrad/...
        let expected = normalize(dir.join("../teenygrad"));
        assert_eq!(root, expected);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn teenygrad_root_from_patches_returns_none_without_a_patch_section() {
        let dir = std::env::temp_dir().join("cargo-teeny-ws-test-nopatch");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let manifest = dir.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        assert_eq!(teenygrad_root_from_patches(&manifest).unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn teenygrad_mount_skips_a_patch_root_that_does_not_exist_on_disk() {
        let dir = std::env::temp_dir().join("cargo-teeny-ws-test-missing-mount");
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
teeny-core = { path = "../teenygrad-does-not-exist/core/teeny-core" }
"#,
        )
        .unwrap();

        // The patch section resolves fine (path doesn't need to exist for that), but
        // `teenygrad_mount` should treat the nonexistent root as "not needed".
        assert!(teenygrad_root_from_patches(&manifest).unwrap().is_some());
        assert_eq!(teenygrad_mount(&manifest).unwrap(), None);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Builds a fake `root/` (with `Cross.toml`, a `[package]`, and one `[workspace]`
    /// member under `root/member/`) so `find_member_manifest`/`resolve` can be exercised
    /// without a real cargo workspace.
    fn make_fake_workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        let member = root.join("member");
        fs::create_dir_all(&member).unwrap();
        fs::write(root.join("Cross.toml"), "[build]\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"root-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [workspace]\nmembers = [\"member\"]\n",
        )
        .unwrap();
        fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"member-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn find_member_manifest_finds_the_named_member() {
        let root = make_fake_workspace("cargo-teeny-ws-member-test");
        let manifest = find_member_manifest(&root, "member-pkg").unwrap();
        assert_eq!(manifest, root.join("member").join("Cargo.toml"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_member_manifest_finds_the_root_package() {
        let root = make_fake_workspace("cargo-teeny-ws-root-test");
        let manifest = find_member_manifest(&root, "root-pkg").unwrap();
        assert_eq!(manifest, root.join("Cargo.toml"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_member_manifest_errors_on_unknown_package() {
        let root = make_fake_workspace("cargo-teeny-ws-unknown-test");
        assert!(find_member_manifest(&root, "no-such-pkg").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_with_package_finds_member_from_workspace_root() {
        let root = make_fake_workspace("cargo-teeny-ws-resolve-test");
        let (manifest, cross_root, manifest_rel) = resolve(&root, Some("member-pkg")).unwrap();
        assert_eq!(manifest, root.join("member").join("Cargo.toml"));
        assert_eq!(cross_root, root);
        assert_eq!(manifest_rel, PathBuf::from("member").join("Cargo.toml"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_without_package_uses_the_root_manifest() {
        let root = make_fake_workspace("cargo-teeny-ws-resolve-default-test");
        let (manifest, cross_root, manifest_rel) = resolve(&root, None).unwrap();
        assert_eq!(manifest, root.join("Cargo.toml"));
        assert_eq!(cross_root, root);
        assert_eq!(manifest_rel, PathBuf::from("Cargo.toml"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_errors_when_cwd_is_not_the_workspace_root() {
        let root = make_fake_workspace("cargo-teeny-ws-resolve-member-cwd-test");
        // Invoking from within the member's own directory (the old `cd demos/parking_garage`
        // workflow) is no longer supported — only the workspace root itself.
        assert!(resolve(&root.join("member"), None).is_err());
        assert!(resolve(&root.join("member"), Some("member-pkg")).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
