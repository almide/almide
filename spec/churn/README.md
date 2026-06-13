# Frees churn gates (ALMIDE_WASM_FREES=1)

Long-loop reclamation fixtures for the real-RC wasm mode. NOT part of the
default corpus (millions of iterations); run via:

```
scripts/check-frees-churn.sh
```

Pass bar per fixture: byte-identical stdout to native, exit 0, and flat RSS
(within 2x of the frees-OFF baseline). The corpus alone passed builds with
live double-frees twice — these shapes are the gate the corpus cannot be.

- record_churn.almd — 2M iterations of construct+drop with a fresh string
  field (free-list reuse, the sentinel/resurrection belt).
- tco_loop_churn.almd — 200k loop-variant TCO calls (the shape that exposed
  the M2 managed-param OOB; acceptance test for per-iteration TCO reclaim).

Compound recursive-drop fixtures (the v1 stage-2 baseline — these pin the three
`emit_typed_rc_dec` branches whose recursive drop schedule is re-decided in the
wasm renderer today and which v1 moves to flat MIR Drop nodes; they are the
targeted regression net for that migration, on the area the byte-gate corpus
covers least). Each deliberately avoids `?? ` (UnwrapOr) to stay a distinct
shape from the open #643 some-box leak:

- map_entry_churn.almd — 200k construct+drop of a Map[String, String] (heap key
  and heap value dropped separately).
- closure_env_churn.almd — 200k construct+drop of a closure capturing a heap
  String (the env must recurse-drop the capture).
- nested_named_churn.almd — 200k construct+drop of a record nesting a
  List[String] (record → list → string recursive drop).
