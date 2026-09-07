# mem

Bump-allocator checkpoints for the wasm leg. `import mem`.

Almide manages memory automatically — reference counting on the native leg, the
Perceus-certified discipline on wasm. `mem` is an escape hatch for one narrow
case: a hot loop that allocates a large number of short-lived values whose
lifetimes nest perfectly. Take a mark before the batch, release back to it
after, and the whole batch is reclaimed in one step.

**This is a raw scope discipline, not a garbage collector.** Restoring to a mark
invalidates every allocation made after it. Anything that outlives the batch must
be produced BEFORE the mark, or copied out before the restore. Reach for it only
when a measurement says the allocator is the bottleneck.

### `mem.save() -> Int`

Return the current allocation mark.

### `mem.restore(mark: Int) -> Unit`

Release everything allocated since `mark`.

```almd
let mark = mem.save()
for row in rows {
  let parsed = parse_row(row)     // scratch, dead at the end of the batch
  accumulate(parsed)
}
mem.restore(mark)
```

On the native leg both calls are no-ops: reference counting already reclaims the
same allocations, so a program using `mem` is meant to behave identically on both
targets.

Today, however, NO wasm leg builds it — measured by the target-availability
sweep (#1827) and declared in `proofs/target-availability.toml`: the MIR lowering
refuses the allocator-mark scalar ("scalar binding outside the value subset",
even in the example above from `main`), `mem.restore` is not an admitted
effectful call, and the structural leg has no arm; `almide build --target wasm`
reports it as E081 at check time. What IS pinned is that the native symbols exist
at all — they used to be declared and never defined, so any program calling them
emitted invalid Rust.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (2 functions)

```
mem.save() -> Int
mem.restore(mark: Int) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
