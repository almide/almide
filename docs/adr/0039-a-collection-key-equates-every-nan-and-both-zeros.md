# ADR-0039: A collection key equates every NaN with every NaN and -0.0 with 0.0

- **Status**: Accepted (2026-11-24). The owner approved the rule (ruling 2026-11-24 evening, binding) as a language-surface change to land before the final 0.70.0, on both targets.
- **Date**: 2026-11-24
- **Scope**: Set membership and Map key lookup, on every leg (native v1, the v0 source emitter, wasm, the reference interpreter). Plain `==` / `!=`, `list.*` equality and every ordering are unchanged.
- **Related**:
  - ALS-C1 / ALS-C2 (Map and Set order), ALS-C3 (structural `==`, IEEE on every Float component), ALS-C9 (orderings use IEEE-754 totalOrder), ALS-T23 (signed zero).
  - C-014 (Set order), C-289 (`list.unique` / `dedup` / `contains` / `index_of` use IEEE `==`), C-464 clause (2) (the native route refused a Set whose element holds a nested Float, E081). This ADR retires that clause.
  - C-466 (almide/als, ALS-C11): the contract for this rule; fixture `spec/wasm_cross/collection_key_total_equality.almd`.

## Context

A Set decides membership, and a Map decides which entry a key names, with the element's (key's) equality. Until now that equality was `==`, which is IEEE on every Float inside the element (ALS-C3):

- `NaN == NaN` is false, so `set.from_list([nan, nan])` held two members, `set.contains(s, nan)` was false for a set that printed a NaN, and `map.get(m, nan)` never found the entry `map.set(m, nan, v)` had just written. A set holding a NaN was not equal to itself (`s == s` false while `set.is_subset(s, s)` was true).
- `0.0 == -0.0` is true, which is what a key wants, but the native v1 render carries a Float nested in a List, tuple, record or variant as its 64 bits and compares the bits. A bitwise compare makes `[0.0]` and `[-0.0]` two keys and makes two NaNs one key only when their payloads match. That disagreement with wasm is why C-464 clause (2) refused `Set[List[Float]]` and its kin on the native route.

So the current rule both surprises the writer (a key that is never found again) and has no native form that agrees with wasm.

### How other languages answer

| Language | Float as a set element / map key | NaN | 0.0 vs -0.0 |
|---|---|---|---|
| Rust | `f64` is neither `Eq` nor `Hash` (nor `Ord`), so `HashSet<f64>` / `BTreeSet<f64>` do not compile; the user wraps the value (`total_cmp`, a newtype) and chooses | — (refused) | — (refused) |
| Java / Kotlin | boxed `Double.equals` / `hashCode` (via `doubleToLongBits`) | every NaN equals every NaN | **distinct** (`0.0` and `-0.0` are two keys) |
| JavaScript | `Set` / `Map` use SameValueZero | every NaN equals every NaN | **equal** (one key) |
| Python | `dict` / `set` test identity, then `==` | a NaN is found only by the same object; two NaNs computed separately are two keys | equal |
| Swift | `Set<Double>` / `Dictionary` use `==` and `hashValue` | NaN != NaN: each insert of a NaN adds a member that can never be found | equal |
| Go | `map[float64]` uses `==` | NaN != NaN: each `m[NaN] = v` adds an unreachable entry | equal |

The languages that use IEEE `==` for keys (Python, Swift, Go) all lose NaN keys. Rust avoids the question by refusing the type. Java and JavaScript define a total collection equality; they differ only on the zeros. The rule the owner chose is the JavaScript one, SameValueZero, applied structurally: every NaN is one key and the two zeros are one key, which keeps `0.0 == -0.0` (true under IEEE) and only changes NaN. Java's split of the zeros would make a key that compares `==` to another key a different key, which a writer reading `==` cannot predict.

## Decision

### D1 — the key equality

Set elements and Map keys are compared with the **key equality** `~`:

- Two `Float` (`Float64`) values are `~` when both are NaN (any sign, any payload), or when neither is NaN and they are `==` (so `0.0 ~ -0.0`). `Float32` is the same rule at its own width.
- Every other scalar (`Int`, the sized integers, `Bool`, `String`, `Bytes`) is `~` exactly when it is `==`.
- A composite (`List`, tuple, record, variant, `Option`, `Result`) is `~` when it has the same shape (length, case) and its components are pairwise `~`. A nested `Set` / `Map` is `~` by D3.

Equivalently: map every Float component to its canonical form (every NaN to the quiet NaN `0x7FF8000000000000`, `-0.0` to `+0.0`; `0x7FC00000` and `+0.0` for `Float32`) and compare the canonical forms bit for bit. A hash, where an implementation keeps one, hashes the canonical form.

### D2 — what uses it

Every Set operation that asks "is this a member": `set.from_list`, `insert`, `remove`, `contains`, `union`, `intersection`, `difference`, `symmetric_difference`, `is_subset`, `is_disjoint`, and every operation built on them. Every Map operation that asks "which entry": `map.get`, `set`, `contains`, `remove`, `update`, `upsert`, `merge`, `from_list`, the index read `m[k]` and write `m[k] = v`, and every operation built on them.

A member or key that is already present stays as it was first stored: inserting `0.0` into a set holding `-0.0` changes nothing (the set still prints `-0.0`), and `map.set(m, 0.0, v)` on a map whose key is `-0.0` replaces the value and keeps the key `-0.0` in its place (ALS-C1 order is unchanged).

