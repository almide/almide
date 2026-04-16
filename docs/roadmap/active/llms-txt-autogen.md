<!-- description: Auto-generate llms.txt from canonical sources (CHEATSHEET, diagnostics, stdlib) -->
# `almide docs-gen` — llms.txt Auto-Generation

Trigger: implement next. `llms.txt` was hand-written in the Phase 3
push; same information lives in `docs/CHEATSHEET.md`, `docs/DESIGN.md`,
`docs/diagnostics/*.md`, and `almide ide stdlib-snapshot`. Every change
to those requires a matching manual edit to `llms.txt` or the two
drift.

## Goal

A single command that rebuilds `llms.txt` from canonical sources:

```bash
almide docs-gen                     # writes llms.txt
almide docs-gen --check             # fails if llms.txt is stale (CI gate)
almide docs-gen --stdout            # print to stdout without writing
```

The output must be byte-stable so CI can `diff` it against the checked-in
file — no timestamps, no tempdir paths, deterministic ordering.

## Sections of llms.txt (source mapping)

| Section | Source | Transform |
|---|---|---|
| Title + mission blurb | `README.md` (first paragraph) | strip markdown links |
| Fast facts | `docs/DESIGN.md` (ambiguity table) | condense to bullet list |
| Link map | filesystem walk of `docs/` | auto-format |
| CLI reference | `src/main.rs` (clap) | extract `#[command]` docs |
| Core idioms | `docs/CHEATSHEET.md` ("Writing Idiomatic Almide") | copy section |
| Diagnostic codes | `docs/diagnostics/*.md` front lines | 1-line title per code |
| Stdlib snapshot pointer | static pointer + `almide ide stdlib-snapshot` | just reference it |
| What Almide is NOT | `docs/REJECTED_PATTERNS.md` | condense |
| Meta (version, repo, branch) | `Cargo.toml`, `git` | read at build |

## Implementation sketch

- New binary / subcommand: `almide docs-gen`.
- Source of truth is **the live repo state**, so no intermediate DB.
- Template for llms.txt lives in `tools/docs-gen/template.md` with
  `{{section:name}}` placeholders.
- Each section is a small function that reads files and returns
  `String`. Order of evaluation is top-down.
- `--check` mode: generate in memory, compare to disk, exit 1 on diff
  with a brief diff summary.

## Non-goals

- Not a general-purpose docs tool. Scoped to llms.txt for now.
- Not replacing `mdbook` or other human-docs tooling.
- Not a templating language; `{{section:x}}` substitution only.

## CI integration (future, not part of MVP)

After the generator works, add a pre-commit hook or CI job that runs
`almide docs-gen --check`. Any PR that changes a source file listed
above but forgets to run `docs-gen` will fail CI.

## Estimated scope

~2 hours for MVP (generator + template + first re-generation).
~1 hour for CI gate + docs.
