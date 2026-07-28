# int64

Width conversions for `Int64` (64-bit signed integer). Auto-imported — no `import` needed.

`Int64` and the canonical `Int` share a representation but are distinct types, so
`Int64` gets its own module for the same reason `int32` does: without it the only
route was the `int.to_string(int.from_int64(x))` pivot, which a reader who has
used `int32.to_string` has no reason to expect.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let n: Int64 = 7
let narrow = n.to_int16()    // resolves to int64.to_int16(n)
let text = n.to_string()
```

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_int64(x)` re-tags, then `int.to_<dst>(...)` narrows or widens.
Both hops collapse — Rust folds the double cast, and the wasm renderer resolves
them inline — so this costs nothing at runtime and both targets agree by
construction.

The rules match Rust's `as`:

| Direction | Behaviour |
|---|---|
| Narrowing an integer | WRAPS (two's complement truncation), never traps |
| Widening a signed integer | Sign-extends |
| Widening an unsigned integer | Zero-extends |
| Integer → float | Nearest representable, ties to even |

Because narrowing wraps rather than trapping, convert deliberately: check the
range first when a value must round-trip.

## Functions

`to_int8`, `to_int16`, `to_int32`, `to_uint8`, `to_uint16`, `to_uint32`,
`to_uint64`, `to_float32`, `to_float64` and `to_string`. There is no `to_int64` —
that is the identity on this type. The exact set is the machine-owned signature
index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

```
int64.to_int8(x: Int64) -> Int8
int64.to_int16(x: Int64) -> Int16
int64.to_int32(x: Int64) -> Int32
int64.to_uint8(x: Int64) -> UInt8
int64.to_uint16(x: Int64) -> UInt16
int64.to_uint32(x: Int64) -> UInt32
int64.to_uint64(x: Int64) -> UInt64
int64.to_float32(x: Int64) -> Float32
int64.to_float64(x: Int64) -> Float64
int64.to_string(x: Int64) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
