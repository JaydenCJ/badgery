# The build manifest (`badgery.json`)

`badgery build` renders a whole badge set in one command — the intended CI
pattern: run it after tests, commit or publish `badges/*.svg`, reference
them from your README with plain relative image links. No server needed.

```json
{
  "outDir": "badges",
  "badges": [
    { "name": "build", "type": "static", "label": "build", "message": "passing", "color": "brightgreen" },
    { "name": "coverage", "type": "endpoint", "file": "ci/coverage.json" },
    { "name": "version", "type": "query", "file": "Cargo.metadata.json", "query": "$.packages[0].version", "label": "version", "prefix": "v", "color": "blue" }
  ]
}
```

Every path in the manifest — data files and `outDir` — resolves **relative
to the manifest file**, so `badgery build --manifest path/to/badgery.json`
behaves identically from any working directory.

## Top-level keys

| Key | Default | Effect |
|---|---|---|
| `outDir` | `badges` | Output directory (created if missing); one `<name>.svg` per entry |
| `badges` | required | Non-empty array of entries |

## Entry keys

| Key | Applies to | Effect |
|---|---|---|
| `name` | all | Output file name (letters, digits, `-`, `_`, `.`; unique) |
| `type` | all | `static`, `endpoint` or `query` |
| `label` | all | Left-side text (for `endpoint`: overrides the file) |
| `color` | all | Message color (for `endpoint`: overrides the file) |
| `labelColor` | all | Label color |
| `style` | all | `flat`, `flat-square`, `plastic`, `for-the-badge` |
| `message` | `static` | Right-side text |
| `file` | `endpoint`, `query` | Data file, relative to the manifest |
| `query` | `query` | JSONPath subset: `$.key`, `["key"]`, `[0]`, `[-1]` |
| `prefix` / `suffix` | `query` | Wrapped around the extracted value |

## Failure semantics

`build` renders every entry it can, prints one `badgery: <name>: <reason>`
line per failure to stderr, then exits non-zero if anything failed. A CI
job therefore fails visibly while the healthy badges still get refreshed —
you never lose the whole wall to one broken data file.
