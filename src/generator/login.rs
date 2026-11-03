//! Login word generation.
//!
//! A login word is a single invented lowercase `a–z` word with no digits and no
//! spaces — "a fictional name, not explicitly a human name, just a name"
//! (e.g. `brindel`, `quoravel`, `velmora`). It is assembled from syllables so
//! the result stays pronounceable, unlike an opaque random string.
//!
//! The public surface is a single infallible function: a login word has no
//! parameters, so [`generate_login`] returns a `String` and never fails. See
//! `docs/adr/0013-login-word-generator.md`.

use rand::Rng;
use rand::seq::SliceRandom;

/// Syllables in the shortest word.
const MIN_SYLLABLES: usize = 2;
/// Syllables in the longest word.
const MAX_SYLLABLES: usize = 3;

/// Every onset, single consonants first and two-letter clusters last.
///
/// `c`, `q`, `x` and `y` never appear alone: `ch` and `qu` are the only onsets
/// that carry them, which keeps pronunciation consistent.
const ONSETS: &[&str] = &[
    "b", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w",
    "z", // singles
    "br", "dr", "fr", "gr", "kr", "pr", "tr", "sh", "th", "ch", "qu", "pl", "fl", "bl", "gl", "kl",
    "sl", // clusters
];

/// The vowels a syllable core may use.
const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u'];

/// Codas a syllable may close on.
const CODAS: &[&str] = &["l", "m", "n", "r", "s", "t", "sh", "th"];

/// Chance that a syllable takes a coda when one is allowed.
const CODA_CHANCE: f64 = 0.5;

/// Substrings that make a candidate unacceptable.
///
/// Entries are ASCII lowercase transliterations, matched as substrings of the
/// finished (also lowercase ASCII) word. The list is deliberately small and
/// trivially extended; see [`is_blocked`] for the case-folding rule. The
/// alphabet can never produce a non-ASCII sequence, so a Cyrillic entry would be
/// dead weight and must not be added.
const BLOCKLIST: &[&str] = &["bitch", "fuk", "nig", "piss", "shit", "slut", "tits"];

/// How many replacement words are tried before the last candidate is returned.
///
/// Bounds the retry loop so [`generate_login`] can neither hang nor panic when a
/// (future, larger) blocklist rejects every attempt.
const MAX_ATTEMPTS: usize = 32;

/// Generates a single invented lowercase word.
///
/// The word is either two or three syllables and is always between 4 and 13
/// characters: onsets are one or two letters, the core is one vowel and the
/// coda is optional and at most two letters.
pub fn generate_login() -> String {
    generate_login_with(&mut rand::thread_rng())
}

/// The pure core of [`generate_login`], generic over the RNG for tests.
fn generate_login_with<R: Rng + ?Sized>(rng: &mut R) -> String {
    let mut candidate = build_word(rng);
    for _ in 0..MAX_ATTEMPTS {
        if !is_blocked(&candidate) {
            return candidate;
        }
        candidate = build_word(rng);
    }
    // The bound was hit; return the last candidate rather than looping forever.
    candidate
}

/// Assembles one word that satisfies every boundary rule by construction.
///
/// The onsets are chosen up front so a medial coda can be checked against the
/// *next* onset: a coda is only allowed before a single-consonant onset, and
/// never when it would double that onset's first letter. Because each syllable
/// has exactly one vowel and consecutive vowels are excluded, two adjacent
/// syllables can never be identical.
fn build_word<R: Rng + ?Sized>(rng: &mut R) -> String {
    let count = rng.gen_range(MIN_SYLLABLES..=MAX_SYLLABLES);
    let onsets: Vec<&str> = (0..count).map(|_| pick_onset(rng)).collect();

    let mut word = String::new();
    let mut previous_vowel: Option<char> = None;
    for (index, onset) in onsets.iter().enumerate() {
        let vowel = pick_vowel(rng, previous_vowel, onset);
        previous_vowel = Some(vowel);
        word.push_str(onset);
        word.push(vowel);
        word.push_str(pick_coda(rng, &onsets, index));
    }
    word
}

/// Picks a single onset or a cluster with equal probability.
fn pick_onset<R: Rng + ?Sized>(rng: &mut R) -> &'static str {
    ONSETS
        .choose(rng)
        .copied()
        .expect("the onset inventory is non-empty")
}

/// Whether `onset` is a two-letter cluster rather than a single consonant.
fn is_cluster(onset: &str) -> bool {
    onset.chars().count() > 1
}

/// Picks the syllable core, avoiding a run of the same vowel.
///
/// The vowel is also kept off the onset's tail: `qu` is the only onset ending
/// in a vowel, and `quu` would double a letter inside the syllable.
fn pick_vowel<R: Rng + ?Sized>(rng: &mut R, previous: Option<char>, onset: &str) -> char {
    let tail = onset.chars().last().expect("an onset is never empty");
    let allowed: Vec<char> = VOWELS
        .iter()
        .copied()
        .filter(|vowel| Some(*vowel) != previous && *vowel != tail)
        .collect();
    *allowed
        .choose(rng)
        .expect("at least one vowel is always allowed")
}

