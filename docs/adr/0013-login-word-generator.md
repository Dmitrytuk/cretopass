# 0013 — Login word generator

ADR 0012 gave `Password` fields a `Ctrl+G` generator. `Login` fields were left
out, so every username had to be typed by hand. This ADR adds an invented-word
generator for `Login` and generalises the trigger to any field type that has a
generator.

**Decision:** add `src/generator/login.rs` — another strict leaf beside
`password.rs` — and let `Ctrl+G` dispatch on the focused field's type, opening a
modal for a password and generating immediately for a login word.

## What is generated

A single invented lowercase `a–z` word with no digits and no spaces: "a
fictional name, not explicitly a human name, just a name" (`brindel`,
`quoravel`, `velmora`). It is not a persona, not an email alias, and not an
opaque random string — a username should look like a name a human could have
picked.

## Strategy

Syllable assembly from a curated onset/vowel/coda inventory, mirroring the
password generator's module shape: `generate_login() -> String` plus a private
`generate_login_with(rng)` that tests seed deterministically. `std` and `rand`
0.8 (already a direct dependency) are the only dependencies; no new crate.

Crates rejected after checking crates.io:

- `fake` 5.1 — needs `rand ^0.10` + `deunicode`; would pull a **second** rand
  major and emits human personas (names, emails, addresses). Contradicts the
  single-rand-0.8 decision in ADR 0012.
- `petname` 3.2 — needs `rand ^0.10`. `petname` 2.0.2 fits `rand ^0.8` but emits
  two-word `adjective-noun` pairs, not one word.
- `names` 0.14 — fits `rand ^0.8`, but also emits two-word names.
- `nanoid` 0.5 — needs `rand ^0.9`, and its output is opaque rather than name-like.

## Algorithm

Two or three syllables are chosen at random; a syllable is onset + vowel +
optional coda. Onsets and codas are built from fixed inventories:

- single onsets `b d f g h j k l m n p r s t v w z`
- onset clusters `br dr fr gr kr pr tr sh th ch qu pl fl bl gl kl sl`
- vowels `a e i o u`
- codas `l m n r s t sh th`

`c`, `q`, `x` and `y` never appear alone (`ch`/`qu` carry `c`/`q` only), which
keeps pronunciation consistent.

Four boundary rules hold **by construction**, so no candidate is ever rejected
for them:

1. no run of the same vowel — a syllable's vowel excludes the previous one;
2. no doubled letter inside a syllable — the vowel also excludes the onset's
   tail, which matters only for `qu`;
3. no coda before a consonant cluster — a medial coda is only allowed when the
   *next* onset is a single consonant (the onsets are drawn up front so this can
   be checked);
4. no doubled letter across a boundary — a medial coda whose last letter equals
   the next onset's first letter is excluded.

Two adjacent syllables can never be identical because each has exactly one vowel
and rule 1 excludes a repeated one.

Given these rules the **reachable** length range is `4..=13`: a minimum of two
single onsets with no codas (`1+1 + 1+1`) and a maximum of `cluster+vowel+coda`
+ `single+vowel+coda` + `single+vowel+coda` (`5+4+4`). A tighter bound such as
`4..=11` would be unprovable and its test would fail.

## Blocklist

A small hardcoded substring list is checked against the finished word; on a hit
the word is regenerated. Because the output alphabet is ASCII `a–z`, the list may
only contain **ASCII transliterations**: a Cyrillic entry can never match and must
not be added. A unit test enforces that every entry is non-empty lowercase ASCII,
so a dead entry fails review instead of shipping.

Retries are bounded (`MAX_ATTEMPTS = 32`) and the last candidate is returned if
the bound is hit, so `generate_login` can neither hang nor panic even against a
much larger future list.

## Collision handling

None. The generator is a blind pure function; the output space is huge and
`Ctrl+G` again is the recovery. There is no vault-wide uniqueness check (a login
only has to be free *on the service*) and no "differs from the old value"
exclusion (it would add a parameter to an otherwise pure generator).

## Dispatcher

`FieldGenerator` (in `tree::app_tree`) becomes a classifier over `FieldType`:

```rust
enum FieldGenerator { Login, Password }

impl FieldGenerator {
    fn for_field(field_type: Option<&FieldType>) -> Option<Self>;
}
```

`generate()` is deleted; the parameters differ per variant, so `handle_form`
matches per variant instead of going through one method. `for_field` is the single
source of truth used by both the key handler and the form hint.

## UI contract

- **Trigger:** `Ctrl+G` at the form's **Input** level only, on a `Login` or
  `Password` row. At the Nav level it is unbound. On rows without a generator it
  is a no-op and the binding is not advertised.
- `Action::GeneratePassword` is renamed `Action::Generate`, because it now
  dispatches by field type.
- **Interaction:** immediate generation, no modal and no settings. `Ctrl+G`
  writes a fresh word; `Ctrl+G` again replaces it (reroll). Nothing is seeded to
  `TreeState` and there is no session memory — a login word has no parameters.
- **Value semantics:** a generation is a normal committed change — it sets
  `modified` and autosaves, and the scratch buffer is synced so leaving Input
  keeps the word. Overwriting a non-empty value has no confirmation.
- **Hint:** `HINT_FORM_PASSWORD_INPUT` is renamed `HINT_FORM_GENERATOR_INPUT` and
  returned for both `Login` and `Password` rows at the Input level. Its text is
  unchanged and accurate before generating and when rerolling. Nav-level hints do
  not mention `Ctrl+G`.

## Coalescing undo

`FormState` gains a `history_coalesce` flag and a `push_history_coalescing`
method: when the top history entry was produced by a generation, the next
generation **replaces** it instead of stacking. One `u` therefore restores the
pre-generation value no matter how many rerolls happened. The flag is cleared by
any other commit and by undo/redo.

The password path is switched to the same coalescing commit so the rule is
uniform; with a single generation the observable behaviour is unchanged. ADR 0012
is updated to say that generation coalesces with a previous generation.

## Why not a modal or settings

A password has parameters (length, symbols) worth remembering for the app run; a
login word has none. Adding a modal with a single "Generate" row, or a length
setting, would be UI with no decision in it. An Add form's first generation is
still the creating commit, which is not undoable; the first reroll after that
starts the coalescing chain.

## Verification

- `src/generator/login.rs` unit tests: lowercase ASCII with no digits or spaces,
  the reachable `4..=13` length range, no doubled letters, no vowel run, blocklist
  ASCII/lowercase and substring matching, a seeded word is deterministic, and no
  generated word is ever blocked.
- `theming::key` tests: `Ctrl+G` at Input maps to `Action::Generate` and plain `g`
  still inserts; `Ctrl+G` stays unbound at Nav.
- `tree::app_tree` tests: `Ctrl+G` on a `Login` row writes a word and leaves the
  form at Input without a modal; it is a no-op on the Name row and on a Login row
  at Nav; two login generations and two password generations each collapse to a
  single undo step; the generator hint is shown for `Login` and `Password` rows
  and stays the plain hint for a `Custom` row.
