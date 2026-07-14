//! Deterministic text measurement.
//!
//! shields.io measures strings against real Verdana font metrics; badgery
//! ships a compact advance-width table for the printable ASCII range
//! (Verdana metrics at font-size 110, i.e. tenths of a pixel at the 11px
//! the badge actually renders at). Because every `<text>` element carries
//! an explicit `textLength`, the renderer stretches or squeezes glyphs to
//! exactly the measured width — so even where a viewer substitutes fonts,
//! the layout never overflows its rectangle and output stays byte-stable
//! across platforms.

/// Advance widths for `' '..='~'` (codepoints 0x20–0x7E) in tenths of a
/// pixel at 11px Verdana.
const ASCII_WIDTHS: [u16; 95] = [
    39, 44, 57, 79, 69, 127, 85, 30, 46, 46, 69, 79, 39, 46, 39, 50, // ' '..'/'
    70, 70, 70, 70, 70, 70, 70, 70, 70, 70, // '0'..'9'
    44, 44, 79, 79, 79, 60, 110, // ':'..'@'
    75, 75, 77, 85, 72, 66, 86, 83, 46, 47, 77, 63, 94, 84, 87, 66, // 'A'..'P'
    87, 76, 75, 67, 82, 75, 109, 75, 68, 75, // 'Q'..'Z'
    46, 50, 46, 79, 70, 70, // '['..'`'
    66, 68, 57, 68, 65, 39, 68, 69, 30, 34, 64, 30, 106, 69, 67, 68, // 'a'..'p'
    68, 46, 57, 44, 69, 64, 89, 64, 64, 57, // 'q'..'z'
    68, 50, 68, 79, // '{'..'~'
];

/// Width assumed for any codepoint above 0x7E (CJK, emoji, accented
/// letters). Deliberately generous: a badge that is a few pixels roomy
/// beats one that clips 日本語 in half.
pub const WIDE_CHAR_WIDTH: u32 = 105;

/// Measured width of a string in tenths of a pixel at font-size 110.
///
/// Control characters contribute nothing (they should never reach a badge,
/// but a stray `\t` in a JSON value must not distort layout).
pub fn width_tenths(s: &str) -> u32 {
    s.chars().map(char_width_tenths).sum()
}

fn char_width_tenths(c: char) -> u32 {
    let code = c as u32;
    if code < 0x20 {
        0
    } else if code <= 0x7E {
        u32::from(ASCII_WIDTHS[(code - 0x20) as usize])
    } else {
        WIDE_CHAR_WIDTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_the_sum_of_character_widths() {
        assert_eq!(width_tenths(""), 0);
        assert_eq!(width_tenths("ab"), width_tenths("a") + width_tenths("b"));
    }

    #[test]
    fn all_digits_share_one_width() {
        // Verdana digits are tabular; "100%" and "999%" must render the
        // same badge width or coverage badges jitter between CI runs.
        let w0 = width_tenths("0");
        for d in "123456789".chars() {
            assert_eq!(width_tenths(&d.to_string()), w0);
        }
    }

    #[test]
    fn narrow_and_wide_letters_differ_sensibly() {
        assert!(width_tenths("i") < width_tenths("m"));
        assert!(width_tenths("W") > width_tenths("I"));
    }

    #[test]
    fn non_ascii_uses_the_wide_fallback() {
        assert_eq!(width_tenths("日"), WIDE_CHAR_WIDTH);
        assert_eq!(width_tenths("日本語"), 3 * WIDE_CHAR_WIDTH);
    }

    #[test]
    fn control_characters_are_ignored() {
        assert_eq!(width_tenths("a\tb\nc"), width_tenths("abc"));
    }

    #[test]
    fn typical_badge_words_land_in_plausible_pixel_ranges() {
        // 11px Verdana "passing" is ~46px in shields' own measurement;
        // stay within a pixel or two so mixed walls of badges line up.
        let passing = width_tenths("passing");
        assert!((380..=480).contains(&passing), "passing = {passing}");
        let build = width_tenths("build");
        assert!((240..=320).contains(&build), "build = {build}");
    }
}
