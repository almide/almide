# Release: Tag + Release Notes

Create a GitHub release with tag. Pushing the tag triggers `.github/workflows/release.yml` which automatically builds binaries for all platforms and attaches them to the release.

## Prerequisites

- PR from develop to main must be merged
- CI must be green
- Version in Cargo.toml must be updated

## Steps

1. **Confirm state**
   ```bash
   git fetch origin main
   git log v{previous_version}..origin/main --oneline
   ```
   Review all commits since the last tag.

2. **Create and push tag**
   ```bash
   git tag v{version} origin/main
   git push origin v{version}
   ```
   This triggers the release workflow. It will:
   - Build binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
   - Create archives: `almide-{os}-{arch}.tar.gz` (`.zip` for Windows)
   - Generate `almide-checksums.sha256`
   - Create a GitHub Release with all assets and auto-generated notes

3. **Verify release** — Check that all 5 binaries + checksums are attached:
   ```bash
   gh release view v{version}
   ```

4. **Edit release notes** (optional) — If auto-generated notes need refinement:
   ```bash
   gh release edit v{version} --notes "..."
   ```
   Structure:
   ```
   ## What's Changed

   ### Category (e.g., Compiler Fixes, New Features, WASM, Stdlib, etc.)
   - **Feature/fix name**: Brief description of what changed and why it matters.

   **Full Changelog**: https://github.com/almide/almide/compare/v{prev}...v{version}
   ```

## Release Assets

Each release automatically includes:

| File | Contents |
|------|----------|
| `almide-linux-x86_64.tar.gz` | Linux x86_64 binary + LICENSE |
| `almide-linux-aarch64.tar.gz` | Linux ARM64 binary + LICENSE |
| `almide-macos-x86_64.tar.gz` | macOS Intel binary + LICENSE |
| `almide-macos-aarch64.tar.gz` | macOS Apple Silicon binary + LICENSE |
| `almide-windows-x86_64.zip` | Windows x86_64 binary + LICENSE |
| `almide-checksums.sha256` | SHA-256 checksums for all archives |

## Notes

- Version format: `v{major}.{minor}.{patch}`
- Release notes must be in English
- The release workflow takes ~10 minutes to complete all builds
- If a build fails, delete the release and tag, fix, and re-tag
