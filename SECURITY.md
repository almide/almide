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

The tools check this themselves (#4127):

- `almide self-update` downloads over the compiler's own HTTPS client (rustls; no
  `curl` from PATH, redirects only to https, #4181), hashes the archive in-process, matches the checksum
  line on the exact file name, stages in a fresh private directory, and
  verifies the attestation with `gh attestation verify`. Without `gh` it
  refuses; `--skip-attestation` installs on the checksum alone and says so.
- `tools/install.sh` / `tools/install.ps1` refuse a missing or mismatched
  checksum, and verify the attestation when `gh` is installed. Without `gh`
  they print the command above, since a `curl | sh` install usually has no
  `gh`.

The qualification dossier attached to each release (`almide-dossier-*.md`)
carries the trust-chain receipts for that tag; it is attested the same way.
