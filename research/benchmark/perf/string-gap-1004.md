# Where the string-workload gap goes (#1004)

**Answer, in one line:** three quarters of the gap is `string.split`'s return
type (`Vec<String>` — Almide has no borrowed-string type), an eighth is
`string.len`'s character-count semantics, and everything people suspected —
the RcCow representation, the rlib crate boundary, the intermediate lists —
is together under a fifth of it. Against a Rust reference that honours the
same two obligations, the compiler's output is within **1.04–1.12x**.

Measured 2026-08-13, M4 Pro (14 cores), `--release`. Every number below comes
from a differential experiment in this directory; nothing is inferred.

---

## The workload and the two references

`strchurn/strchurn.almd` is the #1004 workload verbatim: N ints →
`int.to_string` → `string.join` → `string.split` → `string.len` → `list.sum`.

It has **two** Rust references, and keeping them apart is what makes the
attribution possible:

| Reference | What it does | Role |
|---|---|---|
| `rust-ref/strchurn.rs` | `split` collects owned `String`s; length is `chars().count()` | same-shape / same-semantics — **the comparison that means something** |
| `rust-ref/strchurn_idiomatic.rs` | `split` yields borrowed `&str`; length is `s.len()` | what a person writes — the API contract's price |

The row is REPORTED by `check-perf-ratio.sh`, not anchored, on the policy that
commit 95b9b6d59 established for the `listbuild` family: a workload whose cost
is allocation compares allocators before it compares codegen, and its
almide/rust ratio does not cancel the machine (listbuild reads 1.58 on an
M4 Pro and 0.91 on the ubuntu-latest runner from one commit). Unlike listbuild
both sides here allocate identically, so this row may well turn out to be
anchorable — that wants a second architecture's reading first.

Both obligations are real, not handicaps invented for the benchmark:
`almide_rt_string_split` is `s.split(sep).map(|x| x.to_string()).collect()`
because Almide's `List[String]` element type is an owned string, and
`almide_rt_string_len` is `s.chars().count()` because `string.len` is defined
as a character count. The workload is pure ASCII, so all three programs print
byte-identical output and `bench.py` verifies that before timing.

## Headline

At N=4M, min of 21 interleaved runs, median of three such batches:

| Binary | ms | vs shipped |
|---|---:|---:|
| `almide build --release` (the shipped rlib path) | 228.0 | 1.00 |
| same program, monolithic build (`ALMIDE_NO_RTLIB=1`) | 218.0 | 0.96 |
| same-shape Rust reference | 205.3 | 0.90 |
| idiomatic Rust reference | 111.5 | 0.49 |

**2.04x vs the Rust a person writes. 1.11x vs the Rust that does the same
work.** The issue's original 1.7x reproduces exactly at its own N=1M
(89.7ms vs 53.8ms = 1.67x) — the issue's "hand-written Rust" was the
idiomatic one.

## The gap, attributed

The method is a **ladder**, and it ships as `strchurn/ladder.py`: take the
compiler's own `--target rust` output and replace ONE cost in `__almide_main`
per rung, leaving every runtime function byte-identical, then compile all
rungs with the exact rustc flags the references use. The ladder is closed at
both ends — the `v3_fused_sum` rung lands on the same-shape reference
(211.7 vs 205.3ms) and the last rung lands on the idiomatic reference
(111.5 vs 111.5ms) — so the deltas account for the whole gap, not for a story
about part of it.

| # | Cost removed | ms | share |
|---|---|---:|---:|
| 1 | `string.split` returns N **owned `String`s** (malloc + memcpy + free per piece, and a 3x fatter result container) | 69.4 | **59.6%** |
| 2 | the split result **container itself** (`collect()` with no size hint → doubling growth) | 17.6 | **15.1%** |
| 3 | `string.len` = `chars().count()` — an O(n) UTF-8 scan, no ASCII fast path | 13.2 | **11.3%** |
| 4 | the **rlib crate boundary** (runtime not inlinable into user code) | 11.0 | **9.4%** |
| 5 | the three list-pipeline intermediates, **combined** — `Rc<dyn Fn>` per element, `list.range`'s `Vec<i64>`, and the un-fused `list.map`→`list.sum` `Vec<i64>` | 5.3 | 4.6% |
| 6 | residual codegen (last rung vs the idiomatic reference) | 0.0 | 0% |
| | **total** | **116.5** | **100%** |

