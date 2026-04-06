# WASM Memory Model

Almide compiles to a standalone WASM binary targeting WASI preview1. The runtime uses two linear memories, a bump allocator with no free/GC, and a scratch buffer protocol for zero-intermediate-allocation string interpolation.

This document describes the exact physical layout, allocation algorithm, data representation, and safety properties of the memory model.

---

## Physical Layout

### Memory 0 (Main)

Memory 0 holds everything: WASI scratch space, static data, string literals, and the heap. Initial size is 64 pages (4 MB). Grows on demand via `memory.grow(0)` in 16-page (1 MB) chunks.

```
Address       Size    Content
──────────────────────────────────────────────────────────────────
0x0000-0x000F  16 B   WASI iov struct (fd_write scratch)
0x0010-0x002F  32 B   int_to_string scratch buffer
0x0030         1 B    Newline byte (0x0A)
0x0031-N       var    String literal data segment
N (8-aligned)  ...    Heap start (bump allocator, grows upward →)
```

#### Byte-Level Example

Consider a program with three string literals: `"hello"`, `"world"`, and `""`.

```
Offset  Hex                                          Decoded
──────  ───────────────────────────────────────────  ────────────────────────
0x0000  00 00 00 00  00 00 00 00                     iov[0].buf_ptr (4B) + iov[0].buf_len (4B)
0x0008  00 00 00 00  00 00 00 00                     iov[1].buf_ptr (4B) + iov[1].buf_len (4B)
0x0010  00 00 00 00  00 00 00 00  ...  (32 bytes)    int_to_string scratch
0x0030  0A                                           newline byte '\n'

── String literal data segment (starts at 0x0031) ──

0x0031  05 00 00 00                                  len=5 (i32 LE)        ← "hello" ptr = 0x31
0x0035  68 65 6C 6C 6F                               "hello" (UTF-8)
0x003A  05 00 00 00                                  len=5 (i32 LE)        ← "world" ptr = 0x3A
0x003E  77 6F 72 6C 64                               "world" (UTF-8)
0x0043  00 00 00 00                                  len=0 (i32 LE)        ← "" ptr = 0x43

── Heap (8-byte aligned) ──

0x0048  ...                                          heap_ptr starts here
```

Key observations:
- String pointer `0x31` points to the length prefix, not the character data. The codegen reads `i32_load(ptr)` to get the byte length and `ptr + 4` to get the UTF-8 data start.
- The empty string `""` is interned like any other literal. Its length prefix is 0 and it occupies exactly 4 bytes.
- Identical string literals share the same offset (deduplicated at compile time via `HashMap<String, u32>`).
- Heap start is the first 8-byte aligned address after the data segment. In this example: `0x0043 + 4 = 0x0047`, aligned up to `0x0048`.

### Memory 1 (String Builder Scratch)

Memory 1 is a dedicated temporary buffer for string interpolation. Initial size is 1 page (64 KB). It holds zero persistent data.

```
Address       Content
──────────────────────────────────────────────
0x0000-M      Temporary bytes from __scratch_write_str calls
              Reset to offset 0 after each __scratch_finalize
```

Memory 1 exists solely to avoid intermediate heap allocations during string building. Without it, interpolating N fragments would require N-1 heap allocations for intermediate concatenation results. With the scratch buffer, there is exactly one allocation (the final string).

---

## Allocator

### Algorithm

The heap allocator is a bump (arena) allocator. One global pointer (`__heap_ptr`, global 0) tracks the next free byte. Allocation advances the pointer; there is no free operation.

```
function __alloc(size: i32) -> i32:
    // Step 1: Align current heap pointer up to 8-byte boundary
    ptr = (heap_ptr + 7) & ~7        // round up: mask off low 3 bits

    // Step 2: Advance heap pointer past the allocation
    heap_ptr = ptr + size

    // Step 3: Grow memory if the new heap_ptr exceeds current memory size
    while heap_ptr > memory.size(0) * 65536:
        result = memory.grow(0, 16)   // grow by 16 pages (1 MB)
        if result == -1:
            trap()                     // out of memory — unreachable

    // Step 4: Return the aligned pointer
    return ptr
```

