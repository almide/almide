# ADR-0013: native String as a shared buffer — implementation

**Status:** stage 1 landed (#3715, 2026-10-15). Stage 2 in progress:
- landed: step 1 (the alias, #3716), the type (#3717) and the literals (#3719);
- on branch `adr0013-stage2`: the `ALMIDE_SHARED_STR` flag, steps 3–4 for the walker and the runtime, and the step 5 alias fixture.
**Decision:** [ADR-0013](../../adr/0013-string-shared-immutable-buffer.md) (accepted 2026-09-04).
**Owner ruling 2026-10-15:** implement this right after the stdlib inspection's individual fixes. Do not change borrow inference for conditionally consumed parameters instead.

## Why now

The stdlib inspection measured `result.filter` at O(n) per call on native, and O(1) on wasm. The call passes a live `String` to a parameter only the failing branch consumes, and native renders `x.clone()` at the call site.

We ran the same shape in six languages (macOS arm64, µs per call at n = 20,000 → 320,000):

| Model | Per call | Grows with n |
|---|---|---|
| Rust, owned parameter + caller clone (Almide native today) | 0.22 → 4.25 | yes (19x) |
| Rust, borrowed + clone on the consuming branch | 0.000 → 0.000 | only on that branch |
| Rust `Rc<str>`; Swift; Go; Python; Node; Ruby | flat | no |

Only the owned-copy model pays for the argument's size. The wasm leg already shares (rc blocks). This is ADR-0013's finding, met again from a different direction.

## Stage 1: the ratchet (landed)

The falsifier: stage 2 is reverted if strchurn or fasta gets **5% or more** slower and the thread-shared variant does not recover it. That is only decidable with a measurement whose noise is well under 5%.

CI's `check-perf-ratio.sh` reports strchurn and decode without anchoring them, because their ratio depends on the architecture. So the falsifier is judged on one fixed machine instead:

- `research/benchmark/perf/gce_ab.sh <ref-a> <ref-b> <out>` builds both refs on one single-use GCE VM. It then runs `bench.py` (full workloads, native + rust legs) alternately A, B, A, B over the same bench sources.
- `research/benchmark/perf/ab_summary.py <out>/res` summarises the run. Per bench it gives the median native time, the median almide/rust ratio of each side, B against A, and the spread of the ratio over all rounds.

**Noise floor:** develop 0d93660ea as both A and B. GCE c4-standard-4 (Intel Xeon Platinum 8581C, 4 vCPU), Ubuntu 24.04, Linux 7.0.0-1014-gcp, rustc 1.99.0. 3 rounds × 5 runs, medians, 2026-10-15.

| Bench | Ratio to the handwritten Rust | B vs A (same build) | Spread over 6 rounds |
|---|---:|---:|---:|
| fasta | 1.164 | +0.07% | 1.18% |
| strchurn (same-shape reference) | 1.126 | −0.67% | 1.38% |
| decode | 0.783 | +2.03% | 3.11% |

- The falsifier rows (fasta, strchurn) repeat within about 1.4%, so a 5% regression is clearly visible.
- decode is noisier (about 3%). It is reported, not used as a falsifier, which matches the ADR.
- These are the stage 2 baseline numbers.

## Stage 2: the representation (next)

Inventory at develop 0d93660ea:

| Surface | Count |
|---|---:|
| `pub fn almide_rt_*` in `runtime/rs/src` | 809 |
| … returning `String` (or `Result<String, …>`) | 61 |
| … taking `String` by value | 17 |
| `Vec<String>` mentions in the runtime | 32 |
| codegen / MIR sites that spell the `String` type | 17 |

Open design points, to settle before writing code:

1. **The type.** First measurement: a Rust microbenchmark, macOS arm64, best of 5, 2026-10-15. Prototypes are in the session scratchpad; re-measure on GCE before choosing.

   | Operation | `String` (today) | `Rc<str>` | `Rc<String>` | One block `[rc, len, cap \| bytes]` |
   |---|---:|---:|---:|---:|
   | Build 2M short strings | 28.1 ms | 34.5 (+21%) | 56.7 (+100%) | 31.0 (+10%) |
   | Share a 100 KB string 20k times | 25.3 ms | 0.00 | 0.00 | 0.00 |
   | Push 2M chars into a unique string | 0.72 ms | quadratic (no capacity) | 1.47 (2×) | 3.18 (4.4×) |
   | Equality scan over 2M | 0.57 ms | 0.51 | 1.05 | 0.73 |

   - **`Rc<str>`** has no capacity, so `string.push` would rebuild the string on every call. It is out.
   - **`Rc<String>`** pays two allocations and a second indirection.
   - **The single block** is the wasm leg's layout. It wins on building and sharing. Its unique push is 4.4× slower in this unoptimized prototype, which checks the count on every push with no inlined fast path. That number must come down to within strbuild's ratchet before stage 2 can land.
   - **The push gap is aliasing, not the count check.** The byte store goes through a pointer the compiler cannot separate from the header, so `rc`, `len` and `cap` reload on every push. Inlining the fast path did not help (3.21 ms).
   - A **unique view** fixes it: the count is checked once, then len and cap are kept in locals and written back when the view ends. That reaches **0.76 ms**, against String's 0.72.
   - So the single block can match `String` everywhere, provided codegen takes one unique view for a run of pushes on the same `var`. The in-place append window for `var s` is already that run.
2. **The `mut` surface.** `string.push` / `string.clear` use RcCow (`Rc::make_mut` semantics): in place when unique, copy when shared. This is the same rule List / Map already follow.
3. **The fan boundary.** Use the Koka thread-shared flip, so only values captured into a scoped thread become atomic. The stage 2 ratchet is the judge: a strchurn regression from the flip stops the stage.
4. **The ABI.** Runtime functions keep taking `&str`, which works through `Deref`. Functions that return `String` return the shared type, converted once at their boundary.

Stage 3 (seamless slices for `split` / `slice` / `trim` / `lines`) follows stage 2. Stage 4 (inline small strings) is out of scope here.

## Stage 2: the migration plan

The surface was mapped at develop 0d93660ea; file:line references are from that commit.

**Two native renderers, chosen per program.**
- `src/cli/mod.rs:240` (`render_v1_native_or_fallback`) tries MIR v1 for the whole program. On any wall it falls back to the v0 walker.
  - v1 walls on multi-module programs, top-level lets, heap tail recursion and Codec / `Value` programs, so the walker renders most real programs.
  - `almide test` and `install` always use v0.
- Metered programs can **splice** v1 regions into v0 code (`mod.rs:323-328`). There, both renderers' String representations must be the same type.
- v1 output does not include `runtime/rs` or the codegen prelude. It has its own header (`render_native.rs:229`) and shims (`render_native_shims.rs`).

**Steps.** Each step lands on its own and keeps every gate green.

1. **The alias, with no behaviour change.**
   - `type AlmideString = String;` goes in `rust_runtime_prelude` (`crates/almide-codegen/src/lib.rs:343`) and once in the v1 header. The metered splice must end up with exactly one definition.
   - Every site that spells the Almide String type emits the alias:
     - walker: `walker/types.rs:138`, `templates/rust.toml:242`, the error-type defaults (`types.rs:172`, `expressions_calls_binops.rs:549`);
     - v1: `NTy::Str` renderings in `render_native*.rs`.
   - The runtime compiles unchanged because the alias is transparent.
   - Gate: a test that greps the generated Rust of the spec corpus for a bare `String` in a type position and finds none.
2. **The type, behind a flag.**
   - `AlmideStr`: the single block `[rc, len, cap | bytes]`, with `Deref<Target = str>`, `From<String>`, `From<&str>`, `Display`, `PartialEq` / `Ord` / `Hash` by bytes, `Clone` = count + 1, and `unique()` returning the view measured above.
   - Unit tests plus a Miri run over the type alone.
   - The flag `ALMIDE_SHARED_STR=1` flips the alias. Default off.
3. **The construction sites.**
   - Literals become `AlmideStr::from("..")`, or better a static immortal block, as Swift does with literals.
   - Concatenation goes through `AlmideConcat` for `AlmideStr`, using the unique view when the left operand is unique.
   - `format!` results are converted at the boundary with `From<String>`.
   - Error payloads (`Result<T, AlmideStr>`).
4. **The runtime ABI.**
   - `&str` parameters (140) keep working through `Deref`. Returns (`-> String`, 51; `Vec<String>`, 16; `Option<String>`, 13; `Result<_, String>`, 124) convert at the function's boundary.
   - The `&mut String` four (`string.push` / `clear`, `value.rs:411`) take `&mut AlmideStr` and use `unique()`.
   - `buildscript/runtime_registry.rs:314` parses modes from the runtime's source text, so its patterns must learn the new spelling.
5. **The in-place windows. This is a correctness step, not a speed step.**
   - MIR v1's append window (`render_native_append.rs:28-45`) renders `s = __str_concat(s, x)` as `s.push_str(x)`. Its stated soundness argument is "every `Dup` is a deep `.clone()`" (its comment, lines 15-20). Under a shared buffer that is false, and the window must go through `unique()`.
   - The walker's `AlmideConcat<String> for String` (`lib.rs:348-353`) has the same obligation.
   - Gate: an alias fixture on both legs, in which `let t = s; s = s + "x"` must leave `t` unchanged.
6. **`AlmideValue::Str`** (`value.rs:18`) becomes `Str(AlmideStr)`. This is ADR-0018 / #1679's stage.
7. **Flip the default.** Run `gce_ab.sh` against the stage 1 baseline: strchurn and fasta must not be 5% or more slower. Then the alias flag goes away.

**What keeps observable behaviour unchanged:**
- Every step runs the full spec corpus on both legs, plus the cross-target contracts.
- Steps 2–6 are behind the flag until step 7, so a regression is a flag flip away from the old representation.

**The step 3–4 ratchet (measured 2026-10-15).**
- **The census:** with the alias pointed at `AlmideStr` (probe flag), emit every `spec/lang` and `spec/stdlib` file (463), compile each with `rustc --test`, and count the errors.
  - With the flag off, the same harness gives 0 errors, except 10 http files whose standalone rustc cannot link rustls. Those are environmental and excluded from both counts.
- **The migration rule:** every construction site moves to a spelling that is the identity while the alias is `String` and a conversion once it is `AlmideStr`. That is `AlmideString::from(..)` / `.into()`. Behaviour with the flag off cannot change, and the census can only fall.

| State | Errors with the flag on |
|---|---:|
| alias only (#3716) | 8,924 |
| + literals as `AlmideString::from("..")` | 7,326 |
| + `int.to_string` / `float.to_string` returning `AlmideString` (probe) | 3,657 |

From here on the census also counts **distinct sites** (an error inside the runtime repeats in all 463 files):

| State (probe, flag on) | Errors | Distinct sites |
|---|---:|---:|
| literals + `AlmideConcat` impls for `AlmideStr` | 6,664 | 1,421 |
| + 46 runtime `-> String` functions return `AlmideString` | 2,631 | 1,193 |
| + 137 runtime `Result<_, String>` signatures spell `AlmideString` | 9,501 | 1,137 |

The raw count rose at the last step because some errors moved into the runtime itself, where each one repeats per file. Distinct sites fell. Flag off, every step stays at 0.

**The flag is real from here on (2026-10-17).** `ALMIDE_SHARED_STR=1` is a registered switch: the prelude (`crates/almide-codegen/src/prelude_string.rs`) splices `almide-str`'s source in as `mod almide_str_block` and points the alias at `AlmideStr`. So the census no longer needs a probe edit, and the harness changed with it:
- Emit with `almide <file> --target rust`, type-check with `rustc --test --emit=metadata`, over the 463 files minus the 10 that need rustls / flate2 (453).
- A distinct site is (error code, message, offending source line).
- The flag-on prelude carries the `AlmideConcat` impls for `AlmideStr` (through `AlmideStr::concat`, which appends in place only when unique) and a structural `into_almide` (String converts; `Vec` / `Option` / `Result` / tuples / `HashMap` / `AlmideMap` map their elements; scalars and `AlmideValue` are the identity).
- The new harness re-measures the starting point, so its rows are not comparable with the rows above.

| State (flag on, this harness) | Errors | Distinct sites | Files with errors |
|---|---:|---:|---:|
| start: the alias, literals, runtime returns and `Result` errors as landed | 15,643 | 724 | 453 |
| + runtime internals through the boundary (`fan`, `fs`, `value`, `json`, `map`, `list`, `error`, `process_core`) | 959 | 681 | 187 |
| + construction sites: `to_almide()` for a String copy, `AlmideString::from(format!(..))` for interpolation, error payloads, `ok_or`, the decode frame key | 115 | 96 | 18 |
| + the unprefixed runtime returns (`almide_regex_*`, `almide_json_*`, `ProcessStatus` fields) | 61 | 44 | 14 |

All 61 remaining errors are the thread boundary: `AlmideStr` is not `Send`/`Sync`, and it reaches a `fan` block, `fan.map_par`, `fs.fold_lines_chunked`, or a top-level `let` (a `static LazyLock`). That is design point 3 (the Koka thread-shared flip), not a construction site.

| + design point 3: the thread-shared mark at every thread boundary | 0 | 0 | 0 |

**Design point 3 (the thread boundary).** This is Koka's thread-shared flip (`kklib.h`: a negative count is atomic; `kk_block_mark_shared` before a task starts).
- `AlmideStr`'s count carries an `ALMIDE_STR_SHARED` bit. A marked block counts with atomic read-modify-writes, and its last drop pairs `Release` with an `Acquire` fence. An unmarked block uses `Relaxed` loads and stores, which are plain moves. The unique check loads with `Acquire`.
- `almide_str_mark_reachable(&v)` marks everything `v` reaches. It clones `v` inside a marking window, a global counter that makes each clone set the bit. So no per-type walker is needed: every Almide value is `Clone`, as Koka's walk covers every block.
- The compiler marks at every boundary it emits, and nowhere else:
  - the free variables of a threaded `fan` block's arms;
  - the free variables of the arguments to a runtime helper whose signature bounds `Send` (`runtime_fn_modes::crosses_threads`, generated from the signatures);
  - the data those helpers deal to their workers (`fan.map_par` items, `list.par_*`, the `fold_lines_chunked` init);
  - a top-level `let`'s value, which a `static LazyLock` hands to every thread.
- `unsafe impl Send / Sync for AlmideStr` rests on that marking. An unmarked block is never reachable from two threads.
- Only values that cross a boundary turn atomic. A clone made on another thread during a marking window also marks its block, which only costs speed.
- Off the flag, `almide_share()` is an empty default method: the runtime's calls compile to nothing, and the compiler emits no marks.

**Flag on, the corpus runs.** The same 463 files built with `rustc --test` at opt-level 0 and run natively: 438 pass with the flag on and 452 with it off. The difference is the 14 thread-boundary files. With design point 3 in place (464 files, `spec/stdlib/thread_shared_string_test.almd` added), 453 pass on both sides and the non-passing set is the same 11 files. The 10 rustls / flate2 files do not link in a standalone rustc, and one deep-recursion test overflows the default test-thread stack at opt-level 0 in this harness (on both sides).

**Step 5 (`let t = s; s = s + "x"`).** `spec/lang/alias_cow_test.almd` pins it, straight and in a loop, and it passes on wasm and natively with the flag on and off. Under the shared buffer the walker's `+` goes through `AlmideStr::concat`, and the v1 append window's `s.push_str(x)` through `AlmideStr::push_str`. Both take the unique view, which copies a shared block before it writes.

**The v1 renderer and http follow.**
- The v1 header splices the same block under the flag (`crates/almide-mir/src/render_native_str.rs`). Its string shims and literals build `AlmideString`.
- `http.rs` and the SSE client return `AlmideString`, and take header maps and chunk callbacks in it.
- The census cannot see either: v1 is not the `--target rust` route, and http needs rustls.
  - For http: the 10 excluded files were built through `almide build` (the cargo route, with rustls and flate2) with the flag on, and all 10 compile.
  - For v1 there is no corpus measurement yet. One program that exercises the append window and every string shim renders through v1 with the flag on and prints the same bytes as with it off.
  - **The v1 census.** `probe_native` (the v1 renderer as an example binary) was run over 543 programs: `spec/`, `examples/`, `research/benchmark/perf` and `exercises`.
    - Only 7 render through v1; the other 536 wall to the walker, for example on `env.args`.
    - With the flag on, all 7 type-check and run with the same stdout and exit code as with it off.

**Not covered yet:**
- Miri over the new atomic path (`crates/almide-str/tests/almide_str_threads_test.rs`) runs only in CI (`miri-almide-str`); this machine has no rustup toolchain for Miri;
- the default flip (step 7), which waits for the GCE A/B;
- speed under the flag: `&str + AlmideStr` and a v1 shim result copy once more than they need to, and `to_almide()` copies where a count increment would do. Step 7's `gce_ab.sh` run is where that is judged.

**Step 6 (`AlmideValue::Str`).** The payload is now `Str(AlmideString)`, so under the flag a Value shares the string block it was built from; ADR-0018 rules ○ for `AlmideStr`.
- Object keys stay `AlmideKey` (`Cow<'static, str>`, interned).
- Nine runtime lines changed spelling, each the identity while the alias is `String`.
- Flag off, the generated user code is byte-identical over all 463 census files (compared after `RT_BOUNDARY_MARKER`). The census stays 0 errors with the flag on and off.
- `spec/stdlib/value_string_payload_test.almd` pins the payload paths: `value.str`, a push on the source, parse/stringify, a derived Codec round trip, and many Values sharing one string. It passes on wasm and natively with the flag on and off.
- Native corpus (465 files): 454 pass with the flag on and with it off, with the same 11 non-passing files.

**The runtime ABI steps void inspection stamps.** Changing a runtime function's text changes its implementation hash, and `tests/stdlib_inspection_test.rs` then requires re-inspection. The `-> String` step alone voided 38 stamped functions. So the ABI steps (returns, `Result`, `Vec`, `Option`) are collected on one branch and re-inspected once on GCE before they are proposed.
