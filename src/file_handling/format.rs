//! The serialised-document formats the gateway imports and exports.
//!
//! Import detects a format from a file extension; export turns a format into
//! one. Both directions live on [`Format`] so the extension table has a single
//! source of truth.

use std::path::Path;

/// A serialised-document format accepted by import and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The cretopass-native JSON document.
    Json,
    /// The XML interchange document.
    Xml,
    /// The flattened CSV interchange document.
    Csv,
}

impl Format {
    /// The extension a document of this format is written with.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Csv => "csv",
        }
    }

    /// The format a path's extension selects, or `None` when it is not one of
    /// the supported extensions.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// Encodes one node name as a single `path` segment of a flattened CSV export.
///
/// The path column joins segments with `" > "`, so a name that itself contains
/// `" > "` would otherwise be re-interpreted as nesting on import. Escaping `\`,
/// `#`, and `>` guarantees two properties the reader relies on:
///
/// - every `>` inside a segment is preceded by `\`, so `" > "` can only be the
///   real separator; and
/// - every `#` inside a segment is preceded by `\`, so an *unescaped* `#` marks
///   the occurrence suffix [`encode_path_segment`]'s caller appends for
///   same-named siblings (see [`decode_path_segment`]).
pub(crate) fn encode_path_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '#' => out.push_str("\\#"),
            '>' => out.push_str("\\>"),
            _ => out.push(c),
        }
    }
    out
}

/// Decodes one `path` segment back to its display name.
///
/// Drops a trailing `#<n>` occurrence suffix (emitted by this module's writer
/// for the second and later same-named siblings) and then undoes the escaping
/// applied by [`encode_path_segment`]. The suffix is only recognised when the
/// `#` is unescaped and followed by digits, so foreign input is not truncated
/// arbitrarily.
///
/// One documented edge remains: an older file whose *intermediate* node name
/// ends in `#<digits>` (and has no row of its own) is decoded as the part before
/// the `#`. A node's own row is never affected, because its name comes from the
/// authoritative `name` column.
pub(crate) fn decode_path_segment(segment: &str) -> String {
    let (name, _) = split_occurrence_suffix(segment);
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('#') => out.push('#'),
            Some('>') => out.push('>'),
            // An escape we never emit (a legacy name's stray `\`), preserved.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits `segment` at its first unescaped `#<digits>`, returning the encoded
/// name and the occurrence suffix (without the `#`) when one is present.
fn split_occurrence_suffix(segment: &str) -> (&str, Option<&str>) {
    let mut chars = segment.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                // Skip the escaped character, whatever it is.
                chars.next();
            }
            '#' => {
                let suffix = &segment[i + 1..];
                if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                    return (&segment[..i], Some(suffix));
                }
                // An unescaped `#` that is not an occurrence marker is part of
                // the name; our writer only ever appends one suffix, at the end.
                return (segment, None);
            }
            _ => {}
        }
    }
    (segment, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_round_trips_through_a_path_segment() {
        assert_eq!(encode_path_segment("Login"), "Login");
        assert_eq!(decode_path_segment(&encode_path_segment("Login")), "Login");
    }

    #[test]
    fn the_path_separator_inside_a_name_is_escaped() {
        assert_eq!(encode_path_segment("A > B"), "A \\> B");
        assert_eq!(decode_path_segment("A \\> B"), "A > B");
        // The encoding never produces a bare `" > "` inside a segment.
        assert!(!encode_path_segment("A > B").contains(" > "));
    }

    #[test]
    fn a_backslash_and_hash_round_trip() {
        for name in ["a\\b", "a#b", "a\\#b", "#2", "a > b#3"] {
            let encoded = encode_path_segment(name);
            assert_eq!(decode_path_segment(&encoded), name, "{encoded:?}");
        }
    }

    #[test]
    fn an_occurrence_suffix_is_dropped_but_a_literal_hash_is_kept() {
        // `#2` appended after the encoded name disambiguates a sibling...
        assert_eq!(decode_path_segment("Dup#2"), "Dup");
        // ...while a `#` that is part of the name stays escaped and survives.
        assert_eq!(decode_path_segment("Dup\\#2"), "Dup#2");
        assert_eq!(decode_path_segment("Dup\\#2#2"), "Dup#2");
    }

    #[test]
    fn an_unescaped_hash_not_followed_by_digits_is_kept() {
        assert_eq!(decode_path_segment("Tag#"), "Tag#");
        assert_eq!(decode_path_segment("Tag#extra"), "Tag#extra");
        // Documented legacy edge: `#<digits>` cannot be told from a suffix.
        assert_eq!(decode_path_segment("Tag#1"), "Tag");
    }
}
