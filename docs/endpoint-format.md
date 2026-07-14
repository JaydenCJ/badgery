# The endpoint document format

`badgery endpoint <file>` and `GET /endpoint?file=<rel>` read a JSON
document in the shields.io **endpoint badge** schema. A file you already
host for shields works with badgery unchanged, and vice versa.

## Fields

| Field | Required | Type | Meaning |
|---|---|---|---|
| `schemaVersion` | yes | number | Must be exactly `1`. |
| `label` | yes | string | Left-side text. May be `""` for a message-only badge. |
| `message` | yes | string | Right-side text. Must be non-empty. |
| `color` | no | string | Message background. Default `lightgrey`. |
| `labelColor` | no | string | Label background. Default `grey`. |
| `style` | no | string | `flat` (default), `flat-square`, `plastic`, `for-the-badge`. |
| `isError` | no | boolean | Render as an error badge (see below). Default `false`. |

Accepted but ignored (they only make sense for the hosted shields service):
`namedLogo`, `logoSvg`, `logoColor`, `logoWidth`, `logoPosition`,
`cacheSeconds`. Unknown fields are ignored too, so a spec can carry extra
metadata for other tools.

## Colors

`color` and `labelColor` accept the shields palette (`brightgreen`, `green`,
`yellowgreen`, `yellow`, `orange`, `red`, `blue`, `grey`, `lightgrey`, plus
`gray`/`lightgray` spellings), the semantic aliases (`success`, `important`,
`critical`, `informational`, `inactive`) and 3- or 6-digit hex with or
without `#`. An unrecognized color falls back to the default — the same
forgiving behavior as shields — so a typo degrades the badge instead of
breaking the pipeline.

## Overrides

CLI flags (`--label`, `--color`, `--label-color`, `--style`) and the
matching server query parameters (`label`, `color`, `labelColor`, `style`)
override the file. The **message can never be overridden** — it is the data
the badge exists to report.

## Error badges (`isError: true`)

Set `isError` when the *producer* of the file failed (a coverage run that
crashed, a scanner that timed out). badgery then:

1. forces the message background to red (`#e05d44`), and
2. ignores any `color` override — an error state must not be paintable
   green from the URL.

`label`, `labelColor` (from the file) and `style` still apply.

## Validation

Unlike hosted shields, badgery is strict about the fields it *does* read:
a missing `schemaVersion`, a numeric `label`, an unknown `style` or an
empty `message` are hard errors (exit code 1 on the CLI). Broken data in
CI should fail loudly, not render a mystery badge. The one exception is the
server's badge routes, which return a red **error badge** with HTTP 200 so
that a broken data file shows up on the page instead of as a broken image
icon.
