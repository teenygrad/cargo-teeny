# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                   # Build
cargo test                    # Run all tests
cargo test <test_name>        # Run a single test by name
cargo run -- sysroot --help   # Run with arguments (note the extra --)
cargo clippy                  # Lint
cargo fmt                     # Format
```

When running as a cargo plugin, it's invoked as `cargo teeny sysroot …` (cargo strips the `cargo-` prefix).

## Architecture

This is a Cargo plugin binary (`cargo-teeny`) with a two-layer structure:

- **`src/cli.rs`** — All clap structs and enums. Adding a new subcommand means adding a variant to `Command` and a corresponding `*Args` struct here.
- **`src/commands/<name>.rs`** — One module per subcommand, each exporting a `run(args: *Args) -> Result<()>`. Dispatch lives in `main.rs`'s match on `cli.command`.

### Adding a new subcommand

1. Add `*Args` struct and `Command` variant to `src/cli.rs`.
2. Create `src/commands/<name>.rs` with `pub fn run(args: *Args) -> Result<()>`.
3. Export it in `src/commands/mod.rs`.
4. Add a match arm in `main.rs`.

### Sysroot command (`src/commands/sysroot.rs`)

Scaffolds an FHS-style directory tree for a cross-compilation sysroot and optionally mirrors remote directories into it via `rsync`.

Key design points:
- Board/environment profiles are a `SysrootType` enum in `cli.rs` (`ValueEnum`). Each variant maps to a fixed list of `SysrootRsyncFolder` entries in `sysroot_rsync_folders()`.
- A marker file (`.cargo-teeny-sysroot`) is written to the sysroot root with version, host triple, type, and (if applicable) rsync metadata. Bump `MARKER_VERSION` whenever the marker format changes.
- `rsync` is invoked as a subprocess with `-a -e <rsync_ssh>`. The trailing slash on the source path means "sync contents, not the directory itself."
- To add a new board profile: add a `SysrootType` variant and a match arm in `sysroot_rsync_folders()`.
