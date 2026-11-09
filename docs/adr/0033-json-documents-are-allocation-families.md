# ADR-0033: A parsed JSON document is one allocation family — its interior blocks keep the layout every reader already reads, the family carries the reference count, and parser, scanner and writer stay self-hosted and certified

- **Status**: Accepted (2026-11-09). The owner answered all thirteen [owner decisions](#owner-decisions-) ○ the same day; Q5 carries four safeguards and Q13 was added.
- **Date**: 2026-11-09
- **Scope**:
  - the representation of `Value` documents on both legs;
  - the representation of the Strings inside them;
  - how `json.parse` scans, how `json.stringify` writes, and how `value.*` constructors allocate;
  - the certificates that cover each of these.
  - The `Value` / `json` API is not in scope. It does not change (D8).
- **Direction**: the owner's direction of 2026-11-09.
  - The target is JSON performance at the level of the fastest libraries, native and wasm.
  - A major rework of `Value` and `String` is accepted.
  - The certification chain is not negotiable: the Trust Spine, the extracted Rocq checker, the witnesses, the anchor check against the shipped `.wasm` bytes, and the ratchets.
  - The route is **self-hosted and certified**. A hand-written trusted runtime JSON was rejected, because it grows the trusted base.
- **Roadmap**: [docs/roadmap/active/json-substrate.md](../roadmap/active/json-substrate.md) (phases, gates, negative controls, targets).
- **Related**:
  - #4613: wasm jsonround is 1.65× Rust; 141 allocations per record.
  - #2157: the Value row; native ≤ wasm.
  - #1679: Value representation. The ADR-0018 ruling: Value's string payload shares the String block.
  - #1004: strchurn.
  - #4297: key index from 16 keys on.
  - #939: the glTF quadratic.
  - #4005 A4 route A: linear certificates for self-host `prim` sites.
  - #4006 C1 / C2: the certified native render and its region window.
  - #4011 (d): anchor check.
  - #4277: RC events held to the bytes.
  - #4013: `MemorySafe`.
  - [ADR-0013](./0013-string-shared-immutable-buffer.md): String is a shared immutable buffer. This ADR rules on its stage 4.
  - [ADR-0016](./0016-numeric-determinism-is-normative.md): numeric determinism.
  - Branch `json-reborn`: the measured base for this ADR.
  - Branch `probe/json-world`: the world reference bench.

## Context

### Where the time goes today

The jsonround program builds 50k four-field records as one `Value`, stringifies the value, parses it back and sums `meta.score`. #4613 measured it on wasm, after AOT, on one GCE n2-standard-8 VM:

| | Almide | Rust (serde_json) |
|---|---:|---:|
| time | 269.5 ms | 163.0 ms |
| wasm instructions per record | 51.6k | 24.3k |
| allocations per record | 141 | 27 |

About 40% of Almide's instructions were allocation and RC traffic:

- `alloc` 19%;
- `free` 10%;
- `rc_inc` / `rc_dec` and their helpers about 12%.

The `json-reborn` branch then rewrote the self-hosted parser and stringify.

- **The parser** shares one position cell, so a read returns no `Result` and no tuple, and an Int is parsed in place.
- **Stringify** writes into one buffer.

Its probe (run 42578965891) splits jsonround into phases:

| Phase | Allocations per record |
|---|---:|
| build | 27 |
| stringify | ~0 |
| parse | 27 |
| **total** | **54** (was 141) |

**The parse count is exactly the floor of today's representation.** A record parses into:

- 9 Value blocks;
- 9 String blocks (6 keys and 3 values);
- 6 pair tuples;
- 3 lists.

9 + 9 + 6 + 3 = 27. The parser allocates nothing else. Better code cannot lower this number. Only a different representation can.

### Lessons the json-reborn branch measured

Every one of these has its own commit.

1. **Reading through calls costs more than the allocations it saves.** Commit 17166b602 shared a record's keys with the previous sibling's equal keys. That saved 3 allocations per record, but it cost **+31% instructions** and **+1.4 KB per module**, because reading a sibling's keys goes through calls. *Layout matters more than the algorithm.*
2. **An in-place Eisel–Lemire float fast path cost +440 B in every module that parses JSON** (0c004e05c). It was reverted.
3. **A run-copy quote saved 1% of jsonround and grew every module that prints a String by 37 B** (6462cef72). It was reverted.

### What the language already gives us

**The block layout.** On wasm every heap value is a block whose header is `[rc u32][len u32][cap u32]`, with its payload at +12 (`almide_layout::PAYLOAD`).

- A `Value` block has a tag word, an index word and an 8-byte payload (`crates/almide-wasm/src/value.rs`).
- A Str or Array payload is a *block address*: a String block, or a list of 4-byte slots.
- An Object payload is a list of `(String, Value)` tuple blocks.
- Every reader of a `Value` reads those offsets. That includes:
  - the emitter's `value.*` lowering;
  - the self-host `value_core.almd`;
  - equality, display and `$copy`;
  - the typed drop walkers;
  - Codec decode.

**The copy-on-write judges.** They read the rc word with *unsigned* compares:

- `rc <= 1` (`$cow`);
- `rc != 1` (`$list_push`);
- `rc == 1` (`$str_append`).

**Statics.** A pool static is recognised by its address, below `G_LINE_END` (`$inc` / `$dec_flat`).

**The region window (#1961, #4006 C2).** It already frees a whole family of blocks at once. For `consume(produce(scalars))` it saves the allocator state, runs both calls and rewinds. That makes binarytrees' frees one rewind. On native the arena mark is witnessed (`RegionSave` is `i`, `RegionRestore` is `d`). Its region-purity verdict is interim trust.

**Native.** `json.parse` / `stringify` are `@intrinsic`s in `runtime/rs/src/json.rs`, which is hand-written safe Rust and trusted.

- `AlmideValue` is a Rust enum. `Array` is a `Vec`, `Object` a pair `Vec` plus an index from 16 keys on. Keys are interned `Cow<'static, str>`.
- `native-trust-spine.md` rejects rendering prim-based self-host bodies natively, because the result would be a linear-memory emulator in Rust.

**What certifies a self-host parser on wasm today.** Two certificates do:

- the `bounds` witness: `xs[i]` / `b[i]`, `for` exits and the Bytes room test. It is proven by `SpatialCert.v` / `BoundsCert.v` and held to the bytes by `almide-anchor-check`.
- the linear certificates for `prim` sites (#4005 A4 route A: `RoutineLin.v` / `RoutineLinCert.v`, `almide-lin-prove`, `--lin-verify`). In the last census they covered 303 self-host bodies; 4943 remain in the counted class.

String bytes, map slots and the runtime's own routines have no certificate yet ([MEMORY-SAFETY.md](../design/MEMORY-SAFETY.md) gap table).

## Survey: how the fastest libraries represent a document

| Library | Document | Strings | Sub-value escape | What it pays |
|---|---|---|---|---|
| **yyjson** | **Immutable doc.** One array of 16-byte `yyjson_val` in preorder: a tag word holding type and length, then an 8-byte payload. A container stores its subtree size, so iteration skips subtrees. **Mutable doc.** A separate type, made of linked nodes in a pool. | Unescaped in place into a copy of the input buffer; a val points into it. | A val is a pointer into the doc. The doc must outlive it (manual). | `arr_get(i)` and `obj_get(k)` are O(n). Mutation needs a conversion. |
| **simdjson** | **Stage 1.** SIMD classifies 64 bytes at a time and writes a structural index. **Stage 2.** It builds a *tape* of 64-bit words; a container word holds the index of its matching end. | A separate string buffer, length-prefixed. | An element is a (tape, index) view, valid while the parser lives. On-Demand parses lazily. | Read-only. Two passes over the input. On-Demand changes the API. |
| **sonic-rs** | An arena DOM with SIMD scanning, plus lazy `get` from raw text. | Borrowed from the input where it can; owned when escaped. | Lifetimes. | Its SIMD path is x86_64 / aarch64 only. |
| **serde_json** | `Value` enum: `Vec`, `BTreeMap` (or `IndexMap`), `String`. | Owned. | Owned; a clone is deep. | About 12 allocations per jsonround record. |
| **Swift / Roc / Koka strings** (ADR-0013 survey) | — | Shared immutable buffer, small-string inline, seamless slices (Roc). | A slice retains the whole buffer. | Retention. Java *removed* substring sharing in 7u6 (2012) because of it. |
| **Umbra / "German" strings** | — | A 16-byte struct: length, a 4-byte prefix, then 12 inline bytes or a pointer. | — | Every string becomes a 16-byte value type. |

Two lessons carry over.

1. **The fast libraries do not allocate per node.** A document is a few large allocations, and a node is an offset into one of them. The 27-per-record floor is the gap.
2. **They all pay for that with an escape rule.** A view outlives its document only by keeping the document alive (simdjson, sonic-rs, Swift, Roc), or the program must ensure it does not (yyjson, by hand). Almide has no lifetimes and no manual frees, so it must pay this with reference counting: the view keeps the document alive.

## Decision

### D1. Allocation families with interior blocks (the substrate)

**A family is one allocation that holds many blocks.** Each block inside it, an *interior block*, has the **ordinary header and layout of its type**. The difference is one rc word:

- Its **top bit is set (INTERIOR).**
- **Its low 31 bits give the byte distance back to the family root.**

The root is an ordinary heap block whose rc counts every reference into the family.

- `$inc` / `$dec` on an interior block take one extra test after the rc load. A negative word redirects to the root. The fast path for ordinary blocks is the same load, followed by one sign test.
- **No interior block is ever freed on its own.** When the root's count reaches zero, the family is freed as one block.
- **Links.** A family may hold references *out of* itself (a node whose value was built elsewhere). Each one is a credit the family owns, recorded in the root's link list. The family's free releases the links, then the one block.
  - The cost of a free is O(links), not O(nodes).
  - A parsed document has no links.
- **Every reader works unchanged, because interior blocks are byte-for-byte the blocks they replace.** That covers `value.get`, display, equality, `$copy`, Codec decode, and every String routine (the bytes are at +12, the length at +4).
- **Every copy-on-write judge is conservative by construction.** It compares the rc word unsigned against 1, so an INTERIOR word reads as "shared". An interior block is copied before it is mutated, and nothing is ever mutated in place inside a family.
- **The redirect is emitted only in modules that link a family producer.** That is `json.parse`, the D4 constructors and D6's splitters. A module that links none cannot hold an interior block, so it keeps today's `$inc` / `$dec` bytes. Its size ledger does not move.

This is the region window's idea (a family freed at once), extended from scalar-result windows to values that escape. The count lives on the root, so a family may outlive the call that built it.

### D2. `json.parse` builds one family in two passes

- **Pass 1 validates and counts.** It runs the whole strict grammar: RFC 8259 + I-JSON, C-087 / C-400 / C-401, the 1000-depth bound, every refusal text and its char position. It counts:
  - the nodes of each kind;
  - the members of each container;
  - the bytes of each decoded string.
  - It allocates nothing. A refused document costs no heap.
- **Pass 2 carves the family.** It allocates the family once, at the exact size from the counts, and fills it.
  - Every carve checks `cursor + size <= end`, so pass 2 does not trust pass 1's arithmetic.
  - A disagreement between the passes is a defined abort, never a write outside the family.
  - The family is never reallocated, so interior back-distances never move.
- **Duplicate keys.** From 16 keys on, the duplicate check uses an index carved inside the family (a deterministic hash, so a value carries no per-process seed).
  - Any bucket that exceeds a fixed chain length falls back to sorting that object's keys.
  - The worst case is therefore O(n log n) on adversarial keys, never the O(n²) of a hash under attack (security row 9).
  - The index stays in the family for `value.get`, so wide objects get O(1) lookups without a lazily mutated cache.
- **Strings.** Each string is decoded into the family as an interior String block.
  - Its spare header bits carry a **clean** flag: no byte needed escaping on the way in, and none needs it on the way out.
  - Pass 1 already knows this.

**Consequence for the API.** Two surfaces stop allocating per element: `value.as_array` and `value.as_object` return the interior list block with one share, in O(1). The interior list is already a `List[Value]`, or a `List[(String, Value)]`. Today both are O(n) with one allocation per element on some routes.

`value.get` is a key scan over contiguous pairs, or the carved index. `value.as_string` returns the interior String with one share and copies nothing.

### D3. Scanning: SWAR on both legs, simd128 on wasm, native SIMD only by ruling

Pass 1 has three hot loops:

- the string body (stop at `"`, `\`, or a byte < 0x20);
- whitespace;
- digit runs.

They run over **word reads of the text at `i` under a room test `i + w <= len`**.

1. **SWAR on 8-byte words** (both legs, in the parser's own source): the has-zero-byte tests, and the 8-digit parse.
2. **wasm simd128** behind one internal byte-class scan op, `__scan_special(text, from, to) -> Int`. It is not user-nameable and not on the LLM-facing surface.
   - The op is an emitter helper: a `v128.load` per 16 bytes under its room test, `i8x16.eq` / `i8x16.lt_u`, `v128.or`, `i8x16.bitmask`, `i32.ctz`, then a scalar tail.
   - It is emitted only in modules that link it.
3. **Native SIMD** (`std::arch` with runtime feature detection, never `target-cpu`) needs `unsafe` lines in the runtime allowlist. It is **not** admitted by this ADR. It waits for a ruling (Q6) and a measured gap.

### D4. Construction: a value literal's shape is one family

- A nested `value.object([...])` / `value.array([...])` whose shape is static allocates one exact family for the whole shape. This includes the derived Codec encoders, which produce exactly that shape.
- A child computed elsewhere becomes a **link**: a moved credit, with no copy.
  - jsonround's `record(i)` goes from 27 allocations to about 4: the family plus three interpolated strings.
- A dynamic `value.object(xs)` keeps today's behaviour: one box, the list moved in when it is unique.
- This is a lowering rule over constructor calls the compiler already sees. It does not depend on the benchmark's shape.

### D5. Writing: one buffer, sized before it is written

- `json.stringify` computes the output length in a counting pass over the tree.
  - A **clean** interior string contributes its length.
  - Any other string is scanned once.
- It allocates the result once, at the exact size, and fills it.
  - A clean string's fill is one bulk copy.
  - The other strings take the existing escape walk.
- The result is one String with no growth, so no realloc. The writer runs only under jsonround's existing stringify ratchet.
- **Numbers.**
  - Ints are written through a two-digit table.
  - **Floats go through `float.to_string`'s own algorithm into the buffer.** That is the one shortest-round-trip printer, and ADR-0016 makes it normative. It gets a buffer entry point; no second printer is added.
  - A table-driven printer (Ryū) is rejected while its tables cost more than the measured gain (Q9).
- **The writer moves off the trusted helper.** On wasm the writer moves from the emitter-built `$vjson` helper into the self-host writer source. `$vjson`'s field accesses are trusted today; the self-host writer's sites are certified (D7).

### D6. The same family serves String producers

`string.split`, `lines` and `words` return their parts as one family: one allocation and one copy of the bytes, instead of N blocks.

This is the strchurn / wordfreq lever (#1004). A map key taken from such a part retains the family. That is the D9 retention rule again.

### D7. Certification: every mechanism has a witness, and the trusted base does not grow

| Mechanism | Clause | Witness / proof | Byte check (`almide-anchor-check`) | Negative-control drill (`scripts/witness-reinject.sh`) | Trusted → proven |
|---|---|---|---|---|---|
| Interior rc redirect, family free, links | TEMPORAL | **New `proofs/FamilyRc.v`.** It states and proves that a trace in which each interior block's inc/dec is credited to its root, and a family is freed only when its root count reaches zero (releasing its links), is `Temporal`. It extends `rc_check_gives_temporal`. `StructuralRuntime.v` transcribes the new `$inc` / `$dec` trees, with the double-free trap kept, and `check-structural-bytes.sh` grounds them. | `rcfns` names are unchanged. The family free's link releases are matched one to one like any release (#4277). | `family-redirect-dropped`: an interior dec decrements its own word. `family-root-off-by-4`. `family-link-not-released`. `cow-judge-reads-interior-unique`: a signed compare. Each must be rejected, by the kernel, the checked allocator or the alias fixture. | No new trusted item. The family's encoding is the representation R, already a trusted row, now written once in `StructuralRuntime.v`. |
| Carving interior blocks in pass 2 and in D4 / D6 | INIT, SPATIAL | **New `carve` sites in the `init` witness:** each interior header and slot stored before the family is published. This generalizes `site_window` to sub-extents (`InitCert.v`). Each carve's `cursor + size <= end` is a room test the `bounds` witness records. | Each carve store is held to its recorded offset. The carve check is held to an unsigned compare that dominates the stores. | `carve-store-misplaced`, `carve-room-loosened-by-one`, `pass2-count-disagrees` (must abort, not write). | Today every allocation site other than list literals and ranges is trusted (gap table, INIT). Families become recorded sites. **trusted → checked per build.** |
| Parser and writer `prim` sites (wasm) | SPATIAL | Linear certificates (route A: `RoutineLin.v` / `RoutineLinCert.v`) over every site in `json_parse.almd`, `value_utils.almd` and the writer. **Gate: zero counted sites for these bodies.** | `--lin-verify` against the shipped functions. | A poisoned `Q` certificate per file (a string copy past its span). | Today most of them sit in the counted (uncertified) class. **counted → proven.** |
| SWAR word reads | SPATIAL | The Bytes room test with width 8, in the `bounds` witness (exists). | Exists. | `swar-room-one-byte-loose`. | — |
| simd128 `v128.load` in `__scan_special` | SPATIAL | The helper-byte-loop route (`e N a N`) at **width 16**. If `BoundsCert.v` is not already width-parametric, it is made so (a proof change, not a trusted item). | The anchor check's decoder learns the `0xFD` prefix: `v128.load` is an `a` of width 16, and its room test a `c`. | `simd-room-15`, `simd-tail-exit-loosened`. | — |
| `as_array` / `as_object` / `as_string` sharing an interior block | TEMPORAL | An ordinary share (`a` event) on a block whose `$inc` redirects. Covered by FamilyRc. | As today. | The `alias_cow` family of fixtures, extended to interior blocks on both legs. | — |
| Native doc (phase 6 of the roadmap) | — | Safe Rust: no new `unsafe`. Its strings are ADR-0013 stage 3 seamless slices (`crates/almide-str`, Miri-gated). | — | Miri / ASan nightly over the JSON corpus. | `runtime/rs/src/json.rs` is retired at phase 7 (D10). |

**What this ADR does not move.**

- The ownership witness's events stay matched to calls, not located one by one.
- The field accesses inside the runtime's own routines stay trusted.
- On native, rustc's compilation of the rendered source stays trusted.

**What this ADR refuses to add.**

- A hand-written trusted JSON routine on either leg.
- An unchecked count from pass 1.
- A family without a byte check.
- Native `unsafe` without a ruling.

### D8. No language-surface change

`json.parse`, `json.stringify`, `json.stringify_pretty`, every `value.*` signature, the error texts, key order (C-401), duplicate refusal (C-400) and NaN as `null` (C-356) are unchanged.

- No lazy, on-demand or cursor API is added. That would be new surface, and #4613 classes lazy parsing as benchmark-shaped.
- The CHEATSHEET and `llms.txt` do not change.
- No new contract id is expected: the observables are today's. New fixtures cite the existing contracts.
- If a phase finds a new observable, the contract lands in als first (the project rule).

### D9. A sub-value keeps its whole document alive

A view keeps its document alive (D1). There is no copy-out on escape.

- This is what makes `get` / `as_array` / `as_string` O(1).
- It is the rule of every surveyed language that shares.
- Java's 2012 reversal is the known cost: a small value pinning a large buffer.

So the roadmap carries a **retention probe**. It records peak memory against live data on the JSON corpus, plus one adversarial program that keeps one key of each of many large documents.

A copy-out rule waits for a falsifier to fire. A plausible rule copies when the parent is dying and the subtree is less than 1/8 of the family. It would then be a separate ADR. The memory cap (row 9) keeps aborting as defined: allocation failure never returns (row 1).

### D10. End state: one parser source for both legs

**The interim.**

- wasm parses through the self-host `prim` parser (json-reborn), certified by route A, and families (D1–D3).
- Native parses through `runtime/rs` with a doc-backed `AlmideValue`.
  - It has a view variant over 16-byte nodes.
  - Its strings are seamless slices.
  - It uses SWAR, in safe Rust.

**The end state** (roadmap phase 7) is one source for both legs.

- The parser and the writer are written in typed Almide, over a zero-copy byte view of the text: no `prim`.
- They use a small **target-neutral doc-builder op set** in MIR, `Op::DocAlloc` / `Op::DocCarve` / `Op::DocLink`. Its precedent is `Op::ListLit`.
- wasm renders the ops to the carve sequences above, with their witnesses.
- Native renders them to a safe-Rust builder, in the certified C1 render where it renders and in v0 otherwise.

After a parity gate, `runtime/rs/src/json.rs` and the `prim` parser are deleted. Native JSON stops being hand-written trusted code.

## Rationale

1. **The floor is the representation.** The reborn parser allocates nothing but its result, and still allocates 27 per record. Each fast library escapes that by allocating per document instead of per node.
2. **Keep the layout and change only the owner.** The sibling-key experiment showed that any layout readers must reach through calls loses more than it saves (+31% instructions). Interior blocks are the same bytes as today's blocks, so:
   - no reader changes;
   - no reader gets slower;
   - `as_array` / `as_object` become O(1);
   - the only new code is the redirect in `$inc` / `$dec` and the carve.
3. **A compact layout loses what the API needs.** yyjson's 16-byte nodes are denser, but `List[Value]` on wasm is a list of 4-byte block pointers. With compact nodes, `as_array` would have to materialize a list, and every reader would change. Block-compatible interior blocks give up some density (about 2× yyjson's bytes per node) to keep O(1) sharing and every reader. The density question stays open behind a falsifier (Q11).
4. **Two passes buy certification as well as speed.**
   - An exact family size means no realloc, so the interior back-distances are stable.
   - Every carve's room test is a check, not a trust: pass 2 does not trust pass 1.
   - A refused document allocates nothing.
   - This is simdjson's stage split, used for a certificate rather than only for speed.
5. **One certified source beats two trusted ones.** Native JSON is hand-written Rust today. Moving both legs onto one typed source shrinks the trusted base, which matches the route the owner chose.

## Alternatives

- **A. Keep per-node RC Values and tune the code.** ×. The 27-per-record floor is reached; the remaining levers each measured at ≤1%, or cost size (lessons 2–3).
- **B. A simdjson-style tape behind the `Value` API.** ×.
  - A tape is read-only and positional. User code builds Values too (D4), so a second, mutable representation would be needed beside it.
  - `as_array` would materialize a list.
  - Every reader changes.
- **C. A yyjson-style compact immutable doc with 16-byte nodes.** × for wasm now; the native doc uses it (D10 interim).
  - On wasm it breaks the "readers unchanged" property and O(1) `as_array`.
  - It stays as Q11's falsifier.
- **D. Copy out on escape.** ×. It makes `get` O(subtree). D9 holds the question open behind a retention probe.
- **E. German / Umbra strings for the general `String`.** × now; this is ADR-0013 stage 4's ruling.
  - Every String would become a 16-byte value. That changes:
    - list slots on wasm (4 → 16 bytes);
    - every string op;
    - the representation R of every certificate;
    - FFI, C export, WASI and the component model's `(ptr, len)` ABI.
  - Its gain is avoiding allocation for short strings. Families already make JSON strings allocation-free (D2), and D6 does the same for split parts.
  - What remains is trim / to_upper / interpolation results in strchurn. Q4 sets the measured condition for reopening.
- **F. Slice views into the input text** (yyjson in-situ, simdjson strings). ×. A String's bytes must start at +12. A view of the input's middle has no header in front of it, so every String reader would need a data pointer. Copying each string into the family costs one bulk copy, and the allocations are what dominate.
- **G. A hand-written trusted JSON (runtime C or Rust) on wasm.** ×. It grows the trusted base. The owner rejected it.
- **H. Lazy / on-demand parsing.** ×. It is new surface (D8), and #4613 classes it as benchmark-shaped.

## Consequences

**Better**:

- Parse allocations go from 27 per jsonround record to one family per document.
- Build allocations go from 27 to about 4 (D4).
- `as_array` / `as_object` / `as_string` on parsed data become O(1).
- Stringify writes once into an exact buffer.
- split / lines / words allocate once.
- Three things move from trusted to checked per build:
  - every family allocation site (INIT);
  - the parser's and writer's `prim` sites (SPATIAL);
  - the stringify writer.
- At the end state, native JSON stops being hand-written trusted code.

**Paid**:

- One sign test in `$inc` / `$dec`, only in modules that link a family producer.
- One new proof (`FamilyRc.v`), plus a generalization of `InitCert.v`, and possibly of `BoundsCert.v` to width 16.
- Anchor-check support for the `0xFD` prefix.
- A doc-backed `AlmideValue` and ADR-0013 stage 3 on native.
- Retention: D9's escape rule.
- Two passes over the input.
- The native phases collide with the other session's active work in `render_native_*` and `runtime/rs`. The roadmap orders them last and marks them.

**Unchanged**: the API, the contracts, the error texts, the determinism (ADR-0016) and the size of modules that link no family producer.

## Falsifier

1. **Speed.** If, after phase 2 (families on wasm), jsonround's wasm instructions per record do not fall by at least 25% against the json-reborn base, families are not the lever. Phase 2 is reverted and this ADR is revised. The measure is deterministic: instruction counts.
2. **Reader regression.** If any reader-bound row gets more than 3% slower from the `$inc` / `$dec` redirect, the redirect moves to a separate routine called only on the slow path. The rows are wordfreq, mapbuild and binarytrees, on GCE A/B. If that still regresses, D1 is withdrawn.
3. **Retention.** If the retention probe shows a real program in the corpus or downstream holding more than 4× its live data because of D9, a copy-out ADR is opened before phase 5 lands.
4. **Two passes.** If pass 1 costs more than 35% of parse time on the string-heavy corpus (twitter, citm_catalog), pass 1 writes a structural index (simdjson stage 1) instead of counts.
5. **Density.** If, at the end state, wasm parse throughput on canada.json / citm_catalog trails yyjson-on-WASI by more than 1.5× and the profile shows bytes moved per node, the compact layout (Alternative C) is reopened for wasm.

## Owner decisions (○×)

One decision per question, each with a recommendation and the owner's answer.

**Answers of 2026-11-09.** Every question was answered ○.

| Q | Answer | Condition the owner attached |
|---|---|---|
| 1 | ○ | — |
| 2 | ○ | Behind the retention probe. |
| 3 | ○ | Per-step bounds in pass 2. Add to the roadmap: after phase 2, re-measure shape reuse (DuckDB-style). |
| 4 | ○ | Reopen inline small strings only if strchurn on wasm trails Rust by more than 1.3× after phase 5. |
| 5 | ○ | Four safeguards: the SWAR fallback is kept; a build option disables simd128; CI runs the suite with simd128 disabled; the release note states the requirement. |
| 6 | ○ (conditional) | Only if phase 6's SWAR trails yyjson by more than 1.3×. Runtime detection, never `target-cpu`; the `unsafe` lines go on the allowlist under Miri and ASan. |
| 7 | ○ | Delete `runtime/rs` `json.rs` and the `prim` parser after the parity gate (phase 7). |
| 8 | ○ | — |
| 9 | ○ | — |
| 10 | ○ | — |
| 11 | ○ | Deterministic hash index from 16 keys on, sort fallback, no random seed. |
| 12 | ○ | Native phases start after the other session's `render_native_*` / `runtime/rs` work lands. |
| 13 | ○ | (Added 2026-11-09.) json-reborn does not land alone. It is carried into phase 2; its size and allocation movements and the stdlib re-inspection are ruled once, after phase 2. |

**Reprioritization from the world bench (run 42587113062).** The leaders' advantage is arenas and layout, not SIMD:

- simdjson without SIMD parses jsonround in 22 ms on wasm, and forcing westmere costs it 0.7 ms natively;
- yyjson builds 250k values with 29 allocations, where json-reborn makes 1.05M (27 per record);
- Almide's stringify makes 2 allocations but takes 21.8 ms natively against yyjson's 5.7 ms, which is per-byte cost (number formatting, escaping, buffer growth).

So families (phase 2) and fused construction (D4, phase 4) come before SIMD (phase 3). The roadmap orders them so.

1. **Adopt allocation families with block-compatible interior blocks (rc top bit + root distance) as the substrate for parsed documents (D1).**
   Recommendation: ○. It is the only option measured to keep every reader unchanged while removing the per-node floor.
2. **A sub-value keeps its whole document alive, with no copy-out on escape, behind a retention probe (D9).**
   Recommendation: ○. Every surveyed sharing design does this, and copy-out makes `get` O(subtree).
3. **Parse in two passes (validate and count, then carve an exact family), with a room check at every carve (D2).**
   Recommendation: ○. It gives an exact size, no realloc and a checked carve, and a refused document allocates nothing.
4. **Do not adopt German / Umbra inline strings for `String` now (ADR-0013 stage 4). Reopen only if, after phase 5, strchurn on wasm still trails Rust by more than 1.3× and the profile shows short-string allocation.**
   Recommendation: ○. It changes every ABI and certificate for a gain families already take on JSON.
5. **Ship wasm simd128 in modules that link the byte-class scan op, with `v128.load` certified at width 16 by the anchor check.**
   Recommendation: ○. Every stock host enables simd128, and modules without JSON do not change.
6. **Admit native SIMD (`std::arch`, runtime detection, never `target-cpu`) as counted `unsafe` lines in the runtime allowlist, only after phase 6 shows SWAR trailing yyjson by more than 1.3× on the string-heavy corpus.**
   Recommendation: ○ with that condition. Without SIMD, native cannot reach simdjson's class, and the condition keeps the allowlist from growing on a guess.
7. **The end state is one typed-Almide JSON source for both legs over target-neutral doc ops; `runtime/rs/src/json.rs` and the `prim` parser are deleted after a parity gate (D10).**
   Recommendation: ○. It is the self-hosted and certified route, and it shrinks the native trusted base.
8. **Keep the API unchanged: no lazy, on-demand or cursor surface (D8).**
   Recommendation: ○.
9. **Float formatting stays `float.to_string`'s algorithm, with a buffer entry point and no second printer.**
   Recommendation: ○. ADR-0016 makes the printer normative, and a second printer is a second thing to prove equal.
10. **The Eisel–Lemire parse fast path (+440 B per JSON module) stays out, until phase 3 measures canada.json's parse gain. It is admitted only if the gain is at least 15%.**
    Recommendation: ○. This follows the ruling of 2026-11-08: no size raise without a number and a reason.
11. **Duplicate-key detection uses a deterministic hash index inside the family, falling back to sorting that object's keys when a chain exceeds a fixed length, so the worst case is O(n log n) with no random seed.**
    Recommendation: ○. It keeps determinism and closes hash flooding on untrusted input.
12. **Order the work wasm first (phases 1–5) and native after (phases 6–7), started once the other session's `render_native_*` / `runtime/rs` work has landed.**
    Recommendation: ○. The wasm phases touch neither area, so the two sessions do not collide.
13. **Do not land json-reborn on its own. Carry it into phase 2, and rule its size and allocation rows and the stdlib re-inspection once, after phase 2.**
    Answer: ○ (2026-11-09). One ruling on the net movement replaces two rulings on intermediate states.

## References

- yyjson data structures: https://ibireme.github.io/yyjson/doc/doxygen/html/md_doc__data_structure.html
- simdjson, Langdale & Lemire, "Parsing Gigabytes of JSON per Second", VLDB Journal 28 (2019): https://arxiv.org/abs/1902.08318
- simdjson On-Demand design: https://github.com/simdjson/simdjson/blob/master/doc/ondemand_design.md
- sonic-rs: https://github.com/cloudwego/sonic-rs
- serde_json `Value`: https://docs.rs/serde_json/latest/serde_json/enum.Value.html
- Neumann & Freitag, "Umbra: A Disk-Based System with In-Memory Performance", CIDR 2020 (the 16-byte string): https://www.cidrdb.org/cidr2020/papers/p29-neumann-cidr20.pdf
- Lemire, "Number Parsing at a Gigabyte per Second" (Eisel–Lemire), Software: Practice and Experience 51 (2021): https://arxiv.org/abs/2101.11408
- Adams, "Ryū: fast float-to-string conversion", PLDI 2018: https://dl.acm.org/doi/10.1145/3192366.3192369
- Java 7u6 `String.substring` copy change (JDK-6924259): https://bugs.openjdk.org/browse/JDK-6924259
- WebAssembly fixed-width SIMD (simd128): https://github.com/WebAssembly/simd/blob/main/proposals/simd/SIMD.md
- In this repo:
  - #4613; the json-reborn commits 17166b602, 0c004e05c and 6462cef72; probe run 42578965891;
  - `crates/almide-wasm/src/value.rs`, `runtime_alloc.rs` (`emit_inc` / `emit_dec_flat`), `runtime_alloc_cow.rs`, `region.rs`;
  - `runtime/rs/src/value.rs`, `json.rs`;
  - `proofs/MemorySafe.v`, `InitCert.v`, `BoundsCert.v`, `SpatialCert.v`, `StructuralRuntime.v`, `RoutineLin.v`;
  - `docs/design/MEMORY-SAFETY.md`, `docs/contracts/proven-vs-trusted.md`, `docs/roadmap/active/native-trust-spine.md`.
