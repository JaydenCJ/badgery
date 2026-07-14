//! The JSONPath subset behind `badgery query` — the same subset that covers
//! the overwhelming majority of shields.io "dynamic JSON badge" queries:
//!
//! - `$` — the root (leading `$` is optional)
//! - `.key` — object member by name
//! - `["key"]` / `['key']` — member by name (for keys containing `.` or `[`)
//! - `[0]` / `[-1]` — array index (negative counts from the end)
//!
//! Filters, wildcards, recursive descent and slices are deliberately out of
//! scope: a badge shows a single scalar, and everything else invites
//! surprises in CI.

use crate::json::Value;

/// One step of a parsed query.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Key(String),
    Index(i64),
}

/// Parse a query expression into segments.
pub fn parse(expr: &str) -> Result<Vec<Segment>, String> {
    let expr = expr.trim();
    let mut chars = expr.strip_prefix('$').unwrap_or(expr).chars().peekable();
    let mut segments = Vec::new();
    loop {
        match chars.peek() {
            None => break,
            Some('.') => {
                chars.next();
                let mut key = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '.' || c == '[' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                }
                if key.is_empty() {
                    return Err(
                        "empty key after '.' (recursive descent '..' is not supported)".to_string(),
                    );
                }
                segments.push(Segment::Key(key));
            }
            Some('[') => {
                chars.next();
                match chars.peek() {
                    Some(&q @ ('"' | '\'')) => {
                        chars.next();
                        let mut key = String::new();
                        loop {
                            match chars.next() {
                                Some(c) if c == q => break,
                                Some(c) => key.push(c),
                                None => return Err("unterminated quoted key".to_string()),
                            }
                        }
                        if chars.next() != Some(']') {
                            return Err("expected ']' after quoted key".to_string());
                        }
                        segments.push(Segment::Key(key));
                    }
                    _ => {
                        let mut digits = String::new();
                        while let Some(&c) = chars.peek() {
                            if c == ']' {
                                break;
                            }
                            digits.push(c);
                            chars.next();
                        }
                        if chars.next() != Some(']') {
                            return Err("unterminated '[' in query".to_string());
                        }
                        let index: i64 = digits
                            .trim()
                            .parse()
                            .map_err(|_| format!("invalid array index '[{digits}]'"))?;
                        segments.push(Segment::Index(index));
                    }
                }
            }
            Some(other) => {
                // Allow a bare leading key ("version" instead of "$.version").
                if segments.is_empty() && *other != '$' {
                    let mut key = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '.' || c == '[' {
                            break;
                        }
                        key.push(c);
                        chars.next();
                    }
                    segments.push(Segment::Key(key));
                } else {
                    return Err(format!("unexpected character '{other}' in query"));
                }
            }
        }
    }
    if segments.is_empty() {
        return Err("query selects the whole document; point it at a scalar".to_string());
    }
    Ok(segments)
}

/// Walk `root` along `segments`, with error messages that name the path
/// taken so far (finding out *where* a query diverged matters in CI logs).
pub fn eval<'a>(root: &'a Value, segments: &[Segment]) -> Result<&'a Value, String> {
    let mut current = root;
    let mut trail = String::from("$");
    for segment in segments {
        match segment {
            Segment::Key(key) => {
                current = match current {
                    Value::Object(_) => current.get(key).ok_or_else(|| {
                        format!("no member '{key}' under {trail} (object has other keys)")
                    })?,
                    other => {
                        return Err(format!(
                            "{trail} is {}, cannot select member '{key}'",
                            other.type_name_with_article()
                        ))
                    }
                };
                trail.push('.');
                trail.push_str(key);
            }
            Segment::Index(index) => {
                current = match current {
                    Value::Array(items) => {
                        let len = items.len() as i64;
                        let resolved = if *index < 0 { len + index } else { *index };
                        if resolved < 0 || resolved >= len {
                            return Err(format!(
                                "index [{index}] out of bounds at {trail} (length {len})"
                            ));
                        }
                        &items[resolved as usize]
                    }
                    other => {
                        return Err(format!(
                            "{trail} is {}, cannot index with [{index}]",
                            other.type_name_with_article()
                        ))
                    }
                };
                trail.push_str(&format!("[{index}]"));
            }
        }
    }
    Ok(current)
}

