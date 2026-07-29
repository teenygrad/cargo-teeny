# cargo-teeny

Cargo commands for the [teenygrad](https://github.com/teenygrad/teenygrad) toolchain: install the
custom `teenyc` compiler, cross-compile and package a project for a target board, ahead-of-time
compile its GPU kernels, and deploy the result over SSH.

## Installation

```bash
cargo install --git https://github.com/teenygrad/cargo-teeny
```

Once installed, subcommands are invoked as `cargo teeny <subcommand>` (cargo strips the `cargo-`
prefix). Run `cargo teeny --help` or `cargo teeny <subcommand> --help` for the full flag
reference — this README covers the concepts and common invocations.

## `cargo teeny install-toolchain`

Installs the custom `teenyc` compiler from the spinorml CDN and registers it with rustup, so it
can be invoked directly (`rustup run teenyc teenyc …`, or via `rustup which --toolchain teenyc
teenyc` for the raw binary path) without going through `cargo`.

`rustup toolchain install` can't be used here — rustup only accepts `stable`/`beta`/`nightly` or a
bare version number as a toolchain name, checked before any network call. So this instead
downloads the package tarball named in the channel manifest, verifies its sha256, extracts it,
and registers it with `rustup toolchain link`.

The default package (`teenyc`) is compiler-only — no `std`/`cargo` — so it can run `teenyc
--version` or compile with `#![no_std]` but can't drive a normal `cargo build`. It is never set as
your rustup default unless you pass `--default`.

```bash
cargo teeny install-toolchain
```

Pass `--local-dir <path>` to install from a local staging directory (same layout the CDN serves)
instead of hitting the network — useful for testing a freshly built toolchain before publishing it.

## Cross-compilation: `build` / `check` / `clippy`

Cross-compiles the current crate for a target board via [`cross`](https://github.com/cross-rs/cross).

```bash
cargo teeny build --target jetson-orin-nano
cargo teeny check --target jetson-orin-nano          # type-check only, faster feedback
cargo teeny clippy --target jetson-orin-nano          # lint
cargo teeny build --target jetson-orin-nano --examples
cargo teeny build --target jetson-orin-nano --example yolo26
cargo teeny build --target jetson-orin-nano --no-release  # debug build
```

Automatically:
- Resolves the teenygrad workspace root from the invoking project's `[patch.crates-io]` entries
  and mounts it into the cross container — `cross` only auto-mounts individual crate directories,
  not the workspace root needed for `Cargo.toml` inheritance.
- Mounts the host's CUDA aarch64 target directory (override with `--cuda-path`) at the path the
  board's cross image expects.

## `cargo teeny sysroot`

Lays out an empty FHS-style sysroot tree for a board (GCC/Clang-style cross links), optionally
mirroring remote directories into it via `rsync`.

```bash
cargo teeny sysroot --host aarch64-unknown-linux-gnu --path ./sysroot --type jetson-orin-nano
# with an rsync pull from the device afterward:
cargo teeny sysroot --host aarch64-unknown-linux-gnu --path ./sysroot --type jetson-orin-nano \
  --rsync-from ubuntu@jetson
```

A marker file (`.cargo-teeny-sysroot`) is written recording the version, host triple, type, and
(if used) rsync metadata.

## `cargo teeny aot`

Builds a binary/example for the **host** (not cross-compiled) and runs it to ahead-of-time compile
GPU kernels for a given device/config — unlike `build`/`check`/`clippy`, which target a board.

```bash
cargo teeny aot --example yolo26 --device cuda --options "capability=sm_87,ptx-version=82"
```

`--device`/`--options`/`--cache-dir`/`--force` are forwarded verbatim to the resulting binary;
cargo-teeny never parses them itself — the binary (typically linking `teeny-cli`) does.

## `cargo teeny package`

Assembles a self-contained deployment package for a board in one step: cross-compiles the
binary/example (delegating to `build`) and AOT-compiles its kernels on the host (delegating to
`aot`, with `--cache-dir` forced to `<dest>/cache`).

```bash
cargo teeny package \
  --target jetson-orin-nano \
  --example yolo26 \
  --dest ./dist/yolo26-orin \
  --device cuda \
  --options "capability=sm_87,ptx-version=82"
```

Produces:

```text
dist/yolo26-orin/
  bin/       # cross-compiled binary/example
  cache/     # AOT-compiled GPU kernels
  conf/      # provenance marker (target/device/options/commit/build time)
  data/      # empty -- populate with models/datasets separately
```

## `cargo teeny deploy`

Pushes a package directory (from `package`) to a remote host over `rsync`/SSH.

```bash
cargo teeny deploy --package ./dist/yolo26-orin --host ubuntu@jetson --dest /home/ubuntu/yolo26
```

- Uses `rsync -a -e <ssh>`; stdio is inherited, so an interactive password prompt (no key-based
  auth configured) passes straight through.
- By default only copies files that aren't already on the remote (`--ignore-existing`, safe to
  re-run after a partial transfer). Pass `--overwrite` to force a full re-sync.
- `--ssh "ssh -p 2222"` for a non-default port.
