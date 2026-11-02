<!-- description: Optional file-backed snapshots beyond the shipped source-literal snapshot assertions -->
# File-backed Snapshot Testing — On Hold

## Already available

`testing.assert_snapshot(actual, expected)` compares output with the expected
**source literal**. `almide test --update-snapshots <file>` accepts a new or
drifted value by rewriting that literal; ordinary test runs do not write,
and CI mode refuses acceptance. There are no sidecar snapshot files.

The current API and acceptance rules live in the
[CLI reference](../../specs/cli.md#スナップショット-testingassert_snapshot).
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
