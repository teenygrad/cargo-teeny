# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/teenygrad/cargo-teeny/compare/v0.1.2...v0.1.3) - 2026-08-16

### Other

- release v0.1.2 ([#4](https://github.com/teenygrad/cargo-teeny/pull/4))

## [0.1.2](https://github.com/teenygrad/cargo-teeny/compare/v0.1.1...v0.1.2) - 2026-08-16

### Other

- release v0.1.2

## [0.1.1](https://github.com/teenygrad/cargo-teeny/compare/v0.1.0...v0.1.1) - 2026-08-03

### Other

- point at the central docs site
- release v0.1.0

## [0.1.0](https://github.com/teenygrad/cargo-teeny/releases/tag/v0.1.0) - 2026-07-31

### Added

- add release-plz config and workflow

### Fixed

- list available manifest targets when host toolchain is unavailable
- correct stale spinorml org reference, apply cargo fmt

### Other

- use SPDX license identifier, pin MSRV, add crates.io metadata
- revert temporary workflow_dispatch + dry_run scaffolding
- temporarily add workflow_dispatch + dry_run for verification
- full crate-level doc coverage, document all subcommands in README
- support local-dir install, fix toolchain-name validation
- Add `cargo teeny deploy` to push a package to a remote host via rsync
- Add `cargo teeny package` to assemble a self-contained deployment package
- Add `cargo teeny aot` to build+run ahead-of-time kernel compilation
- Add `cargo teeny install-toolchain` to install teenyc from the CDN
- Don't track .beads symlink (beads repos are private)
- Add beads (br) issue tracking
- strip 'teeny' arg injected by cargo plugin convention
- add cross-compilation build/check/clippy subcommands
- Add CLAUDE.md and AGENTS.md for AI agent guidance.
- type-driven rsync folder list for Jetson Orin Nano.
- add --type value enum (jetson-orin-nano).
- require --host and --path flags.
- Add cargo-teeny CLI scaffolding and sysroot command.
- Initial commit
