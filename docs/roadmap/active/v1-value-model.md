# v1 dynamic Value model — the yaml keystone (path A: self-host + ONE trusted recursive-drop routine)

**Status: DESIGN (2026-06-19). CEO chose path A ("Aでいくぞ"): keep the trusted base minimal, PROVE the Value model (constructors/extractors/serializer self-hosted in .almd, cert-verified), with the recursive free as the ONE trusted runtime routine (like `DropListStr` already is). Coq-free; byte-match-verified vs v0 `runtime/rs/src/value.rs` per brick.**

## Why this is the keystone

yaml's remaining 22 v1 walls are dominated by the dynamic `Value` model: `value.array`/`value.object` build it (collect_seq/collect_map), `value.as_array` binds the array payload (yaml:249 `ok(items) => emit_seq(items, ind)`), `value.stringify` + the yaml `emit` serializer read it. v1 self-hosts SCALAR Values (value_core.almd: Null/Bool/Int/Float tags 0-3, Str tag 4) but NOT the COMPOUND ones (Array tag 5, Object tag 6). The blocker is the RECURSIVE FREE: a `List[Value]`/Array's elements are themselves Values with their own heap payloads, so the existing `DropListStr` (a FLAT per-slot `rc_dec`) LEAKS them.

## The pieces (a COUPLED unit — observable only end-to-end via stringify, so build + byte-verify together)

1. **`List[Value]` materialize** — extend `try_lower_str_list_literal` (binds.rs:103) to admit Call elements (`[value.int(1), value.int(2)]`) like the tuple/ResultOk Call paths; AND mark the list as a VALUE-element list (new set `value_elem_lists`) so its drop is the recursive value-aware one, NOT the flat `DropListStr` (which would leak str/array/object elements). Same for the call-arg position (calls.rs, mirror the f89cbfca List[String] brick).

2. **Recursive `$__drop_value` trusted runtime routine** (preamble, render_wasm.rs ~1040 — the ONE trust addition, like `$rc_dec`/`$alloc`/the inline DropListStr). At rc==1 (last ref):
   - tag < 4 (scalar): nothing nested → `rc_dec(p)`.
   - tag == 4 (Str): `rc_dec(payload@12)` (frees the String) → `rc_dec(p)`. (== today's inline DropValue.)
   - tag == 5 (Array): payload@12 = a `List[Value]`. `for i in 0..len(list): $__drop_value(elem_addr(list,i))`; then `rc_dec(list)`; then `rc_dec(p)`.
   - tag == 6 (Object): payload = the key/value store (match v0's Object layout — `Vec<(String, Value)>`). `for each pair: rc_dec(key String); $__drop_value(value)`; then `rc_dec(store)`; then `rc_dec(p)`.
   The cert sees ONE `d` (an `Op::DropValue`, opaque). The recursion is the trusted routine — verified by the rt-oracle-registry DIFFERENTIAL test (a `value.stringify` round-trip fixture proves no leak/no double-free vs v0). A `value_elem_lists` List drops via a sibling `$__drop_list_value` (loop `$__drop_value` per element + free list).

3. **`value.array`/`value.object` self-host** (value_core.almd): `alloc_value`, `store32(h+4, 5/6)`, `store64(h+12, <list/store handle>)` — MOVE the items in (v0 does `Array(items.clone())`, so a DEEP COPY: the self-host either copies items or takes ownership; match v0's bytes via the stringify round-trip). Register in render_wasm/registry.rs.

4. **`value.as_array`/`value.as_object`** — read tag; tag 5/6 → `Ok(payload list/store)` (BINDS the heap payload — yaml:249), else `Err`. A heap-Ok Result (cap-as-tag, like value.as_string — reuse the str-result machinery from commit 7b24ef8f).

5. **`value.stringify`** self-host — recursive: scalar → "null"/"true"/n/float.to_string; Str → `"\"" + escape(s) + "\""` (escape `\\ " \n \r \t` EXACTLY as v0 value.rs:228); Array → `"[" + (items |> list.map(stringify) |> list.join(",")) + "]"`; Object → `"{" + (pairs |> list.map((k,v) => "\"" + escape(k) + "\":" + stringify(v)) |> join(",")) + "}"`. The self-call is inside `list.map` (defunctionalized by C1, NON-tail) so the self-rec GUARD does not apply — it should lower. float via the self-hosted float.to_string (#63). VERIFY: `value.stringify(value.array([value.int(1), value.str("a")]))` byte-matches v0 (`[1,"a"]`).

## Gates (per brick, the proven methodology — 9 bricks this session)

corpus-wall ACCEPT (3 props, ownership = the leak/double-free check that catches a bad recursive drop) + a Value-model byte-match probe (build + stringify, compared to v0) + cargo test + output-parity baseline. A drop bug = a leak/double-free → corpus-wall REJECTS or the probe diverges → revert (never ship). After the Value model: TCO (mid-loop-break-with-result, docs/roadmap/active/v1-tco-self-recursion.md) + float.parse → yaml 0 walls.

## Honest scale

This is a COUPLED unit (~5 pieces, recursive runtime drop + serializer), genuinely a focused multi-brick push, not a single-turn task — but every piece is Coq-free and byte-verifiable, and the recursive-drop is the ONLY trusted-base addition (one routine, like DropListStr). Path A keeps the Value model PROVEN, which is the v1 differentiator.
