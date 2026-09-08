# The float printer: measured, then chosen (#1985)

`println("${1.5}")` cost 7,783 bytes of wasm against a 1,337-byte Hello, world — 6.4 KB
for `float.to_string` alone, the largest fixed cost any Almide program could pay. The
printer was a Dragon4 transcription: exact big-integer arithmetic, five 128-limb bignums,
a per-digit loop of bignum multiply/compare/subtract. Its output — Rust's
`format!("{}", x)` (shortest round-trip, fixed notation) plus the `.0` integer suffix — is a
cross-target contract (C-023) and could not change. Issue #1985 asked for a measurement,
not a literature answer: build the alternatives in Almide, measure the emitted bytes on
the structural leg, and either ship the smaller exact printer or record Dragon4 as the
floor.

This is that measurement. Date: 2026-09-08, develop at `8517914d8` (almide 0.62.0),
structural leg, `almide build --target wasm` (the shipped WASI form, no wasm-opt).

## Result

| variant | exact? (mismatches / tested) | wasm bytes, `println("${1.5}")` | Δ vs Dragon4 | ns/conversion native | ns/conversion wasm | notes |
|---|---|---:|---:|---:|---:|---|
| Dragon4 (0.62.0, the incumbent) | 0 / 20M (its own validation) | **7,783** | — | 295 (`format!`) | 13,715 | exact bignums, no table; 47 functions |
| **Schubfach, g(k) derived at run time** (shipped) | **0 / 1,032,489** vs native `format!` | **5,508** | **−2,275 (−29 %)** | 295 (`format!`) | **1,044** | no table at all: g(k) from one mul-by-small and one div-by-small bignum loop |
| Schubfach, compact table (78 × 189-bit, every 8th k, rescaled) | 0 / 52,489 | 8,543 ¹ | +760 | — | 448 | exact for all 617 k (checked offline); 234 `Int` literals |
| Schubfach, full table (617 × 126-bit) | 0 / 37,489 | 24,372 ¹ | +16,589 | — | 583 | 1,234 `Int` literals at ~17 B each; the table is rebuilt on every call |
| Grisu3 + exact fallback | not built | ≥ fallback alone | > 0 | — | — | any exact fallback is a whole printer; Grisu3 can only add bytes on top of it (see below) |

¹ measured as `println(sf(1.5))` with the printer inlined in the probe program (the
shipped row is the real `${1.5}` with the printer in the stdlib; the inlined form of the
shipped variant measures 4,921, so the inlined table rows are ~600 B optimistic).