This is the actual logic of the `compile_alloc` function in `crates/almide-codegen/src/emit_wasm/runtime.rs`. The WASM instructions correspond 1:1.

### Alignment

All allocations are 8-byte aligned. The alignment formula is:

```
aligned = (raw + 7) & 0xFFFFFFF8    // equivalent to (raw + 7) & ~7
```

This means:
- Requesting 3 bytes returns a pointer at an 8-byte boundary, and the next allocation starts 3 bytes later (but will itself be re-aligned to the next 8-byte boundary).
- The wasted padding between a 3-byte allocation and the next 8-byte boundary is **5 bytes**. This is the price of alignment.
- 8-byte alignment is chosen because `i64` and `f64` values are 8 bytes, and unaligned loads/stores for these types can trap on some WASM implementations. This matches the conventions of wasi-libc and Emscripten.

Example: three sequential allocations of sizes 3, 12, and 5.

```
heap_ptr = 0x0048  (initial, already 8-aligned)

alloc(3):
  ptr = (0x0048 + 7) & ~7 = 0x0048    ← returned
  heap_ptr = 0x0048 + 3 = 0x004B

alloc(12):
  ptr = (0x004B + 7) & ~7 = 0x0050    ← returned (5 bytes padding)
  heap_ptr = 0x0050 + 12 = 0x005C

alloc(5):
  ptr = (0x005C + 7) & ~7 = 0x0060    ← returned (4 bytes padding)
  heap_ptr = 0x0060 + 5 = 0x0065
```

Memory after these allocations:
```
0x0048  [3 bytes used] [5 bytes padding]
0x0050  [12 bytes used] [4 bytes padding]
0x0060  [5 bytes used] ...
```

### Growth Strategy

Memory 0 starts at 64 pages (4 MB). When an allocation would exceed the current memory size, the allocator grows memory in 16-page (1 MB) increments.

Why 16 pages at a time:
- **Amortization.** `memory.grow` is expensive (the runtime must zero-fill or remap pages). Growing 16 pages at once means fewer grow calls. A program that allocates 10 MB of heap will call `memory.grow` approximately 6 times instead of ~150 times (if growing 1 page at a time).
- **1 MB is a practical minimum.** A single large string or list can easily be hundreds of KB. Growing by 64 KB (1 page) would cause repeated grow calls during a single allocation loop.
- **Memory is virtual.** On modern OSes, WASM runtimes (Wasmtime, Wasmer) use virtual memory. Requesting 1 MB does not immediately consume 1 MB of physical RAM; pages are demand-paged. The cost of over-requesting is negligible.

The growth loop runs repeatedly until memory is large enough:

```
while heap_ptr > memory.size(0) * 65536:
    if memory.grow(0, 16) == -1:
        unreachable   // trap: OOM
```

If `memory.grow` returns -1 (host refused to allocate), the program traps immediately. There is no fallback or error recovery — in WASM, OOM is fatal.

### Why Bump-Only, No Free?

Almide WASM agents are **short-lived, single-execution processes**. A typical agent:
1. Starts, allocates working memory.
2. Processes input, produces output.
3. Exits. The entire linear memory is discarded by the host.

In this execution model, a free/GC allocator adds complexity and overhead with zero benefit:

| Property | Bump allocator | Free-list / GC |
|----------|---------------|-----------------|
| Allocation speed | O(1), one pointer bump | O(n) search or GC pause |
| Implementation size | ~30 WASM instructions | Thousands of instructions |
| Fragmentation | None (contiguous) | Can fragment over time |
| Determinism | Fully deterministic | GC pauses are unpredictable |
| Code size | Minimal | Significant runtime overhead |
| Correctness risk | None (no dangling pointers) | Use-after-free, double-free possible |

The "leak" of unreachable memory is bounded by the agent's lifetime. When the WASM instance is destroyed, the host reclaims everything. For long-running WASM programs, a bump allocator would be inappropriate — but Almide's design target is agents, not servers.

