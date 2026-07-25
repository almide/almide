# uint64

Width conversions for `UInt64` (64-bit unsigned, 0 … 18,446,744,073,709,551,615). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: UInt64 = 9000000000000000000
let wide = x.to_int64()      // resolves to uint64.to_int64(x)
let text = x.to_string()
```

> **Convert to `Int` before passing to an `Int`-taking function.** A narrower
> result (`x.to_int32()`) is not implicitly widened, and the checker currently
> accepts the mismatch where the generated code does not build ([#867]).
> `x.to_int64()` is the idiom that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_uint64(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
Both hops collapse — Rust folds the double cast, and the wasm renderer resolves
them inline — so this costs nothing at runtime and both targets agree by
construction.

The rules match Rust's `as`:

| Direction | Behaviour |
|---|---|
| Narrowing an integer | WRAPS (two's complement truncation), never traps |
| Widening a signed integer | Sign-extends |
| Widening an unsigned integer | Zero-extends |
| Float → integer | Saturating truncation (NaN → 0, out-of-range → the nearest bound) |
| Integer → float | Nearest representable, ties to even |

Because narrowing wraps rather than trapping, convert deliberately: check the
range first when a value must round-trip.

## Functions

`to_int8`, `to_int16`, `to_int32`, `to_int64`, `to_uint8`, `to_uint16`,
`to_uint32`, `to_uint64`, `to_float32`, `to_float64` and `to_string`, minus the
identity conversion for this module's own type. The exact set is the machine-owned
signature index below.

Literals of a sized type are range-checked at compile time — `let a: UInt64 = -1`
is an E024 error, not a silent fold.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

```
uint64.to_int8(x: UInt64) -> Int8
uint64.to_int16(x: UInt64) -> Int16
uint64.to_int32(x: UInt64) -> Int32
uint64.to_int64(x: UInt64) -> Int64
uint64.to_uint8(x: UInt64) -> UInt8
uint64.to_uint16(x: UInt64) -> UInt16
uint64.to_uint32(x: UInt64) -> UInt32
uint64.to_float32(x: UInt64) -> Float32
uint64.to_float64(x: UInt64) -> Float64
uint64.to_string(x: UInt64) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
