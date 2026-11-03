//! Password generation parameters and the generator itself.
//!
//! The public surface is small on purpose:
//!
//! - [`PasswordLength`] is a validated length (`4..=128`); invalid lengths are
//!   unrepresentable, so [`generate_password`] never has to fail.
//! - [`SymbolSet`] is a fixed selection over the canonical [`SYMBOLS`] alphabet.
//! - [`PasswordParams`] bundles the two, and [`generate_password`] turns a
//!   `PasswordParams` into a password of exactly that length.

use std::fmt;
use std::str::FromStr;

use rand::Rng;
use rand::seq::SliceRandom;
use zeroize::Zeroize;

/// The canonical special-character alphabet offered by the generator.
pub const SYMBOLS: [char; 17] = [
    '!', '~', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '=', '-', '/', '<', '>',
];

// `SymbolSet` stores the selection in a `u32` bitmask built from
// `(1u32 << SYMBOLS.len()) - 1`, so the alphabet must stay below 32 entries:
// shifting a `u32` by 32 overflows. This makes a `SYMBOLS` that outgrows the
// mask a compile error rather than a panic in debug and a silently empty set in
// release at `Ctrl+G`.
const _: () = assert!(SYMBOLS.len() < u32::BITS as usize);

/// Lower- and upper-case ASCII letters, the fallback for every non-symbol,
/// non-digit position.
const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// ASCII digits, guaranteed to appear at least once in every password.
const DIGITS: &[u8] = b"0123456789";

/// A validated password length.
///
/// The inner value is always within `MIN..=MAX`, so a `PasswordLength` can be
/// passed to [`generate_password`] without further checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PasswordLength(u8);

impl PasswordLength {
    /// The shortest allowed password.
    pub const MIN: u8 = 4;
    /// The longest allowed password.
    pub const MAX: u8 = 128;
    /// The length used when no other value has been chosen.
    pub const DEFAULT: Self = Self(10);

    /// The validated length.
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl Default for PasswordLength {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for PasswordLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a value could not become a [`PasswordLength`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordLengthError {
    /// The input contained no digits or was not a number.
    NotANumber,
    /// The parsed number is outside `PasswordLength::MIN..=PasswordLength::MAX`.
    OutOfRange,
}

impl fmt::Display for PasswordLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotANumber => f.write_str("password length must be a whole number"),
            Self::OutOfRange => write!(
                f,
                "password length must be between {} and {}",
                PasswordLength::MIN,
                PasswordLength::MAX
            ),
        }
    }
}

impl std::error::Error for PasswordLengthError {}

impl TryFrom<usize> for PasswordLength {
    type Error = PasswordLengthError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        let Ok(value) = u8::try_from(value) else {
            return Err(PasswordLengthError::OutOfRange);
        };
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(PasswordLengthError::OutOfRange)
        }
    }
}

impl FromStr for PasswordLength {
    type Err = PasswordLengthError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PasswordLengthError::NotANumber);
        }
        // A very long digit string overflows `usize`; treat that as out of range
        // rather than "not a number" because every character was a digit.
        let value: usize = trimmed
            .parse()
            .map_err(|_| PasswordLengthError::OutOfRange)?;
        Self::try_from(value)
    }
}

/// A fixed selection over the canonical [`SYMBOLS`] alphabet.
///
/// Stored as a bitmask wide enough for all [`SYMBOLS`]. Unknown characters are
/// ignored by the mutating methods, so the UI can pass a symbol straight from
/// the checklist without indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolSet(u32);

impl SymbolSet {
    /// Every symbol selected. This is the domain default.
    pub const fn all() -> Self {
        Self((1u32 << SYMBOLS.len()) - 1)
    }

    /// No symbols selected.
    pub const fn none() -> Self {
        Self(0)
    }

    /// Whether no symbol is selected.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The number of selected symbols.
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// Whether `symbol` is selected.
    pub fn contains(self, symbol: char) -> bool {
        Self::index_of(symbol).is_some_and(|index| self.0 & (1 << index) != 0)
    }

    /// Adds or removes `symbol` from the selection.
    ///
    /// Unknown characters are ignored.
    pub fn toggle(&mut self, symbol: char) {
        if let Some(index) = Self::index_of(symbol) {
            self.0 ^= 1 << index;
        }
    }

    /// Selects or deselects `symbol`.
    ///
    /// Unknown characters are ignored.
    pub fn set(&mut self, symbol: char, selected: bool) {
        if let Some(index) = Self::index_of(symbol) {
            if selected {
                self.0 |= 1 << index;
            } else {
                self.0 &= !(1 << index);
            }
        }
    }

    /// Selects every symbol when the set is incomplete, otherwise clears it.
    pub fn toggle_all(&mut self) {
        *self = if self.len() == SYMBOLS.len() {
            Self::none()
        } else {
            Self::all()
        };
    }