### Comparison with Other WASM Language Allocators

| Language | Allocator | Strategy | Trade-off |
|----------|-----------|----------|-----------|
| **Almide** | Bump (no free) | Single pointer advance, 8-byte aligned | Zero overhead, zero complexity, leaks are harmless for short-lived agents |
| **Rust (wasm32)** | dlmalloc | Free-list with coalescing | Full general-purpose allocator, ~3 KB code overhead, handles long-running programs |
| **AssemblyScript** | GC (TLSF + reference counting) | Two-level segregated fit + ref-counting GC | Supports long-lived objects, but adds ~8 KB runtime and GC pauses |
| **Zig (wasm32)** | Page allocator | Directly maps to `memory.grow`, no sub-page allocation | Simple but wasteful (minimum 64 KB per allocation); suitable for large buffers |
| **Go (TinyGo)** | Conservative GC | Mark-sweep with stack scanning | Full GC with ~20 KB runtime, significant pause times |

Almide's bump allocator produces the smallest possible runtime footprint — roughly 30 WASM instructions total. This directly contributes to Almide's goal of minimal binary size.

---

## Data Representation

### Primitives

Primitives live on the WASM operand stack. They are never heap-allocated unless stored inside a composite type.

| Type | WASM type | Stack slots | Memory size (in record) | Notes |
|------|-----------|-------------|-------------------------|-------|
| Int | i64 | 1 | 8 bytes | 64-bit signed integer |
| Float | f64 | 1 | 8 bytes | IEEE 754 double |
| Bool | i32 | 1 | 4 bytes | 0 = false, 1 = true |
| Unit | — | 0 | 0 bytes | No representation |

#### Byte-Level Example: Int

```
Value: 42
WASM stack: i64 = 0x000000000000002A
In a record field at offset 8:  2A 00 00 00 00 00 00 00  (i64 LE)
```

#### Byte-Level Example: Float

```
Value: 3.14
WASM stack: f64 = 0x40091EB851EB851F
In a record field at offset 0:  1F 85 EB 51 B8 1E 09 40  (f64 LE)
```

#### Byte-Level Example: Bool

```
Value: true
WASM stack: i32 = 0x00000001
In a record field at offset 0:  01 00 00 00  (i32 LE)
```

### Heap Objects

All heap objects are referenced by i32 pointers into memory 0. The pointer value is the byte offset from address 0.

#### String

Layout: `[len:i32][utf8_bytes...]`

- `len` is the **byte** length of the UTF-8 data (not character count).
- Total memory: `4 + len` bytes.
- `string.len` in Almide returns the **character count** (number of Unicode code points), which requires a UTF-8 scan at runtime.

```
"Hello" at ptr = 0x0100:

Offset  Hex                        Decoded
0x0100  05 00 00 00                len = 5 (i32 LE)
0x0104  48 65 6C 6C 6F            'H' 'e' 'l' 'l' 'o'

Access pattern:
  byte_len  = i32.load(ptr)           → 5
  first_byte = i32.load8_u(ptr + 4)   → 0x48 ('H')
```

A multibyte example — `"日本"`:

```
"日本" at ptr = 0x0200:

Offset  Hex                        Decoded
0x0200  06 00 00 00                len = 6 (byte length, not char count)
0x0204  E6 97 A5                   '日' (U+65E5, 3 bytes UTF-8)
0x0207  E6 9C AC                   '本' (U+672C, 3 bytes UTF-8)

string.len("日本") = 2  (character count, computed at runtime)
```

#### List[T]

Layout: `[len:i32][elem0][elem1]...`

- `len` is the number of elements.
- Each element occupies `byte_size(T)` bytes (8 for Int/Float, 4 for pointers/Bool).
- Total memory: `4 + len × elem_size` bytes.

