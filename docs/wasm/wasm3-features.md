# WASM 3.0 Features

Almide emits WASM 3.0 binaries. No 2.0 fallback.

Three post-MVP proposals are used unconditionally: **tail calls**, **multi-memory**, and (deferred) **exception handling**. Each eliminates a class of overhead that would otherwise require workarounds in the compiler or runtime.

## Tail Calls

All tail-position calls emit `return_call` / `return_call_indirect`.

**Not just self-recursion.** Any call in tail position — mutual recursion, higher-order calls, cross-function tail calls — all use native WASM tail calls. Loop-based TCO (`TailCallOptPass`) is excluded from the WASM pipeline entirely.

### Implementation

- `TailCallMarkPass` (nanopass): walks function bodies, converts `Call` → `TailCall` at tail positions
- Tail positions: direct return, last expr of Block, both If branches, all Match arm bodies
- `TailCall` IR node: same as `Call` but emitter outputs `return_call`
- Pipeline position: **last pass** (after all other passes that may create calls)

The WASM pipeline (`target.rs`) runs `TailCallMarkPass` as the final step, after closure conversion and all other transformations:

```
ListPatternLowering → LICM → EffectInference → ResultPropagation
  → Peephole → ClosureConversion → FanLowering → TailCallMark
```

This ordering is critical. Closure conversion lifts lambdas into top-level functions and rewrites computed calls. If `TailCallMark` ran before closure conversion, it would mark calls that no longer exist in the final IR.

### What IS a tail position

```
fn foo(n) =
  if n == 0 then bar()      // tail — If branch
  else baz(n - 1)            // tail — If branch

fn qux(x) = {
  let y = compute(x)         // not tail — Block statement
  transform(y)               // tail — Block tail expr
}

fn corge(x) = match x {
  0 => alpha()               // tail — Match arm body
  _ => beta(x)               // tail — Match arm body
}
```

### What is NOT a tail position

- `ResultOk { Call(...) }` — the Ok wrapper changes the return type
- `Try { Call(...) }` — Try unwraps the Result, type mismatch with return_call
- Call inside a statement (not the tail expression)
- Call whose result is used by another expression

### Before vs After: Loop-Based TCO vs Native Tail Calls

For self-recursive functions, the Rust target uses `TailCallOptPass` which transforms recursion into a `while` loop. The WASM target skips this entirely and uses native `return_call`.

**Example Almide source:**
```almide
fn sum_to(n: Int, acc: Int) -> Int =
  if n <= 0 then acc else sum_to(n - 1, acc + n)
```

**Loop-based TCO (Rust target) — IR after TailCallOptPass:**
```
fn sum_to(n: Int, acc: Int) -> Int {
  var __tco_result: Int = 0
  while true {
    if n <= 0 {
      __tco_result = acc
      break
    } else {
      let __tco_tmp_n = n - 1
      let __tco_tmp_acc = acc + n
      n = __tco_tmp_n        // param reassignment
      acc = __tco_tmp_acc    // param reassignment
      continue
    }
  }
  __tco_result
}
```

**Native tail call (WASM target) — IR after TailCallMarkPass:**
```
fn sum_to(n: Int, acc: Int) -> Int =
  if n <= 0 then acc else TailCall(sum_to, [n - 1, acc + n])
```

**Emitted WASM (from `emit_tail_call`):**
```wasm
(func $sum_to (param $n i64) (param $acc i64) (result i64)
  local.get $n
  i64.const 0
  i64.le_s
  if (result i64)
    local.get $acc
  else
    local.get $n
    i64.const 1
    i64.sub
    local.get $acc
    local.get $n
    i64.add
    return_call $sum_to      ;; no stack growth, no loop scaffolding
  end
)
```

### Why Not Just Loop-Based TCO?

Loop-based TCO works and is used for the Rust target. But it has fundamental limitations that native tail calls eliminate:

**1. Self-recursion only.** Loop-based TCO can only optimize calls where the function calls itself. Mutual recursion (A calls B, B calls A) cannot be expressed as a single loop. With `return_call`, mutual recursion is zero-cost:

```almide
fn is_even(n: Int) -> Bool =
  if n == 0 then true else is_odd(n - 1)

fn is_odd(n: Int) -> Bool =
  if n == 0 then false else is_even(n - 1)
```

