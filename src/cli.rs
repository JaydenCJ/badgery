//! Command-line interface: argument parsing and the five subcommands.
//!
//! Exit codes: `0` success, `1` runtime failure (unreadable file, invalid
//! data), `2` usage error. Badges go to stdout by default so the CLI
//! composes with shell redirection; `--out` writes a file instead.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::badge::{parse_static_path, Badge, Style};
use crate::endpoint::{self, Overrides};
use crate::json;
use crate::jsonpath;
use crate::manifest;
use crate::render;
use crate::server;

const USAGE: &str = "\
badgery — shields-compatible SVG badges from local JSON, fully offline

USAGE:
    badgery <COMMAND> [OPTIONS]

COMMANDS:
    static      Render a badge from shields path syntax or --label/--message
    endpoint    Render a badge from a shields endpoint-schema JSON file
    query       Render a badge from any local JSON file via a JSONPath query
    build       Render every badge declared in a badgery.json manifest
    serve       Serve shields-compatible badge URLs from local files
    help        Print this help

OPTIONS:
    -h, --help       Print help (add after a command for its options)
    -V, --version    Print version

Run 'badgery <COMMAND> --help' for command-specific options.
";

const STATIC_HELP: &str = "\
badgery static — render a fixed badge

USAGE:
    badgery static <SPEC> [OPTIONS]
    badgery static --label <TEXT> --message <TEXT> [OPTIONS]

SPEC uses shields path syntax: <label>-<message>-<color>, with '--' for a
literal dash, '__' for a literal underscore and '_' for a space.
Example: badgery static build-passing-brightgreen

OPTIONS:
    --label <TEXT>          Left-side text (overrides SPEC)
    --message <TEXT>        Right-side text (overrides SPEC)
    --color <COLOR>         Message background (name or hex; overrides SPEC)
    --label-color <COLOR>   Label background (default: grey)
    --style <STYLE>         flat | flat-square | plastic | for-the-badge
    --out <FILE>            Write the SVG to FILE instead of stdout
";

const ENDPOINT_HELP: &str = "\
badgery endpoint — render a badge from a shields endpoint-schema JSON file

USAGE:
    badgery endpoint <FILE> [OPTIONS]

FILE is a local JSON document with schemaVersion/label/message/... exactly
as documented for shields.io endpoint badges ('-' reads stdin).

OPTIONS:
    --label <TEXT>          Override the label from the file
    --color <COLOR>         Override the message color (ignored on isError)
    --label-color <COLOR>   Override the label color
    --style <STYLE>         flat | flat-square | plastic | for-the-badge
    --out <FILE>            Write the SVG to FILE instead of stdout
";

const QUERY_HELP: &str = "\
badgery query — render a badge from any JSON file via a JSONPath query

USAGE:
    badgery query <FILE> <QUERY> [OPTIONS]

