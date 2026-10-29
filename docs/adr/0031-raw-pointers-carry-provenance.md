# ADR-0031: A pointer the program can read through carries its provenance — `RawPtr` splits into an opaque `Handle` and a bounds-checked `Region` (base, length, owner), and a foreign pointer becomes a `Region` only where its `@extern` declaration states the length

- **Status**: Accepted (2026-10-27). The owner answered ○ to all eight [Owner decisions](#owner-decisions-); see [Implementation](#implementation-2026-10-27) for what landed and where it reads the decisions narrowly. Amended 2026-10-29: [Regions at the host boundary](#amendment-2026-10-29-regions-at-the-host-boundary).
- **Date**: 2026-10-27
- **Scope**: the `RawPtr` type, the `ffi` module, the `@extern` signature ABI for pointers (`crates/almide-types/src/types/extern_abi.rs`), the deprecated `bytes` raw-pointer spellings, and security-table row 5.
- **Related**:
  - `docs/roadmap/active/security-by-construction.md` row 5 (memory errors through FFI or raw pointers; status partly).
  - #4043 (`import ffi` is the opt-in, E100), #4008 (`@memory_unsafe`, the critical profile, the `Built` line), #3054 (E090, the `@extern(c)` ABI table), #3420 (an address borrows its block), #3966 (a write through `bytes.as_mut_ptr` changed a `let`).
  - `tests/raw_ptr_surface_test.rs`: the part of this ADR that needs no ruling, landed with it.

## Context

Row 5's remaining gap, in its own words: inside a file that wrote `import ffi`, nothing checks that an address and a length describe memory the program owns, and no check can, because a `RawPtr` is a bare address on both targets.

What a `RawPtr` is today:

| | native | wasm |
|---|---|---|
| representation | `*mut u8` | `i32` offset into linear memory (an `i64` in MIR) |
| length attached | none | none |
| owner attached | none (#3420 keeps the source block alive by borrow analysis, not by the value) | same |

Where a user program gets one (pinned by `tests/raw_ptr_surface_test.rs`, derived from declared signatures):

1. **The address of a `Bytes` it owns**: `ffi.address(b)`, and the deprecated `bytes.as_ptr` / `as_mut_ptr`. The owner and the length are known at the mint, then thrown away.
2. **A foreign function's return**: `@extern(c, …) fn f(…) -> RawPtr`. The only record of the allocation is the foreign library's.

Nothing else mints one. There is no `Int → RawPtr` path a user file can name (`prim.int_to_ptr` is behind E085), no arithmetic on `RawPtr`, no literal, and no Codec field. Addresses do also travel as `Int`, through `ffi.to_int` and `bytes.data_ptr`, which a wasm `@extern` takes as `(ptr: Int, len: Int)` (`spec/wasm_host_js/extern_data_ptr_lifetime.almd`).

So the length is lost at exactly two places, and both are places where the compiler knows something: at (1) the runtime knows the block's length, and at (2) the declaration is the one place a binding author states facts about the foreign function. The ADR attaches provenance at those two mints and checks every access against it.

## Survey: what other languages put in a pointer (measured 2026-10)

| Language | Pointer that can be read through | Where the length comes from | Owner / lifetime | What stays trusted |
|---|---|---|---|---|
| **Rust** (strict provenance, stable since 1.84, 2025-01) | `&[T]` is (ptr, len); `*const T` carries provenance in the abstract machine, and `addr()` / `with_addr()` / `without_provenance()` keep the integer and the pointer apart; `with_exposed_provenance` is the explicit escape | `slice::from_raw_parts(ptr, len)`, an `unsafe` call: the length is asserted once, at the conversion | borrow checker for `&[T]`; none for `*const T` | every `unsafe` block; Miri checks provenance in tests only |
| **CHERI** (Arm Morello, CHERIoT) | a capability: address + base + bounds + permissions + a validity tag in hardware | set at allocation, can only shrink (monotonic) | an integer cannot become a capability: writing over one clears its tag | the allocator that sets the first bounds |
| **Zig** | `[]T` slice (ptr, len), bounds-checked in Debug and ReleaseSafe; `[*:0]T` sentinel-terminated; `[*]T` many-pointer of unknown length; `[*c]T` only from translated C | `ptr[0..n]`: the length is asserted at the slicing | none | the slicing site; `@ptrFromInt` |
| **Swift** 6.2 | `UnsafeBufferPointer` (base, count; checked in debug only); `Span` / `MutableSpan` / `RawSpan` are non-escapable (`~Escapable`) views, always bounds-checked, whose lifetime is tied to the owner; SE-0458 `-strict-memory-safety` marks every unsafe use | C declarations annotated with Clang's `-fbounds-safety` `__counted_by(n)` / `__sized_by(n)` are imported as safe overloads taking `Span` | the owner the view depends on | the `__counted_by` annotation |
| **Go** (cgo) | `unsafe.Pointer`; `unsafe.Slice(ptr, len)`; `C.GoBytes(p, n)` copies | asserted at `unsafe.Slice` / `C.GoBytes` | C may not keep a Go pointer after the call returns unless pinned (`runtime.Pinner`, 1.21); `GODEBUG=cgocheck=1` checks dynamically | the length at the conversion; C's behaviour |
| **Wasm component model** (WASI 0.2 / 0.3) | no pointer crosses a component boundary | `list<u8>` is copied by the canonical ABI (lift / lower through `cabi_realloc`), so the receiver gets a length the runtime measured | `resource` values cross as `own<T>` / `borrow<T>` handles: table indices checked on every use; a `borrow` ends with the call, dropping an `own` runs the destructor | the component's own code |

What they agree on:

- **Two kinds, never one.** Every design separates a token that is only passed back (CM `resource`, Rust `*mut Opaque`, Swift `OpaquePointer`) from a view that is read through (slice, `Span`, capability). Almide's `RawPtr` is both.
- **The length is stated once, where the pointer enters**, and checked on every access after that: `from_raw_parts`, `ptr[0..n]`, `__counted_by`, `unsafe.Slice`. The statement site is the trusted base, and it is small and countable.
- **An integer is not a pointer.** CHERI enforces it in hardware, Rust's strict provenance in the API, the CM by having no pointers at all.
- **The owner is part of the view**: Swift's `Span` depends on its owner, a CM `own` handle runs the destructor, Go pins.

## Decision

### D1. `RawPtr` splits into two types

- **`Handle`**: an opaque foreign token (`FILE*`, `sqlite3*`, a context pointer). It can only be passed to an `@extern` fn. No `ffi` function reads or writes through it, and it has no `Int` form. This is what most `@extern(c) … -> RawPtr` returns are today. It is the CM `resource`.
- **`Region`**: a view that can be read or written, carrying **(base, length, permission, owner)**. `ffi.read(r, off, n)` and `ffi.write(r, off, b)` check `0 ≤ off` and `off + n ≤ len` before they touch memory, on both targets, and refuse a write through a read-only region.

Representation: native `struct { ptr: *mut u8, len: usize, perm: u8, owner: Owner }`; wasm three `i32`s plus the owner slot. `Region` is a heap value under Perceus like `Bytes`, so its owner reference is counted.

### D2. A `Region` is minted at exactly three places, each knowing the length

1. `ffi.region(b: Bytes) -> Region`: **read-only**, length `bytes.len(b)`, owner = `b` (one RC share, so the block outlives every view of it — #3420's lifetime by ownership instead of by borrow analysis).
2. `ffi.alloc(n: Int) -> Region`: **writable**, fresh, unique; `ffi.to_bytes(r) -> Bytes` copies out. A writable region never aliases an Almide value, so the #3966 class (a write through a pointer changes a `let`) has no spelling.
3. A foreign return declared with its length:

   ```almide
   @extern(c, "z", "zbuf_new")
   @region(len = n, release = "zbuf_free")
   fn zbuf_new(n: Int) -> Region
   ```

   `len` names a parameter or a constant. `release` names the C function that frees it, run when the last `Region` share drops. The same declaration without `@region` must return a `Handle` (E090 otherwise). This is Clang's `__counted_by` / Swift's safe import moved to the binding.

### D3. A `Region` argument crosses the C ABI as two arguments

`fn zfill(buf: Region)` binds the C prototype `void zfill(uint8_t *buf, size_t len)`. The callee is handed the length the runtime holds, never one the caller typed, so passing a region to C cannot overstate it. On wasm, a `Region` crosses a `@extern(wasm)` as `(i32 ptr, i32 len)`; on a component (p2/p3) it is lowered as `list<u8>` by the canonical ABI, a copy, and a `Handle` as `own<resource>`.

### D4. Addresses as integers leave

`ffi.to_int`, `bytes.data_ptr`, and the deprecated `bytes.as_ptr` / `as_mut_ptr` / `from_raw_ptr` / `copy_to_ptr` are removed. The wasm hosts that take `(ptr: Int, len: Int)` take a `Region` instead (D3). After this there is no user-reachable conversion between `Int` and any pointer, in either direction.

### D5. Landing

One dialect epoch, no deprecation window (the project's ruling on mismatched shapes). `almide fix` rewrites `ffi.address(b)` to `ffi.region(b)` and an `@extern … -> RawPtr` without a length to `-> Handle`. The `Built` line keeps naming every `@extern` fn and adds every `@region` length clause.

### What stays trusted after D1–D5

1. Each `@region(len = …)` clause: the binding author's statement that the foreign function returns that many bytes. One per declaration, counted on the `Built` line and refused under `--profile critical`.
2. The foreign code itself: native C can write anywhere in the process. No language closes that without process or sandbox isolation (CHERI hardware, or wasm, where the host is the foreign code).
3. The runtime bodies of `ffi.read` / `ffi.write`, counted by `scripts/check-runtime-unsafe.sh`.

Row 5 can then close by its own definition: the bounds check is a mechanism, and the gate is a differential fixture per mint kind (owned, allocated, foreign) where an access at `len` and at `-1` is an `Err` on both targets, plus `tests/raw_ptr_surface_test.rs` extended from `RawPtr` to `Region`.

## Implemented now (needs no ruling)

`tests/raw_ptr_surface_test.rs` pins the precondition of D2: provenance can only be attached at the mints, so the mints must stay the two that exist. Derived from the declared signatures, never from a name list:

- every user-reachable stdlib fn whose signature holds a `RawPtr` is `@memory_unsafe`;
- every user-reachable stdlib fn returning a `RawPtr` takes a `Bytes`;
- the module that mints one from an `Int` (`prim`) is excluded by the same declaration E085 reads (`stdlib::is_user_nameable_module`), and the exclusion is checked to be non-vacuous;
- in a file that wrote `import ffi`, arithmetic on a `RawPtr`, an integer literal typed `RawPtr`, a `RawPtr` Codec field and `prim.int_to_ptr` are each refused, with a clean control.

## Implementation (2026-10-27)

All eight decisions land in dialect epoch 15, with no deprecation window (`proofs/dialect-epochs.toml`: each removed spelling is a `breaks` entry, `almide fix` renames a removed call whose rewrite keeps its arguments — `ffi.address(b)` / `bytes.as_ptr(b)` / `bytes.data_ptr(b)` → `ffi.region(b)` — and the type `RawPtr` → `Handle`; the rest carry a hint).

- **D1.** `Handle` is the builtin head that was `RawPtr`. `Region` is `mod type Region = { owner, base, len, writable, keep }` in stdlib/ffi.almd: opaque outside the module (E033), so a program cannot read `base` or forge `len`. The three raw operations (`__address`, `__load`, `__store`) are `local` to stdlib/ffi.almd (E420 from anywhere else) and run only after `read` / `write` checked the bounds. Native implements them in runtime/rs/src/ffi.rs, wasm in stdlib/ffi_bridge.almd.
- **D2.** `ffi.region(b)` (read-only, owner `b`), `ffi.alloc(n)` (an `effect fn`, so two calls are never merged into one shared buffer), and an `@extern(c)` fn declaring `@region(len = …, release = "…")` (check/extern_abi_check.rs validates the clause: E090 without it, or when `len` names no integer parameter). The clause is counted on the `Built` line.
- **D3.** An `@extern(c)` Region parameter is `(*mut u8, usize)` in the glue (walker/extern_c.rs). An `@extern(wasm)` Region parameter is split into `(base, len)` before the structural emitter reads the program (crates/almide-wasm/src/extern_region.rs), and the JS host glue sees the same two numbers.
- **D4.** A foreign Region is read-only too: D4's "writable regions come only from `ffi.alloc(n)`" is read literally. A foreign library writes its own buffer; the program hands it a Region (D3) to fill.
- **D5.** `ffi.read` / `ffi.write` answer `err` with the offset, count and length; list indexing still aborts (C-067).
- **D6.** The glue stores a release guard in the Region's `keep` closure; every copy shares one `Rc`, so the release function runs once, when the last copy drops.
- **D7.** `ffi.to_int`, `ffi.address`, `bytes.data_ptr` and the four other `bytes` raw-pointer spellings are gone; `prim.int_to_ptr` / `ptr_to_int` remain the stdlib's own floor (E085 outside it).

Read narrowly, and named for a decision if the owner wants otherwise:

- D3's wasm `(i32 ptr, i32 len)`: the two values cross as the emitter's integer scalar (an `i64` at the import, a number at the JS hook), the ABI every other `Int` import parameter has. Narrowing them to `i32` needs a new scalar class in the wasm import ABI.
- D3's component-model lowering (`list<u8>` / `own<resource>`) is not part of this change: the p2/p3 components take no `@extern(wasm)` hooks today.

Gates: tests/raw_ptr_surface_test.rs (signature-derived: no user-reachable stdlib fn holds a Handle; every Region fn is `@memory_unsafe`; the forgery spellings are refused; the bounds and permission checks answer `err` on the native and the wasm leg), spec/wasm_cross/bytes_rawptr.almd (C-062, C-376), tests/extern_rust_abi_test.rs (the C ABI pair and the single release), scripts/check-rawptr-surface.py.

## Amendment (2026-10-29): Regions at the host boundary

The epoch-15 migration left about 30 downstream repos red, because a Region could not
be exchanged with a host. The JS drivers of about 20 fizz repos and the nendo-ceangal-web
pool asked an export for a buffer address, wrote into it, and called the next export;
ptcg-abc and bindgen's scaffolding pass a C buffer into an `@export(c)` fn. The owner
answered each gap on 2026-10-29. The binding requirement is answer 5: the inbound host
data path is zero-copy and still bounded, because the ideal must not cost performance.
The length always comes from the Region. No `Bytes` copy happens at the boundary.

### A1. An `@export(wasm)` fn may return a Region

It crosses as `(ptr: i32, len: i32)` through wasm's multi-value return, the wasm32 form
of the `(base, length)` pair a Region argument crosses an import as (D3).

**Ownership.** The returned Region stays alive until the host hands `ptr` back to the
module's `__region_release` export. Exactly:

1. The adapter the emitter exports keeps one share of the Region's block for the host.
   The program's own copies of the Region drop by the ordinary rules, and none of them
   can free the block while the host's share exists. So `ptr` never dangles, and the
   bytes the host writes stay where they are across later calls.
2. `__region_release(ptr)` gives that share back. A host that never calls it keeps the
   block for the instance's lifetime. That is a leak, never a dangling pointer.
3. An empty Region crosses as `(0, 0)` and holds nothing. Releasing 0 is a no-op.
4. Only a whole Region crosses: one whose length is its block's length (`ffi.alloc`,
   `ffi.region`). A sub-region (`ffi.slice`) has no block of its own, so there is no
   address the host could give back. The export aborts with the reason instead of
   handing one out.
5. An `effect fn` export's `err` has no slot in `(ptr, len)`. The adapter writes it to
   stderr and traps.

Mechanism: an IR rewrite (crates/almide-wasm/src/extern_region.rs,
`wrap_region_exports`) moves the export to a compiler-made wrapper returning
`ffi.__export_owner(f(…))`, the Region's block, so the record is released by the
ordinary ownership rules. A stamp on the wrapper (`region_export`) tells the emitter to
export an adapter in its place. The adapter (crates/almide-wasm/src/region_export.rs)
calls the wrapper and returns the block's data address and length, keeping the
wrapper's share for the host.

### A2. An `@export(c)` fn may take Region parameters

A Region parameter crosses as the C caller's `(const uint8_t *ptr, size_t len)` and is
read-only. A parameter the declaration names with `@region(out = p)` crosses as
`(uint8_t *ptr, size_t len)` and is writable: the caller-provided buffer form. Either one
is **lent for the call**. It owns nothing, because the memory is the caller's. Every
access is checked against the caller's length (D5: out of bounds is `err`). After the
call, the wrapper stops the process if any copy of the Region is still held, for example
in a top-level `var`. That keeps the Region from outliving a buffer the caller may free
the moment it regains control. Nothing can revoke a value from every copy, so the
check is Go's `cgocheck` answer, applied always. `check` refuses (E090) a Region return
from an `@export(c)` fn, because the caller could not free it. It also refuses an `out`
naming anything but a Region parameter, and a `@region` clause on a fn that neither
binds nor exports C. The `Built` line counts each lent Region (`@export(c) Region `p`
of `f``), because its length is the caller's word, like a `@region(len = …)` clause.

### A3. `ffi.slice(r, off, n) -> Result[Region, String]`

A read-only view of the `n` bytes at `off` inside `r`. The answer is `err` unless
`0 <= off` and `off + n <= ffi.len(r)`, and a negative count is `err` too. The view
reads the parent's memory, so a later write to a writable parent shows through it. It
holds the parent's owner (and its release guard), so the memory outlives the view. It is
never writable, even when the parent is. It mints no new provenance: the new base is
computed only after the bounds check (`__offset`, a `local` intrinsic). It removes the
per-frame outbound sub-range copies in nendo and ceangal, because a slice handed to an
`@extern` is a sub-range with no copy. C-062 is amended to state it.

### A4. Zero-copy inbound host data

- **Push.** The host writes straight into the writable Region that an A1 export returned,
  then calls the next export, which reads it through the program's own copy (a top-level
  `var`).
- **Pull.** A writable `ffi.alloc` Region passed to an `@extern(wasm)` or `@extern(c)`
  fn is lent for that call (D3 already did this). The foreign code fills it in place.

### How others cross a buffer (surveyed 2026-10)

| | Outbound view | Inbound, caller's buffer | Lifetime across the boundary |
|---|---|---|---|
| **Rust** | `&[u8]` → `(ptr, len)` via `as_ptr` / `len`; `Box<[u8]>::into_raw` to give one away | `slice::from_raw_parts(_mut)(ptr, len)` in an `extern "C" fn`: the length is the caller's word, asserted once | borrowed for the call (`'a` unbounded in practice); `into_raw` / `from_raw` hand ownership over and back |
| **Swift** 6.2 | `Span` / `RawSpan` (non-escapable views) | `MutableSpan` / `OutputSpan` for a caller-provided buffer; `withUnsafeMutableBytes` | `~Escapable`: the compiler forbids the view outliving the call — Almide checks it at run time instead (A2) |
| **Wasm component model** | `list<u8>` is copied by the canonical ABI into the callee's memory (`cabi_realloc`) | same copy | `borrow<T>` handles end with the call, `own<T>` runs the destructor: the shape of A1's share and A2's lend, without zero-copy for bytes |
| **Zig** | `[]u8` = `(ptr, len)`; `export fn f(p: [*]u8, n: usize)` and `p[0..n]` | `[*]u8` + `usize`, sliced once | none checked; a slice may outlive its memory |

Almide takes Rust's and Zig's `(ptr, len)` crossing, because answer 5 rules out the
component model's copy. It takes the component model's `own` / `borrow` split as its
lifetime rule: A1 hands over a share, which is like `own`, and A2 lends for one call,
which is like `borrow`. Where Swift checks non-escape statically, Almide checks it at
run time, because a non-escape rule is what the checker does not have (see the
scoped-borrow alternative below).

### What stays trusted (added)

- The C caller's `(ptr, len)` for an `@export(c)` Region: one statement per call, counted
  on the `Built` line per declaration.
- The wasm host, which can write anywhere in linear memory. A1 gives it a share and a
  length, not a capability. A read-only Region the host is handed is read-only by
  contract only.

### Read narrowly, named for a decision

- The `(ptr, len)` of an A1 export is `(i32, i32)`, as answer 7 of the original decisions
  says. A Region argument to an `@extern(wasm)` import still crosses as the `Int` scalar
  (`i64`), the narrowing the [Implementation](#implementation-2026-10-27) section left
  open. The two directions differ until that narrowing lands.
- A1 exports no JS-host marshalling (`--host js` notes no shape for the adapter): a JS
  driver reads `instance.exports.f()` as the two numbers directly.
- There is no C header generator. A2's prototypes are written by hand:
  `int32_t f(const uint8_t *buf, size_t buf_len)`, and `uint8_t *` for an `out` parameter.

Evidence: crates/almide-wasm/tests/ffi_host_boundary.rs (A1, A4 push and pull on wasm,
driven by wasmtime), tests/ffi_host_boundary_test.rs (A2 on native: read-only and `out`
lends, the bounds `err`s, the escape stop, the E090 rules),
tests/extern_rust_abi_test.rs (A4 pull on native, `(uint8_t*, size_t)`),
tests/raw_ptr_surface_test.rs and spec/wasm_cross/bytes_rawptr.almd (A3 on both
targets, C-062).

## Alternatives considered

- **Keep `RawPtr` and check against an allocation table.** The runtime records every block it hands out and `ffi.read` looks the address up. It covers mint (1) only: a foreign pointer is not in the table, and that is the case that matters. It also costs a lookup per access and grows the trusted runtime. Rejected.
- **A length parameter on `ffi.read` checked against nothing.** That is today's design; the length is the caller's word.
- **Scoped borrows (`ffi.with_region(b, (r) => …)`, Swift `withUnsafeBytes`).** Sound for mint (1), but it needs a non-escaping closure rule the checker does not have, and it says nothing about foreign pointers. D2's RC share gives the same lifetime with no new rule.
- **Hardware capabilities (CHERI).** The strongest answer, and not available on the targets Almide ships.
- **Remove raw memory entirely; bind C only through `@extern(rust)` shims.** It moves the same unchecked pointer into a `native/*.rs` file the checker does not read. The trusted base grows rather than shrinks.

## Consequences

- Good: an out-of-bounds read or write through `ffi` becomes an `Err` on both targets. The trusted base shrinks from every `ffi.read` / `ffi.write` call to one length clause per foreign declaration. #3420 and #3966 lose their spellings.
- Cost: a type split and an ABI change (`Region` is two C arguments). Every existing `@extern` binding with a pointer return is rewritten (mechanically, by `almide fix`). Each access pays one bounds check.
- The `Handle` / `Region` split makes the CM mapping direct (`own<resource>` / `list<u8>`).

## Falsifier

- A binding in the wild whose foreign function returns a buffer whose length is not a parameter, a constant or a returned out-value, and that cannot be bound as a `Handle` plus accessor functions. Then `@region(len = …)` needs a richer form, or D2.3 is wrong.
- The bounds check costing more than 5% on `research/benchmark/perf` workloads that use `ffi`.

## Owner decisions (○×)

One decision per question, each with a recommendation.

1. **Split `RawPtr` into `Handle` (opaque, passed back to foreign code only) and `Region` (base, length, permission, owner; every access bounds-checked).**
   Recommendation: ○. Every surveyed design separates the two (CM resource vs `list`, Swift `OpaquePointer` vs `Span`, Rust opaque pointer vs slice).
2. **A foreign pointer return is a `Region` only when its `@extern` declaration states the length (`@region(len = …)`); otherwise it must be a `Handle`.**
   Recommendation: ○. This is Clang's `__counted_by` and Swift's safe C import moved onto the binding, and it makes each length claim a counted trusted line.
3. **A `Region` argument crosses the C ABI as two arguments (pointer, `size_t` length), so the callee always gets the runtime's length.**
   Recommendation: ○. With a separate caller-typed length, D1's check stops at the boundary.
4. **A `Region` made from a `Bytes` is read-only; a writable `Region` comes only from `ffi.alloc(n)`.**
   Recommendation: ○. It keeps value semantics: no write can reach a `let` (#3966). Rust's `&[u8]` vs `&mut [u8]` and CHERI's permission bits make the same cut.
5. **An out-of-bounds `ffi.read` / `ffi.write` returns an `Err`, not an abort.**
   Recommendation: ○. Both are already effect fns with an error channel, and a foreign length that turns out wrong is a runtime fact the caller can handle. Indexing a list keeps aborting (C-067); this is the ffi surface only.
6. **A foreign `Region` names its release function at the declaration (`release = "…"`), and the runtime calls it when the last share drops; Almide code never frees.**
   Recommendation: ○. It is the CM `own` destructor, and it removes double-free and use-after-free from the Almide side.
7. **Remove the address-as-`Int` spellings (`ffi.to_int`, `bytes.data_ptr`) and pass `Region` to wasm `@extern`s as `(i32, i32)`.**
   Recommendation: ○. Otherwise an `Int` still reaches the host as an address and the length check is bypassed.
8. **Land D1–D7 as one dialect epoch with no deprecation window, with `almide fix` rewriting the old spellings.**
   Recommendation: ○. This follows the project's ruling that mismatched shapes are removed outright, and the surface is behind `import ffi`, so the set of affected files is small and visible.

## References

- Rust strict provenance (stabilized in 1.84): https://doc.rust-lang.org/std/ptr/index.html#strict-provenance
- `slice::from_raw_parts`: https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html
- CHERI ISA (UCAM-CL-TR-987): https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-987.pdf ; CHERIoT: https://cheriot.org/
- Zig pointers and slices: https://ziglang.org/documentation/master/#Pointers , https://ziglang.org/documentation/master/#Slices
- Swift SE-0447 `Span`: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0447-span-access-shared-contiguous-storage.md ; SE-0458 strict memory safety: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0458-strict-memory-safety.md
- Clang `-fbounds-safety`: https://clang.llvm.org/docs/BoundsSafety.html
- Go cgo pointer passing rules: https://pkg.go.dev/cmd/cgo#hdr-Passing_pointers ; `runtime.Pinner`: https://pkg.go.dev/runtime#Pinner
- Component model canonical ABI and resources: https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md , https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#handle-types