Loop-based TCO: stack overflow at depth ~10,000 (WASM default stack ~1MB).
Native tail calls: runs to any depth, zero stack growth.

**2. Higher-order tail calls.** After closure conversion, a tail-position call through a function variable becomes `return_call_indirect`. Loop-based TCO cannot handle this at all:

```almide
fn apply_n(f: (Int) -> Int, n: Int, x: Int) -> Int =
  if n == 0 then x else apply_n(f, n - 1, f(x))
```

The self-call `apply_n(...)` could be loop-optimized, but `f(x)` in tail position of a different function cannot.

**3. Binary size.** Loop-based TCO introduces per-function overhead:

| Overhead per function | Loop-based TCO | Native `return_call` |
|---|---|---|
| Result variable | `var __tco_result` + default init | None |
| Temporary variables | 1 per parameter (`__tco_tmp_*`) | None |
| Control flow | `while true { ... break/continue }` | Single instruction |
| Parameter mutations | N assignments per recursive call | None |
| Return type constraint | Must be default-initializable | None |

For a function with 3 parameters, loop-based TCO adds 4 locals, a while loop, break, continue, and 6 assignment statements. Native `return_call` replaces all of this with a single WASM instruction.

**4. Return type restriction.** Loop-based TCO requires a default-initializable return type (Int, String, Bool, List, Option, etc.) because it must declare `var __tco_result = <default>`. Functions returning user-defined records or variants cannot use loop-based TCO. Native tail calls have no such restriction.

### Edge Cases and Gotchas

**Effect functions.** An `effect fn` returns `Result[T, String]`. If the function body ends with `ok(recursive_call(...))`, the `ResultOk` wrapper prevents tail-call marking — the call is wrapped in `Ok(...)`, which changes the return type. The recursive call is `T`, but the function returns `Result[T, String]`. `TailCallMarkPass` correctly rejects these:

```almide
effect fn count_lines(path: String) -> Int =
  ok(count_lines(next_path))  // NOT a tail call: Ok wrapper changes type
```

For the Rust target, `TailCallOptPass` handles this case specially: it strips `ResultOk` in tail position and wraps the final `__tco_result` in `Ok(...)` outside the loop. But for WASM, this cannot be expressed with `return_call` — the Ok allocation must happen after the call returns.

**Closures after closure conversion.** `ClosureConversionPass` lifts lambdas into top-level functions with an explicit `env_ptr` parameter. A tail call to a closure variable becomes `return_call_indirect` through the function table. The emitter handles this in `emit_tail_call`:

```rust
// From crates/almide-codegen/src/emit_wasm/calls.rs
CallTarget::Computed { callee } => {
    // Load closure [table_idx, env_ptr]
    // Push env_ptr as first arg, then user args, then table_idx
    // return_call_indirect(type_idx, table=0)
}
```

The scratch local holding the closure pointer must be freed even though `return_call_indirect` never returns. The current implementation frees it after the instruction (which is dead code but satisfies the `ScratchAllocator` leak assertion during compilation).

**Builtin/stdlib fallback.** Not all calls resolve to user-defined WASM functions. Builtin runtime functions (like `__alloc`, `__concat_str`, `__scratch_finalize`) are registered in `func_map` and do get `return_call` when in tail position. But if a `CallTarget::Named` function is not found in `func_map`, the emitter falls back to a normal `call` — these are typically stubs or imported functions where `return_call` may not apply.

### Verification

To confirm that tail calls are being emitted correctly in a compiled WASM binary:

```bash
# Build a WASM binary
almide build app.almd --target wasm -o app.wasm

# Dump and search for return_call instructions
wasm-tools dump app.wasm | grep return_call

# Expected output (one per tail-position call):
#   0x0142 | 12 03       | return_call 3
#   0x01a8 | 13 05 00    | return_call_indirect type=5 table=0

# Count total tail calls vs normal calls
wasm-tools dump app.wasm | grep -c "return_call"
wasm-tools dump app.wasm | grep -c "call " | head -1
```

To verify that a specific function uses tail calls, find its function index and inspect:

```bash
# Full disassembly of a specific function
wasm-tools print app.wasm | grep -A 50 "(func \$sum_to"
```

A correctly compiled self-recursive function should have zero `call` instructions to itself — only `return_call`.

### Runtime Support

