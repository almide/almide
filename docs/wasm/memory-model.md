# WASM Memory Model

## Physical Layout

### Memory 0 (Main)

```
Address     Content
─────────────────────────────────────
0x00-0x0F   iov struct (WASI fd_write scratch)
0x10-0x2F   int_to_string scratch (32 bytes)
0x30        Newline byte (0x0A)
0x31-N      String literal data: [len:i32 LE][utf8 bytes]... (interned, deduplicated)
N (aligned) Heap start (bump allocator, grows upward)
...         ↓ heap grows via memory.grow(0)
```

### Memory 1 (String Builder Scratch)

```
Address     Content
─────────────────────────────────────
0x00-M      Temporary bytes from scratch_write_str calls
            Reset to 0 after each scratch_finalize
```

## Allocator

Bump allocator. No free, no GC. `__alloc(size: i32) -> i32`.

- 8-byte alignment (wasi-libc convention)
- Grows memory 0 in 16-page (1MB) chunks on demand
- Global 0 = heap_ptr (memory 0)
- Global 1 = scratch_ptr (memory 1)

## Data Representation

### Primitives

| Type | WASM type | Memory size | Stack size |
|------|-----------|-------------|------------|
| Int | i64 | 8 bytes | 1 slot (i64) |
| Float | f64 | 8 bytes | 1 slot (f64) |
| Bool | i32 | 4 bytes | 1 slot (i32) |
| Unit | — | 0 bytes | 0 slots |

### Heap Objects (all i32 pointers)

| Type | Layout | Total size |
|------|--------|------------|
| String | `[len:i32][utf8 bytes...]` | 4 + len |
| List[T] | `[len:i32][elem0][elem1]...` | 4 + len × elem_size |
| Record | `[field0][field1]...` | sum of field sizes |
| Variant | `[tag:i32][payload...]` | 4 + max payload size |
| Tuple | `[elem0][elem1]...` | sum of elem sizes |
| Option[T] | ptr (0 = None, nonzero = Some) | pointer or elem |
| Result[T,E] | `[tag:i32][value]` (0=Ok, 1=Err) | 4 + max(T, E) size |
| Closure | `[table_idx:i32][env_ptr:i32]` | 8 |
| Closure env | `[cap0:8bytes][cap1:8bytes]...` | captures × 8 |

### String Interning

String literals are interned at compile time into the data segment (memory 0). Same string → same offset. Deduplicated.

Runtime-created strings (concat, interp, slice) are allocated on the heap. No interning at runtime.

## Scratch Buffer Protocol

For string interpolation with N ≥ 2 parts:

```
1. For each part:
   - Convert to string (int_to_string, float_to_string, or identity)
   - Call __scratch_write_str(str_ptr)
     → Copies string bytes to memory 1 at scratch_ptr
     → Advances scratch_ptr
     → Grows memory 1 if needed

2. Call __scratch_finalize()
   → total_len = scratch_ptr
   → __alloc(4 + total_len) on memory 0
   → Write length prefix
   → memory.copy(src=mem1, dst=mem0)
   → Reset scratch_ptr = 0
   → Return memory 0 pointer
```

Zero intermediate allocations. One final allocation for the complete string.
