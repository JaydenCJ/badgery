//! The `badgery.json` build manifest: declare every badge a repository
//! wants, run `badgery build` in CI, commit the SVGs. One manifest entry
//! maps to one `<outDir>/<name>.svg`.

use std::path::Path;

use crate::badge::{Badge, Style};
use crate::endpoint::{self, Overrides};
use crate::json::{self, Value};
use crate::jsonpath;

/// A parsed manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub out_dir: String,
    pub badges: Vec<Entry>,
}

/// One badge declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub label: Option<String>,
    pub color: Option<String>,
    pub label_color: Option<String>,
    pub style: Option<Style>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Fixed text: `{"type": "static", "message": "…"}`.
    Static { message: String },
    /// A shields endpoint-schema file: `{"type": "endpoint", "file": "…"}`.
    Endpoint { file: String },
    /// Any JSON file + JSONPath query: `{"type": "query", "file", "query"}`.
    Query {
        file: String,
        query: String,
        prefix: String,
        suffix: String,
    },
}

/// Parse and validate a manifest document.
pub fn parse_manifest(doc: &Value) -> Result<Manifest, String> {
    if !matches!(doc, Value::Object(_)) {
        return Err("manifest must be a JSON object".to_string());
    }
    let out_dir = match doc.get("outDir") {
        None => "badges".to_string(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(_) => return Err("'outDir' must be a non-empty string".to_string()),
    };
    let raw_badges = match doc.get("badges") {
        Some(Value::Array(items)) if !items.is_empty() => items,
        Some(Value::Array(_)) => return Err("'badges' array is empty".to_string()),
        _ => return Err("manifest needs a 'badges' array".to_string()),
    };
    let mut badges: Vec<Entry> = Vec::with_capacity(raw_badges.len());
    for (i, item) in raw_badges.iter().enumerate() {
        let entry = parse_entry(item).map_err(|e| format!("badges[{i}]: {e}"))?;
        if badges.iter().any(|b| b.name == entry.name) {
            return Err(format!(
                "badges[{i}]: duplicate badge name '{}' (each name is one output file)",
                entry.name
            ));
        }
        badges.push(entry);
    }
    Ok(Manifest { out_dir, badges })
}

/// Badge names become file names, so they are restricted to a safe set.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn parse_entry(item: &Value) -> Result<Entry, String> {
    if !matches!(item, Value::Object(_)) {
        return Err("each badge must be a JSON object".to_string());
    }
    let name = req_str(item, "name")?;
    if !valid_name(&name) {
        return Err(format!(
            "invalid name '{name}' (use letters, digits, '-', '_', '.'; no leading dot)"
        ));
    }
    let kind_name = req_str(item, "type")?;
    let kind = match kind_name.as_str() {
        "static" => Kind::Static {
            message: req_str(item, "message")?,
        },
        "endpoint" => Kind::Endpoint {
            file: req_str(item, "file")?,
        },
        "query" => Kind::Query {
            file: req_str(item, "file")?,
            query: req_str(item, "query")?,
            prefix: opt_str(item, "prefix")?.unwrap_or_default(),
            suffix: opt_str(item, "suffix")?.unwrap_or_default(),
        },
        other => {
            return Err(format!(
                "unknown type '{other}' (expected static, endpoint or query)"
            ))
        }
    };
    let style = match opt_str(item, "style")? {
        None => None,
        Some(raw) => Some(Style::parse(&raw).ok_or_else(|| format!("unknown style '{raw}'"))?),
    };
    Ok(Entry {
        name,
        kind,
        label: opt_str(item, "label")?,
        color: opt_str(item, "color")?,
        label_color: opt_str(item, "labelColor")?,
        style,
    })
}

/// Resolve one manifest entry to a renderable [`Badge`]. `base` is the
/// directory of the manifest file; data files resolve relative to it, so
/// `badgery build` works from any working directory.
pub fn resolve_entry(entry: &Entry, base: &Path) -> Result<Badge, String> {
    let badge = match &entry.kind {
        Kind::Static { message } => {
            let mut badge = Badge::new(entry.label.clone().unwrap_or_default(), message.clone());
            if let Some(c) = &entry.color {
                badge = badge.with_color(c);
            }
            badge
        }
        Kind::Endpoint { file } => {
            let doc = read_json(base, file)?;
            let spec = endpoint::parse_spec(&doc).map_err(|e| format!("{file}: {e}"))?;
            let ov = Overrides {
                label: entry.label.clone(),
                color: entry.color.clone(),
                label_color: entry.label_color.clone(),
                style: entry.style,
            };
            return Ok(endpoint::to_badge(&spec, &ov));
        }
        Kind::Query {
            file,
            query,
            prefix,
            suffix,
        } => {
            let doc = read_json(base, file)?;
            let value = jsonpath::query(&doc, query).map_err(|e| format!("{file}: {e}"))?;
            let mut badge = Badge::new(
                entry.label.clone().unwrap_or_default(),
                format!("{prefix}{value}{suffix}"),
            );
            if let Some(c) = &entry.color {
                badge = badge.with_color(c);
            }
            badge
        }
    };
    let mut badge = badge;
    if let Some(lc) = &entry.label_color {
        badge = badge.with_label_color(lc);
    }
    if let Some(style) = entry.style {
        badge = badge.with_style(style);
    }
    Ok(badge)
}

