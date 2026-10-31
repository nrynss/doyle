//! Canonical keys and ID derivation for the legacy source.
//!
//! The scheme is `docs/legacy-ids.md`: an element ID is `le2_` plus the 64
//! lowercase hex characters of the SHA-256 of its canonical key's UTF-8
//! bytes, and a change ID is `lc2_` plus the SHA-256 of a key built from
//! the subject's `le2_` ID, the change kind and the attribute that changed.
//! Names are COBOL names as the parser uppercases them; line numbers, file
//! order and comments are never in a key. Every ID is computed here, never
//! copied: the story tests recompute each fixture ID from its key.

use sha2::{Digest, Sha256};

use crate::core::{ChangeId, ElementId};

/// The SHA-256 hex of `key`, the single fingerprint behind every legacy ID.
fn fingerprint(key: &str) -> String {
    Sha256::digest(key.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// An element ID from its canonical key: `le2_` plus the key's fingerprint.
pub fn element_id(canonical_key: &str) -> ElementId {
    ElementId(format!("le2_{}", fingerprint(canonical_key)))
}

/// The canonical key of a program element.
pub fn program_key(program: &str) -> String {
    format!("legacy:{program}")
}

/// The canonical key of a paragraph element.
pub fn paragraph_key(program: &str, paragraph: &str) -> String {
    format!("legacy:{program}:para:{paragraph}")
}

/// The canonical key of a section element.
pub fn section_key(program: &str, section: &str) -> String {
    format!("legacy:{program}:section:{section}")
}

/// The canonical key of a copybook record element.
pub fn record_key(program: &str, record: &str) -> String {
    format!("legacy:{program}:record:{record}")
}

/// The canonical key of a copybook field element: `path` is the dot-joined
/// level path from the record down (`PATIENT-RECORD.BALANCE`).
pub fn field_key(program: &str, path: &str) -> String {
    format!("legacy:{program}:field:{path}")
}

/// The canonical key of a file element: `select` is the `SELECT` name.
pub fn file_key(program: &str, select: &str) -> String {
    format!("legacy:{program}:file:{select}")
}

/// The canonical key of a change fact: the subject's element ID plus the
/// change kind and the attribute on either side of the change (`old_attr`
/// and `new_attr` are empty for adds and removes, whose kind and subject
/// already identify them). Fixed here per `docs/legacy-ids.md`: T9.6 owns
/// this derivation, and the story fixtures carry no change IDs to copy.
pub fn change_key(subject: &ElementId, kind: &str, old_attr: &str, new_attr: &str) -> String {
    format!(
        "legacy-change:{}:{}:{}:{}",
        subject.0, kind, old_attr, new_attr
    )
}

/// A change ID from its parts: `lc2_` plus the change key's fingerprint.
pub fn change_id(subject: &ElementId, kind: &str, old_attr: &str, new_attr: &str) -> ChangeId {
    ChangeId(format!(
        "lc2_{}",
        fingerprint(&change_key(subject, kind, old_attr, new_attr))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ids_match_the_worked_examples() {
        // Spot checks against docs/legacy-ids.md: computed, never copied
        // into production code (these literals live only in the test).
        let cases = [
            (
                program_key("BILLBATCH"),
                "le2_be3caaf70f3125fa6cc719da40eb9b356e258afe83ba1527943e65867dead079",
            ),
            (
                paragraph_key("BILLBATCH", "CALC-LATE-FEE"),
                "le2_3422f89619e902b31f7fe202a2fc9a05701bbc7ae83f09cf366989f5e95f749e",
            ),
            (
                record_key("BILLBATCH", "PATIENT-RECORD"),
                "le2_690243c98922cdcded58beed19f46cb8725f9690399b9f1c297cc465c4f21533",
            ),
        ];
        for (key, want) in cases {
            assert_eq!(element_id(&key).0, want, "key {key}");
        }
    }

    #[test]
    fn change_ids_are_prefixed_deterministic_and_distinct() {
        let subject = element_id(&paragraph_key("BILLBATCH", "CALC-LATE-FEE"));
        let first = change_id(&subject, "paragraph-statements-changed", "a", "b");
        let again = change_id(&subject, "paragraph-statements-changed", "a", "b");
        assert_eq!(first, again, "same inputs give the same ID");
        assert!(first.0.starts_with("lc2_"), "lc2_ prefix: {}", first.0);
        assert_eq!(first.0.len(), 4 + 64, "lc2_ plus 64 hex chars");
        assert!(
            first.0[4..].chars().all(|c| c.is_ascii_hexdigit()),
            "lowercase hex"
        );
        let other_kind = change_id(&subject, "paragraph-removed", "", "");
        assert_ne!(first, other_kind, "kind is in the key");
        let other_subject = element_id(&paragraph_key("BILLBATCH", "LOG-EXEMPT"));
        assert_ne!(
            first,
            change_id(&other_subject, "paragraph-statements-changed", "a", "b"),
            "subject is in the key"
        );
    }
}