Universal: Chrome 112, Firefox 121, Safari 18.2, Wasmtime 22, Wasmer 7.1.

All major WASI runtimes (Spin, wasmCloud, Docker+WASM) support tail calls by default. No feature flag needed.

---

## Multi-Memory

Two memories in every WASM binary.

| Memory | Index | Initial Size | Growable | Purpose |
|--------|-------|-------------|----------|---------|
| Main | 0 | 64 pages (4MB) | Yes, unbounded | Data segment + heap allocations |
| Scratch | 1 | 1 page (64KB) | Yes, unbounded | String builder temporary buffer |

### Memory 0 Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ Memory 0 (main)                                                      │
│                                                                      │
│  0x00  ┌─────────────────┐                                          │
│        │  Reserved (48B)  │  SCRATCH_ITOA (16), padding, newline     │
│  0x30  ├─────────────────┤  NEWLINE_OFFSET = 48                     │
│        │  Data Segment    │  Interned string literals [len:i32][data]│
│        │  (variable size) │  Deduplicated, laid out sequentially     │
│        ├─────────────────┤  heap_start = align8(NEWLINE_OFFSET+data) │
│        │                  │                                          │
│        │  Heap            │  Bump-allocated: strings, lists,         │
│        │  (grows upward)  │  records, variants, closures, Results    │
│        │  ↓               │                                          │
│        │                  │                                          │
│  4MB   └─────────────────┘  Initial: 64 pages, grows on demand      │
└──────────────────────────────────────────────────────────────────────┘
```

The heap pointer is stored in **Global 0** and initialized to the first 8-byte-aligned address after the data segment. Every `alloc(size)` call bumps this pointer and returns the previous value (bump allocator, no free).

### Memory 1 Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ Memory 1 (scratch)                                                   │
│                                                                      │
│  0x00  ┌─────────────────┐                                          │
│        │  Scratch buffer  │  Written by scratch_write_str            │
│        │  (raw bytes,     │  Read by scratch_finalize                │
│        │   no headers)    │  Reset to 0 after each finalize          │
│        │  ↓               │                                          │
│  64KB  └─────────────────┘  Initial: 1 page, grows on demand        │
└──────────────────────────────────────────────────────────────────────┘
```

The scratch pointer is stored in **Global 1** and starts at 0. Unlike the main heap, it resets to 0 after every `scratch_finalize()` — the buffer is purely transient.

### String Interpolation Optimization

Before (single memory):
```
"${a} is ${b}" → concat(a, concat(" is ", b))
                  ↳ 2 intermediate heap allocations, each copying bytes
```

After (multi-memory):
```
"${a} is ${b}" → scratch_write(a) → scratch_write(" is ") → scratch_write(b)
                  → scratch_finalize() → 1 final heap allocation + memory.copy(1→0)
```

N-part interpolation: N-1 intermediate allocations → 0.

### Scratch Buffer Lifecycle: Step-by-Step

Consider the expression `"Hello, ${name}! You are ${age} years old."` where `name = "Alice"` (5 bytes) and `age` has been converted to the string `"30"` (2 bytes).

**Step 0: Initial state**
```
Global 1 (scratch_ptr) = 0
Memory 1: [empty]
```

**Step 1: `scratch_write_str(ptr_to_"Hello, ")`**

The runtime reads `mem0[ptr]` to get the byte length (7), then byte-copies the string data from `mem0[ptr+4..ptr+11]` to `mem1[0..7]`:

```
Memory 1:  H  e  l  l  o  ,  ·
offset:    0  1  2  3  4  5  6
Global 1 (scratch_ptr) = 7
```

Before copying, the runtime checks if `scratch_ptr + len > memory.size(1) * 65536`. If so, it grows memory 1 by `ceil((needed - current) / 65536) + 1` pages.

**Step 2: `scratch_write_str(ptr_to_"Alice")`**

```
Memory 1:  H  e  l  l  o  ,  ·  A  l  i  c  e
offset:    0  1  2  3  4  5  6  7  8  9 10 11
Global 1 (scratch_ptr) = 12
```

**Step 3: `scratch_write_str(ptr_to_"! You are ")`**

```
Memory 1:  H  e  l  l  o  ,  ·  A  l  i  c  e  !  ·  Y  o  u  ·  a  r  e  ·
offset:    0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21
Global 1 (scratch_ptr) = 22
```

