# Option record literals: #2057 verification (2026-09-09)

On develop `cbf04dbc3`, the reported direct `none` literal is already accepted
by the structural backend. The original reproduction builds a verified structural
WASM artifact (2292 bytes) and prints `0`, matching native. No emitter change was
needed: `crates/almide-wasm/src/data.rs::lower_sum` takes the expected Option shape,
and the record emitter supplies each declared field type to `lower`.

The new C-255 fixture exercises absent and present Option fields over Int, Float,
Bool, String, List[Int] and a named record, all inside a Result-returning function.
Native and structural WASM both print `none\nsome\n`; the interpreter run corpus
pins the same output. The Codec field-matrix test again compares the whole decoded
record to a literal with `none` fields, for both null and missing input.

Only the new fixture's measured rows were added to the allocation and both size
ledgers: heap watermark 69984, emitted size 14022, shipped WASI size 14556 bytes.
Existing rows are unchanged. AST/diagnostic manifests also include the new fixture
and the changed Codec fixture's AST hash. The contract evidence links are symmetric.

## Incumbent determinism domain

Host CI run 34321896641 emits identical bytes for all compared fixtures,
but the new `record_option_none_cells.almd` fixture walls on both incumbent
hosts. It increases their unsupported-input count from 27 to 28. The host
and browser determinism ceilings are updated with this specific reason,
following their existing structural-only fixture policy. This is not a
claim that the incumbent supports the field matrix: the native and current
structural paths execute it, and its allocation/size ledgers cover those
structural bytes. No previously compared fixture is excluded.
