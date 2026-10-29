# ffi

Memory at the foreign-function boundary. `import ffi`.

A pointer a program can read through carries its provenance
([ADR-0031](../adr/0031-raw-pointers-carry-provenance.md)):

- **`Handle`** is an opaque foreign token (`FILE*`, `sqlite3*`). It comes back
  from an `@extern` fn and is only passed back to one; nothing reads or writes
  through it, and it has no `Int` form.
- **`Region`** is a view with a base, a length, a permission and an owner. Its
  fields are opaque. `ffi.read` and `ffi.write` check `0 <= off` and
  `off + n <= ffi.len(r)` before they touch memory, on both targets, and an
  access outside the region answers `err` — never a read or write of other
  memory. A write through a read-only region answers `err` too.

A Region is made in exactly three places, each knowing the length:

| Mint | Length | Writable |
|---|---|---|
| `ffi.region(b)` | `bytes.len(b)`; the region holds `b` | no |
| `ffi.alloc(n)!` | `n`, fresh and zeroed, aliasing no Almide value | yes |
| an `@extern(c)` fn with `@region(len = …, release = "…")` | the declared length | no |

```almide
import ffi

@extern(c, "z", "zbuf_new")
@region(len = n, release = "zbuf_free")
fn zbuf_new(n: Int) -> Region

@extern(c, "z", "zfill")
fn zfill(buf: Region) -> Unit
```

A `Region` parameter of an `@extern(c)` fn is two C arguments, the pointer and
its `size_t` length (`void zfill(uint8_t *buf, size_t len)`), and of an
`@extern(wasm)` fn two numbers, base and length: the callee always gets the
length the runtime holds. A foreign region's release function runs once, when
the last copy of the region goes away; Almide code never frees.

`import ffi` is the opt-in: outside the stdlib, a call to any `ffi` function is
E100 in a file that does not write `import ffi` (#4043). `almide check
--profile critical` refuses every `ffi.*` call even with the import (E100,
#4008). The `Built` line names every `@extern` fn and every `@region` length
clause: the length a binding states is trusted, and so is the foreign code.

**Both targets.** The base is a `*mut u8` on native and a linear-memory offset
on wasm; it is never observed. The bytes moved and the `err` texts are the same
on both (`spec/wasm_cross/bytes_rawptr.almd`, C-062).

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  ffi.write(r, 0, bytes.from_string("hi"))!
  let back = ffi.read(r, 0, 2)!
  println(bytes.to_string(back) ?? "")
  let past = match ffi.read(r, 2, 4) {
    ok(_) => "read",
    err(e) => e,
  }
  println(past)
}
```

**Slices.** `ffi.slice(r, off, n)` is a view of the `n` bytes at `off` inside
`r`. It answers `err` unless `0 <= off` and `off + n <= ffi.len(r)`, so it never
reaches past its parent. A slice is read-only, even of a writable parent. It
reads the parent's memory, so a later write to the parent shows through it, and
it holds the parent's block, so the memory outlives the view. Handing a slice to
an `@extern` passes a sub-range with no copy.

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  let tail = ffi.slice(r, 2, 2)!
  ffi.write(r, 2, bytes.from_list([7, 8]))!
  println(bytes.to_list(ffi.read(tail, 0, 2)!) |> list.map((x) => int.to_string(x)) |> list.join(","))
  let far = match ffi.slice(r, 3, 2) {
    ok(_) => "sliced",
    err(e) => e,
  }
  println(far)
}
```

**Host buffers, with no copy** (ADR-0031 amendment, 2026-10-29). The length
always comes from the Region.

- *Push, wasm.* An `@export(wasm)` fn may return a Region. It crosses as
  `(ptr: i32, len: i32)` through the multi-value return. The host holds one
  share of the Region's block: the bytes stay where they are, whatever the
  program does with its own copies, until the host passes `ptr` to the
  module's `__region_release` export. The host can write straight into an
  `ffi.alloc` Region the export returned, then call the next export, which
  reads it (a top-level `var` holds the program's copy). An empty Region
  crosses as `(0, 0)`. Only a whole Region crosses: an export returning a
  slice aborts. An `effect fn` export's `err` prints and traps.
- *Pull.* A writable `ffi.alloc` Region passed to an `@extern(wasm)` or
  `@extern(c)` fn is lent for that call, as `(base, len)` or
  `(uint8_t*, size_t)`, and the foreign code fills it in place.
- *C callers.* An `@export(c)` fn may take a Region parameter: the caller's
  `(const uint8_t*, size_t)`, read-only, or, for a parameter the declaration
  names with `@region(out = p)`, `(uint8_t*, size_t)`, writable. Either is lent
  for the one call. Every access is still checked against the length the
  caller passed. A copy still held when the call returns stops the process
  rather than outlive the caller's buffer.

```almide
import ffi

var inbox: Region = ffi.region(bytes.new())

@export(wasm, "inbox_open")
effect fn inbox_open(n: Int) -> Region = {
  inbox = ffi.alloc(n)!
  inbox
}

@export(c, "fill")
@region(out = out)
effect fn fill(out: Region, byte: Int) -> Int = {
  ffi.write(out, 0, bytes.from_list(list.repeat(byte, ffi.len(out))))!
  ffi.len(out)
}
```

The removed spellings (dialect epoch 15; `almide fix` renames the first three):

| Removed | Write |
|---|---|
| `ffi.address(b)`, `bytes.as_ptr(b)` | `ffi.region(b)` |
| `bytes.data_ptr(b)` handed to a wasm `@extern` | the `@extern` takes a `Region`: `ffi.region(b)` |
| `ffi.to_int(p)` | nothing: an address has no `Int` form |
| `bytes.as_mut_ptr(b)` + `bytes.copy_to_ptr` | `ffi.alloc(n)!` + `ffi.write(r, off, b)!` |
| `bytes.from_raw_ptr(p, n)`, `ffi.read(p, n)` | `ffi.read(r, 0, n)!` |
| the type `RawPtr` | `Handle`, or `Region` with `@region(…)` |

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (7 functions)

```
// A read-only view of b; b lives as long as the view.
// @since unreleased
ffi.region(b: Bytes) -> Region

// A fresh writable region of n zero bytes; err if n is negative.
// @since unreleased
effect ffi.alloc(n: Int) -> Region

// The region's length in bytes.
// @since unreleased
ffi.len(r: Region) -> Int

// True if write may change the region (ffi.alloc and writable foreign regions).
// @since unreleased
ffi.is_writable(r: Region) -> Bool

// Copy of n bytes at off; err unless 0 <= off and off + n <= len(r).
// @since unreleased
effect ffi.read(r: Region, off: Int, n: Int) -> Bytes

// Writes b at off; err if the region is read-only or b does not fit.
// @since unreleased
effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit

// A copy of the whole region as Bytes.
// @since unreleased
effect ffi.to_bytes(r: Region) -> Bytes
```

<!-- END GENERATED SIGNATURE INDEX -->
