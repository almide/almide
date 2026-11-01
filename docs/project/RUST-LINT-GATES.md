# Rust lint and formatting gates (#1462)

The three lanes the compiler-championship survey found missing, and where
each one stands. This is the ledger the exclusion below is recorded in —
nothing here is implicit in a Makefile comment.

## Live lanes

| Lane | Gate | Discipline |
|---|---|---|
| `.almd` formatting, whole tree | `scripts/check-fmt-gate.sh` (CI `checks` job): `spec/` + `examples/` + `tools/almide-gates/` plain (excluding deliberately invalid gauntlet cells), `stdlib/` under `--no-import-edit` (splice-context sources, so import auto-insertion is excluded; the AST-conservation verifier still guards every rewrite) | any drift is red |
| rustc warnings | `scripts/check-rustc-warnings.sh` (CI `build` job) | shrink-only baseline, currently 0, fails both directions |
| clippy warnings | `scripts/check-clippy-warnings.sh` (CI `build` job) | shrink-only baseline (`scripts/clippy-warnings-baseline.txt`): growth fails on PRs; a lower count is a notice there and fails in the nightly strict `clippy-baseline.yml` lane; deny-level lints (e.g. `overly_complex_bool_expr`) fail the compile itself — the first run caught a vestigial `\|\| true` in the parser's named-arg path |

## Narrow prose-only CI lane

For allowlisted prose-only changes, CI can reuse the Linux compiler artifact
from a successful `ci.yml` push run at the exact compared base commit in this
repository. The artifact must have matching repository/run/SHA provenance and
a verified SHA-256 digest. Missing, expired, untrusted, or unverifiable artifacts
fall back to the normal build; this is not a promise that every documentation
change avoids compilation.

The classifier is loaded from the trusted comparison base. Source, workflow,
generator, compiled-in diagnostic/template, normative ALS, and unknown paths
are not eligible for this new prose fast path. A merge group qualifies only
when every changed path qualifies. Otherwise the normal compiler build and
existing job selection apply: the older `docs_only` classifier may still skip
its heavy test matrix for some documentation paths. A false `prose_only` value
is therefore not a promise that every job runs. Source/workflow changes also
fail the older docs-only classification. Main-target cross-platform builds
remain unchanged. Initial installation cannot reuse this lane because its
base does not yet contain the classifier.

### What the lane admits

Root `README*.md`, `CLAUDE.md`, `CHANGELOG.md`, `SUPPORT.md`, `SECURITY.md`;
the top-level `docs/` pages listed in `TOP_DOCS`; `docs/benchmarks/README.md`;
`.claude/commands/*.md`; and `docs/{roadmap,specs,design,project}/**/*.md`
minus the files below. A path is admitted only after checking what reads it
(`include_str!`, Rust and Almide tests, gates in jobs the lane skips).

Not admitted, because something other than a kept documentation check reads
them: `docs/GRAMMAR.md` (a syntax test parses its `assign` rule),
`docs/stdlib/**` (generated signature indexes several Rust tests parse),
`docs/specs/cli.md`, `docs/project/CLAUDE_TEMPLATE.md`, `docs/diagnostics/**`,
`docs/specs/als/**`, and the generated `docs/roadmap/README.md`. The older
`docs_only` classifier treats the first three as code as well, so changing them
runs the Rust test matrix.

A change to an admitted file also leaves the lane when any changed line is an
HTML comment or sits inside a generated block (`…:generated:start/end`,
`BEGIN/END GENERATED …`, `…: begin/end`), on either side of the diff. Those
lines feed generators and freshness gates, so the full lane runs for them.

### What still runs, and what is skipped

In the lane, `Build (Linux)` produces only the release binary (from the
verified base artifact when one exists). It skips the nextest archive and the
rustc/clippy ratchets, because nothing in the lane reads them. Lean, the
Acceptance Ring, Cross-Target and the mutation sweep are skipped. Emit &
Format still runs every step that reads a document: the LLM-surface fences,
`docs-gen --check`, stdlib and roadmap indexes, the generated blocks on the
measurements page, contract traceability (which reads the headings that
contract declarations cite), contract citations, and the grammar gate. It
skips only steps that read compiler source, `spec/` corpora, fixtures,
`docs/stdlib` (outside the lane), release tags or the GitHub API. Those steps
are the JS host gate, stability closure, installer attestation, release seals,
interface-differ controls, pass-isolated fixtures, pass-order shuffle,
ownership certifier, spec-test-file runner checks and doc fences. The list is
pinned by `test_emit_and_format_lane_skips_only_the_audited_steps`; to add a
step, first audit what it reads. A failed or empty classification runs the
whole job.

Named required matrix contexts continue to report results;
branch protection and merge-queue requirements do not change. Scheduled/manual
verification remains on the full lane. See [`scripts/ci_prose.py`](../../scripts/ci_prose.py) and its offline
regression controls in [`scripts/test_ci_prose.py`](../../scripts/test_ci_prose.py) for the exact allowlist and
failure behavior. The classifier prints its reason (`prose lane: "…"`) in the
`Change class` job log of each workflow.

### Reading a run

Each workflow's classification job prints one line, `prose lane: "<reason>"`,
followed by `prose_only=true|false`. CI calls this job `Change class`; the
Acceptance Ring, Cross-Target and mutation sweep call it `prose-changes`. A
`false` reason names the first path or line that kept the change out of the
lane. When the lane is taken, the `Lane` step of Emit & Format prints
`PROSE_LANE=true`, and the skipped steps show as skipped in the job view. In
`Build (Linux)`, `verified exact-base compiler artifact <id>` means the base
binary was reused. `prose compiler reuse unavailable (…)` means it was built
again. Both are valid outcomes.

## Recorded exclusion: `cargo fmt --all -- --check`

Not gated, on purpose, as of 2026-08-28:

1. The tree measures **6,168 rustfmt hunks** — adoption is a flag-day
   reformat that would conflict with every in-flight branch and pollute
   `git blame` in one stroke. It has to land alone, not ride a gate PR.
2. The committed generated sources (`crates/almide-codegen/src/generated/*`)
   are produced by generators that do not emit rustfmt shape; a fmt gate
   would fight the regen-diff gate that pins them. Adoption needs a
   `rustfmt.toml` ignore set for generated trees first.
3. `include!`-split files (the codopsy discipline for >800-line modules)
   contain item fragments that rustfmt reflows unhelpfully at the split
   boundaries; the split set needs auditing before a tree-wide reformat.

Adoption plan, in order: land the `rustfmt.toml` ignore set → one
dedicated flag-day PR (`cargo fmt --all`) at a moment with no long-lived
branches → add `cargo fmt --all -- --check` next to the clippy step. Until
then, this section is the honest record that the lane is missing and why.
