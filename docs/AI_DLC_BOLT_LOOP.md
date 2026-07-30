# Bolt Loop — One AI-DLC Iteration (Inception ⇄ Construction)

Advance the current Unit from `docs/roadmap/ROAD_TO_1_0.md` by exactly one step, following
the operating model in `docs/AI_DLC.md` and the pair discipline in `docs/ai-dlc/README.md`.
Designed to run under `/loop` (dynamic pacing) — each invocation is one iteration.
Never ask permission for actions this file authorizes (push, issue updates, normal minor
releases); escalate only at Mob points.

## 0. Sync & health

```bash
git switch develop && git fetch origin && git pull --ff-only
gh run list --branch develop --limit 3
```

- If HEAD CI is red: this iteration's Bolt = fix CI **forward**. Never revert or
  checkout files you did not modify yourself (other agents may own them).
- If `git status` shows unexpected local changes: stop and escalate M6 — do not touch them.

## 1. Select the Unit

- Released = latest `v*` tag. Current Unit = the ladder row for the next unreleased minor.
- Read the row, its linked issue(s), and `docs/ai-dlc/units/<version>/` if it exists.
- If the row's Issue cell is `—`, create the issue and link it in the ladder in the same
  commit (ladder rule). On 403: `gh auth switch --user O6lvl4` and retry.

## 2. Phase dispatch — the pair discipline

Every Unit lives as a pair: `units/<version>/inception.md` (what & why, Mob-validated)
and `units/<version>/construction.md` (Bolt ledger with evidence). No construction
without an approved inception.

- **No `units/<version>/` yet → Inception iteration**: draft `inception.md` from the
  ladder row + issues using `docs/ai-dlc/inception-template.md`. Ground every number in
  evidence (read the issues; never invent). Commit, push, then escalate **M0**: a `mob`
  issue linking the file, plus PushNotification. Stop this Unit.
- **inception.md exists, M0 record empty**: check the `mob` issue for approval. If
  approved → record approver/date/notes in `inception.md`, create `construction.md`
  from the 提案 Bolt section (use `construction-template.md`), commit — then fall
  through to Construction. If not approved → stop (only red-CI fixes are permitted work).
- **Pair exists, M0 approved → Construction iteration**: first settle evidence — verify
  the previous Bolt's CI run and record SHA + run URL in its ledger row. Then execute
  the next 未着手 Bolt.

## 3. Execute ONE Bolt

- State the Bolt's DoD (from construction.md) before touching code.
- Verify tiered and minimal — CI owns the full wall (no local full gates):
  - `cargo test -p <touched crates>` must be zero-error before push
  - `almide test <relevant spec dirs>` for language/stdlib-visible changes
  - `make install` after compiler changes so the PATH binary is current
- **Contract tripwire (M2)**: an observable cross-target behavior change without a
  `C-NNN` ledger entry in the same commit → stop, escalate.
- **Ratchet tripwire (M4)**: if green would require loosening any ratchet, wall, or
  gate → stop, escalate. Fix forward is the only path.
- Deviation from plan that stays inside the inception's Scope → note it in 実行メモ.
  Deviation beyond Scope → M6 escalate.

## 4. Ship the Bolt

- Commit in English, one concise line, no prefix — include the construction.md status
  update (状態 → 完了; evidence lands next iteration per the ledger rule). Push
  `origin develop` without asking.

## 5. Release check

- When construction.md's 完了判定 is fully satisfied (all Bolts evidenced, DoD↔evidence
  mapping written):
  - Normal minor: release automatically — bump `Cargo.toml` to the Unit's version,
    commit, push, PR develop→main, merge on green (never force-merge), tag the merge
    commit, let release.yml create the release (see `.claude/commands/almide-release.md`),
    verify the 5 binaries + checksums, close the Unit's completed issues.
  - Decade gate (0.50/0.60/0.70/0.80/0.90): post the audit brief on the issue,
    escalate M1, and do NOT release without approval.
- After release: the next iteration's step 2 will open the next Unit's Inception.

## 6. Pace or stop (under /loop)

- Waiting on CI → schedule the next wakeup at ~480–600s (match CI duration; do not poll).
- Waiting on an external clock (e.g. nightly runs) → wake once per expected event,
  not more; meanwhile the only permitted look-ahead is drafting the NEXT Unit's
  inception (never its construction).
- More actionable Bolts exist → continue promptly next iteration.
- Everything blocked on Mob → send the notification and stop the loop.

## Escalation protocol (Mob)

Create an issue labeled `mob` containing: 事象 / 根拠 / 選択肢 / 推奨 (what happened,
evidence, options, recommendation). Send a PushNotification. Continue only with
independent work; otherwise stop. A Mob escalation that turns out not to need a human
is a loop defect — record it and tighten this file.
