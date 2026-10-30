//! The Doyle mark: a magnifying glass over tangled threads — stale-memory
//! detection as detective work.
//!
//! Inline SVG, drawn only with `currentColor` strokes and fills so it takes
//! the header's colour in both the light and the dark theme. No text is baked
//! into the paths; the wordmark beside it is ordinary HTML. The thread paths
//! stay at reduced opacity behind the lens so the glass reads first, and the
//! small loop inside the lens is the one thread pulled straight. It holds up
//! at 16–32 px: two stroke weights, no fine detail.

/// The logo as an inline SVG element, sized by the caller's CSS.
pub const DOYLE_LOGO: &str = concat!(
    r##"<svg class="logo" viewBox="0 0 32 32" width="30" height="30" aria-hidden="true" focusable="false">"##,
    // Tangled threads behind the glass.
    r##"<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" opacity="0.45">"##,
    r##"<path d="M2 21 C7 27, 10 8, 17 13 S27 21, 30 14"/>"##,
    r##"<path d="M4 5 C11 11, 8 25, 16 27"/>"##,
    r##"<path d="M8 29 C13 22, 24 26, 29 27"/>"##,
    r##"</g>"##,
    // The one thread pulled straight, inside the lens.
    r##"<path d="M14.5 13.5 C16 10.8, 20 10.8, 21.5 13.2 C22.6 15, 21 17, 19 16.6" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>"##,
    r##"<circle cx="17.6" cy="13.4" r="1.15" fill="currentColor"/>"##,
    // The magnifying glass.
    r##"<circle cx="18.5" cy="13" r="7.2" fill="none" stroke="currentColor" stroke-width="2.2"/>"##,
    r##"<line x1="23.8" y1="18.3" x2="28.6" y2="24.6" stroke="currentColor" stroke-width="2.7" stroke-linecap="round"/>"##,
    r##"</svg>"##,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_is_theme_aware_and_text_free() {
        // Only currentColor paint: nothing hard-codes a light or dark colour.
        assert!(DOYLE_LOGO.contains("currentColor"));
        for hard in ["#", "rgb(", "fill=\"black", "fill=\"white"] {
            assert!(
                !DOYLE_LOGO.contains(hard),
                "logo must not hard-code paint: {hard}"
            );
        }
        // No text baked into the mark.
        assert!(!DOYLE_LOGO.contains("<text"));
        assert!(DOYLE_LOGO.starts_with("<svg"));
        assert!(DOYLE_LOGO.ends_with("</svg>"));
    }
}