```
[10, 20, 30]: List[Int] at ptr = 0x0300:

Offset  Hex                                          Decoded
0x0300  03 00 00 00                                  len = 3
0x0304  0A 00 00 00 00 00 00 00                      elem[0] = 10 (i64 LE)
0x030C  14 00 00 00 00 00 00 00                      elem[1] = 20 (i64 LE)
0x0314  1E 00 00 00 00 00 00 00                      elem[2] = 30 (i64 LE)

Total: 4 + 3 × 8 = 28 bytes

Access pattern:
  count    = i32.load(ptr)                             → 3
  elem[i]  = i64.load(ptr + 4 + i × 8)                → value
```

```
["ab", "cd"]: List[String] at ptr = 0x0400:

Offset  Hex              Decoded
0x0400  02 00 00 00      len = 2
0x0404  XX XX XX XX      elem[0] = pointer to "ab" string (i32)
0x0408  YY YY YY YY      elem[1] = pointer to "cd" string (i32)

Total: 4 + 2 × 4 = 12 bytes

Access pattern:
  count    = i32.load(ptr)                             → 2
  elem[i]  = i32.load(ptr + 4 + i × 4)                → string pointer
```

Size calculation formula:
```
list_byte_size = 4 + len × elem_size
elem_size = byte_size(T)    // 8 for Int/Float, 4 for String/Bool/pointers
```

#### Record

Layout: `[field0][field1]...` — fields stored sequentially in definition order.

No length prefix. No padding between fields. The compiler knows the exact layout from the type definition at compile time.

```
type Point = { x: Int, y: Int, label: String }

Point { x: 100, y: 200, label: "A" } at ptr = 0x0500:

Offset  Hex                                Decoded
0x0500  64 00 00 00 00 00 00 00            x = 100 (i64 LE, 8 bytes)
0x0508  C8 00 00 00 00 00 00 00            y = 200 (i64 LE, 8 bytes)
0x0510  XX XX XX XX                        label = pointer to "A" (i32, 4 bytes)

Total: 8 + 8 + 4 = 20 bytes

Access pattern (reading field "y"):
  field_offset("y") = 8                              (skip x: 8 bytes)
  value = i64.load(ptr + 8)                          → 200

Access pattern (reading field "label"):
  field_offset("label") = 16                         (skip x: 8, y: 8)
  str_ptr = i32.load(ptr + 16)                       → pointer to "A"
```

Size calculation formula:
```
record_byte_size = sum(byte_size(field_ty) for each field)
field_offset(N) = sum(byte_size(field_ty) for fields 0..N-1)
```

Fields are laid out in **type definition order**, not in construction order. This is critical: `Point { y: 200, x: 100, label: "A" }` produces the same memory layout as the example above. The compiler reorders fields during emission.

#### Variant

Layout: `[tag:i32][payload...]`

- `tag` is a 0-based index identifying the variant case.
- Payload size is the **maximum** across all cases (so all cases have equal allocation size, enabling `mem_eq` comparison).

```
type Shape =
  | Circle(radius: Float)
  | Rect(w: Float, h: Float)

Circle(radius: 2.5) at ptr = 0x0600:

Offset  Hex                                          Decoded
0x0600  00 00 00 00                                  tag = 0 (Circle)
0x0604  00 00 00 00 00 00 04 40                      radius = 2.5 (f64 LE)
0x060C  00 00 00 00 00 00 00 00                      padding (unused, fills to max case size)

Rect(w: 3.0, h: 4.0) at ptr = 0x0620:

Offset  Hex                                          Decoded
0x0620  01 00 00 00                                  tag = 1 (Rect)
0x0624  00 00 00 00 00 00 08 40                      w = 3.0 (f64 LE)
0x062C  00 00 00 00 00 00 10 40                      h = 4.0 (f64 LE)

Max payload: Rect has 16 bytes (two f64). Circle has 8 bytes.
Allocation: 4 (tag) + 16 (max payload) = 20 bytes for both.
```

Size calculation formula:
```
variant_alloc_size = 4 + max(payload_size(case) for each case)
payload_size(case) = sum(byte_size(field_ty) for each field in case)
```

#### Tuple

Layout: `[elem0][elem1]...` — identical to a record but with positional (unnamed) fields.

