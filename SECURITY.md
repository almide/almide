# Security Policy

## Reporting a vulnerability

Report privately via **GitHub Security Advisories**:
https://github.com/almide/almide/security/advisories/new — do not open a
public issue for an unfixed vulnerability. You should receive an initial
response within 7 days. Coordinated disclosure: we ask for a 90-day window
(shorter by agreement once a fix ships).

In scope: the compiler and its emitted code (a miscompilation that breaks
the memory-safety claims IS a security bug — see the severity taxonomy in
`docs/project/ISSUE-TAXONOMY.md`: `I-unsound` / `I-miscompile` are
release-blocking by CI policy), the runtime (`runtime/rs`), the wasm legs
and their host shims, and the release pipeline.

## Supported versions

The latest minor release line receives fixes. Older lines: fixes are not
backported; upgrade. Release-blocker policy: a FINAL tag cannot ship over
an open `I-unsound` / `I-miscompile` / `I-divergence` / `regression`
issue (enforced by the release workflow, #1482).

## Dependency policy

The dependency tree is audited in CI against the RustSec advisory
database (`cargo audit`, weekly + on every lockfile change — see
`.github/workflows/dependency-audit.yml`). An advisory with no fixed
release is triaged in an issue rather than silenced; permanent ignores
require a written justification in the workflow file.

## Release integrity

Every release ships `almide-checksums.sha256`, and release artifacts are
**attested with Sigstore** (GitHub artifact attestation). Verify any
downloaded asset:

```
gh attestation verify <asset> -R almide/almide
```

The qualification dossier attached to each release (`almide-dossier-*.md`)
carries the trust-chain receipts for that tag; it is attested the same way.
