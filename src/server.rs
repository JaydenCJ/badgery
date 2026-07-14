//! A tiny loopback HTTP server exposing shields-compatible badge URLs.
//!
//! Point internal README image tags at it and existing shields URL muscle
//! memory keeps working: `/badge/build-passing-brightgreen.svg`,
//! `/endpoint?file=ci/coverage.json`, `/query?file=meta.json&query=$.version`.
//! The server never makes an outbound connection: every byte it serves
//! comes from the request or from files under `--root`. Routing is a pure
//! function ([`route`]) so the whole HTTP surface is unit-testable without
//! sockets.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::badge::{parse_static_path, Badge, Style};
use crate::endpoint::{self, Overrides};
use crate::json;
use crate::jsonpath;
use crate::render;

/// Server configuration (already validated by the CLI).
pub struct Config {
    pub addr: String,
    pub root: PathBuf,
    /// Exit cleanly after this long — for supervised runs, tests, demos.
    pub exit_after: Option<Duration>,
}

/// A response ready to serialize.
#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    fn svg(body: String) -> Response {
        Response {
            status: 200,
            content_type: "image/svg+xml; charset=utf-8",
            body,
        }
    }

    fn text(status: u16, body: impl Into<String>) -> Response {
        Response {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.into(),
        }
    }
}

/// Percent-decode a URL path segment (no `+`-as-space here; that rule only
/// applies to query strings). Invalid sequences pass through literally.
pub fn decode_path(s: &str) -> String {
    decode(s, false)
}

/// Parse `a=1&b=two%20words&c` into pairs; `+` decodes to space.
pub fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((k, v)) => (decode(k, true), decode(v, true)),
            None => (decode(part, true), String::new()),
        })
        .collect()
}