fn read_json(base: &Path, rel: &str) -> Result<Value, String> {
    let path = base.join(rel);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn req_str(item: &Value, field: &str) -> Result<String, String> {
    match item.get(field) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::String(_)) => Err(format!("'{field}' must not be empty")),
        Some(other) => Err(format!(
            "'{field}' must be a string, got {}",
            other.type_name_with_article()
        )),
        None => Err(format!("missing required field '{field}'")),
    }
}

fn opt_str(item: &Value, field: &str) -> Result<Option<String>, String> {
    match item.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!(
            "'{field}' must be a string, got {}",
            other.type_name_with_article()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn manifest_from(text: &str) -> Result<Manifest, String> {
        parse_manifest(&json::parse(text).unwrap())
    }

    #[test]
    fn minimal_manifest_defaults_out_dir_to_badges() {
        let m = manifest_from(
            r#"{"badges": [{"name": "b", "type": "static", "label": "l", "message": "m"}]}"#,
        )
        .unwrap();
        assert_eq!(m.out_dir, "badges");
        assert_eq!(m.badges.len(), 1);
        assert_eq!(
            m.badges[0].kind,
            Kind::Static {
                message: "m".into()
            }
        );
    }

    #[test]
    fn all_three_entry_types_parse() {
        let m = manifest_from(
            r#"{"outDir": "out", "badges": [
                {"name": "a", "type": "static", "label": "l", "message": "m", "color": "green"},
                {"name": "b", "type": "endpoint", "file": "cov.json", "style": "flat-square"},
                {"name": "c", "type": "query", "file": "meta.json", "query": "$.version",
                 "label": "version", "prefix": "v", "color": "blue"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(m.out_dir, "out");
        assert!(matches!(m.badges[1].kind, Kind::Endpoint { .. }));
        match &m.badges[2].kind {
            Kind::Query { prefix, suffix, .. } => {
                assert_eq!(prefix, "v");
                assert_eq!(suffix, "");
            }
            other => panic!("expected query entry, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_names_are_rejected() {
        let err = manifest_from(
            r#"{"badges": [
                {"name": "x", "type": "static", "message": "1"},
                {"name": "x", "type": "static", "message": "2"}
            ]}"#,
        )
        .unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn errors_carry_the_entry_index() {
        let err = manifest_from(
            r#"{"badges": [
                {"name": "ok", "type": "static", "message": "m"},
                {"name": "bad", "type": "mystery"}
            ]}"#,
        )
        .unwrap_err();
        assert!(err.starts_with("badges[1]:"), "{err}");
    }

    #[test]
    fn unsafe_names_are_rejected() {
        for bad in ["../evil", "a/b", ".hidden", "", "sp ace"] {
            let text =
                format!(r#"{{"badges": [{{"name": "{bad}", "type": "static", "message": "m"}}]}}"#);
            assert!(manifest_from(&text).is_err(), "{bad:?} should be rejected");
        }
        assert!(valid_name("cov_line-2.0"));
    }

    #[test]
    fn resolve_static_entry_applies_colors_and_style() {
        let m = manifest_from(
            r#"{"badges": [{"name": "a", "type": "static", "label": "docs",
                "message": "fresh", "color": "brightgreen", "labelColor": "444",
                "style": "plastic"}]}"#,
        )
        .unwrap();
        let badge = resolve_entry(&m.badges[0], Path::new(".")).unwrap();
        assert_eq!(badge.color, "#4c1");
        assert_eq!(badge.label_color, "#444");
        assert_eq!(badge.style, Style::Plastic);
    }

    #[test]
    fn resolve_query_entry_reads_relative_to_base_dir() {
        let dir = std::env::temp_dir().join(format!("badgery-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"version": "2.3.1"}"#).unwrap();
        let m = manifest_from(
            r#"{"badges": [{"name": "v", "type": "query", "file": "meta.json",
                "query": "$.version", "label": "version", "prefix": "v"}]}"#,
        )
        .unwrap();
        let badge = resolve_entry(&m.badges[0], &dir).unwrap();
        assert_eq!(badge.message, "v2.3.1");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_data_file_error_names_the_path() {
        let m = manifest_from(
            r#"{"badges": [{"name": "e", "type": "endpoint", "file": "nope.json"}]}"#,
        )
        .unwrap();
        let err = resolve_entry(&m.badges[0], Path::new("/nonexistent-base")).unwrap_err();
        assert!(err.contains("nope.json"), "{err}");
    }
}
