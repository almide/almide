# Frees churn gates (ALMIDE_WASM_FREES=1)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/churn/README.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


Long-loop reclamation fixtures for the real-RC wasm mode. NOT part of the
default corpus (millions of iterations); run via:

```
scripts/check-frees-churn.sh
```

The script requires successful completion under its 600-second watchdog and
compares captured native output with the Wasm capture (whose stderr is merged).
It does not measure RSS, and shell command substitution removes trailing
newlines; it is not a full byte-level stdout/stderr/exit comparison. The corpus alone passed builds with
live double-frees twice — these shapes are the gate the corpus cannot be.

- record_churn.almd — 2M iterations of construct+drop with a fresh string
  field (free-list reuse, the sentinel/resurrection belt).
- tco_loop_churn.almd — 200k loop-variant TCO calls (the shape that exposed
  the M2 managed-param OOB; acceptance test for per-iteration TCO reclaim).

Compound recursive-drop fixtures originated as the v1 stage-2 migration
baseline, targeting the then-current `emit_typed_rc_dec` branches and proposed
flat MIR Drop nodes. That migration rationale is historical; the fixtures
remain reclamation regressions for the current structural Wasm route. Each deliberately avoids `?? ` (UnwrapOr) to stay a distinct
shape from the open #643 some-box leak:

- map_entry_churn.almd — 200k construct+drop of a Map[String, String] (heap key
  and heap value dropped separately).
- closure_env_churn.almd — 200k construct+drop of a closure capturing a heap
  String (the env must recurse-drop the capture).
- nested_named_churn.almd — 200k construct+drop of a record nesting a
  List[String] (record → list → string recursive drop).
