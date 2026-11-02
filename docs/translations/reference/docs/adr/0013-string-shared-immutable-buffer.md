# ADR-0013: `String` becomes a shared immutable buffer — the codec ceiling is a representation, not a law

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0013-string-shared-immutable-buffer.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0013-string-shared-immutable-buffer.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0013-string-shared-immutable-buffer.zh-CN.md)

> Non-normative reader copy of a pinned historical decision. The original remains authoritative; this copy is not automatically synchronized. Recorded measurements, comparisons and implementation claims are preserved, not independently revalidated. Code examples and comments are preserved verbatim, including their original language. The dated adoption, staged plan, numerical gains and rollback threshold remain historical source claims, not confirmation that all stages are currently implemented. The source explicitly lacks primary evidence for the Roc row and gives none for some other comparisons; this copy does not independently establish them. It does not establish current cross-target equivalence or memory safety. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md)

- **Status**: Accepted (approved 2026-09-04. The first draft, “keep String owned and record the ceiling in the ledger,” was rejected after re-examining ../almide-references: all the higher-level languages use an immutable shared buffer; none defaults to owned copying. This is a rewrite adopting Alternative A.)
- **Date**: 2026-08-30 (first draft) / 2026-09-04 (revision and ratification)
- **Decision scope**: The native **representation** of Almide `String` values (owned `String` → reference-counted shared immutable buffer), and copy-on-write on the `mut` surface (`string.push` / `string.clear`). The representation of `Value` is excluded (#1679, a separate arc).
- **Related**: #1673 / #1678 / #1679 / #1004, `docs/design/MEMORY-SAFETY.md` (RcCow), ADR-0004 (the stdlib String endpoint), C-132 (the write-back convention for mut arguments).

<a id="context"></a>

## Context

With the goal of “not losing to the upstream schema-validation library” (under `../almide-references`, v4.5.2), all mechanisms that could be tightened were tightened on the same workload (one million decodes of a record with 8 fields, a nested record, and `List[String]`, arm64, ns/op), with these results:

| | v0.59.2 | develop (2026-08-30) | Upstream |
|---|---:|---:|---:|
| `for v in data { User.decode(v) }` | 1693 | **205** | `safeParse` ~100 |
| `json.parse` (same document) | 948 | **515** | `JSON.parse` ~500 |
| parse + decode (end to end) | 1576 | **735** | ~600 |

The remaining difference has been narrowed to one cause. Of the 205 ns of `User.decode`, about 160 ns is **owned copying** of the record's eight `String` fields, which upstream does not pay (JS strings are shared). Handwriting the same shape in Rust gives 53 ns for a record borrowing its fields and 168 ns for a record with owned `String` fields (#1679).

The same root cause appears in another benchmark. **3/4** of the strchurn difference (#1004) comes from `string.split` returning `Vec<String>`: Almide has no borrowed string type, so substrings are always copied (`research/benchmark/perf/string-gap-1004.md`).

<a id="参照コンパイラはどうしているかalmide-references2026-09-04-再調査"></a>

### What reference compilers do (../almide-references, re-examined 2026-09-04)

| Language | `String` representation | Primary source |
|---|---|---|
| Koka (the direct ancestor of this design's Perceus) | Immutable, reference-counted sharing. Four representations: empty singleton / ≤7 B inline small string / normal / raw buffer. Counts are **non-atomic**; only thread-shared values switch to atomic counting with a negative count | `koka/kklib/include/kklib/string.h:13-22`, `kklib.h:103-114` |
| Swift | Copy-on-write + 15 B inline small strings; literals are immortal (ARC omitted) | `swift/stdlib/public/core/StringObject.swift:28-45` |
| Roc | Inline small strings + reference counting; seamless slices share buffers | (The tree is shallow and no primary source is available. A well-known design.) |
| Gleam / Erlang | Shared immutable binaries; sub-binaries share subsequences | — |
| Rust / Zig | Separate owned and borrowed forms | — |

No higher-level language defaults to owned copying. Rust is owned but has `&str`; precisely what Almide lacks accounts for the #1004 difference.

<a id="almide-側の事実"></a>

### Facts on the Almide side

- **The wasm leg already uses a shared representation**: String is a reference-counted block with `rc@0` (`crates/almide-layout`). Only native performs owned copying. “Changing the representation adds to the byte-identity proof obligation” has it backwards: this aligns the two legs (the first draft's withdrawal condition, “if wasm gets a shared representation,” had already been met).
- **There are only two destructive operations**: `string.push(mut s)` and `string.clear(mut s)`. RcCow, already used by List / Map (in-place if unique, copy if shared), can be applied as is, without changing value semantics.
- **The prototype the first draft measured as slow was something else**: `Arc<str>` + HashSet interning on the `Value` side, including hash lookup (parse +6% / decode +20%). That was not a comparison between simple sharing (clone = count +1, ~1-5 ns) and allocation + memcpy (~20 ns per field).

<a id="decision"></a>

## Decision

**Native `String` becomes a reference-counted shared immutable buffer.** The `mut` surface (`push` / `clear`) uses RcCow to select in-place modification or copying based on uniqueness. Substrings (`split` / `slice` / `trim` …) return seamless slices pointing to the same buffer. This is observationally equivalent to the wasm leg's representation (rc blocks).

Stages (each lands independently, and the ratchet protects the preceding stage):

1. **Put the ratchet in place first**: add strchurn / fasta / decode (`User.decode` one million times) to the A/B ledger in `research/benchmark/perf/` and make them targets of `scripts/check-perf-ratio.sh`.
2. **Switch representation**: change `String` in generated Rust and `runtime/rs` to `AlmideStr` (`Rc<str>`-based, non-atomic). `push` / `clear` use RcCow. Crossing fan uses Koka's thread-shared approach (only values passed to fan become atomic). If the stage-2 measurements show atomic RC worsening strchurn, stop here and restore the representation (a red ratchet means it does not land).
3. **Seamless slices**: the `split` / `slice` / `trim` / `lines` families share buffers. This eliminates 3/4 of #1004.
4. **Inline small strings** (Koka ≤7 B / Swift 15 B) belong in a separate ADR: they change the ABI and diverge from wasm, so are out of scope here.

Implementation is **after** 0.62.0 (representation changes do not straddle a release boundary).

<a id="rationale"></a>

## Rationale

1. **COW preserves value semantics.** The program's observable result is identical to owned copying. RcCow already operates for List / Map; extending it to String is not a new mechanism. No additional rule needs to be known by the LLM writer.
2. **The two legs take the same shape.** Wasm already shares. Aligning native lets C-132 mut write-back and alias_safety be discussed on the basis of “the same representation on both legs.”
3. **The gap was representation, not a ceiling.** “decode ≒ 100 ns + 20 ns × number of String fields” is the linear law of owned copying; a shared buffer reduces that to ~1-5 ns × the number of fields (the handwritten 53 ns is evidence of the lower bound).

<a id="alternatives"></a>

## Alternatives

- **A'. Keep ownership and record the ceiling in the ledger (first draft).** Rejected. Contrary both to higher-level language designs and to the wasm leg's existing representation.
- **B. Borrowed records on the decode side.** Not expressible because the language has no reference types. Rejected.
- **C. Sharing `Value`.** Slower in measurements (#1679). Rejected (a separate arc).
- **D. SSO alone.** Removes allocations but leaves copying and the `split` problem. Consider after this decision as stage 4.

<a id="consequences"></a>

## Consequences

- Improves: decode's String-field cost ~20 ns → ~1-5 ns per field; strchurn's split copies disappear; String representations align across both legs.
- Costs: the String type changes in `runtime/rs` and generated Rust (ADR-0004's endpoint is an API, not a representation, so this does not touch the frozen surface). `push` / `clear` gain a uniqueness branch. Accept the cost of making fan-crossing values atomic only after measuring it with the stage-2 ratchet.
- Unchanged: contracts (observables remain identical), the `String` API, and the wasm leg.

<a id="falsifier"></a>

## Falsifier

If the stage-2 ratchet shows either strchurn or fasta **worsening by 5% or more**, and the thread-shared approach does not recover it, revert stage 2, mark this ADR Superseded, and return to A'.

<a id="references"></a>

## References

- #1673, #1678, #1679, #1004, PR #1680–#1687
- `../almide-references/RESEARCH.md` §5 (JSON string scanning), §6 (Koka's borrowed/owned environments)
- `../almide-references/koka/kklib/include/kklib/string.h`, `kklib.h` (representation and thread-shared RC)
- `../almide-references/swift/stdlib/public/core/StringObject.swift`
- `research/benchmark/perf/string-gap-1004.md`
