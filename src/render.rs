//! SVG generation for the four supported styles.
//!
//! Geometry follows shields.io's badge-maker: text is laid out at
//! font-size 110 inside a `transform="scale(.1)"` group (so all text math
//! happens in integer tenths of a pixel), and every `<text>` carries an
//! explicit `textLength` so viewers without Verdana still render the exact
//! measured width. Output is deterministic: same input, same bytes.

use crate::badge::{xml_escape, Badge, Style};
use crate::color;
use crate::text;

/// Horizontal padding on each side of a text run, in tenths (5px).
const PAD: u32 = 50;
/// for-the-badge: wider padding (9px) and per-character letter spacing.
const FTB_PAD: u32 = 90;
const FTB_LETTER_SPACING: u32 = 12;

/// Render a badge to a complete standalone SVG document.
pub fn render(badge: &Badge) -> String {
    match badge.style {
        Style::ForTheBadge => render_for_the_badge(badge),
        Style::Flat => render_classic(badge, ClassicParams::FLAT),
        Style::FlatSquare => render_classic(badge, ClassicParams::FLAT_SQUARE),
        Style::Plastic => render_classic(badge, ClassicParams::PLASTIC),
    }
}

/// Format tenths-of-a-pixel as a CSS pixel value ("885" -> "88.5").
fn px(tenths: u32) -> String {
    if tenths % 10 == 0 {
        format!("{}", tenths / 10)
    } else {
        format!("{}.{}", tenths / 10, tenths % 10)
    }
}

fn aria_label(badge: &Badge) -> String {
    if badge.label.is_empty() {
        badge.message.clone()
    } else {
        format!("{}: {}", badge.label, badge.message)
    }
}

/// Shared knobs for the three 20/18px-tall styles.
struct ClassicParams {
    height: u32,
    /// Corner radius in px; 0 disables the clip path entirely.
    rx: u32,
    /// Gradient overlay definition, or empty for flat-square.
    gradient: &'static str,
    /// Whether dark backgrounds get the classic text drop shadow.
    shadow: bool,
    /// Baseline for the main text run, in tenths.
    text_y: u32,
}

impl ClassicParams {
    const FLAT: ClassicParams = ClassicParams {
        height: 20,
        rx: 3,
        gradient: r##"<linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>"##,
        shadow: true,
        text_y: 140,
    };
    const FLAT_SQUARE: ClassicParams = ClassicParams {
        height: 20,
        rx: 0,
        gradient: "",
        shadow: false,
        text_y: 140,
    };
    const PLASTIC: ClassicParams = ClassicParams {
        height: 18,
        rx: 4,
        gradient: r##"<linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#fff" stop-opacity=".7"/><stop offset=".1" stop-color="#aaa" stop-opacity=".1"/><stop offset=".9" stop-color="#000" stop-opacity=".3"/><stop offset="1" stop-color="#000" stop-opacity=".5"/></linearGradient>"##,
        shadow: true,
        text_y: 130,
    };
}

fn render_classic(badge: &Badge, p: ClassicParams) -> String {
    let label_tw = text::width_tenths(&badge.label);
    let msg_tw = text::width_tenths(&badge.message);
    let label_w = if badge.label.is_empty() {
        0
    } else {
        label_tw + 2 * PAD
    };
    let msg_w = msg_tw + 2 * PAD;
    let total_w = label_w + msg_w;
    let aria = xml_escape(&aria_label(badge));
    let h = p.height;

    let mut svg = String::with_capacity(1400);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{h}\" \
         role=\"img\" aria-label=\"{aria}\">\n",
        px(total_w)
    ));
    svg.push_str(&format!("  <title>{aria}</title>\n"));
    if !p.gradient.is_empty() {
        svg.push_str("  ");
        svg.push_str(p.gradient);
        svg.push('\n');
    }
    if p.rx > 0 {
        svg.push_str(&format!(
            "  <clipPath id=\"r\"><rect width=\"{}\" height=\"{h}\" rx=\"{}\" fill=\"#fff\"/></clipPath>\n  <g clip-path=\"url(#r)\">\n",
            px(total_w),
            p.rx
        ));
    } else {
        svg.push_str("  <g shape-rendering=\"crispEdges\">\n");
    }
    if label_w > 0 {
        svg.push_str(&format!(
            "    <rect width=\"{}\" height=\"{h}\" fill=\"{}\"/>\n",
            px(label_w),
            badge.label_color
        ));
    }
    svg.push_str(&format!(
        "    <rect x=\"{}\" width=\"{}\" height=\"{h}\" fill=\"{}\"/>\n",
        px(label_w),
        px(msg_w),
        badge.color
    ));
    if !p.gradient.is_empty() {
        svg.push_str(&format!(
            "    <rect width=\"{}\" height=\"{h}\" fill=\"url(#s)\"/>\n",
            px(total_w)
        ));
    }
    svg.push_str("  </g>\n");
    svg.push_str(
        "  <g text-anchor=\"middle\" font-family=\"Verdana,Geneva,DejaVu Sans,sans-serif\" \
         text-rendering=\"geometricPrecision\" font-size=\"110\">\n",
    );
    if label_w > 0 {
        push_text(
            &mut svg,
            &badge.label,
            label_w / 2,
            p.text_y,
            label_tw,
            &badge.label_color,
            p.shadow,
            false,
        );
    }
    push_text(
        &mut svg,
        &badge.message,
        label_w + msg_w / 2,
        p.text_y,
        msg_tw,
        &badge.color,
        p.shadow,
        false,
    );
    svg.push_str("  </g>\n</svg>\n");
    svg
}

