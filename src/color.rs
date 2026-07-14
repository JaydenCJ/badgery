//! shields.io color handling: the named palette, semantic aliases, hex
//! parsing, and the brightness rule that decides black-on-light vs
//! white-with-shadow-on-dark text. Hex values below are the exact ones
//! shields.io ships, so a badge rendered by badgery is pixel-identical in
//! color to its shields counterpart.

/// The classic shields palette (name → hex), plus the semantic aliases
/// shields accepts (`critical`, `success`, …).
pub const NAMED: &[(&str, &str)] = &[
    ("brightgreen", "#4c1"),
    ("green", "#97ca00"),
    ("yellowgreen", "#a4a61d"),
    ("yellow", "#dfb317"),
    ("orange", "#fe7d37"),
    ("red", "#e05d44"),
    ("blue", "#007ec6"),
    ("grey", "#555"),
    ("lightgrey", "#9f9f9f"),
    ("gray", "#555"),
    ("lightgray", "#9f9f9f"),
    ("critical", "#e05d44"),
    ("important", "#fe7d37"),
    ("success", "#4c1"),
    ("informational", "#007ec6"),
    ("inactive", "#9f9f9f"),
];

/// Default right-side (message) background: shields `lightgrey`.
pub const DEFAULT_COLOR: &str = "#9f9f9f";
/// Default left-side (label) background: shields `grey`.
pub const DEFAULT_LABEL_COLOR: &str = "#555";
/// Background used for `isError` endpoint badges: shields `red`.
pub const ERROR_COLOR: &str = "#e05d44";

/// Resolve a user-supplied color to a normalized `#hex` string.
///
/// Accepts palette names, aliases, and 3- or 6-digit hex with or without a
/// leading `#` (`4c1`, `#4c1`, `007EC6` all work — shields URL syntax omits
/// the `#`). Returns `None` for anything else.
pub fn resolve(input: &str) -> Option<String> {
    let lower = input.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if let Some((_, hex)) = NAMED.iter().find(|(name, _)| *name == lower) {
        return Some((*hex).to_string());
    }
    let digits = lower.strip_prefix('#').unwrap_or(&lower);
    if (digits.len() == 3 || digits.len() == 6) && digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Some(format!("#{digits}"));
    }
    None
}

/// Resolve with the shields fallback: an unrecognized color renders the
/// badge with the default rather than failing the pipeline. Callers that
/// want strictness use [`resolve`] directly.
pub fn resolve_or_default(input: &str, default: &str) -> String {
    resolve(input).unwrap_or_else(|| default.to_string())
}

/// Perceived brightness in `0.0..=1.0` (ITU-R BT.601 luma, the same formula
/// shields uses). `hex` must be a normalized `#rgb`/`#rrggbb` string.
pub fn brightness(hex: &str) -> f64 {
    let (r, g, b) = rgb(hex);
    (f64::from(r) * 299.0 + f64::from(g) * 587.0 + f64::from(b) * 114.0) / 255_000.0
}

/// Text treatment for a given background: `(fill, shadow)`.
///
/// Light backgrounds get near-black text and no shadow; dark backgrounds get
/// white text with the classic `#010101` drop shadow. The 0.69 threshold is
/// shields' own.
pub fn text_for(background_hex: &str) -> (&'static str, Option<&'static str>) {
    if brightness(background_hex) <= 0.69 {
        ("#fff", Some("#010101"))
    } else {
        ("#333", None)
    }
}

fn rgb(hex: &str) -> (u8, u8, u8) {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    let expand = |b: u8| {
        let d = hex_digit(b);
        d * 16 + d
    };
    match digits.len() {
        3 => {
            let bytes = digits.as_bytes();
            (expand(bytes[0]), expand(bytes[1]), expand(bytes[2]))
        }
        6 => {
            let bytes = digits.as_bytes();
            (
                hex_digit(bytes[0]) * 16 + hex_digit(bytes[1]),
                hex_digit(bytes[2]) * 16 + hex_digit(bytes[3]),
                hex_digit(bytes[4]) * 16 + hex_digit(bytes[5]),
            )
        }
        _ => (0, 0, 0),
    }
}

fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_palette_resolves_to_shields_hex() {
        assert_eq!(resolve("brightgreen").as_deref(), Some("#4c1"));
        assert_eq!(resolve("blue").as_deref(), Some("#007ec6"));
        assert_eq!(resolve("lightgrey").as_deref(), Some("#9f9f9f"));
    }

    #[test]
    fn semantic_aliases_and_spelling_variants_map_to_palette_colors() {
        assert_eq!(resolve("critical"), resolve("red"));
        assert_eq!(resolve("success"), resolve("brightgreen"));
        assert_eq!(resolve("informational"), resolve("blue"));
        assert_eq!(resolve("inactive"), resolve("lightgrey"));
        assert_eq!(resolve("gray"), resolve("grey"));
        assert_eq!(resolve("lightgray"), resolve("lightgrey"));
    }

    #[test]
    fn hex_with_and_without_hash_normalizes_lowercase() {
        assert_eq!(resolve("4C1").as_deref(), Some("#4c1"));
        assert_eq!(resolve("#007EC6").as_deref(), Some("#007ec6"));
        assert_eq!(resolve("abcdef").as_deref(), Some("#abcdef"));
    }

    #[test]
    fn junk_colors_are_rejected_and_fall_back_like_shields() {
        for bad in ["", "reddish", "#12", "#1234", "12345g", "rgb(1,2,3)"] {
            assert_eq!(resolve(bad), None, "{bad:?} should not resolve");
        }
        assert_eq!(resolve_or_default("nope", DEFAULT_COLOR), DEFAULT_COLOR);
        assert_eq!(resolve_or_default("red", DEFAULT_COLOR), "#e05d44");
    }

    #[test]
    fn brightness_orders_dark_to_light() {
        assert!(brightness("#000") < brightness("#555"));
        assert!(brightness("#555") < brightness("#dfb317"));
        assert!(brightness("#dfb317") < brightness("#fff"));
        // 3-digit hex expands before the brightness math.
        assert!((brightness("#4c1") - brightness("#44cc11")).abs() < 1e-9);
    }

    #[test]
    fn dark_backgrounds_get_white_text_with_shadow() {
        assert_eq!(text_for("#555"), ("#fff", Some("#010101")));
        assert_eq!(text_for("#e05d44"), ("#fff", Some("#010101")));
        // shields' yellow sits just *under* the 0.69 threshold — it keeps
        // white text, a classic off-by-a-shade trap when reimplementing.
        assert_eq!(text_for("#dfb317"), ("#fff", Some("#010101")));
    }

    #[test]
    fn light_backgrounds_get_dark_text_without_shadow() {
        assert_eq!(text_for("#fff"), ("#333", None));
        assert_eq!(text_for("#ffd700"), ("#333", None));
        assert_eq!(text_for("#eee"), ("#333", None));
    }
}