```
(42, "hello", true) at ptr = 0x0700:

Offset  Hex                                Decoded
0x0700  2A 00 00 00 00 00 00 00            elem[0] = 42 (i64 LE, 8 bytes)
0x0708  XX XX XX XX                        elem[1] = pointer to "hello" (i32, 4 bytes)
0x070C  01 00 00 00                        elem[2] = true (i32, 4 bytes)

Total: 8 + 4 + 4 = 16 bytes
```

#### Option[T]

Representation depends on T:
- Heap types (String, List, Record, etc.): `0` = None, nonzero pointer = Some(value). No wrapper allocation.
- Value types (Int, Float): `[tag:i32][value]` like a variant. tag 0 = None, tag 1 = Some.

```
Option[String]:
  None  → i32 value 0x00000000 (null pointer)
  Some("hi") → i32 value 0x00000XXX (pointer to "hi" string)

Option[Int]:
  None at ptr:
    0x0000  00 00 00 00                    tag = 0 (None)
    0x0004  00 00 00 00 00 00 00 00        padding (8 bytes)

  Some(42) at ptr:
    0x0000  01 00 00 00                    tag = 1 (Some)
    0x0004  2A 00 00 00 00 00 00 00        value = 42 (i64 LE)
```

#### Result[T, E]

Layout: `[tag:i32][value]` — tag 0 = Ok, tag 1 = Err. Value region is `max(byte_size(T), byte_size(E))`.

```
Result[Int, String]:
  Ok(42) at ptr:
    0x0000  00 00 00 00                    tag = 0 (Ok)
    0x0004  2A 00 00 00 00 00 00 00        value = 42 (i64 LE)

  Err("fail") at ptr:
    0x0000  01 00 00 00                    tag = 1 (Err)
    0x0004  XX XX XX XX                    value = pointer to "fail" (i32)
    0x0008  00 00 00 00                    padding to match Int size

Size: 4 + max(8, 4) = 12 bytes
```

#### Closure

Layout: `[table_idx:i32][env_ptr:i32]` — always 8 bytes.

- `table_idx` is the index into the WASM function table (for `call_indirect`).
- `env_ptr` is a pointer to the captured environment (or 0 if no captures).

```
Closure at ptr = 0x0800:

Offset  Hex              Decoded
0x0800  05 00 00 00      table_idx = 5 (function table index)
0x0804  A0 08 00 00      env_ptr = 0x08A0 (pointer to capture environment)
```

Calling convention: `call_indirect(env_ptr, arg0, arg1, ...) -> ret`. The env_ptr is always the first argument.

#### Closure Environment

Layout: `[cap0:8bytes][cap1:8bytes]...` — all captures padded to 8 bytes regardless of actual type.

This uniform 8-byte slot size avoids alignment issues and simplifies offset computation. A captured Bool (4 bytes) wastes 4 bytes of padding, but closure environments are typically small.

```
Environment capturing x: Int = 42 and name: String = ptr(0x31):

Offset  Hex                                Decoded
0x08A0  2A 00 00 00 00 00 00 00            cap[0] = 42 (i64 LE, Int)
0x08A8  31 00 00 00 00 00 00 00            cap[1] = 0x31 (i32 ptr, zero-extended to 8 bytes)

Access pattern:
  cap[N] = load(env_ptr + N × 8)
  For i64 captures: i64.load(env_ptr + N × 8)
  For i32 captures: i32.load(env_ptr + N × 8)   // upper 4 bytes ignored
```

### String Interning

**Compile time:** All string literals are interned into the data segment of memory 0. The compiler maintains a `HashMap<String, u32>` mapping string content to its memory offset. Identical string literals produce identical pointers — no duplication.

**Runtime:** Strings created at runtime (concatenation, interpolation, slicing, int_to_string, etc.) are allocated on the heap. There is no runtime interning. Two runtime strings with identical content will occupy separate heap memory and have different pointers. Equality comparison uses byte-by-byte comparison (via `__mem_eq`), not pointer comparison.

---

## Scratch Buffer Protocol

