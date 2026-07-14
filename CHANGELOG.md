# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-13

### Added

- SVG badge renderer with shields.io geometry: Verdana-metric text measurement, explicit `textLength` on every text run, the exact shields palette hex values, and the 0.69-brightness rule for white-vs-dark text.
- Four badge styles: `flat` (default), `flat-square`, `plastic`, `for-the-badge`.
- `badgery static`: shields static path syntax (`build-passing-brightgreen`) with the full escaping rules (`--` dash, `__` underscore, `_` space), message-only badges, and `--label`/`--message`/`--color`/`--label-color`/`--style` flags.
- `badgery endpoint`: reads shields **endpoint-schema** JSON files (`schemaVersion`/`label`/`message`/`color`/`labelColor`/`style`/`isError`) from disk or stdin, with strict validation, shields' override rules, and forced-red `isError` handling (`docs/endpoint-format.md`).
- `badgery query`: extract a value from any local JSON file with a JSONPath subset (`$.key`, `["key"]`, `[0]`, `[-1]`), `--prefix`/`--suffix`, integer-clean number formatting and array joining.
- `badgery build`: a `badgery.json` manifest renders a whole badge set to `<outDir>/<name>.svg` with manifest-relative paths, per-entry error reporting and partial-failure semantics (`docs/manifest.md`).
- `badgery serve`: a loopback HTTP server exposing shields-compatible URLs (`/badge/<spec>.svg`, `/endpoint?file=…`, `/query?file=…&query=…`, `/health`) with query-parameter overrides, path-traversal protection, red error badges for broken data files, and `--exit-after` for supervised runs.
- Strict std-only JSON parser (RFC 8259: surrogate pairs, depth limit, byte-offset errors) and named-color/hex resolution with shields' semantic aliases.
- Test suite: 80 unit tests, 9 CLI integration tests (including a server end-to-end pass), and `scripts/smoke.sh`.

[0.1.0]: https://github.com/JaydenCJ/badgery/releases/tag/v0.1.0
