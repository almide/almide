# Release: Tag + Release Notes

Create a GitHub release with tag and English release notes.

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

2. **Create tag**
   ```bash
   git tag v{version} origin/main
   git push origin v{version}
   ```

3. **Create release notes** — Write in English. Structure:
   ```
   ## What's Changed

   ### Category (e.g., Compiler Fixes, New Features, WASM, Stdlib, etc.)
   - **Feature/fix name**: Brief description of what changed and why it matters.

   ### Other
   - Minor changes that don't fit a category.

   **Full Changelog**: https://github.com/almide/almide/compare/v{prev}...v{version}
   ```

4. **Create GitHub release**
   ```bash
   gh release create v{version} --title "v{version}" --notes "..."
   ```

## Notes

- Version format: `v{major}.{minor}.{patch}`
- Release notes must be in English
- Group related changes under meaningful headings
- Lead with the most impactful changes
- Include the Full Changelog link at the bottom
