# Fan semantics audit probes

The programs behind every "probe" citation in
[docs/design/fan-semantics-audit.md](../../../docs/design/fan-semantics-audit.md),
pinned at 0746eb689. Run them from the repository root on both targets:

```bash
almide run research/fan-audit/probes/B/race_order.almd
almide run research/fan-audit/probes/B/race_order.almd --target wasm
```

- `A/`: the block, `fan.map` and `fan.settle` forms. Only the two probes that
  found divergences are kept; the other block/map/settle observations are
  pinned by `tests/fan_acceptance_test.rs`.
- `B/`: `fan.any` and `fan.race`.
- `C/`: `fan.bounded`, `fan.timeout`, the time constructors, and effect
  admission.

These are evidence, not tests. `almide test` skips them because they have no
`test` blocks. Several are expected to fail to compile: those probes check
that a program is refused, for example with E006.
