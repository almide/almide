# ADR-0013: The conformance corpus stays contract-ID-keyed; semantic reorganization is rejected

- **Status**: Accepted
- **Date**: 2026-08-20
- **Context**: The 2026-08 reference survey (../almide-references) documented
  rustc banning flat/mechanical test placement (`deny_new_top_level_ui_tests`,
  compiler-team#902) and warning against mechanical-key filenames
  (`issue-123456.rs`), which reads as an argument against this repository's
  path-stable corpus layout. Meanwhile every consumer pins this repository by
  commit: golden manifests embed fixture paths in oracle-hashed output, the
  contract ledger's evidence entries and 591 `// @contract:` headers cite
  paths, and the incumbent implementation still carries diffable copies.
- **Decision**: The corpus keeps its current layout and path-stable identity.
  Fixtures are keyed to contract ids (`C-NNN`), not to their location;
  navigation is served by generated indexes (the ALS section index, the
  conformance report), never by moving files. New fixtures must carry
  subject-first snake_case names (`map_upsert_str.almd`), and bare-number or
  `issue`/`bug`-prefixed names are rejected by the style gate.
- **Rationale**: rustc's own exception is the invertible direction — paths
  *derived from a stable opaque id* (`tests/ui/error-codes/EXXXX.rs`) are
  legitimate, and that is exactly this corpus's shape. The survey's warning
  targets corpora whose only key is the path; ours is `C-NNN ⇄ fixture`,
  bidirectionally gated. A reorganization would repay nothing that generated
  indexes do not already provide, while forcing golden-manifest regeneration
  in every pinned consumer, rewriting every citation, and destroying the
  copy-diffability that Stage B cutovers depend on.
- **Alternatives**: (a) rustc-style semantic directories with a
  README⇄filesystem gate — rejected: benefits already served by generated
  indexes; costs enumerated above. (b) Do nothing — rejected: the naming
  discipline half of the survey's warning is real and now gated.
- **Consequences**: Corpus paths are effectively append-only identifiers.
  The style gate owns fixture-name discipline. If the corpus ever needs a
  new axis of organisation, it is added as a generated index, not a move.
- **Falsifier**: A demonstrated navigation or review failure that a
  generated index cannot serve but a directory move would — none observed in
  1,749 corpus files to date.
- **References**: ../almide-references survey report (D1), rustc
  `src/tools/tidy/src/ui_tests.rs`, `tests/best-practices.md`.