QUERY is a JSONPath subset: $.key, [\"key\"], [0], [-1].
Example: badgery query package.json '$.version' --label version --prefix v

OPTIONS:
    --label <TEXT>          Left-side text (default: empty)
    --prefix <TEXT>         Prepended to the extracted value
    --suffix <TEXT>         Appended to the extracted value
    --color <COLOR>         Message background (default: lightgrey)
    --label-color <COLOR>   Label background (default: grey)
    --style <STYLE>         flat | flat-square | plastic | for-the-badge
    --out <FILE>            Write the SVG to FILE instead of stdout
";

const BUILD_HELP: &str = "\
badgery build — render every badge declared in a manifest

USAGE:
    badgery build [OPTIONS]

Reads badgery.json (format: docs/manifest.md, example: examples/badgery.json),
renders each entry and writes <outDir>/<name>.svg. Paths in the manifest
resolve relative to the manifest file, so the command works from any directory.

OPTIONS:
    --manifest <FILE>       Manifest path (default: badgery.json)
    --out-dir <DIR>         Override the manifest's outDir
";

const SERVE_HELP: &str = "\
badgery serve — shields-compatible badge URLs from local files

USAGE:
    badgery serve [OPTIONS]

ROUTES:
    /badge/<label>-<message>-<color>.svg   static badge (query params:
                                           style, label, labelColor, color)
    /endpoint?file=<rel.json>              endpoint-schema badge
    /query?file=<rel.json>&query=$.x       dynamic JSON badge (label,
                                           prefix, suffix, color, style)
    /health                                liveness probe

OPTIONS:
    --addr <HOST:PORT>      Bind address (default: 127.0.0.1:8331)
    --root <DIR>            Directory data files resolve under (default: .)
    --exit-after <DUR>      Shut down after DUR (e.g. 30s, 5m) — for tests
";

/// Entry point used by `main`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut args = args.iter().map(String::as_str);
    let command = match args.next() {
        None => {
            eprint!("{USAGE}");
            return 2;
        }
        Some(c) => c,
    };
    let rest: Vec<&str> = args.collect();
    match command {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            0
        }
        "-V" | "--version" => {
            println!("badgery {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "static" => cmd_static(&rest),
        "endpoint" => cmd_endpoint(&rest),
        "query" => cmd_query(&rest),
        "build" => cmd_build(&rest),
        "serve" => cmd_serve(&rest),
        other => {
            eprintln!("badgery: unknown command '{other}'\n");
            eprint!("{USAGE}");
            2
        }
    }
}

/// Parsed flags shared by the rendering subcommands.
#[derive(Debug, Default)]
struct Flags {
    positionals: Vec<String>,
    label: Option<String>,
    message: Option<String>,
    color: Option<String>,
    label_color: Option<String>,
    style: Option<Style>,
    out: Option<PathBuf>,
    prefix: Option<String>,
    suffix: Option<String>,
    manifest: Option<PathBuf>,
    out_dir: Option<String>,
    addr: Option<String>,
    root: Option<PathBuf>,
    exit_after: Option<Duration>,
    help: bool,
}

fn parse_flags(args: &[&str], allowed: &[&str]) -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut it = args.iter().copied();
    let value_for = |flag: &str, it: &mut dyn Iterator<Item = &str>| -> Result<String, String> {
        it.next()
            .map(str::to_string)
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(arg) = it.next() {
        match arg {
            "-h" | "--help" => flags.help = true,
            _ if arg.starts_with("--") => {
                if !allowed.contains(&arg) {
                    return Err(format!("unknown option '{arg}'"));
                }
                let value = value_for(arg, &mut it)?;
                match arg {
                    "--label" => flags.label = Some(value),
                    "--message" => flags.message = Some(value),
                    "--color" => flags.color = Some(value),
                    "--label-color" => flags.label_color = Some(value),
                    "--style" => {
                        flags.style = Some(Style::parse(&value).ok_or_else(|| {
                            format!(
                                "unknown style '{value}' (expected one of: {})",
                                Style::ALL.map(Style::name).join(", ")
                            )
                        })?)
                    }
                    "--out" => flags.out = Some(PathBuf::from(value)),
                    "--prefix" => flags.prefix = Some(value),
                    "--suffix" => flags.suffix = Some(value),
                    "--manifest" => flags.manifest = Some(PathBuf::from(value)),
                    "--out-dir" => flags.out_dir = Some(value),
                    "--addr" => flags.addr = Some(value),
                    "--root" => flags.root = Some(PathBuf::from(value)),
                    "--exit-after" => flags.exit_after = Some(parse_duration(&value)?),
                    _ => unreachable!("gated by `allowed`"),
                }
            }
            positional => flags.positionals.push(positional.to_string()),
        }
    }
    Ok(flags)
}

/// Parse durations like `500ms`, `30s`, `5m`, `2h` (bare numbers = seconds).
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (number, unit): (&str, &str) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(split) => (&s[..split], &s[split..]),
        None => (s, "s"),
    };
    let n: u64 = number
        .parse()
        .map_err(|_| format!("invalid duration '{s}'"))?;
    let millis = match unit {
        "ms" => n,
        "s" => n * 1_000,
        "m" => n * 60_000,
        "h" => n * 3_600_000,
        _ => return Err(format!("invalid duration unit '{unit}' (use ms, s, m, h)")),
    };
    Ok(Duration::from_millis(millis))
}

/// Emit a rendered badge to `--out` or stdout.
fn emit(svg: &str, out: Option<&Path>) -> Result<(), String> {
    match out {
        None => {
            print!("{svg}");
            Ok(())
        }
        Some(path) => {
            std::fs::write(path, svg).map_err(|e| format!("cannot write {}: {e}", path.display()))
        }
    }
}

/// Read a JSON document from a path, `-` meaning stdin.
fn read_document(path: &str) -> Result<json::Value, String> {
    let text = if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        buf
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?
    };
    json::parse(&text).map_err(|e| format!("{path}: {e}"))
}

