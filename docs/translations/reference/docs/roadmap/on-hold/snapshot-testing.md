<!-- description: Optional file-backed snapshots beyond the shipped source-literal snapshot assertions -->
# File-backed Snapshot Testing — On Hold

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](snapshot-testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](snapshot-testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](snapshot-testing.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/snapshot-testing.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Already available

`testing.assert_snapshot(actual, expected)` compares output with the expected
**source literal**. `almide test --update-snapshots <file>` accepts a new or
drifted value by rewriting that literal; ordinary test runs do not write,
and CI mode refuses acceptance. There are no sidecar snapshot files.

The current API and acceptance rules live in the
[CLI reference](../../../../../specs/cli.md#スナップショット-testingassert_snapshot).
Implementation: `stdlib/testing.almd` and
`src/cli/commands.rs::cmd_test_update_snapshots`. Evidence:
`spec/stdlib/testing_snapshot_test.almd` and `tests/snapshot_update_test.rs`
(exact rewrites, repeat acceptance, ordinary failure and CI write refusal).

## Remaining design question

The original proposal stored expected output in separate files. That is a
different feature from the shipped literal-backed assertions, and remains
on hold. Revisit only if a concrete use case needs external snapshot files;
then define file naming, reviewable acceptance, stale-file handling and CI
write refusal without changing existing literal-backed tests.

The earlier `snapshot(result)` sketch was a proposal, not an implemented API.
Design references remain MoonBit's `inspect` expectations and Gleam's
file-based test snapshots. Neither implies an Almide sidecar-file contract.
