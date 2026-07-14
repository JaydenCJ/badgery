//! End-to-end tests against the compiled `badgery` binary: every
//! subcommand, exit codes, file output, the manifest build and the HTTP
//! server. Everything runs against temporary directories and 127.0.0.1.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_badgery")
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("failed to run badgery binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("badgery-cli-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn help_and_version() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    let text = stdout(&help);
    for cmd in ["static", "endpoint", "query", "build", "serve"] {
        assert!(text.contains(cmd), "help must mention '{cmd}'");
    }

    let version = run(&["--version"]);
    assert!(version.status.success());
    assert_eq!(
        stdout(&version).trim(),
        format!("badgery {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn unknown_command_and_bad_flags_exit_2() {
    let out = run(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown command"));

    let out = run(&["static", "a-b-c", "--bogus", "x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--bogus"));

    let out = run(&["static"]);
    assert_eq!(out.status.code(), Some(2), "no SPEC and no --message");
}

#[test]
fn static_renders_shields_path_syntax_to_stdout() {
    let out = run(&["static", "build-passing-brightgreen"]);
    assert!(out.status.success());
    let svg = stdout(&out);
    assert!(svg.starts_with("<svg xmlns="));
    assert!(svg.contains(">build</text>"));
    assert!(svg.contains(">passing</text>"));
    assert!(svg.contains("#4c1"));

    // Escapes: '--' is a dash, '_' a space; flags override the spec.
    let out = run(&[
        "static",
        "docs_site-up--to--date-green",
        "--style",
        "flat-square",
    ]);
    let svg = stdout(&out);
    assert!(svg.contains(">docs site</text>"), "{svg}");
    assert!(svg.contains(">up-to-date</text>"));
    assert!(
        !svg.contains("linearGradient"),
        "flat-square has no gradient"
    );
}

#[test]
fn static_writes_a_file_with_out_and_prints_nothing() {
    let dir = tempdir("out");
    let path = dir.join("build.svg");
    let out = run(&[
        "static",
        "--label",
        "cov",
        "--message",
        "97%",
        "--color",
        "green",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(
        stdout(&out).is_empty(),
        "--out keeps stdout clean for pipes"
    );
    let svg = fs::read_to_string(&path).unwrap();
    assert!(svg.contains(">97%</text>"));
    assert!(svg.contains("#97ca00"));
}

#[test]
fn endpoint_renders_schema_file_and_rejects_bad_schema_with_exit_1() {
    let dir = tempdir("endpoint");
    let ok = dir.join("coverage.json");
    fs::write(
        &ok,
        r#"{"schemaVersion": 1, "label": "coverage", "message": "92%", "color": "green", "style": "plastic"}"#,
    )
    .unwrap();
    let out = run(&["endpoint", ok.to_str().unwrap()]);
    assert!(out.status.success());
    let svg = stdout(&out);
    assert!(svg.contains(">92%</text>"));
    assert!(svg.contains("height=\"18\""), "style from the file applies");

    let bad = dir.join("bad.json");
    fs::write(&bad, r#"{"label": "x", "message": "y"}"#).unwrap();
    let out = run(&["endpoint", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("schemaVersion"));

    let out = run(&["endpoint", dir.join("missing.json").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn endpoint_reads_stdin_when_file_is_dash() {
    let mut child = Command::new(bin())
        .args(["endpoint", "-", "--style", "for-the-badge"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"schemaVersion": 1, "label": "release", "message": "v2.1.0"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let svg = stdout(&out);
    assert!(
        svg.contains(">V2.1.0</text>"),
        "for-the-badge uppercases: {svg}"
    );
    assert!(svg.contains("height=\"28\""));
}

#[test]
fn query_extracts_values_with_prefix_and_fails_cleanly_on_bad_query() {
    let dir = tempdir("query");
    let meta = dir.join("meta.json");
    fs::write(
        &meta,
        r#"{"version": "1.4.2", "tests": {"passed": 89, "failed": 0}}"#,
    )
    .unwrap();

    let out = run(&[
        "query",
        meta.to_str().unwrap(),
        "$.version",
        "--label",
        "version",
        "--prefix",
        "v",
        "--color",
        "blue",
    ]);
    assert!(out.status.success());
    let svg = stdout(&out);
    assert!(svg.contains(">v1.4.2</text>"), "{svg}");
    assert!(svg.contains("#007ec6"));

    let out = run(&[
        "query",
        meta.to_str().unwrap(),
        "$.tests.passed",
        "--label",
        "tests",
        "--suffix",
        " passed",
    ]);
    assert!(stdout(&out).contains(">89 passed</text>"));

    let out = run(&["query", meta.to_str().unwrap(), "$.tests.skipped"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("skipped"),
        "error names the missing key"
    );
}

#[test]
fn build_renders_every_manifest_entry_and_reports_partial_failure() {
    let dir = tempdir("build");
    fs::write(
        dir.join("coverage.json"),
        r#"{"schemaVersion": 1, "label": "coverage", "message": "92%", "color": "green"}"#,
    )
    .unwrap();
    fs::write(dir.join("meta.json"), r#"{"version": "0.9.0"}"#).unwrap();
    let manifest = dir.join("badgery.json");
    fs::write(
        &manifest,
        r#"{
            "outDir": "badges",
            "badges": [
                {"name": "build", "type": "static", "label": "build", "message": "passing", "color": "brightgreen"},
                {"name": "coverage", "type": "endpoint", "file": "coverage.json"},
                {"name": "version", "type": "query", "file": "meta.json", "query": "$.version", "label": "version", "prefix": "v", "color": "blue"}
            ]
        }"#,
    )
    .unwrap();

    // Run from a *different* directory: manifest-relative paths must hold.
    let out = run(&["build", "--manifest", manifest.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("built 3/3 badges"), "{text}");
    for name in ["build", "coverage", "version"] {
        let svg = fs::read_to_string(dir.join("badges").join(format!("{name}.svg"))).unwrap();
        assert!(svg.starts_with("<svg "), "{name}.svg is an SVG");
    }
    let version = fs::read_to_string(dir.join("badges/version.svg")).unwrap();
    assert!(version.contains(">v0.9.0</text>"));

    // Break one data file: build still writes the others but exits 1.
    fs::write(dir.join("meta.json"), "{not json").unwrap();
    let out = run(&["build", "--manifest", manifest.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("built 2/3 badges"));
    assert!(stderr(&out).contains("version"), "failure names the badge");
}

#[test]
fn serve_answers_shields_compatible_urls_on_loopback() {
    let dir = tempdir("serve");
    fs::create_dir_all(dir.join("ci")).unwrap();
    fs::write(
        dir.join("ci/coverage.json"),
        r#"{"schemaVersion": 1, "label": "coverage", "message": "88%", "color": "yellowgreen"}"#,
    )
    .unwrap();

    // Port picked from the ephemeral range, keyed on PID to avoid clashes.
    let port = 20000 + (std::process::id() % 20000) as u16;
    let addr = format!("127.0.0.1:{port}");
    let mut daemon = Command::new(bin())
        .args([
            "serve",
            "--addr",
            &addr,
            "--root",
            dir.to_str().unwrap(),
            "--exit-after",
            "10s",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let get = |target: &str| -> (u16, String) {
        // The server may need a moment to bind; retry the connect only.
        let mut last_err = None;
        for _ in 0..100 {
            match TcpStream::connect(&addr) {
                Ok(mut stream) => {
                    stream
                        .write_all(
                            format!("GET {target} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes(),
                        )
                        .unwrap();
                    let mut response = String::new();
                    stream.read_to_string(&mut response).unwrap();
                    let status: u16 = response
                        .split_whitespace()
                        .nth(1)
                        .and_then(|s| s.parse().ok())
                        .expect("status line");
                    let body = response
                        .split_once("\r\n\r\n")
                        .map(|(_, b)| b.to_string())
                        .unwrap_or_default();
                    return (status, body);
                }
                Err(e) => {
                    last_err = Some(e);
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        panic!("could not connect to {addr}: {last_err:?}");
    };

    let (status, body) = get("/health");
    assert_eq!((status, body.trim()), (200, "ok"));

    let (status, body) = get("/badge/build-passing-brightgreen.svg?style=flat-square");
    assert_eq!(status, 200);
    assert!(body.contains(">passing</text>"));
    assert!(!body.contains("linearGradient"));

    let (status, body) = get("/endpoint?file=ci/coverage.json");
    assert_eq!(status, 200);
    assert!(body.contains(">88%</text>"), "{body}");

    let (status, _) = get("/endpoint?file=../../etc/hostname");
    assert_eq!(status, 400, "traversal is refused");

    let (status, body) = get("/endpoint?file=ci/nothere.json");
    assert_eq!(status, 200, "data errors come back as error badges");
    assert!(body.contains("#e05d44"));

    let (status, _) = get("/nope");
    assert_eq!(status, 404);

    daemon.kill().ok();
    daemon.wait().ok();
}
