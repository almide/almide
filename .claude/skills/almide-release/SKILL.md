---
name: almide-release
description: Bump version, build, install, and create PR from develop to main
disable-model-invocation: true
---

# Almide Release

Perform a version bump, build, install, and create a PR from develop to main.

## Steps

1. **Confirm branch**: Must be on `develop`. Abort if not.

2. **Version bump**: Read current version from `Cargo.toml`, increment patch version (e.g., 0.4.2 → 0.4.3). If the user specified a version level (major/minor/patch) or explicit version, use that instead.

3. **Build**: Run `cargo build --release`. Abort on failure.

4. **Install**: Copy `target/release/almide` to `~/.local/almide/almide`.

5. **Commit & push**:
   - Stage `Cargo.toml` and `Cargo.lock`
   - Commit with message: `Bump version to X.Y.Z`
   - Push to `origin develop`

6. **Create PR**: Use `gh pr create` from develop to main:
   - Title: `vX.Y.Z: <summary from commits since main>`
   - Body: Summarize all commits between main and develop with bullet points
   - Include test plan section
   - End with `Generated with [Claude Code]` footer

7. **Report**: Print the PR URL.

## Commit message format

```
Bump version to X.Y.Z

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```

## Notes

- Never force push
- Always check that all exercises pass before creating PR (ask user if unsure)
- If there are uncommitted changes besides the version bump, ask the user what to do
