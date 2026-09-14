# Ownership convention: borrow by default

The default is to **borrow**. Every `.clone()`, `.to_owned()`, `.to_string()`,
and `.copied()` must earn its place. A clone that exists only to placate the
borrow checker is a design smell, not a solution — fix the signature, don't
hide it behind an allocation.

This document is the single source of truth for the
`.opencode/agents/code-reviewer.md` agent's "Borrowing & cloning" criterion.

## The rule

> Take `&T` when you only read. Take `&mut T` when you mutate without replacing.
> Take `T` only when you must own it. Clone only when ownership is genuinely
> required (see [Acceptable cases](#acceptable-cases)).

## Vocabulary: Copy vs Clone

Do not treat all "copying" equally — flag only expensive duplication.

- `Copy` — implicit, bitwise, no allocation (`i32`, `bool`, `&T`, small arrays,
  tuples of `Copy`). Cheap and usually fine. Do **not** flag `Copy`.
- `Clone` — explicit, arbitrary cost. `String`, `Vec`, `HashMap`, `Arc` payloads
  allocate. This is what the convention targets.

## Smells to flag

Each of these is a finding. Rank per [Severity](#severity).

1. **Clone in a loop** — `.clone()` inside `for`/`while`/iterator body repeats
   the allocation per iteration.
2. **Clone to satisfy the borrow checker** — the call compiles only because you
   cloned instead of restructuring lifetimes or passing a reference.
3. **Clone of an allocation-heavy type** where `&T`, `&str`, `&[T]`, or
   `&mut T` would work — `String` instead of `&str`, `Vec<T>` instead of
   `&[T]`.
4. **Clone then never mutate** — the owned copy is used read-only; a reference
   suffices.
5. **Clone in a hot path** — inside `map`/`filter`/`fold` closures or other
   frequent call sites.
6. **Clone to dodge ownership design** — `Arc<Mutex<_>`/`Rc<RefCell<_>` or
   blanket `.clone()`-and-move patterns that signal unowned responsibilities.

## Acceptable cases

A clone is fine (do not flag) when it is the correct design:

- `Copy` types — free-ish; not a finding.
- Cheap explicit clones of small, non-allocating types.
- Owned data stored per element (e.g. building a `Vec<String>` from
  individual strings already owned).
- Crossing `Send`/thread/`'static` boundaries (spawning a task that must own
  its data).
- An API that must return an owned value by contract (caller frees the borrow).
- Growth that requires cloned capacity (e.g. doubling a buffer).
- `iter().cloned()` to convert `&T` out of a borrowed iterator — idiomatic when
  the consumer needs ownership once; still check it is not simply feeding a
  function that could borrow.

## Concrete fixes (prefer in this order)

For each smell, suggest the minimal structural fix.

1. **Change the parameter/return to a reference** — `&str` for `String`,
   `&[T]`/`&[T; N]` for `Vec<T>`, `&HashMap<K, V>` for the map.
2. **Borrow then clone at the single point ownership is truly needed** — defer
   the clone to the end rather than eagerly allocating.
3. **Take the value by move** — if the caller is done with it, consume `T`
   instead of `&T` then cloning.
4. **Reuse a buffer** — mutate in place via `&mut Vec<T>`/`&mut String`.
5. **Use `mem::take`, `std::mem::replace`, or `drain(..)`** to extract owned
   data out of a mutable binding without cloning.
6. **Use `Cow<'_, str>`/`Cow<'_, [T]>`** when a function borrow-and-return or
   own-and-return are both valid.
7. **Use `as_deref()`/`as_ref()`/`iter()`** to convert `Option<T>`/`Vec<T>`
   into borrowed views before passing them on.

## Severity

Map findings to the code-reviewer verdict levels:

- **blocking** — clone in a hot loop on a large allocation-heavy type; clone
  that should be a borrow on a public API (`&str` vs `String`) forcing every
  caller to allocate.
- **important** — clone to placate the borrow checker that signals a structural
  fix; clone of large types inside the crate's core/data path.
- **minor** — redundant clone removable by a trivial signature change; clone of
  a small type where a borrow is cleaner.
- **nit** — clone on a `Copy` type done by habit (`clone()` instead of relying
  on copy semantics); `to_owned()` where `to_string()` idiom fits.

Every finding must name the specific clone and show the borrow-based fix.