/// Picks an optional coda for the syllable at `index`, or an empty string.
///
/// The final syllable is always eligible. A medial syllable is eligible only
/// when the next onset is a single consonant, and codas that would double that
/// onset's first letter are excluded.
fn pick_coda<R: Rng + ?Sized>(rng: &mut R, onsets: &[&str], index: usize) -> &'static str {
    let allowed: Vec<&str> = match onsets.get(index + 1) {
        None => CODAS.to_vec(),
        Some(next) if is_cluster(next) => Vec::new(),
        Some(next) => {
            let first = next.chars().next().expect("an onset is never empty");
            CODAS
                .iter()
                .copied()
                .filter(|coda| !coda.ends_with(first))
                .collect()
        }
    };

    if allowed.is_empty() || !rng.gen_bool(CODA_CHANCE) {
        return "";
    }
    allowed
        .choose(rng)
        .copied()
        .expect("a coda is allowed here")
}

/// Whether `word` contains any blocklisted substring.
///
/// The word and the blocklist are both ASCII lowercase, so a plain `contains` is
/// already a case-insensitive match; there is no non-ASCII text to fold.
fn is_blocked(word: &str) -> bool {
    BLOCKLIST.iter().any(|entry| word.contains(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// A deterministic generator so rule checks are reproducible.
    fn rng() -> StdRng {
        StdRng::seed_from_u64(0x10_600D)
    }

    fn words(count: usize) -> Vec<String> {
        let mut rng = rng();
        (0..count).map(|_| generate_login_with(&mut rng)).collect()
    }

    #[test]
    fn every_word_is_lowercase_ascii_with_no_digits_or_spaces() {
        for word in words(512) {
            assert!(!word.is_empty());
            assert!(
                word.chars().all(|c| c.is_ascii_lowercase()),
                "unexpected character in {word:?}"
            );
        }
    }

    #[test]
    fn every_word_is_within_the_reachable_length_range() {
        let mut shortest = usize::MAX;
        let mut longest = 0;
        for word in words(50_000) {
            let length = word.chars().count();
            assert!(
                (4..=13).contains(&length),
                "length {length} out of range for a login word: {word:?}"
            );
            shortest = shortest.min(length);
            longest = longest.max(length);
        }
        // Both bounds are reachable, not merely permitted: a generator narrowed
        // to a middle band would fail here.
        assert_eq!(shortest, 4, "the shortest word is never generated");
        assert_eq!(longest, 13, "the longest word is never generated");
    }

    #[test]
    fn words_start_with_an_onset() {
        for word in words(256) {
            let first = word.chars().next().expect("word is non-empty");
            assert!(
                ONSETS.iter().any(|onset| onset.starts_with(first)),
                "word does not start with an onset: {word:?}"
            );
        }
    }

    #[test]
    fn no_adjacent_letters_are_doubled() {
        for word in words(512) {
            let mut previous = None;
            for c in word.chars() {
                assert_ne!(previous, Some(c), "doubled letter in {word:?}");
                previous = Some(c);
            }
        }
    }

    #[test]
    fn no_run_of_the_same_vowel() {
        for word in words(512) {
            let chars: Vec<char> = word.chars().collect();
            for pair in chars.windows(2) {
                if pair[0] == pair[1] && VOWELS.contains(&pair[0]) {
                    panic!("vowel run in {word:?}");
                }
            }
        }
    }

    #[test]
    fn pick_coda_never_returns_a_coda_before_a_consonant_cluster() {
        let onsets = ["b", "br"];
        let mut rng = rng();
        for _ in 0..256 {
            assert_eq!(pick_coda(&mut rng, &onsets, 0), "");
        }
    }

    #[test]
    fn pick_coda_never_doubles_the_next_onset_first_letter() {
        let onsets = ["b", "l"];
        let mut rng = rng();
        for _ in 0..256 {
            let coda = pick_coda(&mut rng, &onsets, 0);
            assert!(!coda.ends_with('l'), "coda {coda:?} doubles the next onset");
        }
    }

    #[test]
    fn pick_coda_can_produce_a_legal_medial_coda() {
        let onsets = ["b", "m"];
        let mut rng = rng();
        let saw_coda = (0..256).any(|_| !pick_coda(&mut rng, &onsets, 0).is_empty());
        assert!(saw_coda, "a medial coda is never produced");
    }

    #[test]
    fn inventories_have_no_duplicates_or_empty_entries() {
        for inventory in [ONSETS, CODAS] {
            for entry in inventory {
                assert!(!entry.is_empty(), "empty entry in {inventory:?}");
            }
            assert_unique(inventory);
        }
        assert!(!VOWELS.is_empty());
        assert_unique(VOWELS);
    }

    fn assert_unique<T: Ord + Copy + std::fmt::Debug>(inventory: &[T]) {
        let mut sorted = inventory.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "duplicate entry in {inventory:?}");
    }

    #[test]
    fn blocklist_entries_are_lowercase_ascii_and_non_empty() {
        for entry in BLOCKLIST {
            assert!(!entry.is_empty());
            assert!(
                entry.chars().all(|c| c.is_ascii_lowercase()),
                "blocklist entry {entry:?} is not a lowercase ASCII transliteration"
            );
        }
    }

    #[test]
    fn blocklist_rejects_known_substrings_and_accepts_plain_words() {
        assert!(is_blocked("shit"));
        assert!(is_blocked("shitsa"));
        assert!(!is_blocked("brindel"));
        assert!(!is_blocked("quoravel"));
    }

    #[test]
    fn generation_never_produces_a_blocked_word() {
        for word in words(512) {
            assert!(!is_blocked(&word), "generated a blocked word: {word:?}");
        }
    }

    #[test]
    fn generation_is_deterministic_for_a_given_seed() {
        let first = generate_login_with(&mut rng());
        let second = generate_login_with(&mut rng());
        assert_eq!(first, second);
    }
}
