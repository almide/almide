<!-- description: Cut v0.14.6 release from llm-first-phase2 branch -->
<!-- done: 2026-04-16 -->
# Cut v0.14.6 Release

Trigger: implement next (Phase 3 MVP is stable; dojo has been running
against the branch for ~1 week).

## Why now

- **`0.14.6` branch dependency is painful**: external users who
  want the Phase 2/3 improvements must `cargo install --git --branch
  llm-first-phase2`. Released 0.14.6 would let them `cargo install almide`.
- **Dojo measurement stability**: releasing the branch as 0.14.6 freezes
  a known-good reference point. Follow-up work continues on `develop`
  toward 0.14.7-phase3.N.
- **CHANGELOG.md is ready**: every `0.14.6` entry is a release note.

## Procedure (per CLAUDE.md release rules)

1. Bump `Cargo.toml` version: `0.14.6` → `0.14.6`.
2. Commit on `develop`, push.
3. Wait for `develop` CI green.
4. Open PR: `develop → main`. Do not force-merge.
5. After merge, tag the merge commit: `git tag v0.14.6 <merge-sha>`
   and `git push origin v0.14.6`.
6. `release.yml` workflow creates the GitHub release from the tag.
7. If custom release notes are wanted: `gh release edit v0.14.6
   --notes "$(cat CHANGELOG.md | section 0.14.6)"` AFTER the workflow
   completes.

**Do NOT** manually `gh release create` — races the workflow.

## Branch cleanup after release

- `llm-first-phase2` branch stays as the historical phase2 marker for
  a few more runs, then deleted.
- `develop` advances to `0.14.7-phase3.1` or similar for next phase.

## Non-goals

- Not changing release cadence policy.
- Not introducing beta / RC tags (0.14.6 is the MVP ship).

## Estimated scope

~30 minutes of repo work, plus CI wait time (5-15 min).