Rows 1+2 are one root cause — `string.split`'s return type — and together are
**74.7%** of the gap.

Row 5 is deliberately reported as one bucket. Rows 1–4 reproduce to within a
few percent across batches; the three rungs inside row 5 do not. Across four
batches their individual deltas read 6.4 / −2.7 / 1.6 ms, then 2.6 / 11.2 /
−8.3 ms — i.e. each is at or below the ±3–8 ms noise floor of a shared box,
while their SUM is stable at ~5 ms. Splitting a 5 ms bucket three ways at this
N would be arithmetic, not measurement. The place where those costs *are*
measurable is finding (a) below, where they are worth 6.6x.

## The counter-hypothesis the issue names: REFUTED

> "`ALMIDE_NO_RTLIB=1` (monolithic LTO build) does NOT close the gap — same
> 0.05s. So this is the runtime value representation."

The first clause is **confirmed and sharpened**; the second is **wrong**.

- Closing the crate boundary moves the shipped binary 228.0 → 218.0 ms while
  the idiomatic reference sits at 111.5. So the boundary is worth **4.4% of
  runtime / 9.4% of the gap** — real (the `cargo_build.rs` comment's "2–5%,
  up to ~10% on string/list-heavy loops" estimate is accurate), and nowhere
  near an explanation. The issue's original test could not see 4% at 10 ms
  timer resolution; it reported "no effect" and the conclusion drawn from it
  was too strong in both directions.
- The representation named in the issue title is **not involved at all**.
  `RcCow<T>` is used for `Bytes` and `Matrix` only
  (`codegen/templates/rust.toml` `[type_bytes]` / `[type_matrix]`); Almide's
  `String` lowers to Rust's `String`, one word for one word. There is no
  refcount traffic on this path — grep the emitted `__almide_main` for
  `RcCow` and it does not appear. **The issue title's premise is false.** The
  cost is not how a string is *represented*; it is how many strings the
  stdlib's *signatures* force into existence.

## Two micro-optimizations, both measured, both rejected

Recorded so nobody re-derives them:

- **Reserve capacity in `almide_rt_string_split`.** Killing the doubling
  growth (row 2) costs one counting pass. Measured at 4M pieces:
  current `collect()` **99.4 ms**, `Vec::with_capacity(matches.count()+1)` +
  `extend` **115.1 ms**, and the counting pass alone **25.6 ms**. The pass
  costs more than the growth it removes. **Net loss — do not land.**
- **ASCII fast path in `almide_rt_string_len`**
  (`if s.is_ascii() { s.len() } else { s.chars().count() }`, exact in general).
  Over 4M short strings: `chars().count()` **10.7 ms** → fast path **8.4 ms**.
  A 2.3 ms win = **2% of the gap**, in exchange for a second scan shape in a
  hot primitive. Parked on the work list as a rounding-error item.

## Two findings outside the string row

**(a) `IterChain` never fires on the Rust target.** The pipeline's fused
iterator node is dead: `list.map` / `filter` / `fold`, in a pipe, in a direct
call, and over a `list.range` source, all emit `almide_rt_list_*(Vec, Rc<dyn
Fn>)` over a materialized `Vec`. No `.into_iter().map(…)` appears in the
emitted `__almide_main` of any benchmark in this suite. What DOES work is the
egg pass's combinator fusion: `range |> map |> fold` becomes a single
`almide_rt_list_fold` with the map composed into the reducer. So the fusion
that #1004's background credits is real at the *combinator* level and absent
at the *iterator* level. The comment at
`crates/almide-codegen/src/pass_stdlib_lowering.rs:426` describing
`try_lower_to_iter_chain` as "the fallback for single-call shape" is stale —
it never fires, because `ResolveCallsPass` has already rewritten the
`CallTarget::Module` it matches on.

What that is worth, on a cheap lambda body where no allocation hides it
(30M elements, model of the emitted shapes, min of 5):

| Shape | ms |
|---|---:|
| `map` + `sum`, `Rc<dyn Fn>`, two materialized `Vec`s (emitted) | 36.1 |
| same, monomorphic closure | 37.9 |
| `range` → `map` → `fold` after egg fusion — one materialized `Vec` | 31.5 |
| fully fused Rust iterator chain | **4.8** |

**6.6x, and the boxed closure is not the reason** — it is worth ~0 here. The
whole cost is that `list.range` is a real 240 MB `Vec` that gets written and
read back instead of an `impl Iterator`.

**(b) The `listbuild` rows' ~1.6x is NOT #1004.** `perf/README.md` and
`scripts/perf-ratio-baseline.txt` both said the residual materialization cost
in the three listbuild rows was "the materialization cost #1004 tracks". It is
not, and it is not materialization. In the emitted Rust, swapping ONLY
`almide_rt_libm_sin`/`_cos` — the deterministic **software libm** the
cross-target byte-identity contract requires — for the platform ones:

| Binary (2^23) | ms |
|---|---:|
| emitted, as shipped | 194.5 |
| emitted, platform `sin`/`cos` | **105.5** |
| handwritten Rust reference (uses platform `sin`/`cos`) | 123.9 |

The row's entire 1.57x is transcendental determinism, and with the platform
libm the compiler's output **beats** the handwritten reference by 1.17x. The
suspects that were being blamed are innocent: a same-shape A/B puts
`list.repeat` zero-fill + bounds-checked indexed writes at **75.5 ms** against
`Vec::with_capacity` + push at **90.6 ms** — the emitted shape is *faster* —
and `ALMIDE_NO_RTLIB=1` moves the row by 0.6 ms. Both comments are corrected
in this change.

## Verdict and work list

**Accept the string number, publish it with the decomposition.** 1.11x against
same-work Rust is not embarrassing; 1.9x against `&str` Rust is the price of
an owned-string stdlib, and that is a language design decision to argue on its
own merits, not a codegen defect to fix quietly.

Named next action per bucket, largest first:

| Bucket | Share | Next action |
|---|---:|---|
| `split` → owned `String`s (rows 1+2) | 75% | A borrowed/slice string type, or a `string.split_iter`-style consumer-fused form, is the only thing that moves this. Language-level; needs its own issue and an MSR argument (a second string type is a writability cost). Until then it is a documented, accepted contract. |
| `string.len` = `chars().count()` | 11% | Leave the semantics. Optional 2% ASCII fast path (measured above) if a profile ever makes it matter. |
| rlib crate boundary | 9% | `#[inline]` on the hot non-generic runtime fns (`string_len`, `int_to_string`, `string_split`) is the standing plan in `cargo_build.rs`; it must be landed against `check-embedded-size.sh`, which is why it has not been. Bounded, real, ~4% of runtime. |
| `Rc<dyn Fn>` per element | 5.5% | Not worth a pass on its own — measured at ~0 on a cheap body. Falls out for free if the item below lands. |
| list intermediates (rows 6+7, and finding (a)) | ~1.5% here, **6.6x elsewhere** | **The biggest real win in this document, and it is not on the string row.** Revive `IterChain` (or lower `list.range`+combinators to a lazy Rust iterator) so combinator chains stop materializing. Needs its own issue; `pass_stdlib_lowering.rs:426`'s stale comment should die with it. |
| listbuild's 1.6x (finding (b)) | — | Not a #1004 bucket. If the software libm's cost is ever judged too high, the lever is a native-only fast path guarded by the cross-target contract — a contract conversation, not a codegen one. |

## Reproducing

```bash
cargo build --release

# the row, on the harness (native vs both references)
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/bench.py --legs native,rust --bench strchurn --runs 7

# the attribution table above, rebuilt from scratch
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/strchurn/ladder.py --runs 21

# the ratchet (prints strchurn's ratio in the reported, un-anchored group)
bash scripts/check-perf-ratio.sh
```

`ladder.py` emits the program, patches one cost out per rung, verifies that
every rung still prints identical output, times them interleaved, and prints
the deltas with their shares. Run it three times: rows 1–4 of the table
reproduce, row 5's internal split does not (see the note under the table). It
also prints how far the two closing rungs land from their references — if
those are not near zero, the deltas no longer account for the gap and the
table has stopped being a measurement.

To reproduce the rlib-boundary row, build the same program both ways into
fresh project dirs (the build cache is keyed on the generated Rust, *not* on
`ALMIDE_NO_RTLIB`, so reusing one dir silently hands back the other build's
binary):

```bash
ALMIDE_RUN_PROJECT_DIR=$PWD/.a  almide build .../strchurn.almd --release -o /tmp/rlib
ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR=$PWD/.b \
  almide build .../strchurn.almd --release -o /tmp/mono
```