fn decode(s: &str, plus_is_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| -> Option<u8> {
                    match b {
                        b'0'..=b'9' => Some(b - b'0'),
                        b'a'..=b'f' => Some(b - b'a' + 10),
                        b'A'..=b'F' => Some(b - b'A' + 10),
                        _ => None,
                    }
                };
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(hi), Some(lo)) => {
                        out.push(hi * 16 + lo);
                        i += 3;
                    }
                    _ => {
                        // Malformed escape: keep the literal '%'.
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Join a user-supplied relative path onto the served root, refusing
/// absolute paths, parent traversal and other escape attempts.
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\\') || rel.contains('\0') {
        return None;
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return None;
    }
    let mut joined = root.to_path_buf();
    for component in rel_path.components() {
        match component {
            Component::Normal(part) => joined.push(part),
            // `.` is harmless but pointless; anything else is an escape.
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(joined)
}

fn find(params: &[(String, String)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

fn overrides_from(params: &[(String, String)]) -> Overrides {
    Overrides {
        label: find(params, "label"),
        color: find(params, "color"),
        label_color: find(params, "labelColor"),
        style: find(params, "style").and_then(|s| Style::parse(&s)),
    }
}

/// Render an error as a badge, shields-style: a broken data file must show
/// up **on the page** as a red badge, not as a broken image icon.
fn error_badge(message: &str) -> Response {
    let badge = Badge::new("badgery", message).with_color("red");
    Response::svg(render::render(&badge))
}

/// Route one GET request. Pure with respect to the network: only local
/// files under `root` are ever read.
pub fn route(target: &str, root: &Path) -> Response {
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let params = parse_query(query);

    if path == "/health" {
        return Response::text(200, "ok\n");
    }

    if let Some(spec) = path.strip_prefix("/badge/") {
        let Some(spec) = spec.strip_suffix(".svg") else {
            return Response::text(404, "badge paths end in .svg\n");
        };
        return match parse_static_path(&decode_path(spec)) {
            Ok((label, message, color_token)) => {
                let ov = overrides_from(&params);
                let mut badge = Badge::new(ov.label.unwrap_or(label), message)
                    .with_color(&ov.color.unwrap_or(color_token));
                if let Some(lc) = ov.label_color {
                    badge = badge.with_label_color(&lc);
                }
                if let Some(style) = ov.style {
                    badge = badge.with_style(style);
                }
                Response::svg(render::render(&badge))
            }
            Err(e) => Response::text(400, format!("{e}\n")),
        };
    }

    if path == "/endpoint" || path == "/query" {
        let Some(rel) = find(&params, "file") else {
            return Response::text(400, "missing required parameter 'file'\n");
        };
        let Some(full) = safe_join(root, &rel) else {
            return Response::text(400, "invalid path: must be relative, inside the root\n");
        };
        let doc = match std::fs::read_to_string(&full) {
            Ok(text) => match json::parse(&text) {
                Ok(doc) => doc,
                Err(e) => return error_badge(&format!("invalid JSON: {}", e.message)),
            },
            Err(_) => return error_badge("file not found"),
        };
        if path == "/endpoint" {
            return match endpoint::parse_spec(&doc) {
                Ok(spec) => Response::svg(render::render(&endpoint::to_badge(
                    &spec,
                    &overrides_from(&params),
                ))),
                Err(e) => error_badge(&e),
            };
        }
        // /query
        let Some(expr) = find(&params, "query") else {
            return Response::text(400, "missing required parameter 'query'\n");
        };
        return match jsonpath::query(&doc, &expr) {
            Ok(value) => {
                let prefix = find(&params, "prefix").unwrap_or_default();
                let suffix = find(&params, "suffix").unwrap_or_default();
                let ov = overrides_from(&params);
                let mut badge = Badge::new(
                    ov.label.unwrap_or_default(),
                    format!("{prefix}{value}{suffix}"),
                );
                if let Some(c) = ov.color {
                    badge = badge.with_color(&c);
                }
                if let Some(lc) = ov.label_color {
                    badge = badge.with_label_color(&lc);
                }
                if let Some(style) = ov.style {
                    badge = badge.with_style(style);
                }
                Response::svg(render::render(&badge))
            }
            Err(e) => error_badge(&e),
        };
    }

    Response::text(
        404,
        "not found; try /badge/<spec>.svg, /endpoint, /query, /health\n",
    )
}

/// Run the server until `exit_after` elapses (or forever).
pub fn serve(config: &Config) -> std::io::Result<()> {
    let listener = TcpListener::bind(&config.addr)?;
    listener.set_nonblocking(true)?;
    let started = Instant::now();
    println!(
        "badgery {} serving http://{}/ (root: {})",
        env!("CARGO_PKG_VERSION"),
        listener.local_addr()?,
        config.root.display()
    );
    loop {
        if let Some(limit) = config.exit_after {
            if started.elapsed() >= limit {
                println!("badgery: --exit-after reached, shutting down");
                return Ok(());
            }
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(e) = handle(stream, &config.root) {
                    eprintln!("badgery: connection error: {e}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e),
        }
    }
}

fn handle(mut stream: std::net::TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_nodelay(true)?;

    // Read until end of headers (bounded; badgery only ever needs the
    // request line, and request bodies are not part of the API).
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");

    let response = match method {
        "GET" | "HEAD" => route(target, root),
        _ => Response::text(405, "method not allowed; badgery is GET-only\n"),
    };
    println!("{method} {target} -> {}", response.status);

    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let mut out = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    if method != "HEAD" {
        out.push_str(&response.body);
    }
    stream.write_all(out.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        // One directory per call: tests run in parallel threads, so a shared
        // path (e.g. keyed only by process id) would let one test delete the
        // fixture files while another is reading them.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "badgery-server-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("ci")).unwrap();
        std::fs::write(
            dir.join("ci/coverage.json"),
            r#"{"schemaVersion": 1, "label": "coverage", "message": "92%", "color": "green"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("meta.json"), r#"{"version": "1.2.0"}"#).unwrap();
        std::fs::write(dir.join("broken.json"), "{oops").unwrap();
        dir
    }

    #[test]
    fn percent_decoding_handles_utf8_and_plus() {
        assert_eq!(decode_path("hello%20world"), "hello world");
        assert_eq!(decode_path("a+b"), "a+b", "path '+' stays literal");
        assert_eq!(decode_path("%E6%97%A5"), "日");
        let q = parse_query("label=hello+world&x=%2Fetc");
        assert_eq!(q[0], ("label".into(), "hello world".into()));
        assert_eq!(q[1], ("x".into(), "/etc".into()));
        // Malformed sequences pass through literally instead of erroring.
        assert_eq!(decode_path("100%"), "100%");
        assert_eq!(decode_path("%zz"), "%zz");
    }

    #[test]
    fn safe_join_accepts_nested_relative_paths() {
        let joined = safe_join(Path::new("/srv/data"), "ci/coverage.json").unwrap();
        assert_eq!(joined, Path::new("/srv/data/ci/coverage.json"));
    }

    #[test]
    fn safe_join_rejects_escape_attempts() {
        let root = Path::new("/srv/data");
        for bad in ["../secret", "a/../../b", "/etc/passwd", "a\\b", ""] {
            assert_eq!(safe_join(root, bad), None, "{bad:?} must be rejected");
        }
    }

    #[test]
    fn badge_route_renders_static_svg_with_overrides() {
        let dir = root();
        let r = route("/badge/build-passing-brightgreen.svg", &dir);
        assert_eq!(r.status, 200);
        assert!(r.content_type.starts_with("image/svg+xml"));
        assert!(r.body.contains(">passing</text>"));
        // style + color come from the query string, shields-style.
        let r = route(
            "/badge/build-passing-red.svg?style=flat-square&color=blue",
            &dir,
        );
        assert!(r.body.contains("#007ec6"), "query color override wins");
        assert!(!r.body.contains("linearGradient"), "flat-square applied");
    }

    #[test]
    fn endpoint_route_reads_file_under_root() {
        let dir = root();
        let r = route("/endpoint?file=ci/coverage.json", &dir);
        assert_eq!(r.status, 200);
        assert!(r.body.contains(">92%</text>"), "{}", r.body);
        assert!(r.body.contains("#97ca00"), "named green resolved");
    }

    #[test]
    fn query_route_extracts_value_with_prefix() {
        let dir = root();
        let r = route(
            "/query?file=meta.json&query=%24.version&label=version&prefix=v&color=blue",
            &dir,
        );
        assert_eq!(r.status, 200);
        assert!(r.body.contains(">v1.2.0</text>"), "{}", r.body);
    }

    #[test]
    fn data_errors_render_a_red_error_badge_not_a_broken_image() {
        let dir = root();
        for target in [
            "/endpoint?file=missing.json",
            "/endpoint?file=broken.json",
            "/query?file=meta.json&query=%24.nope",
        ] {
            let r = route(target, &dir);
            assert_eq!(r.status, 200, "{target}");
            assert!(r.content_type.starts_with("image/svg+xml"), "{target}");
            assert!(r.body.contains("badgery"), "error badge label: {target}");
            assert!(r.body.contains("#e05d44"), "error badge is red: {target}");
        }
    }

    #[test]
    fn traversal_and_missing_params_are_hard_400s() {
        let dir = root();
        assert_eq!(route("/endpoint?file=../etc/passwd", &dir).status, 400);
        assert_eq!(route("/endpoint", &dir).status, 400);
        assert_eq!(route("/query?file=meta.json", &dir).status, 400);
    }

    #[test]
    fn unknown_paths_are_404_and_health_is_200() {
        let dir = root();
        assert_eq!(route("/health", &dir).status, 200);
        assert_eq!(route("/nope", &dir).status, 404);
        assert_eq!(
            route("/badge/build-passing-green", &dir).status,
            404,
            "missing .svg"
        );
    }
}
