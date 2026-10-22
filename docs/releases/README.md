# Release notes

One file per tag: `docs/releases/<tag>.md` (an `-rc` tag gets its own file too).
`.github/workflows/release.yml` publishes the GitHub release title and body
from it, and refuses to build a tag whose file is missing or malformed. Write
the file and land it on `develop` → `main` **before** pushing the tag; the tag
must point at a commit that contains it.

Check it before tagging:

```bash
bash scripts/check-release-notes.sh vX.Y.Z
```

## Format

```markdown
# vX.Y.Z — <lowercase headline: one or two concrete claims joined by ", and">

<One or two sentences: what this release is. For an rc: "**Release candidate N**
(#1484): published as a prerelease for the soak window, excluded from "latest"."
and what it adds after the previous tag.>

## <Theme — e.g. Language surface, Stdlib, Correctness, wasm, Performance, Diagnostics, Tooling>

- **<The observable change, stated plainly>.** What was wrong or new, and what
  a user now sees (#PR, issue #N).

## Gates and evidence

- <Internal gates, ledgers and logs — last and brief.>

## Dependencies

<Bumped crates, one line.>

**Full Changelog**: https://github.com/almide/almide/compare/vPREV...vX.Y.Z
```

Rules the check enforces:

- Line 1 is `# <tag> — <headline>` (em dash; the headline is 10–120 characters). It becomes the release title.
- Line 2 is blank; everything after it is the body.
- No auto-generated `## What's Changed` list and no `* <PR title> by @user in <url>` lines. Group changes by theme and say what changed for a user; merge related PRs into one bullet.
- The body ends with the `**Full Changelog**` compare link to this tag.

Examples: [v0.68.0-rc1](./v0.68.0-rc1.md), [v0.68.0-rc2](./v0.68.0-rc2.md).
