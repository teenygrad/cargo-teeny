# AGENTS.md

Guidance for AI agents working in this Rust repository.

## Error Handling

Use `anyhow::Result` for fallible functions. Chain context with `.with_context(|| ...)` (closure form, not `.context(...)`) so the message is only allocated on failure.

```rust
// Good
fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;

// Avoid
fs::read_to_string(&path).unwrap();
fs::read_to_string(&path).expect("read file");  // only acceptable in tests
```

Define typed errors with `thiserror` when callers need to match on variants. Use `anyhow` when errors are only displayed to a human.

Never use `unwrap()` in non-test code unless the invariant is locally obvious and a comment explains why the value can never be `None`/`Err`.

## Types and Ownership

Prefer borrowing over cloning. Take `&str` / `&Path` in function arguments; return owned `String` / `PathBuf` when the caller needs to store the value.

```rust
// Good: accepts both &str and String via deref coercion
fn validate_host(host: &str) -> Result<()> { ... }

// Good: return owned when storing
fn build_path(root: &Path, rel: &str) -> PathBuf {
    root.join(rel)
}
```

Use newtypes to encode domain invariants in the type system rather than runtime checks:

```rust
struct ValidatedHost(String);
impl ValidatedHost {
    fn new(s: &str) -> Result<Self> { ... }
}
```

Use `Into<T>` / `From<T>` for infallible conversions; never add a separate `from_str` helper when `impl From<&str>` suffices.

## Enums and Pattern Matching

Prefer exhaustive `match` over `if let` chains when all variants are relevant. Add `#[non_exhaustive]` to enums in public APIs where new variants are expected.

Model state and configuration with enums rather than stringly-typed fields or boolean flags:

```rust
// Good
enum OutputMode { Quiet, Verbose }

// Avoid
fn run(verbose: bool) { ... }
```

## Iterators

Prefer iterator combinators over `for` loops with mutation:

```rust
// Good
let paths: Vec<_> = folders.iter().map(|f| f.remote_path).collect();

// Avoid
let mut paths = Vec::new();
for f in folders { paths.push(f.remote_path); }
```

Use `.collect::<Result<Vec<_>>>()` to short-circuit on the first error inside an iterator.

## Strings

Use `&str` for read-only string data; `String` for owned data. Use `format!` for building strings, not repeated `push_str`. For static string data, prefer `&'static str`.

## Structs

Use the builder pattern only when there are many optional or mutually exclusive fields. For simple structs, plain construction is clearer. Implement `Default` when a zero-value struct makes sense.

## Traits

Implement standard traits where applicable before defining bespoke methods: `Display` instead of `to_string_custom()`, `From` instead of `new_from_*`, `Iterator` instead of `next_item()`.

Use `impl Trait` in return position for zero-cost static dispatch. Use `Box<dyn Trait>` only when the concrete type is unknown at compile time.

## Subprocess / Side Effects

Always check exit status with `status.success()` and return an `Err` with context. Never swallow subprocess stderr — let it pass through to the user's terminal by not redirecting it.

```rust
let status = Command::new("rsync")
    .args([...])
    .status()
    .with_context(|| "spawn rsync")?;
anyhow::ensure!(status.success(), "rsync exited with {status}");
```

## Diagnostics

Use `eprintln!` for progress/diagnostic output (not `println!`). This keeps stdout clean for machine-readable output and matches the convention in this codebase.

## Tests

Put unit tests in a `#[cfg(test)] mod tests` block at the bottom of the file under test. Integration tests go in `tests/`. Name tests after the behaviour being verified, not the function:

```rust
// Good
#[test]
fn rejects_host_with_path_separator() { ... }

// Avoid
#[test]
fn test_validate_host() { ... }
```

Use real temporary directories (`std::env::temp_dir()`) for filesystem tests; avoid mocking the filesystem.

## Clippy and Formatting

The codebase must pass `cargo clippy` and `cargo fmt --check` with no warnings. Treat every clippy lint as an error unless there is a specific `#[allow(...)]` with a comment explaining why.

Common lints to watch:
- `clippy::redundant_closure` — use `f` not `|x| f(x)`
- `clippy::map_unwrap_or` — prefer `.unwrap_or_else`
- `clippy::needless_pass_by_value` — take `&T` not `T` when you don't consume it
- `clippy::wildcard_imports` — avoid `use foo::*` except in `#[cfg(test)]`
