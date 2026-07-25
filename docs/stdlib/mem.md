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
same allocations, so a program using `mem` behaves identically on both targets.

The example above runs on both targets from `main`, but it is not covered by a
spec test: `mem.save` walls inside a promoted test fn ("scalar binding outside the
value subset"), and the walled-real ratchet treats a walled function as a
regression. What IS pinned is that the native symbols exist at all — they used to
be declared and never defined, so any program calling them emitted invalid Rust.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (2 functions)

```
mem.save() -> Int
mem.restore(mark: Int) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
