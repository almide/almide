# Bolt Loop Procedure — What One Iteration Does

> 日本語版: [AI_DLC_BOLT_LOOP.ja.md](./AI_DLC_BOLT_LOOP.ja.md)

This procedure is meant to run under `/loop`; one invocation = one iteration.
Every iteration follows the same rhythm: **sync → read the current phase → make one move → record it**.
Actions this procedure authorizes (push, issue updates, ordinary minor releases) are never
re-confirmed with a human. A human is called only at the Mob points (M0–M6) of
[AI_DLC.md](./AI_DLC.md).

## 0. Sync and health check

```bash
git switch develop && git fetch origin && git pull --ff-only
gh run list --branch develop --limit 3
```

- If develop's CI is red, this iteration's job is fixing it — **forward**. Never revert or
  checkout files you did not modify yourself; they may belong to another agent.
- If `git status` shows changes you don't recognize, do not touch them — call a human (M6).

## 1. Identify the current Unit

- Released = the latest `v*` tag. The current Unit = the ladder row (ROAD_TO_1_0.md) for the
  next unreleased minor.
- Read the row, its linked issue(s), and `docs/ai-dlc/units/<version>/` if it exists.
- If the row's Issue cell is `—`, create the issue first and link it in the ladder in the same
  commit (ladder rule). On a 403, run `gh auth switch --user O6lvl4` and retry.

## 2. Read the phase, choose one move

A Unit advances as a pair: the plan (`inception.md`) and the ledger (`construction.md`).
No work starts before the plan is approved.

- **`units/<version>/` does not exist yet → this is a plan-writing iteration.**
  Write `inception.md` from the ladder row and the issues, in the shape of
  [inception-template.md](./ai-dlc/inception-template.md). Pull every number from the issues;
  invent nothing. Commit + push, then request approval (M0) via a `mob` issue and a
  notification, and stop this Unit.
- **The plan exists but its approval record is empty → this is an approval-check iteration.**
  Check the `mob` issue. If approved, record the approver and date in the plan, build the
  ledger from the plan's "proposed Bolts" section
  ([construction-template.md](./ai-dlc/construction-template.md)), and continue to step 3.
  If not yet approved, stop (the only permitted work is fixing red CI).
- **Approved → this is a Bolt iteration.**
  First settle the previous Bolt: check its CI result and write the evidence
  (commit SHA, CI run URL) into its ledger row. Then execute the next pending Bolt.

## 3. Execute ONE Bolt

- Before touching code, state the Bolt's done-criteria from the ledger.
- Keep verification minimal and tiered — full judgment belongs to CI:
  - `cargo test` for the touched crates must be zero-error before pushing
  - `almide test` on the relevant spec directories for changes visible from the language or stdlib
  - `make install` after compiler changes, so the PATH binary is current
- **Two tripwires that stop the iteration:**
  - Observable cross-target behavior changes without a contract (`C-NNN`) in the same commit
    → stop, call M2
  - Green would require loosening a ratchet or wall → stop, call M4. Loosening is not an option
- Deviations from the plan: inside the plan's "scope" → note them in the ledger's notes and
  continue. Beyond scope → call M6.

## 4. Push and record

- Commit in English, one concise line, no prefix. Include the ledger's status update
  (status → done; evidence lands next iteration during settlement) in the same commit.
  Push without asking.

## 5. Release check

- When the ledger's "Unit completion" section is fully satisfied:
  - **Ordinary minor** → release automatically: bump `Cargo.toml` → push → PR develop→main →
    merge on green (never force-merge) → tag the merge commit → let release.yml create the
    release → verify the 5 binaries + checksums → close the finished issues.
    (Detailed steps: `.claude/commands/almide-release.md`)
  - **Milestone (0.50 / 0.60 / 0.70 / 0.80 / 0.90)** → write the audit brief on the issue and
    call M1. Never release without approval.
- After a release, the next iteration starts with the next Unit's plan.

## 6. When to act next

- Waiting on CI → wake in 480–600 seconds. Do not peek more often.
- Waiting on an external clock (e.g. a nightly run) → wake once per expected event.
  Permitted look-ahead during the wait: drafting the NEXT Unit's plan — never its ledger
  (that would be pre-approval work).
- Actionable Bolts remain → continue promptly.
- Everything is waiting on a human → notify. Running standalone, stop here. Running under
  `/loop`, arm a watch on the open `mob` issue (new comments and closure are the wake
  signals) and idle on a long heartbeat (~30 min) instead — an answer from anywhere
  resumes the loop.

## How to call a human

Write a `mob`-labeled issue with four points: **what happened / evidence / options /
recommendation**. Write it in English — the repo's issue language — and link the `.ja.md`
documents where they help the reader. Send a notification. Continue with independent work if any exists;
otherwise stop. If a call turns out not to have needed a human, that is a defect in this
procedure — record it and tighten this document.