fn usage_error(message: &str, help: &str) -> i32 {
    eprintln!("badgery: {message}\n");
    eprint!("{help}");
    2
}

fn runtime_error(message: &str) -> i32 {
    eprintln!("badgery: {message}");
    1
}

fn cmd_static(args: &[&str]) -> i32 {
    const ALLOWED: &[&str] = &[
        "--label",
        "--message",
        "--color",
        "--label-color",
        "--style",
        "--out",
    ];
    let flags = match parse_flags(args, ALLOWED) {
        Ok(f) => f,
        Err(e) => return usage_error(&e, STATIC_HELP),
    };
    if flags.help {
        print!("{STATIC_HELP}");
        return 0;
    }
    if flags.positionals.len() > 1 {
        return usage_error("too many arguments", STATIC_HELP);
    }
    let (mut label, mut message, mut color_token) = (String::new(), None::<String>, None::<String>);
    if let Some(spec) = flags.positionals.first() {
        match parse_static_path(spec) {
            Ok((l, m, c)) => {
                label = l;
                message = Some(m);
                color_token = Some(c);
            }
            Err(e) => return usage_error(&e, STATIC_HELP),
        }
    }
    if let Some(l) = flags.label {
        label = l;
    }
    if let Some(m) = flags.message {
        message = Some(m);
    }
    if let Some(c) = flags.color {
        color_token = Some(c);
    }
    let Some(message) = message else {
        return usage_error("a SPEC or --message is required", STATIC_HELP);
    };
    let mut badge = Badge::new(label, message);
    if let Some(c) = color_token {
        badge = badge.with_color(&c);
    }
    if let Some(lc) = flags.label_color {
        badge = badge.with_label_color(&lc);
    }
    if let Some(style) = flags.style {
        badge = badge.with_style(style);
    }
    match emit(&render::render(&badge), flags.out.as_deref()) {
        Ok(()) => 0,
        Err(e) => runtime_error(&e),
    }
}

fn cmd_endpoint(args: &[&str]) -> i32 {
    const ALLOWED: &[&str] = &["--label", "--color", "--label-color", "--style", "--out"];
    let flags = match parse_flags(args, ALLOWED) {
        Ok(f) => f,
        Err(e) => return usage_error(&e, ENDPOINT_HELP),
    };
    if flags.help {
        print!("{ENDPOINT_HELP}");
        return 0;
    }
    let [file] = flags.positionals.as_slice() else {
        return usage_error("exactly one FILE argument is required", ENDPOINT_HELP);
    };
    let doc = match read_document(file) {
        Ok(doc) => doc,
        Err(e) => return runtime_error(&e),
    };
    let spec = match endpoint::parse_spec(&doc) {
        Ok(spec) => spec,
        Err(e) => return runtime_error(&format!("{file}: {e}")),
    };
    let overrides = Overrides {
        label: flags.label,
        color: flags.color,
        label_color: flags.label_color,
        style: flags.style,
    };
    let badge = endpoint::to_badge(&spec, &overrides);
    match emit(&render::render(&badge), flags.out.as_deref()) {
        Ok(()) => 0,
        Err(e) => runtime_error(&e),
    }
}

fn cmd_query(args: &[&str]) -> i32 {
    const ALLOWED: &[&str] = &[
        "--label",
        "--prefix",
        "--suffix",
        "--color",
        "--label-color",
        "--style",
        "--out",
    ];
    let flags = match parse_flags(args, ALLOWED) {
        Ok(f) => f,
        Err(e) => return usage_error(&e, QUERY_HELP),
    };
    if flags.help {
        print!("{QUERY_HELP}");
        return 0;
    }
    let [file, expr] = flags.positionals.as_slice() else {
        return usage_error("expected exactly: <FILE> <QUERY>", QUERY_HELP);
    };
    let doc = match read_document(file) {
        Ok(doc) => doc,
        Err(e) => return runtime_error(&e),
    };
    let value = match jsonpath::query(&doc, expr) {
        Ok(v) => v,
        Err(e) => return runtime_error(&format!("{file}: {e}")),
    };
    let message = format!(
        "{}{value}{}",
        flags.prefix.unwrap_or_default(),
        flags.suffix.unwrap_or_default()
    );
    let mut badge = Badge::new(flags.label.unwrap_or_default(), message);
    if let Some(c) = flags.color {
        badge = badge.with_color(&c);
    }
    if let Some(lc) = flags.label_color {
        badge = badge.with_label_color(&lc);
    }
    if let Some(style) = flags.style {
        badge = badge.with_style(style);
    }
    match emit(&render::render(&badge), flags.out.as_deref()) {
        Ok(()) => 0,
        Err(e) => runtime_error(&e),
    }
}

