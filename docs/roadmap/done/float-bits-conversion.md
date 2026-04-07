<!-- description: Add int.bits_to_float and float.to_bits for raw IEEE 754 conversion -->
<!-- done: 2026-04-07 -->
# Float Bits Conversion

Add stdlib functions for reinterpreting between Int (i64) and Float (f64) at the bit level.

## Proposed API

```almide
int.bits_to_float(bits: Int) -> Float    // reinterpret i64 bits as f64
float.to_bits(f: Float) -> Int           // reinterpret f64 as i64 bits
```

## Motivation

Found while implementing porta's WASM binary parser. WASM `f64.const` stores 8 raw bytes (IEEE 754 double). After reconstructing the i64 from little-endian bytes, we need to reinterpret as f64:

```almide
let bits = byte_at(b, 0)
  + int.bshl(byte_at(b, 1), 8)
  + int.bshl(byte_at(b, 2), 16)
  + ... // 8 bytes → i64
let float_val = int.bits_to_float(bits)  // reinterpret as f64
```

Without this, the WASM parser cannot decode float constants.

## Implementation

- Rust target: `f64::from_bits(n as u64)` / `f64::to_bits() as i64`
- WASM target: `f64.reinterpret_i64` / `i64.reinterpret_f64` (single instruction each)
