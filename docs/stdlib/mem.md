# mem

Arena checkpoints — the C-041 contract surface, `bytes.heap_save` /
`bytes.heap_restore`'s twin. `import mem`.

Almide manages memory automatically — reference counting on the native leg, the
Perceus-certified discipline on wasm. `mem` is the scope-discipline spelling for
one narrow case: a hot loop that allocates a large number of short-lived values
whose lifetimes nest perfectly. Take a mark before the batch and release back to
it after.

**This is a raw scope discipline, not a garbage collector.** The contract lets an
implementation invalidate every allocation made after a mark on restore, so
anything that outlives the batch must be produced BEFORE the mark, or copied out
before the restore — a program that respects that rule produces identical results
on every target, whatever the allocator underneath does.

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

On every leg both calls are the trivial pair today: native's runtime returns `0`
and ignores the mark (`runtime/rs/src/mem.rs`), and v1's wasm legs link the same
pair from `stdlib/mem_checkpoint.almd` — reference counting already reclaims
scratch deterministically at scope end, exactly what a restore would reclaim, so
there is nothing left for the mark to do (v0's wasm leg reset a bump pointer here;
that allocator is gone). The mark is opaque and never meaningful to print. A
program using `mem` builds and runs on `--target wasm` (both the structural leg
and the incumbent) and behaves byte-identically to native; the parity fixture is
`spec/wasm_cross/mem_checkpoint.almd`. Before 0.63 no wasm leg built it (E081 at
check time, #1423), and before that the native symbols were declared but never
defined, so any program calling them emitted invalid Rust — both are pinned now.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (2 functions)

```
// Opaque checkpoint for restore.
mem.save() -> Int

// Rewinds to mark; a no-op under RC today.
mem.restore(mark: Int) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
