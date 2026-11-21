# ADR-0037: A native List is a shared block with a count, copied only when written while shared

- **Status**: Accepted (2026-11-20). The owner ruled ○ on the direction and on Q1 and Q2; see [Owner decisions](#owner-decisions-2026-11-20).
- **Date**: 2026-11-20
- **Scope**: how the native v1 renderer spells `List[T]` (and `Bytes`), the runtime list operations in `runtime/rs/src/list.rs`, and the thread-share marking of values that cross into a `fan` arm or a callback.
- **Related**:
  - ADR-0013 (`docs/adr/0013-string-shared-immutable-buffer.md`): `AlmideStr`, the shared string block this ADR copies for lists.
  - ADR-0034: a slot credit moves under a runtime uniqueness test. That is the wasm leg's copy-on-write judge, which already does for lists what this ADR asks of native.
  - Issues #4776 (`bigint.max` / `min` copy the BigInt they return on native) and #4781 (`deque.reverse` copies both lists of a shared input on native). The design issue that tracks this ADR links both.

## Context

On the wasm leg every heap block, lists included, carries a reference count (`[rc, len, cap]`, `crates/almide-layout/src/lib.rs`). Handing a list to another holder adds one to the count, and a write to a block whose count is above 1 copies it first (`$cow`, ADR-0034).

On native, a list is a `Vec<T>`, owned by exactly one holder. Native shares nothing. When a value is still needed after it is handed on, the renderer copies it in full:

- a `Dup` that is not at its last use becomes `.clone()` (`render_native_move.rs`);
- an if-value join becomes `.clone()` or `.to_vec()` (`render_native_b_tail.rs`);
- a record field built from a borrowed list becomes `.to_vec()` (`render_native_addr.rs` `owned_from`);
- a closure handoff becomes `ToOwned::to_owned` (`render_native_closure.rs`).

Each copy costs O(n) where the wasm leg pays O(1). The stdlib re-inspection measured two cases:

- `bigint.max(a, b)` returns one of its arguments. The call copies it on native (≈5.3 KB per call at k=1024) and moves no bytes on wasm (#4776).
- `deque.reverse(d)` only swaps two fields. Native copies both lists when `d` is still live: 8 B per element, or a refcount walk for String elements (#4781).

Both stay within their declared order, so no gate fails. But the class split — one leg doing O(n) work the other shows is unnecessary — is exactly what the inspection ruling files as a defect. It recurs wherever a function returns or re-packages a borrowed list: `min`, `clamp`, record updates, `option.unwrap_or` on a list, and so on. Fixing each site with a special case would be a name-keyed patch. The cause is the representation.

## Decision

### D1 — one shared block per list

A native `List[T]` is `AlmideList<T>`: one heap block `[rc, len, cap | elements]`, laid out like `AlmideStr`.

- `Clone` adds one to `rc`.
- `Drop` takes one away. At zero it drops the elements and frees the block.
- `rc == 0` marks an immortal static block (the empty list, constant literals) that is never written or freed.

`Bytes` becomes `AlmideList<u8>` (Q1).

### D2 — writes go through `unique()`

Every mutating operation (`push`, `set`, `pop`, `insert`, `sort` in place, the `mut` parameter write-back) first calls `unique()`. When `rc > 1`, `unique()` copies the block (cloning each element, so String elements gain a count rather than being copied) and points this holder at the copy. This is `Rc::make_mut`: value semantics are unchanged, and a write is in place exactly when the holder is the only one.

### D3 — reads borrow a slice

`AlmideList<T>: Deref<Target = [T]>`. Every runtime operation that takes `&[T]` today (48 of the 101 in `list.rs`) is unchanged. A borrowed parameter stays `&[T]`, and the renderer's borrow inference keeps its present rules.

The operations that take `Vec<T>` by value (34) take `AlmideList<T>` and call `unique()` only where they write. One that only reads (e.g. a `sort_by` returning a new list) takes `&[T]` instead.

### D4 — the renderer's copies become count increments

The sites in the Context list emit `.clone()` on an `AlmideList`, which is O(1).

A borrowed slice cannot be counted, because it has no block. Where an owned list is needed from a `&[T]` parameter, the renderer passes the caller's `&AlmideList<T>` instead of a bare slice: a "borrowed handle" `NTy`, so that `owned_from` can `.clone()` it. Only a slice of a slice (`list.slice` views, if introduced later) would still copy.

### D5 — threads mark the block

A list reachable from a value that crosses into a `fan` arm or a callback environment is marked shared exactly as `AlmideStr` is: the marking window clones it, and a clone inside the window sets the block's shared bit. From then on its count changes atomically. `almide_str_mark_reachable` generalizes to a `mark_reachable` covering both block kinds, at the same two native call sites (`render_native_fan.rs` `render_fan_group`, `native_hof_callbacks.rs` `bind_env`).

### D6 — one representation for both native routes

The v0 source emitter (`--target rust`) spells lists through the same runtime and switches with it. No route keeps `Vec<T>`.

## Consequences

- `bigint.max` / `min` / `clamp`, `deque.reverse` and every function that returns or re-packages a borrowed list become O(1) on native, matching wasm. #4776 and #4781 close by measurement: the plain-call rows fit 1, and the held rows are re-stamped.
- Native element access goes through one more pointer (the block instead of the Vec's own buffer pointer), the cost `AlmideStr` already pays. The perf gates (`check-native-v1-perf.sh`, the shapes gate) and the v1-vs-v0 ratio judge it. A loss row is a stop, not a ratchet raise.
- The change touches the list arm of the native type enum, the move/clone policy, every `.to_vec()` site, the runtime list file and the marking. It lands behind the existing native parity gates (native ⇄ wasm stdout, the alloc and witness ledgers on wasm are unaffected), in stages:
  1. `AlmideList` with its own unit and Miri tests.
  2. Runtime operations.
  3. The renderer's owned spellings.
  4. Borrowed handles.
  5. Marking.
- `Bytes` follows in the same stages. It shares the runtime type, so it is not a separate design.

## Alternatives

- **Patch the call sites** (return a borrowed parameter by moving, special-case `reverse`). Rejected: it fixes the two measured functions and leaves the class. Deciding by function name is what the owner's rules forbid.
- **`Rc<[T]>` / `Rc<Vec<T>>` from std.** Rejected: there is no thread-share bit (`Arc` everywhere costs atomics on every count change in single-threaded code), two allocations for `Rc<Vec>`, and no static-block form. `AlmideStr` was built for the same reasons.
- **Borrow inference only** (lend more, own less). Already done as far as it goes. A value that is returned, or stored in a record that outlives the call, must be owned, and owning a `Vec` is a copy.

## Owner decisions (2026-11-20)

The owner ruled ○ on the direction, then ruled on Q1 and Q2 after a cross-language comparison.

- **Q1 ○: `Bytes` moves to the same block, in the same series.** Conditions:
  - Before a writable `&mut [u8]` or a raw pointer is handed to FFI or to a runtime operation, the block is made unique (rc == 1). It stays uncloneable while it is lent.
  - There is no slice-sharing representation for now (no view of part of a block).
  - Counts are atomic by default on any block that can cross a `fan` thread. A non-atomic fast path comes only after measurement.
- **Q2 ○: one static empty block, shared by every element type.** Conditions:
  - It is aligned to 16. An element type aligned above 16 allocates its empty list normally. The first element's offset after the header is checked against the element's alignment as well.
  - Its count is immortal: never incremented or decremented, never freed, and never reported as unique, so a write to an empty list always allocates.
  - No free path may reach it. A debug assertion guards every free.
  - Address comparison is not exposed.
  - Each of these cases has a Miri test.

## Implementation notes (2026-11-20)

- **Spelling.** The v1 native header splices the `AlmideList` block after `AlmideStr` and exports `AlmideList` and an `almide_list!` macro (the `vec!` forms). The renderer's emitted text and its signature matchers spell a list `AlmideList<T>` explicitly; `Vec` is never rebound, so a `Vec` in emitted or spliced code always means std's.
- **Host plumbing stays `Vec`.** What is not an Almide value keeps std's vector, spelled `std::vec::Vec` in the shared runtime text: every almide-rt-core file, the socket / process / hook tables, the map and set storage, key bytes and index, the regex machine's stacks and the zlib buffers. A runtime wrapper converts with `AlmideList::from` where a host result becomes an Almide value.
- **v0 (D6).** The v0 source emitter spells lists the same way: its prelude splices the same block and exports `AlmideList`, `AlmideToList` and `almide_list!`; a list type renders `AlmideList<T>`, a literal `almide_list![..]`, an owned copy of a borrowed `&[T]` `.to_list()`, and a collect `collect::<AlmideList<_>>()`. Bytes is `AlmideRcCow<AlmideList<u8>>`. No `AlmideList = Vec` binding remains, in the emitters or in the host-side test mirrors, which include the runtime files on the real type. A program that walls into v0 (bigint.max's probe, #4776) therefore shares its lists by count too.
- **`almide_list![x; n]`** builds one block at its final capacity and fills it in place (`AlmideList::from_elem`: `x` and `n - 1` clones, as `vec!`). An element type that declares `AlmideZeroValid` (no padding, zero is a value: the primitive scalars) with an all-zero `x` allocates the block zeroed instead, keeping the allocator's untouched pages. The choice is method lookup on `&&AlmideListElem` (autoref), so a generic `T: Clone` context takes the clone fill.
- **The `@extern(rust)` host ABI is unchanged** (owner ruling (A), 2026-11-20): the host keeps `&[T]` / `Vec<T>` / `&mut Vec<T>` / `Vec<u8>` / `AlmideMatrix` (whose `data` stays a std `Vec<f64>`) exactly as docs/specs/module-system.md says, and the wrappers convert at the boundary as ADR-0013 does for String: a borrowed list or Bytes is a slice on both sides (no copy); an owned return and a `mut` param are copied, O(n), the `mut` one back into the caller's list block after the call.
- **Element access in loops** (owner ruling, 2026-11-21: the ADR's first build made fasta, fft and fannkuchredux 29-56% slower). The length and the count live in the header, which an element store may alias, so a loop writing `xs[i]` reloaded the length after every store and checked the count on every write. The certified native render now takes a loop's elements once (`render_native_elem_slices`): when every mention of a list inside a loop is a scalar element read or write, it binds `&mut [T]` (`make_mut`, one uniqueness check and at most one copy, at the loop's entry) or `&[T]` before the loop and indexes the slice. Copy-on-write holds — the copy of a shared block happens before any write; the one change is that a loop that writes a shared list on no pass still copies it once. `push` inlines its common case (a unique block with room) and keeps growth and the shared copy out of line. The count stays atomic (Q1).
- **A list field of a program record stays `AlmideList` for host code** (owner ruling, 2026-11-21): records and variants are the program's own types, so host code (an `@extern(rust)` body or a host calling an Almide package) that reads or builds such a record sees the field as `AlmideList<T>` and builds it with `vec![..].into()`. Function parameters and returns whose list sits in a structural position (top level, `Option` / `Result` / tuple / list element) stay `Vec` / `&[T]` with the boundary copies above.
- **Bytes** flips with List on v1 (Q1). A writable view (`make_mut`, `as_mut_slice`, `DerefMut`) copies a shared block first, so a `&mut [u8]` or pointer handed to foreign code always addresses a unique block; `a_foreign_write_through_a_pointer_never_reaches_another_owner` runs that hand-off under Miri.