The scratch buffer protocol eliminates intermediate allocations during string interpolation. For an interpolation with N parts (where N >= 2), the naive approach would allocate N-1 intermediate strings. The scratch buffer reduces this to exactly one allocation.

### Complete Walkthrough

Source code:

```almide
let name = "Alice"
let age = 30
let msg = "Hello ${name}, you are ${int.to_string(age)} years old"
```

The interpolation `"Hello ${name}, you are ${int.to_string(age)} years old"` has 5 parts:
1. `"Hello "` (string literal)
2. `name` (string variable)
3. `", you are "` (string literal)
4. `int.to_string(age)` (int-to-string conversion)
5. `" years old"` (string literal)

The compiler emits:

```
// Part 1: "Hello " (literal at offset 0x31, len=6)
call __scratch_write_str(0x31)

// Part 2: name (variable holding pointer to "Alice")
call __scratch_write_str(name_ptr)

// Part 3: ", you are " (literal at offset 0x3F, len=10)
call __scratch_write_str(0x3F)

// Part 4: int.to_string(30) → allocates "30" on heap, returns ptr
call __int_to_string(30_i64)
call __scratch_write_str(result_ptr)

// Part 5: " years old" (literal at offset 0x51, len=10)
call __scratch_write_str(0x51)

// Finalize: allocate final string on heap, copy from scratch
call __scratch_finalize() → returns final_ptr
```

### Memory 1 Contents at Each Step

```
After step 1: __scratch_write_str("Hello ")
  Memory 1:  48 65 6C 6C 6F 20                                "Hello "
  scratch_ptr = 6

After step 2: __scratch_write_str("Alice")
  Memory 1:  48 65 6C 6C 6F 20 41 6C 69 63 65                 "Hello Alice"
  scratch_ptr = 11

After step 3: __scratch_write_str(", you are ")
  Memory 1:  ...41 6C 69 63 65 2C 20 79 6F 75 20 61 72 65 20  "Hello Alice, you are "
  scratch_ptr = 21

After step 4: __scratch_write_str("30")
  Memory 1:  ...61 72 65 20 33 30                              "Hello Alice, you are 30"
  scratch_ptr = 23

After step 5: __scratch_write_str(" years old")
  Memory 1:  ...33 30 20 79 65 61 72 73 20 6F 6C 64           "...30 years old"
  scratch_ptr = 33
```

### The __scratch_write_str Algorithm

```
function __scratch_write_str(str_ptr: i32):
    len = i32.load(str_ptr)                    // read byte length from memory 0

    // Ensure memory 1 has capacity
    if scratch_ptr + len > memory.size(1) * 65536:
        pages_needed = (scratch_ptr + len - memory.size(1) * 65536) / 65536 + 1
        memory.grow(1, pages_needed)

    // Byte-copy loop: mem1[scratch_ptr + i] = mem0[str_ptr + 4 + i]
    for i in 0..len:
        i32.store8(mem1, scratch_ptr + i, i32.load8_u(mem0, str_ptr + 4 + i))

    scratch_ptr += len
```

Note: this is a byte-by-byte copy loop. Future optimization could use `memory.copy` between memories, but the current implementation uses a manual loop because `memory.copy` within the same memory index is more commonly supported than cross-memory copy in the write direction.

### The __scratch_finalize Algorithm

```
function __scratch_finalize() -> i32:
    total_len = scratch_ptr                    // all bytes written so far

    if total_len == 0:
        return intern_ptr("")                  // return the interned empty string

    // Allocate on memory 0: [len:i32][data...]
    result = __alloc(4 + total_len)

    // Write length prefix
    i32.store(result, total_len)

    // Copy all scratch bytes from memory 1 to memory 0
    memory.copy(dst=result+4 in mem0, src=0 in mem1, len=total_len)

    // Reset scratch pointer for next interpolation
    scratch_ptr = 0

    return result
```

### Final State After __scratch_finalize