fn cmd_build(args: &[&str]) -> i32 {
    const ALLOWED: &[&str] = &["--manifest", "--out-dir"];
    let flags = match parse_flags(args, ALLOWED) {
        Ok(f) => f,
        Err(e) => return usage_error(&e, BUILD_HELP),
    };
    if flags.help {
        print!("{BUILD_HELP}");
        return 0;
    }
    if !flags.positionals.is_empty() {
        return usage_error("build takes no positional arguments", BUILD_HELP);
    }
    let manifest_path = flags
        .manifest
        .unwrap_or_else(|| PathBuf::from("badgery.json"));
    let doc = match read_document(&manifest_path.to_string_lossy()) {
        Ok(doc) => doc,
        Err(e) => return runtime_error(&e),
    };
    let mut manifest = match manifest::parse_manifest(&doc) {
        Ok(m) => m,
        Err(e) => return runtime_error(&format!("{}: {e}", manifest_path.display())),
    };
    if let Some(dir) = flags.out_dir {
        manifest.out_dir = dir;
    }
    let base = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let out_dir = base.join(&manifest.out_dir);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return runtime_error(&format!("cannot create {}: {e}", out_dir.display()));
    }
    let total = manifest.badges.len();
    let mut failures = 0;
    for entry in &manifest.badges {
        match manifest::resolve_entry(entry, &base) {
            Ok(badge) => {
                let path = out_dir.join(format!("{}.svg", entry.name));
                match std::fs::write(&path, render::render(&badge)) {
                    Ok(()) => println!("wrote {}", path.display()),
                    Err(e) => {
                        eprintln!(
                            "badgery: {}: cannot write {}: {e}",
                            entry.name,
                            path.display()
                        );
                        failures += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("badgery: {}: {e}", entry.name);
                failures += 1;
            }
        }
    }
    let noun = if total == 1 { "badge" } else { "badges" };
    println!(
        "built {}/{total} {noun} in {}",
        total - failures,
        out_dir.display()
    );
    if failures > 0 {
        1
    } else {
        0
    }
}

fn cmd_serve(args: &[&str]) -> i32 {
    const ALLOWED: &[&str] = &["--addr", "--root", "--exit-after"];
    let flags = match parse_flags(args, ALLOWED) {
        Ok(f) => f,
        Err(e) => return usage_error(&e, SERVE_HELP),
    };
    if flags.help {
        print!("{SERVE_HELP}");
        return 0;
    }
    if !flags.positionals.is_empty() {
        return usage_error("serve takes no positional arguments", SERVE_HELP);
    }
    let root = flags.root.unwrap_or_else(|| PathBuf::from("."));
    if !root.is_dir() {
        return runtime_error(&format!("--root {}: not a directory", root.display()));
    }
    let config = server::Config {
        addr: flags.addr.unwrap_or_else(|| "127.0.0.1:8331".to_string()),
        root,
        exit_after: flags.exit_after,
    };
    match server::serve(&config) {
        Ok(()) => 0,
        Err(e) => runtime_error(&format!("server error on {}: {e}", config.addr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse_with_all_units() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(
            parse_duration("7").unwrap(),
            Duration::from_secs(7),
            "bare = seconds"
        );
        for bad in ["", "s", "10x", "-3s", "1.5s"] {
            assert!(parse_duration(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn parse_flags_rejects_unknown_and_missing_values() {
        let err = parse_flags(&["--bogus", "x"], &["--label"]).unwrap_err();
        assert!(err.contains("--bogus"));
        let err = parse_flags(&["--label"], &["--label"]).unwrap_err();
        assert!(err.contains("needs a value"));
    }

    #[test]
    fn parse_flags_collects_positionals_and_options() {
        let flags = parse_flags(&["spec", "--style", "plastic"], &["--style"]).unwrap();
        assert_eq!(flags.positionals, vec!["spec".to_string()]);
        assert_eq!(flags.style, Some(Style::Plastic));
    }
}
