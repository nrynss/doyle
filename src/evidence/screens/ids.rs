//! Stable IDs for screens, elements, steps and change facts.
//!
//! The rule mirrors `docs/legacy-ids.md`: an ID derives from what the
//! element *is* — console, screen, kind, text and position for an element;
//! description and span for a step — never from its order in a reply, so a
//! reordered reply indexes identically. Prefixes are `se2_` for elements and
//! `sc2_` for change facts (`ve2_`/`vc2_` stay Vimanam's, `le2_`/`lc2_` the
//! legacy source's). The hash is SHA-256 over the canonical key's UTF-8
//! bytes, implemented here by hand so IDs need no new dependency.

use crate::core::{ChangeId, ElementId};

/// `se2_` plus the SHA-256 of the screen's canonical key.
pub fn screen_id(console: &str, screen: &str) -> ElementId {
    ElementId(format!(
        "se2_{}",
        sha256_hex(canonical_screen_key(console, screen).as_bytes())
    ))
}

/// `se2_` plus the SHA-256 of the element's canonical key: console, screen,
/// kind, normalized text and grid position.
pub fn element_id(
    console: &str,
    screen: &str,
    kind: &str,
    text: &str,
    row: u32,
    col: u32,
) -> ElementId {
    let key = format!(
        "screen:{console}:{screen}:element:{kind}:{}:{row}x{col}",
        normalize_text(text)
    );
    ElementId(format!("se2_{}", sha256_hex(key.as_bytes())))
}

/// `se2_` plus the SHA-256 of the recording's canonical key.
pub fn recording_id(console: &str, recording: &str) -> ElementId {
    ElementId(format!(
        "se2_{}",
        sha256_hex(format!("recording:{console}:{recording}").as_bytes())
    ))
}

/// `se2_` plus the SHA-256 of the step's canonical key: recording,
/// normalized description and span. The span rounds through
/// [`format_seconds`], so `28.0` and `28` key identically.
pub fn step_id(
    console: &str,
    recording: &str,
    description: &str,
    start_s: f64,
    end_s: f64,
) -> ElementId {
    let key = format!(
        "recording:{console}:{recording}:step:{}:{}-{}",
        normalize_text(description),
        format_seconds(start_s),
        format_seconds(end_s)
    );
    ElementId(format!("se2_{}", sha256_hex(key.as_bytes())))
}

/// `sc2_` plus the SHA-256 of the change's canonical key: the subject's ID,
/// the change kind, and the detail digest the differ computed (sorted, so
/// element order never moves the fact ID).
pub fn change_id(subject: &ElementId, kind: &str, detail_digest: &str) -> ChangeId {
    let key = format!("change:{}:{kind}:{detail_digest}", subject.0);
    ChangeId(format!("sc2_{}", sha256_hex(key.as_bytes())))
}

/// The screen's canonical key, exposed so the differ and the tests name the
/// same scope.
pub fn canonical_screen_key(console: &str, screen: &str) -> String {
    format!("screen:{console}:{screen}")
}

/// Upper-case with whitespace collapsed, the T9.2 scoring rule: OCR case
/// and spacing wobble do not move an ID.
pub fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// Canonical seconds for ID keys: integers print bare, fractions print
/// shortest-round-trip.
pub fn format_seconds(seconds: f64) -> String {
    format!("{seconds}")
}

/// SHA-256 of `data`, lowercase hex, FIPS 180-4.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut schedule = [0u32; 64];
        for (i, word) in schedule.iter_mut().enumerate().take(16) {
            let bytes: [u8; 4] = block[4 * i..4 * i + 4]
                .try_into()
                .expect("block slices into 16 words");
            *word = u32::from_be_bytes(bytes);
        }
        for i in 16..64 {
            let small0 = schedule[i - 15].rotate_right(7)
                ^ schedule[i - 15].rotate_right(18)
                ^ (schedule[i - 15] >> 3);
            let small1 = schedule[i - 2].rotate_right(17)
                ^ schedule[i - 2].rotate_right(19)
                ^ (schedule[i - 2] >> 10);
            schedule[i] = schedule[i - 16]
                .wrapping_add(small0)
                .wrapping_add(schedule[i - 7])
                .wrapping_add(small1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let big1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(big1)
                .wrapping_add(choice)
                .wrapping_add(K[i])
                .wrapping_add(schedule[i]);
            let big0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = big0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    let mut hex = String::with_capacity(64);
    for word in state {
        hex.push_str(&format!("{word:08x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn ids_carry_the_screen_prefix() {
        let id = screen_id("fernwhistle", "lookup");
        assert!(id.0.starts_with("se2_"));
        assert_eq!(id.0.len(), 4 + 64);
        assert_eq!(id, screen_id("fernwhistle", "lookup"));
        assert_ne!(id, screen_id("fernwhistle", "decommission"));
        assert_ne!(id, screen_id("other", "lookup"));
    }

    #[test]
    fn element_ids_ignore_case_and_spacing_but_not_position() {
        let a = element_id("c", "s", "label", "TYPE  CLINIC NAME", 10, 4);
        let b = element_id("c", "s", "label", "type clinic name", 10, 4);
        assert_eq!(a, b);
        assert_ne!(a, element_id("c", "s", "label", "TYPE CLINIC NAME", 10, 5));
        assert_ne!(a, element_id("c", "s", "value", "TYPE CLINIC NAME", 10, 4));
    }

    #[test]
    fn step_ids_treat_whole_seconds_canonically() {
        assert_eq!(
            step_id("c", "r", "Type the clinic name", 33.0, 45.0),
            step_id("c", "r", "type  the CLINIC name", 33.0, 45.0)
        );
        assert_ne!(
            step_id("c", "r", "Type the clinic name", 33.0, 45.0),
            step_id("c", "r", "Type the clinic name", 33.0, 44.0)
        );
    }
}
