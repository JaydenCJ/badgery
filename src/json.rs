//! Minimal, strict, dependency-free JSON parser (RFC 8259).
//!
//! badgery only ever reads small local files (endpoint specs, manifests,
//! project metadata), so the parser optimizes for correctness and clear
//! error messages over throughput. Object member order is preserved.

use std::fmt;

/// A parsed JSON value. Numbers are kept as `f64`, which is exactly what
/// JavaScript (and therefore shields.io) does, so behavior matches.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// Object member lookup (first match wins, like `JSON.parse` order).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Short human-readable type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// [`type_name`](Value::type_name) with its indefinite article, for
    /// error messages that read as prose ("an object", not "a object").
    pub fn type_name_with_article(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Number(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "an array",
            Value::Object(_) => "an object",
        }
    }
}

/// Parse error with the byte offset where parsing failed.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid JSON at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Maximum nesting depth. Deeper documents are rejected instead of risking
/// a stack overflow on hostile input.
const MAX_DEPTH: usize = 128;

/// Parse a complete JSON document. Trailing non-whitespace is an error.
/// A leading UTF-8 BOM is tolerated because Windows editors love them.
pub fn parse(input: &str) -> Result<Value, ParseError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let value = p.value(0)?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(p.err("trailing characters after the document"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(format!("expected '{}'", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, ParseError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.err(format!("expected '{word}'")))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting deeper than 128 levels"));
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(other) => Err(self.err(format!("unexpected character '{}'", other as char))),
            None => Err(self.err("unexpected end of input")),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_ws();
            let key = self.string().map_err(|e| ParseError {
                message: format!("object key: {}", e.message),
                ..e
            })?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(members));
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    out.push(self.escape()?);
                }
                Some(c) if c < 0x20 => {
                    return Err(self.err("unescaped control character in string"));
                }
                Some(_) => {
                    // Copy one full UTF-8 scalar. Input is a &str, so byte
                    // boundaries are guaranteed valid.
                    let rest = &self.bytes[self.pos..];
                    let s = std::str::from_utf8(rest).expect("input was a valid &str");
                    let ch = s.chars().next().expect("peeked a byte");
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self) -> Result<char, ParseError> {
        let c = self.peek().ok_or_else(|| self.err("unterminated escape"))?;
        self.pos += 1;
        Ok(match c {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode_escape(),
            other => {
                self.pos -= 1;
                return Err(self.err(format!("invalid escape '\\{}'", other as char)));
            }
        })
    }

    fn hex4(&mut self) -> Result<u16, ParseError> {
        if self.pos + 4 > self.bytes.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let s = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4])
            .map_err(|_| self.err("non-ASCII in \\u escape"))?;
        let n = u16::from_str_radix(s, 16).map_err(|_| self.err("invalid \\u escape"))?;
        self.pos += 4;
        Ok(n)
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let hi = self.hex4()?;
        if (0xD800..=0xDBFF).contains(&hi) {
            // High surrogate: a low surrogate escape must follow.
            if self.peek() == Some(b'\\') && self.bytes.get(self.pos + 1) == Some(&b'u') {
                self.pos += 2;
                let lo = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&lo) {
                    return Err(self.err("expected low surrogate after high surrogate"));
                }
                let code = 0x10000 + ((hi as u32 - 0xD800) << 10) + (lo as u32 - 0xDC00);
                return char::from_u32(code).ok_or_else(|| self.err("invalid surrogate pair"));
            }
            return Err(self.err("lone high surrogate"));
        }
        if (0xDC00..=0xDFFF).contains(&hi) {
            return Err(self.err("lone low surrogate"));
        }
        char::from_u32(hi as u32).ok_or_else(|| self.err("invalid \\u escape"))
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        // Integer part: a single 0, or a nonzero digit followed by digits.
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.err("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("digit expected after decimal point"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("digit expected in exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).expect("ASCII number");
        let n: f64 = text.parse().map_err(|_| self.err("number out of range"))?;
        if !n.is_finite() {
            return Err(self.err("number out of range"));
        }
        Ok(Value::Number(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        assert_eq!(parse("null").unwrap(), Value::Null);
        assert_eq!(parse("true").unwrap(), Value::Bool(true));
        assert_eq!(parse("false").unwrap(), Value::Bool(false));
        assert_eq!(parse("42").unwrap(), Value::Number(42.0));
        assert_eq!(parse("-0.5e2").unwrap(), Value::Number(-50.0));
        assert_eq!(parse("\"hi\"").unwrap(), Value::String("hi".into()));
        // Windows editors love BOMs; tolerate a leading one.
        assert_eq!(parse("\u{feff}{}").unwrap(), Value::Object(vec![]));
    }

    #[test]
    fn parses_nested_document_and_preserves_member_order() {
        let v = parse(r#"{"b": [1, {"c": null}], "a": "x"}"#).unwrap();
        match &v {
            Value::Object(members) => {
                assert_eq!(members[0].0, "b");
                assert_eq!(members[1].0, "a");
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(v.get("a").and_then(Value::as_str), Some("x"));
    }

    #[test]
    fn string_escapes_including_surrogate_pairs() {
        let v = parse(r#""a\n\t\"\\\/é😀""#).unwrap();
        assert_eq!(v.as_str(), Some("a\n\t\"\\/\u{e9}\u{1F600}"));
        // Raw non-ASCII passes through untouched.
        assert_eq!(
            parse("\"日本語 café\"").unwrap().as_str(),
            Some("日本語 café")
        );
    }

    #[test]
    fn rejects_lone_surrogates() {
        assert!(parse(r#""\ud83d""#).is_err());
        assert!(parse(r#""\udc00""#).is_err());
    }

    #[test]
    fn rejects_unescaped_control_characters() {
        assert!(parse("\"a\nb\"").is_err());
    }

    #[test]
    fn rejects_trailing_garbage_and_trailing_commas() {
        assert!(parse("{} extra").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse(r#"{"a":1,}"#).is_err());
    }

    #[test]
    fn rejects_malformed_numbers() {
        // Leading zeros, bare minus and dangling exponents are all invalid JSON.
        for bad in ["01", "-", "1.", "1e", "+1", ".5"] {
            assert!(parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn rejects_excessive_nesting() {
        let deep = "[".repeat(200) + &"]".repeat(200);
        let err = parse(&deep).unwrap_err();
        assert!(err.message.contains("nesting"));
    }

    #[test]
    fn error_reports_byte_offset() {
        let err = parse("[1, x]").unwrap_err();
        assert_eq!(err.offset, 4);
        assert!(err.to_string().contains("byte 4"));
    }

    #[test]
    fn duplicate_keys_first_match_wins_on_get() {
        let v = parse(r#"{"a": 1, "a": 2}"#).unwrap();
        assert_eq!(v.get("a").and_then(Value::as_f64), Some(1.0));
    }
}