**Step 4: `scratch_write_str(ptr_to_"30")`**

```
Memory 1:  ...  e  ·  3  0
offset:         20 21 22 23
Global 1 (scratch_ptr) = 24
```

**Step 5: `scratch_write_str(ptr_to_" years old.")`**

```
Global 1 (scratch_ptr) = 35
```

**Step 6: `scratch_finalize()`**

1. Read `total_len = scratch_ptr` → 35
2. Check `total_len == 0` → no (skip empty-string fast path)
3. Allocate on memory 0 heap: `alloc(4 + 35)` → returns `result_ptr`
4. Write length prefix: `mem0[result_ptr] = 35` (i32 little-endian)
5. Bulk copy: `memory.copy(dst=mem0, src=mem1)` — copies `mem1[0..35]` to `mem0[result_ptr+4..result_ptr+39]`
6. Reset: `scratch_ptr = 0`
7. Return `result_ptr`

```
Memory 0 (heap):  [35, 0, 0, 0] H e l l o , · A l i c e ! · Y o u · a r e · 3 0 · y e a r s · o l d .
                   ↑ len prefix   ↑ string data (35 bytes)
```

The key insight: 5 string parts, but only 1 heap allocation. Without multi-memory, this would require 4 intermediate `concat_str` calls, each allocating a progressively larger intermediate string on the heap.

### WASM Instruction Sequence

The compiled WASM for `"Hello, ${name}!"` looks like:

```wasm
;; Part 1: literal "Hello, "
i32.const 0x0064        ;; interned string ptr in data segment
call $__scratch_write_str

;; Part 2: expression ${name}
local.get $name         ;; already a string ptr
call $__scratch_write_str

;; Part 3: literal "!"
i32.const 0x0080        ;; interned string ptr in data segment
call $__scratch_write_str

;; Finalize: copy scratch → heap, return heap ptr
call $__scratch_finalize
;; Stack: [i32] — pointer to the final concatenated string
```

### Why Not Separate Heap Memories?

An alternative design would use separate memories for different data types — a string pool in memory 1, closure environments in memory 2, etc. This has a fundamental problem: **cross-memory pointers**.

A record field of type `String` stores an i32 pointer into memory 0's heap. If strings lived in memory 2, every record would need to know which memory its pointer targets. WASM `i32.load` takes a static memory index — you cannot dynamically dispatch `load(mem_idx, offset)`. This means:

- Every pointer dereference would need a static memory index annotation
- The type system would need to track which memory each pointer targets
- Passing a string to a function that also accepts list elements (both i32 pointers) would require memory-polymorphic functions
- `memory.copy` can only copy between two specific memories, not "whatever memory this pointer points to"

