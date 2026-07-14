//! The shields.io **endpoint badge** JSON schema, read from local files.
//!
//! This is the exact contract documented at shields.io/badges/endpoint-badge
//! (`schemaVersion`, `label`, `message`, `color`, `labelColor`, `isError`,
//! `style`), minus the fields that only make sense for a hosted service
//! (`namedLogo`, `logoSvg`, `cacheSeconds` — accepted and ignored, so
//! specs written for shields keep working verbatim). See
//! `docs/endpoint-format.md` for the precise merge rules.

use crate::badge::{Badge, Style};
use crate::color;
use crate::json::Value;

/// A parsed, validated endpoint spec (colors still raw user strings).
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointSpec {
    pub label: String,
    pub message: String,
    pub color: Option<String>,
    pub label_color: Option<String>,
    pub style: Option<Style>,
    pub is_error: bool,
}

/// Overrides supplied on the command line or as server query parameters.
/// Mirrors shields: the *message* can never be overridden — it is the data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    pub label: Option<String>,
    pub color: Option<String>,
    pub label_color: Option<String>,
    pub style: Option<Style>,
}

/// Validate an endpoint document. Errors name the offending field.
pub fn parse_spec(doc: &Value) -> Result<EndpointSpec, String> {
    if !matches!(doc, Value::Object(_)) {
        return Err(format!(
            "endpoint document must be a JSON object, got {}",
            doc.type_name_with_article()
        ));
    }
    match doc.get("schemaVersion") {
        Some(Value::Number(n)) if *n == 1.0 => {}
        Some(other) => {
            return Err(format!(
                "schemaVersion must be the number 1, got {}",
                describe(other)
            ))
        }
        None => return Err("missing required field 'schemaVersion' (must be 1)".to_string()),
    }
    let label = require_string(doc, "label")?;
    let message = require_string(doc, "message")?;
    if message.is_empty() {
        return Err("'message' must not be empty".to_string());
    }
    let color = optional_string(doc, "color")?;
    let label_color = optional_string(doc, "labelColor")?;
    let style = match optional_string(doc, "style")? {
        None => None,
        Some(raw) => Some(Style::parse(&raw).ok_or_else(|| {
            format!(
                "unknown style '{raw}' (expected one of: {})",
                Style::ALL.map(Style::name).join(", ")
            )
        })?),
    };
    let is_error = match doc.get("isError") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(format!(
                "'isError' must be a boolean, got {}",
                describe(other)
            ))
        }
    };
    Ok(EndpointSpec {
        label,
        message,
        color,
        label_color,
        style,
        is_error,
    })
}

/// Merge a spec with overrides into a renderable badge, following shields'
/// endpoint rules:
///
/// 1. Overrides win over the file for `label`, `color`, `labelColor`, `style`.
/// 2. When `isError` is true the badge always renders red and the *color*
///    overrides are ignored — an error state must not be paintable green.
pub fn to_badge(spec: &EndpointSpec, ov: &Overrides) -> Badge {
    let label = ov.label.clone().unwrap_or_else(|| spec.label.clone());
    let mut badge = Badge::new(label, spec.message.clone());
    if let Some(style) = ov.style.or(spec.style) {
        badge = badge.with_style(style);
    }
    if spec.is_error {
        badge.color = color::ERROR_COLOR.to_string();
        if let Some(raw) = &spec.label_color {
            badge = badge.with_label_color(raw);
        }
        return badge;
    }
    if let Some(raw) = ov.color.as_ref().or(spec.color.as_ref()) {
        badge = badge.with_color(raw);
    }
    if let Some(raw) = ov.label_color.as_ref().or(spec.label_color.as_ref()) {
        badge = badge.with_label_color(raw);
    }
    badge
}

fn require_string(doc: &Value, field: &str) -> Result<String, String> {
    match doc.get(field) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(format!(
            "'{field}' must be a string, got {}",
            describe(other)
        )),
        None => Err(format!("missing required field '{field}'")),
    }
}

fn optional_string(doc: &Value, field: &str) -> Result<Option<String>, String> {
    match doc.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!(
            "'{field}' must be a string, got {}",
            describe(other)
        )),
    }
}

