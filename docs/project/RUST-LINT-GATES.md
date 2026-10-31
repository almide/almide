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

Document fences, generated-reference freshness, and the existing Emit & Format
checks still run. Named required matrix contexts continue to report results;
branch protection and merge-queue requirements do not change. Scheduled/manual
verification remains on the full lane. See [`scripts/ci_prose.py`](../../scripts/ci_prose.py) and its offline
regression controls in [`scripts/test_ci_prose.py`](../../scripts/test_ci_prose.py) for the exact allowlist and
failure behavior.

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
