# The regex reference table

`reference-answers.tsv` is what an ENGINE OUTSIDE this project answers for a
shared subset of the regex language, one `op<TAB>pattern<TAB>subject<TAB>answer`
row per case. `scripts/check-regex-reference.sh` asks the same questions of both
Almide legs and fails on any difference.

## Why a table and not a fuzz against a linked engine

C-032 already fuzzes the engine — but its oracle is `runtime/rs/src/regex.rs`,
*our own* native engine. That certifies the two legs agree; it cannot see a
rule both legs read the same way and both read wrong. #2129 is exactly that
gap, and the two defects it found were invisible to every existing gate:

- `[]]` — a `]` in the first position of a class is a literal member (POSIX,
  and PCRE / Python / Rust / Go / JS all follow it). Read as the terminator,
  the pattern became an empty class plus a stray `]` and silently never
  matched. No error, no match, nothing to see from outside.
- `split` over a pattern that can match empty — `split("^", "abc")` answered
  `["a", "bc"]`, moving a character out of the field it belongs to.

A committed table rather than a linked reference engine because the gate must
be hermetic: no network, no new runtime dependency, and the same answer on
every machine forever. The cost is that the table is refreshed by hand when the
subset grows, which is the trade the oracle manifests in
`crates/almide-spine/tests/golden/` already make.

## What is in the subset, and what is deliberately not

IN: literals, `.`, classes (including the `]`-first and `[`-in-class forms),
anchors, greedy and lazy quantifiers including braces, groups and
non-capturing groups, alternation, `\d \w \s \D \W \S`, `\b`, and the six
public operations.

OUT, because the reference engines disagree with EACH OTHER there, so a
difference would say nothing about correctness:

- An empty alternation arm (`ab|`, `|ab`). Python 3.7+ retries a non-empty
  match at the position of an empty one; Rust and Go do not. Almide matches
  Rust and Go — verified against `ripgrep` (Rust's `regex`) during #2129.
- `split` returning capture groups. Python includes them; Rust, Go and Almide
  do not. Where the subset needs a grouped pattern the table's generator
  rewrites the groups to non-capturing before asking.

## Regenerating

The generator is recorded in the #2129 section of
`docs/bugfix-research-2026-09-12.md`. Growing the subset means adding patterns
or subjects and re-running it against the reference engine — never editing a
row to match what Almide happens to answer, which would turn the gate into a
mirror.