fn render_for_the_badge(badge: &Badge) -> String {
    let label = badge.label.to_uppercase();
    let message = badge.message.to_uppercase();
    let label_tw = ftb_width(&label);
    let msg_tw = ftb_width(&message);
    let label_w = if label.is_empty() {
        0
    } else {
        label_tw + 2 * FTB_PAD
    };
    let msg_w = msg_tw + 2 * FTB_PAD;
    let total_w = label_w + msg_w;
    let aria = xml_escape(&aria_label(badge));

    let mut svg = String::with_capacity(1200);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"28\" \
         role=\"img\" aria-label=\"{aria}\">\n",
        px(total_w)
    ));
    svg.push_str(&format!("  <title>{aria}</title>\n"));
    svg.push_str("  <g shape-rendering=\"crispEdges\">\n");
    if label_w > 0 {
        svg.push_str(&format!(
            "    <rect width=\"{}\" height=\"28\" fill=\"{}\"/>\n",
            px(label_w),
            badge.label_color
        ));
    }
    svg.push_str(&format!(
        "    <rect x=\"{}\" width=\"{}\" height=\"28\" fill=\"{}\"/>\n",
        px(label_w),
        px(msg_w),
        badge.color
    ));
    svg.push_str("  </g>\n");
    svg.push_str(
        "  <g text-anchor=\"middle\" font-family=\"Verdana,Geneva,DejaVu Sans,sans-serif\" \
         text-rendering=\"geometricPrecision\" font-size=\"100\">\n",
    );
    if label_w > 0 {
        push_text(
            &mut svg,
            &label,
            label_w / 2,
            175,
            label_tw,
            &badge.label_color,
            false,
            false,
        );
    }
    push_text(
        &mut svg,
        &message,
        label_w + msg_w / 2,
        175,
        msg_tw,
        &badge.color,
        false,
        true,
    );
    svg.push_str("  </g>\n</svg>\n");
    svg
}

/// for-the-badge width: measured width plus letter spacing per character
/// (spread by `textLength`'s default `lengthAdjust="spacing"`).
fn ftb_width(s: &str) -> u32 {
    let n = s.chars().count() as u32;
    text::width_tenths(s) + n.saturating_sub(1) * FTB_LETTER_SPACING
}

