# ffi

Raw addresses at the foreign-function boundary. `import ffi`.

This is the only stdlib module outside Almide's memory-safety guarantee
(R-L0-45). `ffi.read` and `ffi.write` copy a length of bytes from or to a raw
address, and nothing checks that the address and the length describe memory
the program owns — the caller promises it. They are `effect fn`s: `write`
changes memory outside Almide's values, and `read` reads memory foreign code
wrote. `ffi.address` and `ffi.to_int` only build an address as a value.

`import ffi` is the opt-in: outside the stdlib, a call to any of these four
functions, or to one of the deprecated `bytes` raw-pointer spellings below, is
E100 in a file that does not write `import ffi` (#4043). The rule is per file,
so a program whose files have no `import ffi` and no `@extern` cannot read or
write memory outside its values. `almide check --profile critical` refuses
every `ffi.*` call even with the import (E100, #4008).

**Both targets.** An address is a `*mut u8` on native and a linear-memory
offset on wasm. Its value differs per target and is never printed; null-ness
and the bytes moved through it are the same on both (`spec/stdlib/ffi_test.almd`).
The address of an empty buffer is null on both targets.

The `bytes` raw-pointer functions this module replaces are deprecated since
dialect epoch 15 and removed at epoch 16 (R-ffi-01). Until then they check only
in a file with `import ffi`, with an E052 warning:

| Deprecated | Write |
|---|---|
| `bytes.as_ptr(b)`, `bytes.as_mut_ptr(b)` | `ffi.address(b)` |
| `bytes.data_ptr(b)` | `ffi.to_int(ffi.address(b))` (0 for an empty `b`) |
| `bytes.from_raw_ptr(p, n)` | `ffi.read(p, n)`, from an `effect fn` |
| `bytes.copy_to_ptr(b, p, c)` | `ffi.write(p, b, c)`, from an `effect fn` |

### `ffi.address(b: Bytes) -> RawPtr`

The address of `b`'s first byte, or null when `b` is empty.

### `ffi.to_int(p: RawPtr) -> Int`

The address as an Int (null is 0) — the form a JS host takes.

### `effect ffi.read(ptr: RawPtr, len: Int) -> Bytes`

A fresh copy of the `len` bytes at `ptr`; empty when `ptr` is null or
`len <= 0`.

### `effect ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int`

Writes the first `min(len(b), cap)` bytes of `b` to `ptr` and returns the count;
0 when `ptr` is null. A negative `cap` is an unsigned capacity and writes all
of `b`.

```almd
import ffi

effect fn main() -> Unit = {
  var dst = bytes.new(4)
  let n = ffi.write(ffi.address(dst), bytes.from_string("hi"), 4)
  println(int.to_string(n))
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (4 functions)

```
// Address of b's first byte; null if b is empty.
// @since unreleased
ffi.address(b: Bytes) -> RawPtr

// The address as an Int; null is 0.
// @since unreleased
ffi.to_int(p: RawPtr) -> Int

// Copy of len bytes at ptr; empty if null or len <= 0.
// @since unreleased
effect ffi.read(ptr: RawPtr, len: Int) -> Bytes

// Writes up to cap bytes of b to ptr; the count, 0 if null.
// @since unreleased
effect ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
