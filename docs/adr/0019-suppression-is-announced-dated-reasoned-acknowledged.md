# ADR-0019: Suppression is announced, dated, reasoned and acknowledged — its shape is fixed before the construct exists

- **Status**: Accepted — decided 2026-09-22 without a separate ○× round,
  under the maintainer's standing instruction *"where a ○× is needed, take
  the recommendation"*: every choice below (spelling, scope, horizon, trailer
  name, exit behaviour) is the recommended option, taken as recommended.
  Nothing here is implemented. The record binds the construct when a PR
  introduces it (#2152 item 4; arena-breakthroughs T8).
- **Date**: 2026-09-22
- **Context**: Almide has no suppression construct — no `@allow`, no comment
  pragma, no manifest waiver, no command-line flag. Every warning a compile
  finds is printed on every compile. (The only switch that hides warnings is
  the REPL's internal `SUPPRESS_WARNINGS`, which no source text can reach.)
  Pressure for one will come from three directions already on the roadmap:
  dialect epochs promote warnings to errors (E053's own module doc plans
  exactly that), lints acquire false positives, and the bounded profile adds
  rules. The writers this language optimizes for are models, and models
  reproduce `#[allow(dead_code)]` and `// eslint-disable-next-line`
  reflexively: a silent, permanent waiver is the shape they will reach for.
  `attr_vocab.rs` already names "a construct that accepts anything and
  reports nothing" as the worst possible surface for such a writer. The
  field survey (arena-breakthroughs §2.3) found one neighbour with
  non-silent, expiring suppression acknowledged in commit history; aver's
  `[[check.suppress]]` requires a reason, applies to warnings only, prints
  how many warnings each file's waivers removed, and reports rules that
  matched nothing.
- **Decision**: When Almide gains a suppression construct, it is this and
  nothing looser:
  1. **Spelling and scope.** `@allow(code = "E053", until = "2027-01-31",
     reason = "…")` on ONE declaration (`fn`, `effect fn`, `type`, top-level
     `let`, `test`), waiving that one code inside that declaration's span.
     One attribute waives one code; two codes are two attributes, each with
     its own date and reason. There is no module-level, file-level, comment,
     manifest or command-line form.
  2. **Warnings only.** Only a code the compiler emits at warning level can
     be waived; naming an error-level code is itself an error. When a dialect
     epoch promotes a warning to an error, every waiver of it becomes an
     error at that epoch — a waiver cannot defer an epoch.
  3. **Non-silent — re-announced on every compile.** Each active waiver
     prints one line on stderr, `note[E053]: waived at <file>:<line> until
     <date> (<n> occurrence(s)): <reason>`, and a waiver that suppressed
     nothing in that compile is reported as unused. `check`, `build`, `run`
     and `test` all print it; `--json` carries it as a `waived` record; no
     flag, environment variable or profile turns it off. A waived warning
     does not count toward `--deny-warnings`; the announcement is not a
     warning and cannot itself be waived.
  4. **Dated — expiry is an error.** `until` is mandatory: an ISO-8601
     calendar date at most **180 days** after the compile date. On the first
     compile after `until` the waiver is an ERROR that names the waiver, its
     reason and the lapsed date. A missing, malformed or beyond-horizon date
     is an error too. The compile date is the system clock unless
     `SOURCE_DATE_EPOCH` is set, so a rebuild of an old tag can be pinned to
     its own date; the date decides only the verdict and never reaches the
     emitted bytes.
  5. **Reasoned.** `reason` is mandatory and non-empty after trimming.
  6. **Acknowledged by a commit trailer.** A commit whose diff adds an
     `@allow(` or changes an existing one's `code`, `until` or `reason`
     carries the trailer **`Waiver-Acked-by: Name <email>`**. Removing a
     waiver needs no trailer.
- **Rationale**: (1) *Non-silent*: `#[allow]` fails because it is written
  once and never read again. Re-announcing every waiver on every build keeps
  the debt in front of whoever can pay it — for a model, inside the context
  window of every later build. aver's removed-warning count and unused-rule
  report are the same instinct; printing each waiver is one step further.
  (2) *Dated*: a reasoned waiver without an end is still permanent; the date
  turns it into a loan. 180 days is long enough for a real upstream fix and
  short enough that `until = "9999-12-31"` cannot turn the loan into a gift.
  (3) *Expiry is an error*: an "expired waiver" WARNING would itself be
  waivable and ignorable — the regress the construct exists to stop.
  (4) *Reasoned*: a waiver nobody explained is a waiver nobody can retire
  (aver's wording for the same rule). (5) *Acknowledged*: an agent cannot
  take the loan silently; history names who agreed to it. (6) *The spelling
  `@allow`*: a model that knows Rust writes `@allow(x)` with no date and no
  reason. With `@allow` the real attribute, that attempt lands on a precise
  error whose fix-it names the missing fields — the prior is caught and
  corrected. Under any other name `@allow(x)` would be an unknown attribute,
  i.e. E053, a warning: the silent path again. (7) *Declaration scope*: edit
  locality — the waiver moves with the code it waives and sits in the diff
  of whoever edits that code. (8) *Warnings only*: an error is a language
  rule, and a waived rule is a private dialect.
- **Alternatives**:
  (a) *Never add suppression* — rejected as a standing decision, though it
  is today's state: the first false-positive lint or epoch promotion under a
  deadline produces an ad-hoc escape (the check disabled, correct code
  rewritten worse, an old compiler pinned). Fixing the shape now is what
  stops a quick silent `@allow` from landing under that pressure.
  (b) *Rust's `#[allow(lint)]`* — silent, permanent, reasonless; RFC 2383's
  `reason =` is optional and `#[expect]` only speaks when the expectation
  stops being met. Fails three of the four properties.
  (c) *A manifest table (aver's `[[check.suppress]]`)* — reasoned, reported,
  centralized; rejected on locality: glob-matched from a file nobody editing
  the function reads, it silently widens or orphans on a rename, and it is
  absent from the function's diff. Its unused-rule report is kept (Decision 3).
  (d) *A command-line flag (`--allow E053`)* — lives in a build script, not
  the source; CI and a local run disagree; no reason, no date.
  (e) *A comment pragma (`// almide: allow E053`)* — comments are not AST:
  the formatter, the LSP and the checker cannot see them, and a misspelled
  pragma is inert — the E053 failure again.
  (f) *Expiry as a warning* — rejected by Rationale (3).
  (g) *A mandatory tracking-issue URL instead of a reason* — a URL rots
  offline and a model cannot read it; a reason can still carry an issue
  number when one exists.
  (h) *Signed acknowledgement* (signed commits or a signed trailer) —
  signature policy is a repository setting, not a language rule, and not
  every contributor signs; the trailer is a recorded claim (Consequences).
- **Enforcement once the construct lands** (the introducing PR carries every
  row; none exists today):

  | Property | Gate |
  |---|---|
  | Non-silent | a `tests/diagnostics/` broken/fixed pair asserting the announcement; a CLI test running `check`, `build`, `run`, `test`, `--json` and `--deny-warnings` on the same waived file and asserting the note each time, plus the unused-waiver report |
  | Dated | an E-code each for missing, malformed, beyond-horizon and lapsed `until`, each with a `tests/diagnostics/` pair; the lapse fixtures pin `SOURCE_DATE_EPOCH` so they are deterministic. Codes are allocated by the introducing PR, not reserved here |
  | Reasoned | an E-code for a missing or empty `reason`, with its pair |
  | Warnings only | an E-code for `@allow` naming an error-level code, and a test that an epoch promotion turns an existing waiver into that error |
  | Acknowledged | `scripts/check-waiver-acks.sh`, run by CI's `checks` job over the PR's commit range and by a lefthook `commit-msg` hook: a commit whose diff adds or modifies an `@allow(` in a `.almd` file must carry `Waiver-Acked-by:` (read with `git interpret-trailers --parse`); a row in `proofs/gate-verification.toml` with its negative test |
  | Inventory | the repository's own active-waiver count outside the diagnostics fixtures is a stamped, shrink-only ceiling, 0 on arrival |

- **Consequences**: Every suppression becomes visible on every build,
  expires, explains itself and has a named acknowledger in history; a model
  that writes the familiar spelling is corrected to the full shape. The
  costs: a build verdict that depends on the date (bounded by
  `SOURCE_DATE_EPOCH` and by the date never reaching emitted bytes), stderr
  noise for code that carries waivers (intended), and a trailer contributors
  must learn (the hook tells them). The acknowledgement is a claim, not a
  proof: git trailers are unsigned text, so the gate enforces presence and
  form, not identity; a forged acknowledgement is a false statement in
  permanent history, visible to review, and that is all the strength
  claimed. Nothing changes today: this is the shape a PR introducing a
  suppression construct is reviewed against.
- **Falsifier**: (1) If, once the construct exists, commits that only move
  `until` outnumber commits that remove waivers across two consecutive
  releases, the loan model is failing and the decision is revisited (a
  renewal limit, or a mandatory tracking issue). (2) If Dojo measurement
  shows model-written patches inserting `@allow` for a code more often than
  fixing it, the familiar-spelling argument was wrong and the name is
  revisited. (3) If the 180-day horizon forces renewals whose upstream fix
  runs on a longer schedule (a dependency's release cycle), the horizon is
  revisited — the expiry is not.
- **References**: #2152 (item 4); docs/roadmap/active/arena-breakthroughs.md
  (T8 item 4, §2.3); crates/almide-frontend/src/attr_vocab.rs (E053);
  `../almide-references/aver/docs/cli.md`, `[[check.suppress]]`; Rust RFC
  2383 (lint reasons), https://rust-lang.github.io/rfcs/2383-lint-reasons.html;
  `git interpret-trailers`, https://git-scm.com/docs/git-interpret-trailers;
  `Acked-by:` in the Linux kernel's patch process,
  https://docs.kernel.org/process/submitting-patches.html;
  `SOURCE_DATE_EPOCH`, https://reproducible-builds.org/docs/source-date-epoch/.
  Numbering: 0018 is held by an in-flight branch (the value-strings record),
  so this record takes 0019 rather than collide with it.
