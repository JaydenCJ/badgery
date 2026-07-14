# Contributing to badgery

Thanks for your interest in improving badgery. Issues, discussions and pull requests are all welcome.

## Getting started

Prerequisites: Rust 1.75 or newer (stable toolchain).

```bash
git clone https://github.com/JaydenCJ/badgery.git
cd badgery
cargo build
cargo test
bash scripts/smoke.sh
```

`scripts/smoke.sh` builds the binary and exercises every subcommand end to end — static path syntax, an endpoint file, a JSONPath query, a manifest build, and the HTTP server on 127.0.0.1 — against a temporary directory. It finishes in well under a minute and must print `SMOKE OK`.

## Before you open a pull request

1. `cargo fmt` — formatting is enforced.
2. `cargo clippy --all-targets -- -D warnings` — clippy must be clean.
3. `cargo test` — unit tests and the CLI integration tests must pass.
4. `bash scripts/smoke.sh` — the smoke test must print `SMOKE OK`.
5. Add tests for behavior changes. Parsing, measurement and rendering live in pure modules (`json`, `jsonpath`, `color`, `text`, `badge`, `render`, `endpoint`, `manifest`) that are easy to unit-test; please keep it that way. Even the server's routing is a pure function (`server::route`) tested without sockets.

## Ground rules

- **Zero dependencies is the point.** badgery currently depends on nothing outside `std`; adding a crate needs an unusually strong justification in the PR description, because the tool exists to be vendorable into airgapped environments as one reviewable unit.
- **No network calls, ever.** Not at startup, not for telemetry, not for fonts or logos. The server only *listens* (loopback by default) and only reads files under `--root`.
- **Shields compatibility first.** Path escaping, the endpoint schema, palette hex values and geometry follow shields.io; divergences must be deliberate, documented (see `docs/`), and covered by a test.
- Rendered SVG must stay deterministic: same input, same bytes. No timestamps, no randomness in output.
- Code comments and doc comments are written in English.

## Reporting bugs

Please include the `badgery --version` output, the exact command line (or URL for `serve`), the input JSON (redact values if needed), and the generated SVG or error text. Rendering bugs are easiest to fix with a side-by-side: the badge badgery produced and the shields.io badge you expected it to match.

## Security

If you find a security issue (path traversal in `serve`, parser crashes on hostile input), please do not open a public issue. Use GitHub's private vulnerability reporting on this repository instead.