```
Memory 0 (heap):
  ptr = 0x0048 (or wherever heap_ptr was)
  0x0048  21 00 00 00                                  len = 33 (i32 LE)
  0x004C  48 65 6C 6C 6F 20 41 6C 69 63 65 2C 20 79   "Hello Alice, y"
  0x005A  6F 75 20 61 72 65 20 33 30 20 79 65 61 72   "ou are 30 year"
  0x0068  73 20 6F 6C 64                               "s old"

  heap_ptr = 0x0048 + 4 + 33 = 0x006D (then aligned to 0x0070 on next alloc)

Memory 1:
  scratch_ptr = 0   (reset, ready for next interpolation)
  Data still physically present but logically dead — will be overwritten
```

Total heap allocations for this interpolation: **1** (the final 37-byte string). The `int.to_string(30)` also allocates one string on the heap (6 bytes), but that is inherent to the conversion, not the interpolation protocol.

### Memory 1 Growth Behavior

Memory 1 starts at 1 page (64 KB). This is sufficient for most string interpolations. If a single interpolation accumulates more than 64 KB of scratch data (e.g., joining thousands of strings), `__scratch_write_str` grows memory 1 on demand.

Growth is computed as: `(bytes_needed - current_capacity) / 65536 + 1` pages. Memory 1 never shrinks — once grown, the pages remain available for subsequent interpolations. Since `scratch_ptr` resets to 0 after each `__scratch_finalize`, the same scratch space is reused across interpolations.

---

## Memory Safety Without GC

The bump allocator's simplicity provides strong safety guarantees that are difficult to achieve with more complex allocators.

### No Use-After-Free

Use-after-free occurs when a program frees memory and later accesses it through a stale pointer. Since `__alloc` has no corresponding `__free`, this class of bug is **impossible by construction**. Every pointer returned by `__alloc` remains valid for the entire lifetime of the WASM instance.

This is not a theoretical claim — there is literally no instruction sequence in the compiled WASM that could cause a use-after-free, because the "free" operation does not exist in the instruction set.

### No Double-Free

Double-free occurs when `free` is called twice on the same pointer, corrupting allocator metadata. Since there is no `free`, double-free is also **impossible by construction**. There is no metadata to corrupt.

### No Heap Corruption

Free-list allocators maintain linked lists or bitmaps in the heap itself. A buffer overflow can corrupt this metadata, leading to arbitrary memory writes on subsequent allocations. The bump allocator has exactly one piece of mutable state: `heap_ptr` (a WASM global, not stored in linear memory). A buffer overflow into heap memory cannot corrupt the allocator state because `heap_ptr` is not in linear memory.

### Deterministic Memory Behavior

The bump allocator is fully deterministic:
- Given the same sequence of `__alloc(size)` calls, the returned pointers are identical across runs.
- There is no GC that could pause at unpredictable times.
- There are no free-list ordering variations.
- Memory usage is monotonically increasing and directly proportional to allocation volume.

This determinism is valuable for debugging and for reproducible behavior in agent pipelines.

### Memory Leak Characteristics

The bump allocator "leaks" all unreachable memory. In practice:

- **Short-lived agents** (the primary Almide WASM use case): The agent runs, produces output, and exits. The host runtime (Wasmtime, Wasmer, browser) destroys the WASM instance and reclaims all linear memory. Leaked memory has zero cost.
- **Processing pipelines**: Each pipeline stage is a separate WASM instance. Memory is reclaimed between stages.
- **Bounded workloads**: An agent processing a 1 MB input file will allocate at most O(input_size) memory. The 4 MB initial memory is sufficient for most workloads; growth to 10-20 MB is typical for large inputs.

The only scenario where leaking is problematic is a long-running server that processes unbounded requests within a single WASM instance. Almide does not target this use case — it is designed for agents, not servers.

### Memory Bounds Checking

WASM provides hardware-enforced bounds checking on every memory access. An out-of-bounds read or write traps immediately. This means:
- Buffer overflows cannot escape the linear memory sandbox.
- A buggy program cannot access host memory.
- Stack smashing is impossible (the WASM operand stack is separate from linear memory).

The bump allocator inherits these guarantees from the WASM execution model. Combined with the no-free invariant, the result is a memory model that is both simple and safe.