### D3 — Set and Map `==`

A Set is equal to another Set when they have the same number of members and every member of one is a member of the other (D2). A Map is equal to another Map when they have the same keys under `~` and, for each key, the values are `==`. Both are what the implementations already did with `==` as the membership test; under D1 a set holding a NaN is now equal to itself.

### D4 — what does not change

- `==` / `!=` on Floats and on every composite that is not a Set or Map stay IEEE (ALS-C3): `nan == nan` is false, `[nan] == [nan]` is false, `0.0 == -0.0` is true.
- `list.contains`, `list.index_of`, `list.unique`, `list.dedup` and `list.unique_by` are list operations defined by `==` and stay IEEE (C-289, C-053). `set.from_list` and `list.unique` therefore differ on NaN (`[nan, nan]` is one member, two list elements); a list has no keys.
- Orderings stay IEEE-754 totalOrder (ALS-C9): `list.sort`, `set.sort`, `min` / `max`. totalOrder puts `-0.0` before `0.0` and is not an equality; nothing here uses it.

### D5 — both targets, one statement

The rule holds on native v1, the v0 source emitter, wasm and the reference interpreter, byte for byte. The native route stops refusing a Set whose element holds a nested Float (C-464 clause (2), E081): it renders that set under D1 instead.

### D6 — which keys exist is unchanged

The checker's admission of keys does not change here. A Set element may hold any Float (E016 refuses only functions). A Map key holding a `Float` stays E058 ("not hashable"); a `Float32` key is admitted today and follows D1. Lifting E058 for `Float` keys, which D1 makes sound, is a separate surface change and an owner question.

## Implementation map (2026-11-24 survey)

- **wasm**: every membership test goes through `keyed_find` → `scan_helper` (`crates/almide-wasm/src/collections.rs`). A top-level Float scans with `Helper::ScanF64` (`runtime.rs` `emit_scan_f64`); a nested Float scans with `Helper::ScanDeep`, whose body calls `emit_val_eq` down to the Float leaf (`equality.rs`). The change is a key-equality mode beside `==`: its own helpers (`ScanF64Key`, a key variant of the outlined `EqTy` / `Eq` helpers) with the D1 leaf, so `==`, `list.unique_by` and the list family keep the IEEE helpers.
- **native (v1 and the v0 source emitter)**: `AlmideSet::position_with` and `AlmideMap::position_with` (`runtime/rs/src/set.rs`, `map.rs`) compare with `PartialEq`. They switch to an `AlmideKeyEq` trait: explicit impls for the scalars (`f64` / `f32` by D1), `AlmideList<T>`, `Option`, `Result`, tuples and `AlmideValue`, and a render-emitted impl per program record and variant (the way `AlmideKeyEncode` is emitted). `almide_rt_list_unique`, which uses an `AlmideMap` as its seen-set, keeps an IEEE path (C-289). The v1 admission (`native_set_element_type`, `exact_eq_type`) then admits a nested Float element, and the E081 clause of C-464 goes.
- **interpreter**: a `key_eq(&Value, &Value)` used at the Set and Map sites of `hofs_map_set_ops.rs` and the map insert, with `PartialEq` left IEEE for the list family; `set.remove`, `is_subset` and `is_disjoint` get interpreter arms so they no longer fall through to the bitwise self-host bodies.

## Consequences

- A program that stored NaN keys and relied on never finding them again changes behaviour: its sets shrink and its lookups succeed. No such use is known in the corpus; the change is a language-surface change and ships under the 0.70.0 release candidate soak.
- The native v1 render needs a key-equality form for every element type it admits, including nested Floats it carries as bits. The canonical-bits form of D1 is that form; it is also the hash input.
- The cheatsheet and `llms.txt` state the rule next to Set and Map, with the D4 contrast, in the same change.
- C-464 keeps clause (1) (E090 for an `@extern(rust)` Region return); its clause (2) is superseded by C-466.

## Alternatives

- **Keep IEEE `==` for keys** (Python / Swift / Go). Rejected: a NaN key is written and never found, and the native render still has no agreeing form for nested Floats, so the E081 refusal would stay.
- **Java's split of the zeros** (`0.0` and `-0.0` distinct keys). Rejected: two keys that are `==` would be different keys. Every language in the table except Java and Kotlin treats them as one key.
- **Refuse Float keys** (Rust). Rejected: `Set[Float]` and `Map[Float, V]` are legal Almide today and their common uses (no NaN) are sound; refusing them breaks written code for a corner case D1 defines.
- **Bitwise equality** (what native v1 does today for nested Floats). Rejected: two NaNs with different payloads are two keys depending on which operation produced them, which a writer cannot see.

## Owner decisions (2026-11-25)

The owner ruled ○ on four follow-up questions:

- **The rule is SameValueZero, as ruled.** The ruling's shorthand called it "Java-style"; Java's `Double.equals` keeps `0.0` and `-0.0` apart, so the rule matches JavaScript's SameValueZero, not Java. The rule stands as ruled: every NaN is one key and the two zeros are one key.
- **Set and Map `==` follow the key equality** (D3): a Set or Map holding a NaN is `==` to itself.
- **Native `Set[List[Float]]` and `Set[Option[Float]]` render before the final**, with the element's Floats as real `f64` values in the set (the top-level `Set[Float]` form, #4795, extended), so no E081 refusal remains.
- **Lifting E058 for `Map[Float, V]` waits until after the final.** D6 stands until then.
