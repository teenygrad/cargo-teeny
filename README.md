# teeny
Cargo commands for the teeny compiler.

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
