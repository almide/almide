<a id="fuzz-true-green-streak-aviation-quality-stage-4"></a>

# Fuzz true-green streak (aviation-quality Stage 4)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> Non-normative source-pinned reader snapshot. Original paths remain authoritative and are not modified or replaced as machine inputs. This copy is not automatically synchronized. Historical measurements and labels, including aviation-quality, are not independent validation or certification; milestones are not achieved results.
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/fuzz-green/README.md)


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
