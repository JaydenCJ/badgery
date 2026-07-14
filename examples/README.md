# badgery examples

Runnable inputs for every subcommand. All commands assume you are in this
directory and built badgery with `cargo build` (or installed it).

```bash
# A shields endpoint-schema file -> SVG on stdout
badgery endpoint coverage.json

# Pull one value out of arbitrary JSON (a release manifest here)
badgery query release.json '$.version' --label version --prefix v --color blue
badgery query release.json '$.artifacts[0].target' --label linux --color informational

# Render the whole badge set declared in badgery.json into ./badges/
badgery build

# Serve the same files over shields-compatible URLs (loopback only)
badgery serve --root . --addr 127.0.0.1:8331
# then: curl 'http://127.0.0.1:8331/endpoint?file=coverage.json'
# or embed: <img src="http://127.0.0.1:8331/badge/build-passing-brightgreen.svg">
```

Files:

- `coverage.json` — a shields **endpoint schema** document, byte-compatible
  with what you would host for shields.io's endpoint badge.
- `release.json` — arbitrary JSON, queried with `badgery query` / `/query`.
- `badgery.json` — a **build manifest**; `badgery build` renders each entry
  to `badges/<name>.svg` (paths resolve relative to the manifest).
