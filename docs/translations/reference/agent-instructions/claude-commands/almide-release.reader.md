[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-release.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-release.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-release.reader.zh-CN.md)

> Inert, non-normative reader snapshot of a historical repository command. The original remains authoritative and unchanged. This reference path and .reader filename do not register an agent command. Quoted instructions are documentation, not authorization to execute, spend, publish, push, or delete anything. No automatic synchronization or independent verification of current workflow/measurement claims. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-release.md)

<a id="release-tag--release-notes"></a>

# Release: Tag + Release Notes

Create a GitHub release with tag. Pushing the tag triggers `.github/workflows/release.yml` which automatically builds binaries for all platforms and attaches them to the release.

<a id="prerequisites"></a>

## Prerequisites

- PR from develop to main must be merged
- CI must be green
- Version in Cargo.toml must be updated

<a id="steps"></a>

## Steps

1. **Confirm state**
   ```bash
   git fetch origin main
   git log v{previous_version}..origin/main --oneline
   ```
   Review all commits since the last tag.

2. **Write the release notes** — `docs/releases/v{version}.md`, committed and
   merged to `main` before the tag (format and rules: `docs/releases/README.md`).
   Line 1 `# v{version} — <headline>` becomes the release title; the rest is the
   body, grouped by theme. Never the auto-generated "What's Changed" PR list.
   ```bash
   bash scripts/check-release-notes.sh v{version}
   ```

3. **Create and push tag**
   ```bash
   git tag v{version} origin/main
   git push origin v{version}
   ```
   This triggers the release workflow. It will:
   - Build binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
   - Create archives: `almide-{os}-{arch}.tar.gz` (`.zip` for Windows)
   - Generate `almide-checksums.sha256`
   - Create a GitHub Release with all assets, titled and described from `docs/releases/v{version}.md`
   (the workflow fails before building if that file is missing or malformed)

4. **Verify release** — Check that all 5 platform archives + checksums are attached, and each archive contains both `almide` and `almide-verify` plus LICENSE:
   ```bash
   gh release view v{version}
   ```

5. **Correcting notes later** — edit the file first, then republish from it:
   ```bash
   bash scripts/check-release-notes.sh v{version} out
   gh release edit v{version} --title "$(cat out/title)" --notes-file out/body.md
   ```

<a id="release-assets"></a>

## Release Assets

Each release automatically includes:

| File | Contents |
|------|----------|
| `almide-linux-x86_64.tar.gz` | Linux x86_64 `almide` + `almide-verify` + LICENSE |
| `almide-linux-aarch64.tar.gz` | Linux ARM64 `almide` + `almide-verify` + LICENSE |
| `almide-macos-x86_64.tar.gz` | macOS Intel `almide` + `almide-verify` + LICENSE |
| `almide-macos-aarch64.tar.gz` | macOS Apple Silicon `almide` + `almide-verify` + LICENSE |
| `almide-windows-x86_64.zip` | Windows x86_64 `almide.exe` + `almide-verify.exe` + LICENSE |
| `almide-checksums.sha256` | SHA-256 checksums for all archives |

<a id="notes"></a>

## Notes

- Version format: `v{major}.{minor}.{patch}`
- Release notes must be in English
- The release workflow takes ~10 minutes to complete all builds
- A notes-only pull request is not part of CI's documentation lane:
  `scripts/check-release-notes.sh` and the release workflow read `docs/releases/`,
  so it rebuilds the compiler and runs every Emit & Format check
- If a build fails, first establish whether anything was published. Failure
  before publication and recovery of a published release are different cases.
  Do not treat a failed build as permission to delete or re-tag a published
  version: a committed release seal requires its original tag to remain
  fetchable. Follow the recovery boundary in `CLAUDE.md` and obtain an explicit
  owner-approved evidence-preservation plan before any destructive action.