fn describe(v: &Value) -> String {
    match v {
        Value::Number(n) => format!("the number {n}"),
        Value::String(s) => format!("the string \"{s}\""),
        other => other.type_name_with_article().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn spec_from(text: &str) -> Result<EndpointSpec, String> {
        parse_spec(&json::parse(text).unwrap())
    }

    #[test]
    fn minimal_valid_spec_parses_with_defaults() {
        let spec =
            spec_from(r#"{"schemaVersion": 1, "label": "coverage", "message": "92%"}"#).unwrap();
        assert_eq!(spec.label, "coverage");
        assert_eq!(spec.message, "92%");
        assert_eq!(spec.color, None);
        assert!(!spec.is_error);
    }

    #[test]
    fn full_spec_with_all_supported_fields() {
        let spec = spec_from(
            r#"{"schemaVersion": 1, "label": "docs", "message": "stale",
                "color": "orange", "labelColor": "444", "style": "flat-square",
                "isError": false}"#,
        )
        .unwrap();
        assert_eq!(spec.color.as_deref(), Some("orange"));
        assert_eq!(spec.label_color.as_deref(), Some("444"));
        assert_eq!(spec.style, Some(Style::FlatSquare));
    }

    #[test]
    fn shields_service_only_fields_are_ignored_not_rejected() {
        // A spec written for hosted shields must keep working locally.
        let spec = spec_from(
            r#"{"schemaVersion": 1, "label": "ok", "message": "yes",
                "namedLogo": "rust", "cacheSeconds": 300}"#,
        );
        assert!(spec.is_ok(), "{spec:?}");
        // And an empty label stays allowed (message-only badges).
        let spec = spec_from(r#"{"schemaVersion": 1, "label": "", "message": "beta"}"#).unwrap();
        assert_eq!(to_badge(&spec, &Overrides::default()).label, "");
    }

    #[test]
    fn schema_version_must_be_exactly_one() {
        assert!(spec_from(r#"{"label": "a", "message": "b"}"#)
            .unwrap_err()
            .contains("schemaVersion"));
        assert!(
            spec_from(r#"{"schemaVersion": 2, "label": "a", "message": "b"}"#)
                .unwrap_err()
                .contains("number 2")
        );
        assert!(
            spec_from(r#"{"schemaVersion": "1", "label": "a", "message": "b"}"#)
                .unwrap_err()
                .contains("string")
        );
    }

    #[test]
    fn message_is_required_and_must_be_nonempty() {
        assert!(spec_from(r#"{"schemaVersion": 1, "label": "a"}"#)
            .unwrap_err()
            .contains("'message'"));
        assert!(
            spec_from(r#"{"schemaVersion": 1, "label": "a", "message": ""}"#)
                .unwrap_err()
                .contains("empty")
        );
    }

    #[test]
    fn wrong_field_types_name_the_field() {
        let err = spec_from(r#"{"schemaVersion": 1, "label": 3, "message": "b"}"#).unwrap_err();
        assert!(err.contains("'label'"), "{err}");
        let err =
            spec_from(r#"{"schemaVersion": 1, "label": "a", "message": "b", "isError": "yes"}"#)
                .unwrap_err();
        assert!(err.contains("'isError'"), "{err}");
    }

    #[test]
    fn unknown_style_in_file_is_an_error_listing_valid_styles() {
        let err = spec_from(r#"{"schemaVersion": 1, "label": "a", "message": "b", "style": "3d"}"#)
            .unwrap_err();
        assert!(err.contains("for-the-badge"), "{err}");
    }

    #[test]
    fn overrides_win_over_the_file() {
        let spec = spec_from(
            r#"{"schemaVersion": 1, "label": "cov", "message": "91%", "color": "green"}"#,
        )
        .unwrap();
        let ov = Overrides {
            label: Some("coverage".into()),
            color: Some("blue".into()),
            style: Some(Style::Plastic),
            ..Overrides::default()
        };
        let badge = to_badge(&spec, &ov);
        assert_eq!(badge.label, "coverage");
        assert_eq!(badge.color, "#007ec6");
        assert_eq!(badge.style, Style::Plastic);
    }

    #[test]
    fn is_error_forces_red_and_ignores_color_overrides() {
        let spec = spec_from(
            r#"{"schemaVersion": 1, "label": "deps", "message": "scan failed",
                "color": "green", "isError": true}"#,
        )
        .unwrap();
        let ov = Overrides {
            color: Some("brightgreen".into()),
            ..Overrides::default()
        };
        let badge = to_badge(&spec, &ov);
        assert_eq!(badge.color, "#e05d44", "error badges are always red");
    }
}
