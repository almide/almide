# ADR-0037: A native List is a shared block with a count, copied only when written while shared

- **Status**: Proposed (2026-11-20), for owner review before any implementation.
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

`Bytes` becomes `AlmideList<u8>`.

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

## Open questions for the owner

- Q1: should `Bytes` move in the same series (recommended), or stay `Vec<u8>` until the list series is measured?
- Q2: should the empty-list static block be shared across element types (a single zero-length block, as the empty string is)? Recommended ○.