/// Parse + eval + stringify: the full `badgery query` pipeline.
///
/// Scalars stringify naturally; arrays of scalars are joined with `", "`
/// (matching shields.io dynamic-badge behavior for list values); objects
/// and nested arrays are an error because they never make a sensible badge.
pub fn query(root: &Value, expr: &str) -> Result<String, String> {
    let segments = parse(expr)?;
    let value = eval(root, &segments)?;
    render_value(value)
}

fn render_value(value: &Value) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(format_number(*n)),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Err("query resolved to null".to_string()),
        Value::Array(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Array(_) | Value::Object(_) => {
                        return Err("query resolved to an array of non-scalars".to_string())
                    }
                    Value::Null => return Err("query resolved to an array containing null".into()),
                    other => parts.push(render_value(other)?),
                }
            }
            Ok(parts.join(", "))
        }
        Value::Object(_) => {
            Err("query resolved to an object; select one of its members".to_string())
        }
    }
}

/// Integers print without a trailing `.0` (a version badge must say `3`,
/// not `3.0`, when the JSON said `3`).
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn doc() -> Value {
        json::parse(
            r#"{
                "version": "1.4.2",
                "coverage": {"lines": 92.5, "branches": 81},
                "jobs": [
                    {"name": "unit", "ok": true},
                    {"name": "e2e", "ok": false}
                ],
                "tags": ["a", "b", "c"],
                "weird.key": {"deep": 7},
                "empty": null
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn dotted_path_reaches_nested_member() {
        assert_eq!(query(&doc(), "$.coverage.lines").unwrap(), "92.5");
        // The leading '$' is optional, and a bare key works too.
        assert_eq!(query(&doc(), "version").unwrap(), "1.4.2");
        assert_eq!(query(&doc(), ".version").unwrap(), "1.4.2");
    }

    #[test]
    fn array_index_and_negative_index() {
        assert_eq!(query(&doc(), "$.jobs[0].name").unwrap(), "unit");
        assert_eq!(query(&doc(), "$.jobs[-1].name").unwrap(), "e2e");
    }

    #[test]
    fn quoted_key_handles_dots_inside_names() {
        assert_eq!(query(&doc(), r#"$["weird.key"].deep"#).unwrap(), "7");
        assert_eq!(query(&doc(), "$['weird.key'].deep").unwrap(), "7");
    }

    #[test]
    fn integers_drop_the_decimal_point_and_booleans_render_as_words() {
        assert_eq!(query(&doc(), "$.coverage.branches").unwrap(), "81");
        assert_eq!(query(&doc(), "$.jobs[0].ok").unwrap(), "true");
    }

    #[test]
    fn scalar_arrays_join_with_comma_space() {
        assert_eq!(query(&doc(), "$.tags").unwrap(), "a, b, c");
    }

    #[test]
    fn missing_member_error_names_the_trail() {
        let err = query(&doc(), "$.coverage.functions").unwrap_err();
        assert!(err.contains("functions"), "{err}");
        assert!(err.contains("$.coverage"), "{err}");
    }

    #[test]
    fn out_of_bounds_index_reports_length() {
        let err = query(&doc(), "$.jobs[5].name").unwrap_err();
        assert!(err.contains("length 2"), "{err}");
    }

    #[test]
    fn indexing_an_object_is_a_type_error() {
        let err = query(&doc(), "$.coverage[0]").unwrap_err();
        assert!(err.contains("is an object"), "{err}");
        assert!(err.contains("cannot index"), "{err}");
    }

    #[test]
    fn null_and_object_results_are_rejected() {
        assert!(query(&doc(), "$.empty").unwrap_err().contains("null"));
        assert!(query(&doc(), "$.coverage").unwrap_err().contains("object"));
    }

    #[test]
    fn parse_rejects_malformed_expressions() {
        for bad in ["$..a", "$.a[", "$.a[b]", "$['x]", "$", ""] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
