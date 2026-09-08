# tasks-example/xlang

A fixture for `runner_edits.rb`, not a benchmark task: `t0-smoke` is a
two-language (Almide, Rust) seed with two ordered edits, a reference solution
and a plausible wrong patch per edit, so that

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run
```

exercises the whole bank gate locally without a model call. The real task set
of #1963 lives in Dojo under `tasks/xlang/` with the same layout (see the
header of `runner_edits.rb`).
