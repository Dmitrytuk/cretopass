# Rust coding rules

*A complete ruleset for an AI coding working in Rust codebases.*

---

## 0. Core Principles

1. **Correctness first, then clarity, then performance.** Never sacrifice correctness for speed unless explicitly asked and measured.
2. **Make invalid states unrepresentable.** Use the type system to enforce invariants at compile time rather than checking at runtime.
3. **Prefer the compiler's opinion over your own.** If the borrow checker fights you, the design is usually wrong—restructure, don't `clone()` your way out.
4. **Leave the code better than you found it**, but do not refactor unrelated code unless asked.
5. **When in doubt, follow the Rust API Guidelines** (https://rust-lang.github.io/api-guidelines/) and the standard library's conventions.

---

## 1. Toolchain & Workflow

###1.1 Always run before declaring work complete
```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps   # if public API was touched
```
- Never submit code that does not compile.
- Never suppress a clippy lint with `#[allow(...)]` without a comment explaining why.
- Treat `cargo clippy` warnings as errors.

### 1.2 Edition & versions
- Use the latest stable edition (currently `2024`) for new crates; respect the existing edition in existing crates.
- Never change `rust-version` (MSRV) or edition without being asked.
- Do not use nightly features unless the project already does.

### 1.3 Formatting
- `rustfmt` is the only style authority. Do not hand-format.
- Respect an existing `rustfmt.toml`; do not add one unless asked.
- Max line width: whatever `rustfmt.toml` says; default 100.

### 1.4 Dependencies
- Before adding a dependency, check if `std` or an existing dependency already covers the need.
- Prefer widely used, actively maintained crates (`serde`, `thiserror`, `anyhow`, `tokio`, `tracing`, `clap`, `rayon`, etc.).
- Pin with caret semver (`"1.2"`), not exact versions, unless the project convention differs.
- Enable only the features you need: `default-features = false` when reasonable.
- Never add a dependency with an incompatible license without flagging it.
- Run `cargo tree -d` if you suspect duplicate versions.

---

## 2. Project Structure

- One concept per module. Keep files under ~500 lines; split when they grow.
- Use `mod.rs`-free layout (`foo.rs` + `foo/bar.rs`) unless the project already uses `mod.rs`.
- `lib.rs` / `main.rs` should be thin: re-exports, top-level docs, and wiring only.
- For binaries: put logic in a library crate, keep `main.rs` to argument parsing + calling `lib::run()`.
- Use `pub(crate)` by default for internal items. Only `pub` what is meant to be a stable API.
- Group imports: `std` → external crates → crate-local, separated by blank lines (rustfmt `group_imports` does this if configured).
- Do not use glob imports (`use foo::*`) except for preludes and in `tests` modules (`use super::*`).

---

## 3. Naming

Follow RFC 430 and the API Guidelines strictly:

| Item | Convention | Example |
|---|---|---|
| Crates, modules, functions, variables, macros | `snake_case` | `parse_header` |
| Types, traits, enum variants | `UpperCamelCase` | `HttpRequest` |
| Constants, statics | `SCREAMING_SNAKE_CASE` | `MAX_RETRIES` |
| Lifetimes | short lowercase | `'a`, `'de` |
| Type parameters | single uppercase or short CamelCase | `T`, `K`, `V`, `Item` |

- Acronyms are treated as words: `HttpClient`, not `HTTPClient`; `Uuid`, not `UUID`.
- Conversion methods: `as_` (free, borrowed→borrowed), `to_` (expensive or borrowed→owned), `into_` (consuming).
- Getters are named after the field: `fn len(&self)`, not `fn get_len(&self)`. Use `get_` only for lookups that may fail (`get(index) -> Option`).
- Iterator-producing methods: `iter()`, `iter_mut()`, `into_iter()`.
- Constructors: `new()` for the primary one; `with_*` / `from_*` for alternatives. Provide `Default` if `new()` takes no arguments.
- Boolean methods read as predicates: `is_empty()`, `has_children()`, `contains()`.
- Do not prefix or suffix with the type name redundantly (`Config::config_path` → `Config::path`).

---

## 4. Ownership, Borrowing & Lifetimes

- Take `&T` unless you need `&mut T`; take `&mut T` unless you need ownership.
- Accept the most general borrowed form in function parameters:
  - `&str` not `&String`
  - `&[T]` not `&Vec<T>`
  - `&Path` not `&PathBuf`
  - `impl AsRef<str>` / `impl Into<String>` whenergonomics justify it
- Return owned types from constructors; return borrows from getters.
- Do not `.clone()` to satisfy the borrow checker. Restructure first. If a clone is genuinely needed, add a brief comment if it's non-obvious.
- Avoid `Rc<RefCell<T>>` / `Arc<Mutex<T>>` as a first resort. Reach for them when shared mutable state is truly required, not to dodge lifetimes.
- Elide lifetimes wherever the compiler allows. Name them only when needed for clarity or necessity.
- Do not use `'static` bounds casually; they are viral.
- Prefer `Cow<'_, str>` for functions that sometimes allocate and sometimes don't.

---

## 5. Error Handling

### 5.1 Rules
- **Never `unwrap()` or `expect()` in library code or production paths**, except:
  - When the invariant is provably guaranteed—then use `expect("reason why this cannot fail")`.
  - In tests, examples, and quick prototypes.
- **Never `panic!` for recoverable errors.** Panics are for bugs (violated invariants), not for bad input or I/O failure.
- Never use `unwrap_or_default()` to hide errors that should be surfaced.
- Use `?` for propagation. Do not write `match` blocks that just re-return errors.
- Do not swallow errors with `let _ = fallible();` unless intentional, and then comment why.
- Do not use `Result<T, String>` or `Result<T, Box<dyn Error>>` in library public APIs.

### 5.2 Library crates
- Define error types with `thiserror`:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum ConfigError {
      #[error("failed to read config at {path}")]
      Read { path: PathBuf, #[source] source: std::io::Error },
      #[error("invalid value for `{key}`: {value}")]
      InvalidValue { key: String, value: String },
  }
  ```
- One error enum per module or per fallible domain, not one giant crate-wide enum (unless the crate is small).
- Implement `std::error::Error` (thiserror does this) and preserve `#[source]` chains.
- Error messages: lowercase, no trailing punctuation, no "Error:" prefix (the caller adds context).
- Mark error enums `#[non_exhaustive]` if you expect to add variants.

### 5.3 Application / binary crates
- Use `anyhow::Result` (or `eyre`) at the application layer.
- Add context at each level: `.with_context(|| format!("loading {}", path.display()))?`.
- `main` should return `anyhow::Result<()>` or explicitly handle and exit with a code.

### 5.4 Option vs Result
- `Option` = absence is normal and expected. `Result` = something went wrong.
- Never use `Option` to hide an error condition.

---

## 6. Types & Data Modeling

- Use newtypes for domain values: `struct UserId(u64)`, not bare `u64`. Derive `Copy` when the inner type is `Copy`.
- Use enums instead of `bool` flags or stringly-typed states:
  ```rust
  // Bad
  fn connect(secure: bool, retry: bool)
  // Good
  fn connect(mode: Mode, retry: RetryPolicy)
  ```
- Use the typestate / builder pattern for complex construction with required fields.
- Derive liberally and in a consistent order: `#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]`.
  - Always derive `Debug` on public types.
  - Derive `Clone` unless there's a reason not to.
  - Derive `Copy` only for small, plain-data types (≤ ~16–32 bytes, no heap).
  - Derive `Eq`/`Hash` together with `PartialEq`. Never derive `Eq` on types containing floats.
- Mark public structs and enums `#[non_exhaustive]` when they may grow.
- Prefer `struct` with named fields over tuples when there are more than 2 elements or the meaning isn't obvious.
- Use `NonZeroU32` etc. for values that cannot be zero; enables niche optimization on `Option`.
- Avoid `Box<dyn Trait>` when generics suffice; avoid generics when `dyn` keeps compile times and binary size sane and the call is not hot. Prefer `dyn` at API boundaries where object collections vary.
- Do not use `String` for things that are `Path`, `Url`, `Duration`, `Ipv4Addr`, etc.

---

## 7. Traits & Generics

- Implement standard traits where they make sense: `Display`, `FromStr`, `From`/`TryFrom`, `AsRef`, `Borrow`, `Iterator`, `IntoIterator`, `Extend`, `FromIterator`, `Default`.
- Implement `From<X> for Y`, never `Into<Y> for X`.
- `Display` is for users; `Debug` is for developers. Never make `Display` output `Debug`-like structure.
- Prefer `impl Trait` in argument position for simple cases, explicit generics when the type parameter is used more than once or needs turbofish.
- Return `impl Iterator<Item = T>` rather than collecting into a `Vec` when the caller might not need all items.
- Keep trait bounds on `impl` blocks and functions, not on struct definitions, unless the struct requires them for invariants.
- Do not create traits with a single implementation "for future flexibility." Wait until there are two.
- Use the sealed-trait pattern for public traits not meant to be implemented downstream.
- Prefer associated types over generic parameters on traits when there is exactly one logical type per implementor.

---

## 8. Functions & Control Flow

- Functions should do one thing. Prefer ≤ ~40 lines; extract when longer.
- Max ~5 parameters. Beyond that, bundle into a struct.
- Use early returns and `let ... else` to reduce nesting:
  ```rust
  let Some(user) = find_user(id) else {
      return Err(Error::NotFound(id));
  };
  ```
- Prefer `match` over `if let` chains when handling more than two variants.
- Prefer `if let Some(x) = opt` over `if opt.is_some() { opt.unwrap() }`.
- Use combinators (`map`, `and_then`, `ok_or_else`, `unwrap_or_else`) when they are *clearer*; use `match` when the combinator chain exceeds ~3 links.
- Use iterators over index loops. `for x in &vec`, not `for i in 0..vec.len()`. Use `.iter().enumerate()` when you need indices.
- Do not `collect()` into intermediate `Vec`s just to iterate again.
- No `return` on the final expression of a function.
- Exhaustive `match` without `_ =>` on your own enums so new variants cause compile errors.
- Avoid `as` casts for numeric narrowing; use `try_from()` / `try_into()` or explicit `u8::try_from(x)?`. `as` is acceptable for widening and for `usize` ↔ index arithmetic where it is obviously safe.
- Avoid shadowing across different meanings; shadowing for type refinement (`let x = x.parse()?`) is fine.

---

## 9. Unsafe

- **Do not write `unsafe` unless explicitly asked or it is genuinely unavoidable** (FFI, specific performance-critical primitives).
- Every `unsafe` block must have a `// SAFETY:` comment immediately above it explaining why the invariants hold.
- Every `unsafe fn` must have a `# Safety` doc section describing what the caller must uphold.
- Keep `unsafe` blocks as small as possible—one operation each.
- Encapsulate `unsafe` inside a safe abstraction with a small, auditable surface.
- Run `cargo miri test` if the project uses it, or recommend it, whenever touching `unsafe`.
- Never use `unsafe` to bypass the borrow checker, `transmute` lifetimes, or `mem::forget` to avoid a `Drop`.
- `#![forbid(unsafe_code)]` in crates that have no legitimate need for it.

---

## 10. Concurrency & Async

### 10.1 Threads
- Use `std::thread::scope` for structured parallelism; use `rayon` for data parallelism.
- Prefer message passing (`std::sync::mpsc`, `crossbeam`, `tokio::sync::mpsc`) over shared state.
- Use `Arc<Mutex<T>>` only for genuinely shared mutable state. Prefer `RwLock` only when reads vastly outnumber writes and are held briefly.
- Never hold a lock across an `.await` or across a blocking call. Scope guards tightly: `{ let g = m.lock(); ... }`.
- Handle `PoisonError` deliberately—`lock().unwrap()` is acceptable only if a poisoned lock genuinely means an unrecoverable bug; document that.
- Prefer `parking_lot` mutexes if the project already uses them (no poisoning, smaller).
- Use atomics with the weakest sufficient ordering; default to `SeqCst` only if you're unsure and comment it.

### 10.2 Async
- Do not mix runtimes. Use whatever the project uses (`tokio` is the default assumption).
- Never block inside async code: no `std::thread::sleep`, `std::fs`, synchronous network, or heavy CPU work. Use `tokio::fs`, `tokio::time::sleep`, `spawn_blocking`.
- Spawned tasks must be `Send + 'static`; if a value isn't, restructure rather than using `unsafe` or `Rc`.
- Always handle `JoinHandle` results—don't silently drop task panics.
- Use `tokio::select!` with care: be aware of cancellation safety; document which branches are cancellation-safe.
- Prefer `async fn` in traits (stable since 1.75) over `async_trait` unless `dyn` dispatch is required.
- Add timeouts to all network operations.
- Use `tracing` (not `log`, not `println!`) for instrumentation; `#[tracing::instrument]` on entry points.
- Do not create unbounded channels in production paths without justification.

---

## 11. Performance

- **Measure before optimizing.** Use `criterion` for benchmarks; never claim a speedup without a benchmark.
- Avoid allocation in hot loops: reuse buffers, use `with_capacity`, use `SmallVec`/arrays when sizes are small and known.
- Prefer `&str` slicing and `split` iterators over `String` construction.
- Use `Vec::with_capacity(n)` when the size is known.
- Prefer `iter().map().collect()` over manual `push` loops (often optimizes better and is clearer).
- Do not pre-emptively `#[inline]`. Use `#[inline]` only for tiny cross-crate hot functions; let LLVM decide otherwise.
- Prefer `impl Iterator` returns over `Vec` to enable lazy evaluation.
- `Box<[T]>` over `Vec<T>` for fixed-size heap data; `Box<str>` over `String` for immutable strings in large structs.
- Enable `lto = "thin"` and `codegen-units = 1` in release profiles only if the project owner wants it; do not silently change profiles.
- Don't use `HashMap` when a `Vec` with linear search is faster for small n (< ~20). Use `BTreeMap` when you need ordering.
- Use `FxHashMap` / `ahash` for non-adversarial hot maps; default `HashMap` (SipHash) when inputs may be attacker-controlled.

---

## 12. Documentation

- Every `pub` item gets a `///` doc comment. Start with a one-line summary sentence in third person ("Returns the…", "Creates a…").
- Crate root gets `//!` docs describing purpose and a short usage example.
- Use sections in this order where applicable:
  ```rust
  /// Summary line.
  ///
  /// Longer explanation.
  ///
  /// # Examples
  /// # Errors
  /// # Panics
  /// # Safety
  ```
- `# Errors` is required on any `pub fn` returning `Result`.
- `# Panics` is required on any `pub fn` that can panic on some input.
- Examples in docs must compile and pass as doctests. Use `# ` to hide setup lines.
- Link items with intra-doc links: `` [`Vec`] ``, `` [`Self::new`] ``.
- Comments explain *why*, not *what*. Delete comments that restate the code.
- Use `// TODO(name): reason` or `// FIXME:` — never leave bare `TODO`.
- Do not leave commented-out code. Delete it; git remembers.
- Enable `#![warn(missing_docs)]` in library crates.

---

## 13. Testing

- Unit tests live in the same file in `#[cfg(test)] mod tests { use super::*; ... }`.
- Integration tests live in `tests/`; each file is a separate crate—share helpers via `tests/common/mod.rs`.
- Test names describe behavior: `returns_none_when_key_missing`, not `test1` or `test_get`.
- One logical assertion per test where practical. Use `assert_eq!(actual, expected)` with `actual` first.
- Use `#[should_panic(expected = "...")]` with the message, never bare `#[should_panic]`.
- Test error paths, not just the happy path. Match on the specific error variant.
- Use `proptest` or `quickcheck` for parsers, serializers, and anything with algebraic laws (roundtrips, idempotence).
- Doctests for every public API example.
- Do not use `sleep` in tests for synchronization; use channels, barriers, or `tokio::time::pause()`.
- Tests must be deterministic and independent of order, environment, network, and wall-clock time.
- Never mark tests `#[ignore]` to make CI green. If a test is flaky, fix it or delete it with an explanation.
- `unwrap()`/`expect()` are acceptable in tests.
- When fixing a bug, first write a failing test that reproduces it.

---

## 14. Macros

- Prefer functions and generics over macros. Write a macro only when you need variadics, repetition over syntax, or compile-time codegen.
- `macro_rules!` before proc macros. Proc macros only for derives or when the project already has a proc-macro crate.
- Use `$crate::` paths inside macros for all crate-internal references.
- Document macros with examples like any public item.
- Keep macros hygienic; never rely on identifiers captured from the call site unless that's the explicit intent.

---

## 15. Serialization, FFI & I/O

- Use `serde` with derives; put `#[serde(rename_all = "snake_case")]` (or project convention) on the type, not per-field renames.
- Mark optional fields `#[serde(default, skip_serializing_if = "Option::is_none")]` where appropriate.
- Use `#[serde(deny_unknown_fields)]` for config files where typos should error.
- Buffer I/O: wrap `File` in `BufReader`/`BufWriter`. Don't `read_to_string` files that may be huge.
- Use `Path`/`PathBuf` for paths; never manipulate paths as strings. Use `.join()`, not `format!("{}/{}")`.
- Use `std::io::Write` and `write!`/`writeln!` for output; `println!` only in CLI final output. Handle `BrokenPipe` gracefully in CLIs.
- FFI: use `#[repr(C)]`, `extern "C"`, `std::ffi::{CStr, CString}`; wrap in a safe module; never expose raw pointers in the public API.

---

## 16. Lints to enable in new crates

Add to `lib.rs`/`main.rs` (or `Cargo.toml` `[lints]`) for new crates, respecting project conventions for existing ones:

```toml
[lints.rust]
unsafe_code = "forbid"          # remove if FFI is required
missing_docs = "warn"
unreachable_pub = "warn"

[lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
todo = "warn"
dbg_macro = "warn"
print_stdout = "warn"           # for libraries
module_name_repetitions = "allow"
must_use_candidate = "allow"
```
