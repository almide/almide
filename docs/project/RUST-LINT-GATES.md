# Rust lint and formatting gates (#1462)

The three lanes the compiler-championship survey found missing, and where
each one stands. This is the ledger the exclusion below is recorded in —
nothing here is implicit in a Makefile comment.

## Live lanes

| Lane | Gate | Discipline |
|---|---|---|
| `.almd` formatting, whole tree | `scripts/check-fmt-gate.sh` (CI `checks` job): `spec/` + `examples/` plain, `stdlib/` under `--no-import-edit` (309 files — splice-context sources, so import auto-insertion is excluded; the AST-conservation verifier still guards every rewrite) | any drift is red |
| rustc warnings | `scripts/check-rustc-warnings.sh` (CI `build` job) | shrink-only baseline, currently 0, fails both directions |
| clippy warnings | `scripts/check-clippy-warnings.sh` (CI `build` job) | shrink-only baseline (`scripts/clippy-warnings-baseline.txt`), fails both directions; deny-level lints (e.g. `overly_complex_bool_expr`) fail the compile itself — the first run caught a vestigial `\|\| true` in the parser's named-arg path |

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
