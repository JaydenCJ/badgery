//! The badge model, the four supported styles, and the shields static
//! badge path syntax (`build-passing-brightgreen` with `--`/`__` escapes).

use crate::color;

/// Badge styles, matching shields.io's names and general geometry.
/// (`social` is intentionally unsupported: it embeds a GitHub logo and a
/// counter bubble, neither of which makes sense offline.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    #[default]
    Flat,
    FlatSquare,
    Plastic,
    ForTheBadge,
}

impl Style {
    pub const ALL: [Style; 4] = [
        Style::Flat,
        Style::FlatSquare,
        Style::Plastic,
        Style::ForTheBadge,
    ];

    /// Parse a shields style name (case-insensitive).
    pub fn parse(s: &str) -> Option<Style> {
        match s.trim().to_ascii_lowercase().as_str() {
            "flat" => Some(Style::Flat),
            "flat-square" => Some(Style::FlatSquare),
            "plastic" => Some(Style::Plastic),
            "for-the-badge" => Some(Style::ForTheBadge),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Flat => "flat",
            Style::FlatSquare => "flat-square",
            Style::Plastic => "plastic",
            Style::ForTheBadge => "for-the-badge",
        }
    }
}

/// A fully resolved badge, ready to render. Colors are normalized `#hex`.
#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub label: String,
    pub message: String,
    pub label_color: String,
    pub color: String,
    pub style: Style,
}

impl Badge {
    /// A badge with shields defaults: grey label, lightgrey message, flat.
    pub fn new(label: impl Into<String>, message: impl Into<String>) -> Badge {
        Badge {
            label: label.into(),
            message: message.into(),
            label_color: color::DEFAULT_LABEL_COLOR.to_string(),
            color: color::DEFAULT_COLOR.to_string(),
            style: Style::default(),
        }
    }

    /// Set the message color from user input (shields fallback on junk).
    pub fn with_color(mut self, input: &str) -> Badge {
        self.color = color::resolve_or_default(input, color::DEFAULT_COLOR);
        self
    }

    /// Set the label color from user input (shields fallback on junk).
    pub fn with_label_color(mut self, input: &str) -> Badge {
        self.label_color = color::resolve_or_default(input, color::DEFAULT_LABEL_COLOR);
        self
    }

    pub fn with_style(mut self, style: Style) -> Badge {
        self.style = style;
        self
    }
}

/// Decode one shields path segment: `--` → `-`, `__` → `_`, `_` → space.
/// (Splitting on unescaped dashes already happened in
/// [`parse_static_path`]; this handles the underscore layer.)
pub fn unescape_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    let mut chars = segment.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                out.push('_');
            } else {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Split a shields static path on unescaped dashes (`--` is a literal `-`).
fn split_escaped_dashes(path: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '-' {
            if chars.peek() == Some(&'-') {
                chars.next();
                parts.last_mut().expect("never empty").push('-');
            } else {
                parts.push(String::new());
            }
        } else {
            parts.last_mut().expect("never empty").push(c);
        }
    }
    parts
}

/// Parse shields static badge path syntax: `<label>-<message>-<color>`.
///
/// Returns `(label, message, color_token)` with escapes applied. Exactly
/// shields' escaping rules; additionally tolerant of *unescaped* interior
/// dashes (four or more parts fold the middle back into the message, which
/// is what the author almost always meant). Two parts are treated as
/// `<message>-<color>` with an empty label, matching shields' message-only
/// badges.
pub fn parse_static_path(path: &str) -> Result<(String, String, String), String> {
    let parts = split_escaped_dashes(path);
    match parts.len() {
        0 | 1 => Err(format!(
            "'{path}' is not a static badge spec; expected <label>-<message>-<color> \
             (escape literal dashes as '--')"
        )),
        2 => Ok((String::new(), unescape_segment(&parts[0]), parts[1].clone())),
        n => {
            let label = unescape_segment(&parts[0]);
            let message = unescape_segment(&parts[1..n - 1].join("-"));
            let color_token = parts[n - 1].clone();
            Ok((label, message, color_token))
        }
    }
}

/// Escape text for use in SVG attribute values and text nodes.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_round_trip() {
        for style in Style::ALL {
            assert_eq!(Style::parse(style.name()), Some(style));
        }
        assert_eq!(Style::parse("FLAT-SQUARE"), Some(Style::FlatSquare));
        assert_eq!(Style::parse("social"), None);
        assert_eq!(Style::parse(""), None);
    }

    #[test]
    fn new_badge_uses_shields_defaults_and_junk_colors_fall_back() {
        let b = Badge::new("build", "passing");
        assert_eq!(b.label_color, "#555");
        assert_eq!(b.color, "#9f9f9f");
        assert_eq!(b.style, Style::Flat);
        let b = Badge::new("x", "y")
            .with_color("not-a-color")
            .with_label_color("bogus");
        assert_eq!(b.color, "#9f9f9f");
        assert_eq!(b.label_color, "#555");
    }

    #[test]
    fn three_part_path_parses_and_two_parts_make_a_message_only_badge() {
        let (label, message, c) = parse_static_path("build-passing-brightgreen").unwrap();
        assert_eq!(
            (label.as_str(), message.as_str(), c.as_str()),
            ("build", "passing", "brightgreen")
        );
        let (label, message, c) = parse_static_path("passing-brightgreen").unwrap();
        assert_eq!(
            (label.as_str(), message.as_str(), c.as_str()),
            ("", "passing", "brightgreen")
        );
    }

    #[test]
    fn double_dash_escapes_a_literal_dash() {
        let (label, message, _) = parse_static_path("release--notes-up--to--date-green").unwrap();
        assert_eq!(label, "release-notes");
        assert_eq!(message, "up-to-date");
    }

    #[test]
    fn underscores_become_spaces_and_double_underscore_is_literal() {
        let (label, message, _) = parse_static_path("last_audit-two__phase_ok-blue").unwrap();
        assert_eq!(label, "last audit");
        assert_eq!(message, "two_phase ok");
    }

    #[test]
    fn unescaped_interior_dashes_fold_into_the_message() {
        // "semver-2.0.0-rc-blue" — the author meant message "2.0.0-rc".
        let (label, message, c) = parse_static_path("semver-2.0.0-rc-blue").unwrap();
        assert_eq!(label, "semver");
        assert_eq!(message, "2.0.0-rc");
        assert_eq!(c, "blue");
    }

    #[test]
    fn single_part_is_rejected_with_a_helpful_error() {
        let err = parse_static_path("justoneword").unwrap_err();
        assert!(err.contains("label"), "{err}");
    }

    #[test]
    fn xml_escape_covers_the_five_specials() {
        assert_eq!(
            xml_escape(r#"<a & "b's">"#),
            "&lt;a &amp; &quot;b&apos;s&quot;&gt;"
        );
        assert_eq!(xml_escape("plain"), "plain");
    }
}
