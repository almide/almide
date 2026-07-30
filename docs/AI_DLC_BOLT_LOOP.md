# Bolt Loop — One AI-DLC Construction Iteration

Execute ONE Bolt of the current Unit from `docs/roadmap/ROAD_TO_1_0.md`, following the
operating model in `docs/AI_DLC.md`. Designed to run under `/loop` (dynamic pacing) —
each invocation is one iteration. Never ask permission for actions this file authorizes
(push, issue updates, normal minor releases); escalate only at Mob points.

## 0. Sync & health

```bash
git switch develop && git fetch origin && git pull --ff-only
gh run list --branch develop --limit 3
```

- If HEAD CI is red: this iteration's Bolt = fix CI **forward**. Never revert or
  checkout files you did not modify yourself (other agents may own them).
- If `git status` shows unexpected local changes: stop and escalate M6 — do not touch them.

## 1. Select the Unit

- Released = latest `v*` tag. In-progress Unit = the ladder row for the next unreleased
  minor in `docs/roadmap/ROAD_TO_1_0.md`.
- Read the row and its linked issue(s). If the row's Issue cell is `—`, create the issue
  first and link it in the ladder in the same commit (ladder rule). If issue creation
  returns 403, run `gh auth switch --user O6lvl4` and retry.

## 2. Mob pre-check, then plan

- **Before any work**: does this Unit require an M2/M3 decision (syntax, stdlib boundary,
  observable-behavior change, outward claim wording) that is not yet decided? If yes →
  escalate (see protocol) and stop this Unit.
- If the Unit's issue has no Bolt plan yet, post one as a checklist comment: each Bolt =
  intent / definition of done / gate, sized at a few hours. This is the persisted plan
  (AI-DLC context memory); update checkboxes as Bolts land.

## 3. Execute ONE Bolt

- Goal-prompt discipline: state the Bolt's DoD before touching code.
- Verify tiered and minimal — CI owns the full wall (no local full gates):
  - `cargo test -p <touched crates>` must be zero-error before push
  - `almide test <relevant spec dirs>` for language/stdlib-visible changes
  - `make install` after compiler changes so the PATH binary is current
- **Contract tripwire (M2)**: an observable cross-target behavior change without a
  `C-NNN` ledger entry in the same commit → stop, escalate.
- **Ratchet tripwire (M4)**: if green would require loosening any ratchet, wall, or
  gate → stop, escalate. Fix forward is the only path.

## 4. Ship the Bolt

- Commit in English, one concise line, no prefix. Push `origin develop` without asking.
- Tick the Bolt's checkbox on the Unit issue with a one-line result and evidence links
  (commit SHA, CI run URL).

## 5. Release check

- When ALL Bolts of the Unit are done and develop CI is green:
  - Normal minor: release automatically — bump `Cargo.toml` to the Unit's version,
    commit, push, PR develop→main, merge on green (never force-merge), tag the merge
    commit, let release.yml create the release (see `.claude/commands/almide-release.md`),
    verify the 5 binaries + checksums, then close the Unit's issues.
  - Decade gate (0.50/0.60/0.70/0.80/0.90): post the audit brief on the issue,
    escalate M1, and do NOT release without approval.

## 6. Pace or stop (under /loop)

- Waiting on CI → schedule the next wakeup at ~480–600s (match CI duration; do not poll).
- More actionable Bolts exist → continue promptly next iteration.
- Everything blocked on Mob → send the notification and stop the loop.

## Escalation protocol (Mob)

Create an issue labeled `mob` containing: 事象 / 根拠 / 選択肢 / 推奨 (what happened,
evidence, options, recommendation). Send a PushNotification. Continue only with
independent work; otherwise stop. A Mob escalation that turns out not to need a human
is a loop defect — record it and tighten this file.