#[allow(clippy::too_many_arguments)]
fn push_text(
    svg: &mut String,
    raw: &str,
    x: u32,
    y: u32,
    text_len: u32,
    background: &str,
    shadow_allowed: bool,
    bold: bool,
) {
    if raw.is_empty() {
        return;
    }
    let content = xml_escape(raw);
    let (fill, shadow) = color::text_for(background);
    let weight = if bold { " font-weight=\"bold\"" } else { "" };
    if shadow_allowed {
        if let Some(shadow_fill) = shadow {
            svg.push_str(&format!(
                "    <text aria-hidden=\"true\" x=\"{x}\" y=\"{}\" fill=\"{shadow_fill}\" \
                 fill-opacity=\".3\" transform=\"scale(.1)\" textLength=\"{text_len}\"{weight}>{content}</text>\n",
                y + 10
            ));
        }
    }
    svg.push_str(&format!(
        "    <text x=\"{x}\" y=\"{y}\" fill=\"{fill}\" transform=\"scale(.1)\" \
         textLength=\"{text_len}\"{weight}>{content}</text>\n"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::badge::Badge;

    fn flat(label: &str, message: &str, color: &str) -> Badge {
        Badge::new(label, message).with_color(color)
    }

    #[test]
    fn flat_badge_has_both_rects_gradient_and_title() {
        let svg = render(&flat("build", "passing", "brightgreen"));
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("fill=\"#555\""), "label rect");
        assert!(svg.contains("fill=\"#4c1\""), "message rect");
        assert!(svg.contains("linearGradient"), "flat has a gradient");
        assert!(svg.contains("<title>build: passing</title>"));
        assert!(svg.contains("aria-label=\"build: passing\""));
    }

    #[test]
    fn total_width_and_text_length_match_measured_text() {
        let svg = render(&flat("build", "passing", "brightgreen"));
        let expected = (text::width_tenths("build") + 100) + (text::width_tenths("passing") + 100);
        assert!(
            svg.contains(&format!("width=\"{}\"", px(expected))),
            "expected total width {} in:\n{svg}",
            px(expected)
        );
        let tw = text::width_tenths("passing");
        assert!(svg.contains(&format!("textLength=\"{tw}\">passing</text>")));
    }

    #[test]
    fn flat_square_has_no_gradient_no_rounding_no_shadow() {
        let badge = flat("build", "passing", "brightgreen").with_style(Style::FlatSquare);
        let svg = render(&badge);
        assert!(!svg.contains("linearGradient"));
        assert!(!svg.contains("clipPath"));
        assert!(!svg.contains("fill-opacity=\".3\""), "no text shadow");
        assert!(svg.contains("crispEdges"));
    }

    #[test]
    fn plastic_is_18px_tall_with_its_own_gradient() {
        let badge = flat("build", "passing", "brightgreen").with_style(Style::Plastic);
        let svg = render(&badge);
        assert!(svg.contains("height=\"18\""));
        assert!(svg.contains("stop-opacity=\".7\""), "plastic gloss stop");
    }

    #[test]
    fn for_the_badge_is_28px_uppercase_and_bold_message() {
        let badge = flat("status", "stable", "blue").with_style(Style::ForTheBadge);
        let svg = render(&badge);
        assert!(svg.contains("height=\"28\""));
        assert!(svg.contains(">STATUS</text>"));
        assert!(svg.contains(">STABLE</text>"));
        assert!(svg.contains("font-weight=\"bold\""));
        // aria keeps the author's casing.
        assert!(svg.contains("aria-label=\"status: stable\""));
    }

    #[test]
    fn message_only_badge_has_single_rect_and_plain_aria() {
        let svg = render(&flat("", "unreleased", "orange"));
        assert!(!svg.contains("fill=\"#555\""), "no label rect:\n{svg}");
        assert!(svg.contains("aria-label=\"unreleased\""));
        // Message rect starts at x=0.
        assert!(svg.contains("<rect x=\"0\" width="));
    }

    #[test]
    fn light_background_gets_dark_text_and_no_shadow() {
        let svg = render(&flat("ci", "flaky", "ffd700"));
        assert!(svg.contains("fill=\"#333\""), "dark text on gold:\n{svg}");
        // The label side is still dark grey -> white text with shadow, so
        // exactly one shadow text element exists (the label's, not the
        // message's).
        assert!(svg.contains("fill=\"#fff\""));
        assert_eq!(svg.matches("fill-opacity=\".3\"").count(), 1, "{svg}");
    }

    #[test]
    fn xml_specials_in_content_are_escaped_everywhere() {
        let svg = render(&flat("a<b", "c&d\"e", "blue"));
        assert!(svg.contains("a&lt;b"));
        assert!(svg.contains("c&amp;d&quot;e"));
        assert!(!svg.contains("c&d"));
    }

    #[test]
    fn output_is_deterministic() {
        let badge = flat("build", "passing", "brightgreen");
        assert_eq!(render(&badge), render(&badge));
    }

    #[test]
    fn digit_swaps_do_not_change_badge_width() {
        // Coverage badges flip 42% -> 98% between runs; a stable width
        // avoids README layout shift in rendered docs.
        let a = render(&flat("coverage", "42%", "green"));
        let b = render(&flat("coverage", "98%", "green"));
        let width = |svg: &str| {
            svg.split("width=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        assert_eq!(width(&a), width(&b));
        // While here: the px formatter drops trailing ".0" but keeps halves.
        assert_eq!(px(880), "88");
        assert_eq!(px(885), "88.5");
        assert_eq!(px(0), "0");
    }
}