    /// Iterates over the selected symbols in canonical order.
    pub fn iter(self) -> impl Iterator<Item = char> {
        SYMBOLS
            .into_iter()
            .filter(move |symbol| self.contains(*symbol))
    }

    fn index_of(symbol: char) -> Option<usize> {
        SYMBOLS.iter().position(|&candidate| candidate == symbol)
    }
}

impl Default for SymbolSet {
    fn default() -> Self {
        Self::all()
    }
}

/// The parameters a generated password is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PasswordParams {
    /// The exact number of characters to generate.
    pub length: PasswordLength,
    /// The special characters that may appear.
    pub symbols: SymbolSet,
}

/// Generates a password of exactly `params.length` characters.
///
/// The budget is split into digits, symbols (when any are selected) and
/// letters, each at least one and always summing exactly to the requested
/// length. Digits and symbols are filled **with replacement**, so a class
/// budget larger than its alphabet is fine, and the whole password is shuffled
/// before it is returned.
pub fn generate_password(params: &PasswordParams) -> String {
    generate_with(params, &mut rand::thread_rng())
}

/// The pure core of [`generate_password`], generic over the RNG for tests.
fn generate_with<R: Rng + ?Sized>(params: &PasswordParams, rng: &mut R) -> String {
    let length = params.length.value() as usize;
    let symbols_enabled = !params.symbols.is_empty();
    let (letters, digits, symbols) = split_budget(length, symbols_enabled);

    let mut chars: Vec<char> = Vec::with_capacity(length);
    for _ in 0..letters {
        chars.push(*LETTERS.choose(rng).expect("letters alphabet is non-empty") as char);
    }
    for _ in 0..digits {
        chars.push(*DIGITS.choose(rng).expect("digits alphabet is non-empty") as char);
    }
    if symbols_enabled {
        let alphabet: Vec<char> = params.symbols.iter().collect();
        for _ in 0..symbols {
            chars.push(*alphabet.choose(rng).expect("symbol set is non-empty"));
        }
    }
    chars.shuffle(rng);
    // The returned `String` is owned by the caller and cannot be wiped here, but
    // this second full copy of the secret can — and is.
    let password: String = chars.iter().copied().collect();
    chars.zeroize();
    password
}

/// Splits `length` into `(letters, digits, symbols)` budgets.
///
/// Digits and symbols each take a rounded 20% of the full length (never less
/// than one); letters take the exact remainder. With symbols disabled the
/// symbol budget is zero and its share goes to letters, so the parts still sum
/// to `length`.
fn split_budget(length: usize, symbols_enabled: bool) -> (usize, usize, usize) {
    let ratio = ratio_budget(length);
    let symbols = if symbols_enabled { ratio } else { 0 };
    let digits = ratio;
    (length - digits - symbols, digits, symbols)
}

/// `round(0.2 * length)`, computed with integer arithmetic.
///
/// The fractional part of `length / 5` is a multiple of `0.2`, so it never ties
/// at `0.5` and `(length + 2) / 5` is exact.
fn ratio_budget(length: usize) -> usize {
    (length + 2) / 5
}

#[cfg(test)]
mod tests {
    use super::*;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn params(length: u8, symbols: SymbolSet) -> PasswordParams {
        PasswordParams {
            length: PasswordLength::try_from(usize::from(length)).expect("valid test length"),
            symbols,
        }
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(0xC0FFEE)
    }

    #[test]
    fn generates_exact_length_at_old_overflow_points() {
        for length in [4u8, 10, 51, 63, 82, 128] {
            let password = generate_with(&params(length, SymbolSet::all()), &mut rng());
            assert_eq!(
                password.chars().count(),
                length as usize,
                "wrong length for {length}"
            );
        }
    }

    #[test]
    fn every_character_comes_from_the_selected_alphabet() {
        let mut allowed: Vec<char> = LETTERS.iter().map(|&b| b as char).collect();
        allowed.extend(DIGITS.iter().map(|&b| b as char));
        allowed.extend(SymbolSet::all().iter());
        // Drop the two symbols we will deselect.
        allowed.retain(|c| *c != '!' && *c != '~');

        let mut symbols = SymbolSet::all();
        symbols.set('!', false);
        symbols.set('~', false);

        for _ in 0..64 {
            let password = generate_with(&params(40, symbols), &mut rng());
            for c in password.chars() {
                assert!(allowed.contains(&c), "unexpected character {c:?}");
            }
        }
    }

    #[test]
    fn symbols_enabled_guarantees_a_digit_and_a_symbol() {
        for _ in 0..64 {
            let password = generate_with(&params(4, SymbolSet::all()), &mut rng());
            assert!(password.chars().any(|c| c.is_ascii_digit()));
            assert!(password.chars().any(|c| SYMBOLS.contains(&c)));
        }
    }