Exactness columns are byte-equality of the printed string. "vs native `format!`" is the
strongest oracle available: the same seeded value stream is printed by the prototype on
the wasm leg and by `float.to_string` on the native leg (which *is* Rust's `format!`),
and the two 1,032,489-line outputs are `cmp`-identical. The stream is 1,000,000 seeded
xorshift64 bit patterns over the whole 64-bit space (every exponent, including specials
and both signs) plus a 32,489-value boundary set: every biased exponent with significands
{0, 1, 2, 3, mid, max−1, max}, every 10^k reachable by repeated ×10 / ÷10 with both bit
neighbours, 0.1+0.2, 1/3, 2^53+1 — each with the sign bit clear and set. The shipped
variant was additionally run against the Dragon4 port on the wasm leg over the same
stream: 0 mismatches.

### Targeted sets (beyond uniform bit patterns)

The same wired `float.to_string` on the wasm leg against native `format!`, one program per
set run on both legs and `cmp`-compared (`research/spike/float-printer/gen_evidence.py`;
where a parser is involved the line also carries the parsed bits, so a parser divergence
would be told apart from a printer one — none occurred). The 0.62.0 release binary (the
Dragon4 printer) was run on the same programs as a second reference.

| set | tested | mismatches vs native `format!` | vs 0.62.0 Dragon4 |
|---|---:|---:|---|
| 1. exponent field 1000..1060 (~1e-7..1e10), random sign and significand | 1,000,000 | **0** | identical on the 519,151 values it printed before running out of memory (3,744 B of scratch per call) |
| 2. decimal round-trips: 1..17 significant digits as `d.dddd`, `dd.dd`, `-d.ddd`, `d.ddde±N`, `ddde±N` (N in −330..309), parsed with `float.parse`, then printed | 500,000 | **0** | identical |
| 3. every integer 0..100000; 10^k for k in −320..308; 2^k for k in −1074..1023; the 201 doubles around 1.0, 1e15, 1e16, 1e17, 1e22, 1e23; the halfway decimals between doubles at those anchors; the subnormal minimum, the max finite; 0.1/0.2/0.3/0.7/1.1/2.675/5e-324/9007199254740993 | 103,969 | **0** | identical |
| 4. `float.to_fixed` (the retained Dragon4 fixed path), random bit patterns × precision 0..10 | 100,000 | **0** | identical, byte for byte |

Set 2 is run under `wasmtime` directly: the wasm-leg `float.parse` takes ~0.3 ms per
string and the `almide run` host interrupts at 30 s. No rule change came out of these
sets; the two Java-vs-Rust departures below were both found by the boundary set. The
out-of-memory in set 1's Dragon4 reference run is the old printer's 3,744 B scratch block
per call under the 0.62.0 release; the shipped printer completes a 10,000,000-call probe
(208 B of scratch per call) on the same host.

Timing is `almide bench` (median of 3 after a warm-up) of 1,000,000 conversions of the
seeded stream — uniformly random bit patterns, so mostly huge and tiny magnitudes, the
expensive end for every printer. Native has no Almide printer: `float.to_string` is
`format!` there, so the native column is the same 295 ns for every row. The 1,044 ns is
the printer inlined in the bench program; wired into the stdlib (`float.to_string` on the
wasm leg) the same 1M-conversion bench measures 1,126 ns.

Hello, world is 1,337 B; the shipped printer's cost above it is 4,171 B, of which ~330 B
is the string renderer Dragon4 also carried (`__dg_render`), ~1,000 B is the run-time
g(k) derivation, and the rest is the Schubfach core (mulhi, rop, candidate selection) and
the runtime helpers the printer pulls in (list/string allocation, `int.to_string` glue).

## Why the table-free Schubfach is the smallest exact printer

Every shortest-round-trip algorithm is a trade between code and a power-of-ten table:

- **Dragon4**: no table, large code (the bignum digit loop is the bulk).
- **Ryū / Schubfach**: small code, a 617-entry × 128-bit table (9.9 KB raw). On this leg a
  literal `List[Int]` costs ~17 B per element and a string literal 1 B per byte, so the
  full table is 12–21 KB however it is encoded — measured at 24,372 B all in. The issue's
  prediction ("a 10 KB table is worse than 6.4 KB of code") holds.
- **Compact table**: keeping every 8th entry with enough guard bits (189) that the
  rescale by 10^0..10^7 reproduces every one of the 617 exact entries (verified offline)
  cuts the data to 1.9 KB raw — 4 KB as literals — and still lands 760 B *above* Dragon4.
- **Table derived at run time**: g(k) = ⌊10^k · 2^-r⌋ + 1 is itself a small exact
  computation. For k ≥ 0, P = 10^k by chunked ×10^9 over u32 limbs and g − 1 is P's top 126
  bits; for k < 0, P = 2^N divided by 10^-k by chunked ÷10^9 — `⌊⌊x/a⌋/b⌋ = ⌊x/(ab)⌋` on
  integers, so the chunked division is exact — leaves exactly the 126 bits of g − 1. That is
  one multiply-by-small loop, one divide-by-small loop, a set-power-of-two and a bit
  extractor: ~1 KB of code in place of 10 KB of data, and 13× faster than Dragon4 because
  the digit loop is gone (three 64×64 products and a couple of comparisons per value).
- **Grisu3** needs a fallback for the ~0.5 % of inputs it cannot certify. The fallback
  must be a complete exact printer — Dragon4, or this Schubfach — so Grisu3 is strictly
  additive in bytes and can only be a speed play. It was not built: the size question it
  would answer is already decided by the row above it, and at 1 µs/conversion on wasm the
  speed question is not open.

Two departures from Giulietti's Java were needed for byte-equality with `format!`, and
both were found by the boundary set, not by reading:

1. **Ties round the magnitude up, not to even.** When the two closest candidates are
   equally close (1669760939663944.25 — both ...4.2 and ...4.3 round-trip), Rust's Dragon4
   (`2R >= S`) picks ...4.3; Java picks the even ...4.2.
2. **The shorter multiple-of-10 candidate is tried from s ≥ 10, not s ≥ 100.** Java prints
   at least two digits, so it never needs a one-digit result. `format!` prints `1e-322`
   for 20 · 2^-1074 (s = 98, and 100 lies in the rounding interval). Likewise Java's
   `C_TINY` branch (which prints MIN_VALUE as `4.9E-324`) is absent: `format!` prints
   `5e-324`.

## What shipped

- `stdlib/float_to_string.almd`: `float.to_string` is Schubfach with the run-time g(k);
  `float.to_fixed` keeps its Dragon4 fixed-mode digit loop (a different algorithm — exactly
  N fractional digits, ties-to-even), and neither reaches the other's code, so a program
  pays only for the printer it calls. The dead shortest-mode Dragon4 functions were removed.
- `spec/stdlib/float_to_string_shortest_test.almd`: the corner cases above, pinned by bit
  pattern.
- `crates/almide-wasm/tests/golden/size-baseline*.txt`: re-ratified — 113 of the corpus
  fixtures moved, 111 down, total 1,823,014 → 1,569,846 B (−253,168 B, −13.9 %; the
  shipped-form ledger −252,348 B). The two that grew (`edge_float_formatting` +470 B,
  `float_concrete` +478 B) call both `to_string` and `to_fixed`, so they now carry two
  printers where the old pair shared one bignum core. `alloc-baseline.txt` −281,632 B in
  total (the 3,744-byte Dragon4 scratch block became 208 bytes); 38 rows moved up by
  64–80 B where the smaller block shifts the allocator's peak.
- `research/spike/float-printer/`: the prototypes (`schubfach.almd` is the shipped core
  as a standalone user program; `schubfach_rt.almd` the first shift-subtract version;
  `gen_table_variants.py` derives the two table variants), the differential harness
  (`gen_harness.py` + `harness_common.almd`: the seeded stream, the boundary set, the
  wasm-vs-native dump/diff and the `bench` programs) and the size probes. To re-run the
  1M check: `python3 gen_harness.py schubfach.almd sf_float_to_string 12345 1000000 out`,
  then `almide run out/dump_proto.almd --target wasm` and `almide run out/dump_native.almd`
  and `cmp` the two outputs.

Gates run on the change: `almide test spec/stdlib/` (161 files) and `spec/lang/` (211),
the 1M cross-target diff of the wired `float.to_string`, every `spec/wasm_cross` fixture
mentioning a float on both legs (105 fixtures, identical stdout), the size ratchet and
alloc ledger re-ratification.

## Recommendation

Ship the table-free Schubfach (done in this PR) and treat 5.5 KB as the new floor for a
program that prints one float, with ~4.2 KB of it being the printer. The remaining lever
is not the algorithm: it is reachability. `${x}` on a float pulls the whole shortest
round-trip path; a program that only prints integer-valued floats, or a fixed precision,
could reach a far smaller formatter — the #1712/#1962 split-by-need discipline applied to
a stdlib family, which is issue #1985's option 3 and is left open.
