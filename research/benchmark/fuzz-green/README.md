# Fuzz true-green streak (aviation-quality Stage 4)

The metric a mission-critical auditor reads: not "how fast do they fix
it" but "how long has it stayed unbroken". A calendar day is CLEAN only
when every Fuzz (nightly) run that day concluded success; any failure
breaks the streak; a day without a run neither grows nor resets it.
First milestone: **90 consecutive clean days**.

Meter: `scripts/fuzz-green-streak.sh` (append a dated row with `--update`).

| measured (UTC) | streak (days) | streak start | latest run day |
|---|---|---|---|
| 2026-08-10 | 0 | - | 2026-08-10 |