    #[test]
    fn symbols_disabled_produces_no_symbols_but_keeps_a_digit() {
        for _ in 0..64 {
            let password = generate_with(&params(6, SymbolSet::none()), &mut rng());
            assert_eq!(password.chars().count(), 6);
            assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
            assert!(password.chars().any(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn a_single_symbol_set_only_emits_that_symbol() {
        let mut symbols = SymbolSet::none();
        symbols.set('#', true);
        assert_eq!(symbols.len(), 1);

        for _ in 0..64 {
            let password = generate_with(&params(40, symbols), &mut rng());
            assert!(
                password
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '#'),
                "unexpected character in {password:?}"
            );
            assert!(password.chars().any(|c| c == '#'));
        }
    }

    #[test]
    fn budget_always_sums_to_length_with_at_least_one_each() {
        for length in PasswordLength::MIN..=PasswordLength::MAX {
            let len = length as usize;
            let (letters, digits, symbols) = split_budget(len, true);
            assert_eq!(letters + digits + symbols, len);
            assert!(letters >= 1 && digits >= 1 && symbols >= 1);

            let (letters, digits, symbols) = split_budget(len, false);
            assert_eq!(letters + digits + symbols, len);
            assert!(letters >= 1 && digits >= 1 && symbols == 0);
        }
    }

    #[test]
    fn ratio_budget_matches_rounded_twenty_percent() {
        for length in 0..=256usize {
            let expected = ((length as f64) * 0.2).round() as usize;
            assert_eq!(ratio_budget(length), expected, "length {length}");
        }
    }

    #[test]
    fn password_length_rejects_out_of_range_and_accepts_bounds() {
        for bad in [0usize, 3, 129, 200, 300, usize::MAX] {
            assert_eq!(
                PasswordLength::try_from(bad),
                Err(PasswordLengthError::OutOfRange)
            );
        }
        for good in [4usize, 10, 128] {
            assert_eq!(
                PasswordLength::try_from(good).map(PasswordLength::value),
                Ok(good as u8)
            );
        }
    }

    #[test]
    fn password_length_from_str_parses_digits_and_normalizes() {
        assert_eq!("10".parse::<PasswordLength>(), Ok(PasswordLength(10)));
        assert_eq!("007".parse::<PasswordLength>(), Ok(PasswordLength(7)));
        assert_eq!("4".parse::<PasswordLength>(), Ok(PasswordLength(4)));
        assert_eq!("128".parse::<PasswordLength>(), Ok(PasswordLength(128)));
        assert_eq!(
            "999".parse::<PasswordLength>(),
            Err(PasswordLengthError::OutOfRange)
        );
        // A digit string too long for `usize` is still all digits, so it is out
        // of range rather than not-a-number.
        assert_eq!(
            "99999999999999999999999".parse::<PasswordLength>(),
            Err(PasswordLengthError::OutOfRange)
        );
        assert_eq!(" 10 ".parse::<PasswordLength>(), Ok(PasswordLength(10)));
    }

    #[test]
    fn password_length_from_str_rejects_non_numbers() {
        assert_eq!(
            "".parse::<PasswordLength>(),
            Err(PasswordLengthError::NotANumber)
        );
        assert_eq!(
            "abc".parse::<PasswordLength>(),
            Err(PasswordLengthError::NotANumber)
        );
        assert_eq!(
            "1a".parse::<PasswordLength>(),
            Err(PasswordLengthError::NotANumber)
        );
    }

    #[test]
    fn symbol_set_defaults_to_all() {
        assert_eq!(SymbolSet::default(), SymbolSet::all());
        assert_eq!(SymbolSet::all().len(), SYMBOLS.len());
        assert_eq!(SymbolSet::none().len(), 0);
        assert!(SymbolSet::none().is_empty());
    }

    #[test]
    fn symbol_set_toggles_set_contains_and_iterates() {
        let mut set = SymbolSet::none();
        assert!(!set.contains('!'));
        set.toggle('!');
        assert!(set.contains('!'));
        assert_eq!(set.len(), 1);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec!['!']);
        set.set('!', false);
        assert!(set.is_empty());

        set.set('#', true);
        set.set('$', true);
        assert_eq!(set.iter().collect::<Vec<_>>(), vec!['#', '$']);
        // Unknown characters are ignored.
        set.toggle('☃');
        set.set('☃', true);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn symbol_set_toggle_all_fills_then_clears() {
        let mut set = SymbolSet::none();
        set.toggle_all();
        assert_eq!(set, SymbolSet::all());
        set.toggle_all();
        assert!(set.is_empty());
    }
}
