# 0012 — Password generator

There was no password-generation code in cretopass: users typed every `Password`
value by hand. The reference implementation studied for this feature
(`/home/dmitry/Dev/password_generator_rust`, a separate project) could not be
ported as-is:

- It fills each character class with `IndexedRandom::sample`, which samples
  **distinct** elements. A class budget can therefore never exceed its alphabet:
  17 symbols overflow at length ≳ 86, 10 digits at ≳ 63, 52 letters at ≳ 82. At
  the planned maximum of 128 it would silently return a shorter password.
- It has no notion of enabling/disabling individual symbols and no UI contract.

**Decision:** add a `generator` module that is a strict leaf, and a
context-sensitive `Ctrl+G` action in the Add/Edit form.

## Module shape

`src/generator.rs` is a facade over `src/generator/password.rs`; it does not use
`mod.rs`. The module depends only on `std` and `rand` and never imports
`FieldType`, `theming`, `tree`, or any other project module. This keeps the
module graph acyclic and makes the generator testable without a terminal.

The tree layer owns the mapping from field type to generator:

```rust
enum FieldGenerator { Password }

impl FieldGenerator {
    fn for_field(field_type: Option<&FieldType>) -> Option<Self>;
    fn generate(self, params: &PasswordParams) -> String;
}
```

An enum rather than a trait: today there is exactly one generator, and a new
variant forces every `match` to be revisited. The enum is the extension point
for future field types; their parameters may need a broader type than
`PasswordParams`.

## Public API

- `SYMBOLS: [char; 17]` — `!~#$%^&*()_+=-/<>`.
- `PasswordLength(u8)` — valid range `4..=128`, default `10`, built through
  `TryFrom<usize>` / `FromStr` with a typed `PasswordLengthError`. Invalid
  lengths are unrepresentable, so generation is infallible.
- `SymbolSet` — a bitmask over `SYMBOLS`; unknown characters are ignored by
  `toggle`/`set`, so the UI never indexes the alphabet with a raw `usize`.
  `Default` is all symbols.
- `PasswordParams { length, symbols }`, `Default = (10, all symbols)`.
- `generate_password(&PasswordParams) -> String` — always returns exactly
  `length` ASCII characters.

`rand = "0.8"` is added as a direct dependency. It is already in the lockfile
through `age`/`age-core` with `std` + `std_rng` enabled, so this compiles no new
crate and creates no duplicate version. `rand` 0.10 (used by the reference)
would add a second version; `getrandom` directly would require hand-rolled
uniform sampling and shuffling.

## Algorithm

Budgets are computed with integer arithmetic:

```text
ratio     = round(0.2 * length)        // (length + 2) / 5; never < 1
symbols   = symbols_enabled ? ratio : 0
digits    = ratio
letters   = length - digits - symbols  // always >= 2 for the 4..=128 range
```

Digits and symbols are each guaranteed at least one when enabled, the parts sum
exactly to `length`, and letters never drop below one. Filling is **with
replacement** (`choose` repeatedly), which removes the reference's silent
short-password defect; the resulting characters are then shuffled. With every
symbol unchecked the symbol budget is zero and its share goes to letters; no
error is raised.

**Rejected:** sampling distinct elements per class (the reference behaviour);
sampling uniformly from one combined alphabet (cannot guarantee a digit and a
symbol, and checking budgets per class is harder to explain).

## UI contract

- **Trigger:** `Ctrl+G` at the form's **Nav** level only, when the focused row is
  an `ExtraField` whose type has a generator. On `Login`, `Custom`, `Name`, or
  the add button it is a no-op, and the binding is not advertised. At the Input
  level `Ctrl+G` remains a literal `g` (the form's Input dispatcher ignores
  modifiers, unchanged by this feature).
- `from_form_nav` checks **only** `Ctrl+G` before the plain-char matches, so
  `Ctrl+u/r/d/p/i/j` keep their existing meanings.
- **Modal:** three flat rows — Length, Special characters, Generate. `j`/`k`,
  arrows and `Tab`/`Shift+Tab` move between rows; `Space` toggles all symbols on
  the summary row; `Enter` on the length row advances to the summary row, on the
  summary row opens the checklist, and on Generate validates and generates;
  `Esc` returns to the form without touching the value.
- **Length input:** the modal holds raw digits (at most three), so empty and
  out-of-range values are representable *in the UI* and show an inline
  `Length must be 4–128` error. Conversion happens only on Generate; the
  parameters themselves stay valid.
- **Length editing:** the row opens with the last-used value seeded and a
  "fresh" flag, so the first digit (or `Backspace`/`Delete`) replaces it instead
  of appending — typing `12` over `10` yields `12`. From then on a visible caret
  supports `Left`/`Right`/`Home`/`End`/`Delete` editing, and the buffer is
  capped at three digits. Caret editing is only meaningful on the length row;
  the other rows ignore these keys. The fresh flag is cleared by the first edit
  or caret move.
- **Checklist:** the 17 symbols are a three-column grid with `[X]`/`[ ]` cells.
  `h`/`l` move one cell, `j`/`k` move one row; `Space`/`Enter` toggle; `Esc`
  returns to the summary row. The raw length buffer, its caret, and the fresh
  flag survive the round trip because both modes carry the same `GeneratorModal`.
- **Result:** generating writes the value into the focused field, pushes a form
  undo entry, marks the tree modified (autosave persists it), remembers the
  params, and returns to the form at Nav. `u` reverts the generated value.
  Overwriting a non-empty password has no confirmation.
- The value is masked in the form/Detail as usual; it is revealed only while
  editing with `i`.
- The generator is **not** added to the global Help overlay: it is
  context-sensitive, and the form's bottom hint is the single source of truth.

## Settings lifetime

`PasswordParams` lives on `HomeState` for the whole app run. Each opened tree is
seeded from it next to `display_full_path`, and `run_tree` copies it back on
every exit path (including an unsaved exit). A fresh launch starts at `10`
characters with all symbols. Nothing is written to the `Configuration` file.

Storing it only on `TreeState` was rejected: `TreeState` is recreated for every
vault open, so the setting would silently reset each time the user returned
Home.

## Reference

`/home/dmitry/Dev/password_generator_rust` was read but not modified. Its crate
metadata has no `license` field, so the implementation here is an independent
reimplementation of the budget split and character classes rather than a copy.
Its distinct-sampling defect is documented above and fixed by with-replacement
filling.

## Verification

- `src/generator/password.rs` unit tests: exact lengths at the reference's
  overflow points, alphabet membership, digit/symbol guarantees, budget sums,
  length parsing, and `SymbolSet` operations.
- `theming::key` tests: `Ctrl+G` at Nav, unchanged other Ctrl chords, literal
  `g` at Input, and both new dispatchers.
- `tree::app_tree` tests: the modal opens only on a `Password` row, generation
  writes/commits/undoes, `Esc` leaves the value alone, invalid length errors,
  length replacement and caret editing, toggle-all, checklist navigation, and
  the password-specific hint.