The scratch buffer avoids this problem entirely because it is **never referenced by pointer**. No i32 in any record, list, or closure ever points into memory 1. Data flows one way: into memory 1 (via `scratch_write_str`), then out to memory 0 (via `scratch_finalize`'s `memory.copy`), then the scratch pointer resets. Memory 1 is an accumulation buffer, not a heap.

### Memory Growth Strategy

**Memory 0 (main heap):** Starts at 64 pages (4MB). The `alloc(size)` runtime function checks if `heap_ptr + size` exceeds `memory.size(0) * 65536`. If so, it grows by `ceil((needed - available) / 65536) + 1` pages. Growth is unbounded — `maximum` is `None` in the MemoryType declaration.

**Memory 1 (scratch):** Starts at 1 page (64KB). Growth is checked at each `scratch_write_str` call before the byte-copy loop. The check:

```wasm
global.get $scratch_ptr
local.get $len
i32.add                     ;; needed = scratch_ptr + len
memory.size 1
i32.const 65536
i32.mul                     ;; available = pages * 64KB
i32.gt_u                    ;; needed > available?
if
  ;; grow by (needed - available) / 65536 + 1 pages
  ...
  memory.grow 1
  drop                      ;; ignore return value (-1 on failure)
end
```

In practice, memory 1 rarely grows beyond 1 page. String interpolation results are typically under 64KB. Even a 10,000-character interpolation result (unusual) fits in a single page. Growth is most likely in hot loops that build large strings incrementally, but `scratch_finalize` resets the pointer each time so the buffer does not accumulate across calls.

### Future Expansion Possibilities

Multi-memory opens the door to further optimizations that could be added without changing the existing two-memory contract:

- **String deduplication cache (memory 2).** A hash table in a dedicated memory for runtime string deduplication. Currently, only compile-time string literals are deduplicated (via `intern_string`). Runtime-constructed strings always allocate fresh. A dedup cache could reduce heap pressure for programs that produce many identical computed strings.

- **Stack-allocated temporaries (memory 2).** Short-lived records and tuples that do not escape the current function could be allocated on a dedicated scratch memory with a frame pointer that resets on function return, avoiding heap allocation entirely.

- **Large-object memory (memory 2).** Lists and maps above a size threshold could be allocated in a separate memory to reduce fragmentation in the main heap. This would require the list/map runtime to parameterize on memory index.

None of these are planned — they are mentioned only to show that the multi-memory foundation supports them without rearchitecting.

### Macro Infrastructure

All load/store macros support optional `memory_index`:
```rust
wasm!(f, { i32_store(0); });        // memory 0 (default)
wasm!(f, { i32_store8(0, 1); });    // memory 1 (explicit)
wasm!(f, { memory_copy(1, 0); });   // src=mem1, dst=mem0
wasm!(f, { memory_grow(1); });      // grow memory 1
wasm!(f, { memory_size(1); });      // query memory 1 page count
```

The `wasm!` macro (defined in `emit_wasm/wasm_macro.rs`) dispatches to `wasm_encoder::Instruction` variants. Memory-indexed instructions pass the index to the encoder, which embeds it in the binary. The default memory index (0) matches the WASM MVP behavior, so existing single-memory code is unchanged.

### Runtime Support

Chrome 120, Firefox 125, Wasmtime 15 (default ON), WasmEdge (default ON). Safari missing — not a concern for server-side containers.

---

## Exception Handling (deferred)

`try_table` / `throw` / `exnref` for zero-cost effect fn error propagation. wasm-encoder 0.225 supports it. Blocked on Wasmtime default-OFF. See [on-hold/wasm-exception-handling.md](../roadmap/on-hold/wasm-exception-handling.md).

### Current Approach: Result Chain

Today, Almide's `effect fn` compiles to a function that returns `Result[T, String]`. Every fallible call in the body gets a `Try { expr }` wrapper (inserted by `ResultPropagationPass`), which compiles to a tag-check-and-early-return sequence.

**Almide source:**
```almide
effect fn process(path: String) -> String = {
  let content = fs.read_text(path)
  let parsed = json.parse(content)
  json.get_str(parsed, "name")
}
```

**Current WASM compilation (Result chain):**

Each `effect fn` call returns a heap-allocated Result: `[tag:i32][payload]`. The `Try` node checks the tag and returns early on error:

```wasm
(func $process (param $path i32) (result i32)
  ;; let content = fs.read_text(path)  →  Try(fs_read_text(path))
  local.get $path
  call $__fs_read_text              ;; returns i32 (Result ptr)
  local.set $scratch0
  local.get $scratch0
  i32.load offset=0                 ;; load tag
  i32.const 0
  i32.ne
  if                                ;; tag != 0 → error
    local.get $scratch0             ;; return the error Result as-is
    return
  end
  local.get $scratch0
  i32.load offset=4                 ;; unwrap ok value (string ptr)
  local.set $content

  ;; let parsed = json.parse(content)  →  Try(json_parse(content))
  local.get $content
  call $__json_parse                ;; returns i32 (Result ptr)
  local.set $scratch0
  local.get $scratch0
  i32.load offset=0                 ;; load tag
  i32.const 0
  i32.ne
  if                                ;; tag != 0 → error
    local.get $scratch0
    return
  end
  local.get $scratch0
  i32.load offset=4
  local.set $parsed

  ;; json.get_str(parsed, "name")  →  Try(json_get_str(parsed, "name"))
  local.get $parsed
  i32.const 0x00A0                  ;; interned "name"
  call $__json_get_str              ;; returns i32 (Result ptr)
  local.set $scratch0
  local.get $scratch0
  i32.load offset=0
  i32.const 0
  i32.ne
  if
    local.get $scratch0
    return
  end
  local.get $scratch0
  i32.load offset=4
  ;; wrap final value in ok(...)
  ;; [alloc Result, tag=0, store value]
)
```

**Cost per `Try` (current):**
- 1 `local.set` + 2 `local.get` (scratch local to hold the Result ptr)
- 1 `i32.load` (read tag)
- 1 `i32.const` + 1 `i32.ne` (compare tag)
- 1 `if/end` block
- 1 conditional `return`
- 1 `i32.load` (unwrap the ok value)

Total: ~9 instructions per fallible call. For a function with N fallible calls, that is 9N instructions of error-checking overhead.

Additionally, every `ok(value)` and `err(msg)` heap-allocates a Result: `alloc(4 + payload_size)`, writes the tag, writes the payload. This means every successful call allocates a Result on the heap that is immediately unwrapped and discarded by the caller's `Try`.

### Future Approach: Native Exception Handling

With WASM exception handling, `effect fn` would return the value directly. Errors propagate via `throw`, and the caller catches with `try_table`:

```wasm
(tag $error (param i32))            ;; error payload: string ptr

(func $process (param $path i32) (result i32)
  ;; Entire function body wrapped in try_table
  ;; On $error: the i32 error payload is on the stack, rewrap and rethrow
  try_table (catch $error 0)
    local.get $path
    call $__fs_read_text            ;; returns string directly, throws on error
    local.set $content

    local.get $content
    call $__json_parse              ;; returns value directly, throws on error
    local.set $parsed

    local.get $parsed
    i32.const 0x00A0
    call $__json_get_str            ;; returns string directly, throws on error
    ;; value is on the stack — done
  end
)
```

**Eliminated per `Try`:**
- No Result heap allocation (callee returns the value directly)
- No tag check (tag is implicit in the exception tag)
- No conditional branch
- No scratch local for the Result pointer

**Eliminated per function:**
- No `ok(value)` wrapper allocation for the return value
- No `err(msg)` wrapper allocation on the error path — just `throw $error`

The savings are both in instruction count (fewer branches, fewer loads) and in heap pressure (no intermediate Result allocations). For a function with 10 fallible calls, that is ~90 instructions of overhead removed and 10 heap allocations eliminated on the happy path.

### Wasmtime Default-OFF Situation

WASM exception handling (the "exnref" proposal, not the older "legacy" proposal) reached Phase 4 in the W3C process and is implemented in all major browsers:

| Runtime | EH Support | Default |
|---------|-----------|---------|
| Chrome | 131+ | ON |
| Firefox | 130+ | ON |
| Safari | 18.2+ | ON |
| Node.js | 23+ | ON |
| Wasmtime | 27+ | **OFF** |
| Wasmer | n/a | not implemented |
| WasmEdge | n/a | not implemented |

The browser/Node story is complete. The problem is the server-side story: Wasmtime is the runtime behind Fermyon Spin, wasmCloud, Docker+WASM (via runwasi), and most production WASI deployments. With EH default-OFF in Wasmtime, any WASM binary using `throw`/`try_table` would fail to instantiate unless the user explicitly enables it via `Config::wasm_exceptions(true)`.

This creates a dual-codegen requirement: the compiler would need to emit both an EH version and a Result-chain fallback version, or detect the runtime's capabilities at load time. Neither is acceptable complexity for a feature that provides a performance improvement but not a correctness one.

**Tracking:** [bytecodealliance/wasmtime#3427](https://github.com/bytecodealliance/wasmtime/issues/3427)

**Trigger:** Implement when Wasmtime enables EH by default. At that point, all WASI container runtimes will inherit the default, and the Result chain can be replaced unconditionally — same as Almide unconditionally uses tail calls and multi-memory today.

### Intermediate Optimization: Result Layout Improvements

While waiting for native EH, the current Result chain can be optimized without any proposal dependency:

- **Unboxed Result for small types.** A `Result[Int, String]` is currently heap-allocated as `[tag:i32][i64 value]` — 12 bytes on the heap. For types that fit in two WASM values (i64 + i32 tag), the Result could be returned as a multi-value `(i32, i64)` pair, eliminating the heap allocation entirely. This is possible today with WASM multi-value returns (universally supported).

- **Error string deduplication.** Error messages from stdlib functions (e.g., "index out of bounds", "key not found") are currently allocated as fresh heap strings on every error. Interning common error strings in the data segment would eliminate these allocations.

These are independent improvements that compose with the future EH migration — when EH arrives, the transition is simpler because fewer Result allocations exist to remove.
