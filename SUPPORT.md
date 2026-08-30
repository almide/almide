# Support policy

What an adopter can count on, stated plainly. This document is the policy
half; every guarantee below names the machinery that enforces it, so the
policy and the enforcement cannot drift apart. Vulnerability reporting is
separate: see [SECURITY.md](./SECURITY.md).

## Which releases receive fixes

Almide is **pre-1.0**. There is exactly one supported line: the latest
release. Fixes land on `develop` and ship as the next tag — there is no
back-porting and no patch stream for older tags. Release cadence is high
(225 tags in the first six months; latest `v0.59.2`), so "the next tag" is
typically days away, not months.

Two mechanisms keep that single line honest:

- **Release blockers** — the release workflow refuses to cut a final tag
  while any issue labeled `I-unsound` / `I-miscompile` / `I-divergence` /
  `regression` is open (the gate of #1482). A known-bad compiler cannot
  become "the supported release".
- **The RC channel** — releases carrying language-surface or
  compiler-behaviour changes tag `vX.Y.Z-rcN` first, published as a GitHub
  prerelease for a soak window before the final tag (#1484).

Every release is an immutable evidence record: its seal
(`proofs/releases/<tag>.toml`) is re-measured against the tag by CI
forever, and release assets are Sigstore-attested
(`gh attestation verify <asset> -R almide/almide`).

**LTS**: there is no LTS line today, and this document will say so until
one exists with a funded owner. A promise of multi-year maintenance
without the organization to honor it would be theater; the honest offer
pre-1.0 is a fast, gated, evidence-sealed head. LTS designation is part of
the organizational ladder tracked in
[docs/roadmap/active/flight-organization.md](./docs/roadmap/active/flight-organization.md).

## What versioning means here

Version numbers are semver-shaped, but the number is not the contract —
these two ledgers are:

- **Dialect epochs** — [`proofs/dialect-epochs.toml`](./proofs/dialect-epochs.toml)
  is the append-only record of every change that can break already-written
  code. The epoch advances **only** for such changes: 225 releases have
  produced 3 epochs. Files carry `@dialect(N)` stamps so the compiler can
  tell a stale-dialect file from a wrong program, and the ledger is
  cross-checked against the compiler's `CURRENT_DIALECT` constant by
  `scripts/check-dialect-epochs.sh` — the record and the enforcement
  cannot disagree.
- **The stdlib interface gate** — `scripts/check-interface-diff.sh`
  classifies the public stdlib surface between any two tags as identical /
  additive / breaking, from committed signature indexes. A breaking diff
  refuses the release unless explicitly declared, and a removal must have
  served its `@deprecated` window (E052 migration diagnostics at every use
  site) and, when it can break written code, its dialect-epoch entry.

Concretely: **within an epoch, code that compiles keeps compiling**.
Surface grows additively; removals are announced in-compiler via
`@deprecated` before they happen; anything that breaks written code is a
new epoch with its breaks enumerated. The LLM-facing surface is
additionally frozen by [docs/STABILITY.md](./docs/STABILITY.md).

## Bus factor and continuity

Stated honestly: Almide has **one human maintainer**, with most code
machine-written under review. That is a bus factor of one, and no wording
here changes it. What is engineered is that nothing about the project's
correctness lives in the maintainer's head:

- **Everything re-derivable by an operator** — the trust spine's rule
  (TOR-2 in `proofs/TOR.md`) is that CI is outside the trust base:
  `make verify-trust && make receipt` re-derives the full evidence chain
  — Rocq kernel proofs, per-build certificates, the cross-target corpus,
  the sealed release record — on commodity hardware, no maintainer
  involvement.
- **The spec is independent of the implementation** — the ALS
  ([almide/als](https://github.com/almide/als)) is a separate normative
  specification with its own reference evaluator; the compiler is judged
  against it, not the reverse.
- **Continuity of artifacts** — releases, seals, and attestations live in
  public infrastructure (GitHub, Sigstore's public log); the license
  (MIT OR Apache-2.0) permits any party to fork and continue.

What does not exist yet, so it is not claimed: a legal entity that can
sign support contracts, paid support of any kind, or a certification
sponsor. Those are organizational-ladder items (#586), not engineering,
and they are listed there rather than implied here.

## Getting help

- **Bugs and feature requests** — [GitHub issues](https://github.com/almide/almide/issues);
  silent-wrong-answer class reports (wrong output, cross-target
  divergence) are triaged first, per the taxonomy in
  [docs/project/ISSUE-TAXONOMY.md](./docs/project/ISSUE-TAXONOMY.md).
- **Security reports** — [SECURITY.md](./SECURITY.md) (private reporting;
  do not open a public issue).
- **Documentation** — [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) for
  writing Almide, [docs/SPEC.md](./docs/SPEC.md) for the language,
  [docs/contracts/](./docs/contracts/README.md) for what is promised
  across targets.
