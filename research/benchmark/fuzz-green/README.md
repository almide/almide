# Fuzz true-green streak (aviation-quality Stage 4)

The metric a mission-critical auditor reads: not "how fast do they fix
it" but "how long has it stayed unbroken". A calendar day is CLEAN only
when its completed runs meet the scorer's clean rule. Sharded runs use the
`Night verdict` job, with at least 75% of planned fuzz-minutes delivered and
no findings (including recovered shard findings), not the workflow conclusion
alone. A failing/partial day breaks the clean streak; a day without a run
neither grows nor resets it. See `scripts/fuzz-green-streak.sh` and the shared
`scripts/lib/fuzz-night-line.sh` scorer for legacy and missing-verdict cases.
First milestone: **90 consecutive clean days**.

Meter: `scripts/fuzz-green-streak.sh`. The nightly ledger-counts run calls it with
`--update`, which appends a dated row whenever the streak differs from the last
row, and the refresh it opens carries the row with the restamped counts.

| measured (UTC) | streak (days) | streak start | latest run day |
|---|---|---|---|
| 2026-08-10 | 0 | - | 2026-08-10 |
