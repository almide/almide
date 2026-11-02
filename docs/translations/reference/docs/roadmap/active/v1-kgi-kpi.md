Description: Historical June 2026 KGI/KPI scoreboard; targets distinguished from measured or proven results.
<a id="v1-kgi--kpi-スコアボード"></a>

# v1 KGI / KPI Scoreboard

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-kgi-kpi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-kgi-kpi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-kgi-kpi.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)

> Source caveat: the filled June 2026 entries and their later corrective framing are historical. Current/safe/perfect/100%-sound wording inside the old scoreboard is preserved source rhetoric, not a new universal assurance. Whole-production safety, a few-hundred-line qualified checker, external qualification and most-accurate machine editing are targets, not demonstrated current achievements. The stated 4/8 labels and safety-bundle enumeration are retained without reconciling their counting; leak-free loop tests do not add a Perceus theorem, pattern checks are not execution, and no-print exit/output agreement leaves returned values unobserved. Preserve the same-day 'not started' entries alongside later slice results, changed corpus counts, incomplete hole-hunt/claim-drift checks and the claim that a leaking commit was fixed before shipment. The original STATUS 6 link and wiki-style references are unresolved source notation; no destination was fabricated. The weekly template is displayed as inert documentation. No shell command, self-host construction, proof check or measurement was executed.

<a id="historical-scoreboard-targets-are-not-achieved-guarantees"></a>

## Historical scoreboard; targets are not achieved guarantees

The latest filled weekly entry below is June 21, 2026. Undated “current” columns are that early scoreboard, not measurements of today's compiler, TS support, proof coverage or MSR. KGI-T's complete safety/qualification bundle is a target, not a claim already established by a green corpus gate.

Use [the current proof boundary](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md) and [receipt generator](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/receipt.sh) for scoped evidence. MSR work belongs to Dojo per [the repository boundary](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md); this update does not invent external results or fill the old counters with estimates. The weekly observations and their original metrics remain below.

> **What this is**: a scoreboard filled weekly with v1's ultimate indicators (KGI) and the indicators that defend/advance them (KPI). This project's structure is unusual: **KPIs split into “offensive” (grow) and “defensive” (invariants that must never degrade); breaking the defensive set instantly invalidates offensive gains**. So inspect defense first.
> **Related**: [v1-system-map](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-system-map.md) / [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) / [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) (L0-L4) / [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md).

<a id="守るものの本質一行"></a>

## The essence of what to defend (one line)

> **Keep what must be trusted small and pure.** Keep the checker at a few hundred lines, the trusted base at its declared floor. If this collapses, adding any number of properties cannot reach flight grade.

---

<a id="kgi終局指標--これが真なら勝った"></a>

## KGI (ultimate indicators—if true, we have “won”)

Victory is a **conjunction**. Either half alone is a known failure mode (trust alone = CompCert niche / writability alone = just another AI language).

| KGI | Achieved state (what must be true) | Current |
|---|---|---|
| **KGI-T (trust)** | A third party trusts **only the irreducible floor (Coq kernel / wasm semantics / faithful wasmtime / HW / ALS validity)** and **re-derives on every build** the [safety bundle] for **all programs used in real production** with a **qualified checker of a few hundred lines**. External qualification (adjacent markets → aviation) | ⬜ Slices only |
| **KGI-W (writability)** | Under equal conditions with a control group, machines can **modify Almide and write it more accurately than other languages** (win controlled MSR) | ⬜ Unmeasured |
| **Conjunction (true victory)** | The above 2 are true **simultaneously**—machines write it most accurately and the output is trustworthy at flight grade | ⬜ |

**[Safety bundle]** = memory safety / no leaks / bounded capabilities / types established / calling-convention conformance / bytes refine source semantics (C-FAITHFUL) / byte reproducibility (C-REPRO).

---

<a id="守る系-kpi不変条件--1つでも破れたら攻める成果は無効"></a>

## Defensive KPIs (invariants—break even one and offensive results are invalid)

**The target is “always = this value / ≤ this limit.” Do not grow it; prevent degradation.**

| Defensive KPI | Value to maintain | Current |
|---|---|---|
| **Checker size** | ≤ a few hundred lines, **independent of program / language / compiler complexity** (size ∝ #event letters + #subset properties + #op→pattern table) | ✅ |
| **Trusted base (TB)** | = declared floor. **Zero creep** (ledger bounds even added imports) | ✅ TRUSTED_BASE.md maintained |
| **Axiom purity** | All theorems' `Print Assumptions` ⊆ standard; zero trust extensions (native_decide etc.) | ✅ "Closed under the global context" |
| **Independent proof checking** | kernel-checked + coqchk + cross-version (0 sorry / 0 Axiom) | ✅ 9.1.1 + 9.2 |
| **Claim drift** | = 0 (public claims ⊆ machine checking) | ⚠️ Machine checking not yet implemented (1 case manually detected and fixed) |
| **Silent acceptance** | = 0 (acceptance ⟹ safe or `[COMPILER BUG]` stop; all shortcuts machine-gated) | 🔄 hole-hunt not yet dry |

**Meaning of this table**: buying offensive gains by breaking defensive KPIs is not progress but a **net loss** (if the checker grows to thousands of lines, even covering the whole language loses the qualification object). This expresses the “getting the structure right” contest in KPIs—**progress that breaks an invariant is worse than no progress**.

---

<a id="攻める系-kpi進捗--kgi-へ向けて伸ばす"></a>

## Offensive KPIs (progress—grow toward KGI)

| Offensive KPI | 0 ──────────→ Complete | Current | gap / C-* |
|---|---|---|---|
| **Property coverage** | How many / 8 of the safety bundle | **4 / 8** (mem✅ name✅ cap✅ type✅ / leak, reuse, call-mode, byte remain) | C-SAFE / C-PROVEN |
| **Certificate-format expressiveness** | eager → perceus → full | **eager** (i/a/d/m legal, r/b rejected) | Cross-cutting (format v1) |
| **Real-program coverage** (③ EXECUTE and match v0 bytes) | Real repos/functions executed end-to-end and matching v0 | **2 real repos fully conquered** (`csv` 4/4 + `svg` records render) + many fixtures—all byte-identical + leak-free (10⁴ loop) + corpus-wall ACCEPT | Gap 3 |
| **Language-surface coverage** (③ execution path) | subset → call → control flow → closure → nested heap → full language | **call / control flow / heap-result if/match / closure (defunc inline) / nested-heap list / records (construction, field, spread, recursive drop, List[Record] literal+concat) / `Map[String,String]` entries**—real repos EXECUTE. Remaining: Set / guards / general Map (non-String values) / TS-target | Gap 3 |
| **Byte binding** | §3 contract (trusted) → proven | **Still trusted** | **Gap 1 (core challenge)** |
| **frees / leak-freedom** | eager (known negative for leaks) → frees (leak-freedom proven) | **eager** | Gap 2 |
| **Controlled MSR** | Unmeasured → difference from control | **Unmeasured** | Pillar A |
| **External** | Local → CI-enforced → diversity, 2 implementations → CertiCoq machine code → adjacent qualification → aviation | **CI-enforced ✅** | KGI-T |

<a id="gap--攻める-kpi-対応"></a>

### Gap ↔ offensive KPI mapping

- **Gap 1** (most important, deepest): byte binding—move witness ⟹ wasm bytes from the §3 contract to proven. WasmCert-Coq import + runtime heap refinement proof + per-build V'.
- **Gap 2**: frees / leak-freedom / reuse—bring a precise ownership model (Perceus per-edge) to MIR. Certificate-format a/m/r letters are the entry to grounding it in facts.
- **Gap 3** (= ③ execution parity, next main goal)—**scope corrected by measurements on 2026-06-14**:
  The verification chain (frontend → MIR → proof checker) **already handles both calls and control flow** (measured ACCEPT: `helper()` call `im` / `if-then-else` `im` / `match Option` `ad|im` / `for` accumulation `ad`; gate.sh already has two_functions/transitive_caps). `render_wasm`/`render_rust` also handle MIR ops (including CallFn), while V (translation_validation #570) **checks patterns** for the presence of op→wasm instructions. **The stale “subset, no calls” statement does not apply to verification**—verification can verify **ownership** of call/control-flow programs (calls remain deferred/elided in the cert).
  **Thus ③'s true frontier is “v1 faithfully EXECUTES real programs and matches v0 bytes”**: V currently matches patterns without executing; execution uses v0 (dual-oracle reference, receipt C-REPRO “until v1 parity”). Lowering defers calls for certificates, so **execution needs an EXECUTION path that un-defers calls (faithful calls + real results)**. ③ = **build v1's execution path and reach v0 parity** (faithful calls → control-flow execution → closures → nested heap), coverage-testing every step against v0 byte equality. Real-program coverage (end-to-end = execute and match v0) measures progress. **GTM unlocking condition**. Because verification is ahead, this is a new “build execution” axis.

  **First-slice measurement (2026-06-14): built the EXECUTION entry tool and ran one program**:
  Added `crates/almide-mir/examples/render_program.rs` (.almd → lower all functions → MirProgram → `render_wasm_program` → complete wat module, EXECUTION-side equivalent of emit_cert_from_source; no production/verification code changed = safe). Measurement: render emits op structure (user-function call `$double`, arithmetic, control flow), but RUNTIME is scaffolding (handwritten WAT preamble: only `$print_list`/`$print_int`/list_copy/itoa; no `$print_str`/`$int.to_string`, etc.).

  **Second-slice measurement (2026-06-14): real HEAP-value programs pass v1's execution path and match v0 bytes (no print; discipline respected)**. Three cases—`fn greeting()->String="hi"; main{let _g=
  greeting()}` / List-returning call / chained String call—give **exit 0 and matching output on v1 (lower→MIR→render_wasm→wasm→wasmtime)**. Two fixes (both in render, no added handwritten WAT):
  ① **Un-defer `Init::Str`** (add `Init::Str(String)` in lib.rs; alloc_init puts real bytes on LitStr. Ownership cert unchanged—Alloc is one `i` regardless of contents).
  ② **Fix call-result-repr** (`value_reprs_wasm` reads CallFn's `result` Repr. String/List-return calls use Ptr=i32 handles; scalar i64 default mismatches `$alloc`'s i32).
  Added regression `heap_returning_call_types_result_as_i32_handle` (end-to-end build_and_run). corpus-wall (in-profile 4083, 3-property ACCEPT), gate, cargo test 93/0 all green.

  **Print = self-host confirmed empirically (discipline wall)**: the only real print route is `println→PrintStr` (`lower/calls.rs`), requiring `$print_str`. Manually adding `$print_str` to the preamble made `println("hello")` run byte-identically to v0, **but** the discipline test (`handwritten_wasm_runtime_does_not_grow`, baseline 11) stopped it immediately—**the very v0 trap**, with test message “self-host the new routine in Almide and call it via CallFn.” So revert the `$print_str` routine, retaining the `PrintStr→(call $print_str)` arm (correct CallFn wiring for future self-hosted print). `$print_int`/`$print_list` have no producers in real source (test-fixture-only): scaffold print is dead from real programs. **Print progress = Phase 3 self-host** (low-level Almide subset: memory/host-call primitives + print_str in Almide).
  **Next candidate slices for ③, in order: (a) scalar call execution → DONE in third slice below. (a') scalar value computation (arithmetic/literals)—`Op::Const` value carriage + `IntBinOp` (IntOp only Add/Sub/Mul; Div/Mod/comparison need extension). Without print, currently unobservable end-to-end ⇒ only PrintInt unit tests can check it; changes to 12 Op-enum sites are invasive. (b) print = self-host (substantial, Phase 3).**

  **Third-slice measurement (2026-06-14): SCALAR calls pass v1's execution path and match v0 + verification coverage improves**.
  Added `try_lower_scalar_call` (lower/calls.rs)—lower scalar-result Named / pure-Module calls to real `CallFn{dst:Some, args:lower_call_args, result:Some(scalar)}` (mirror heap calls in binds.rs:114 without heap-push); on failure at bind/tail, **rollback→defer** (Const + elided marker) preserves totality (zero new walls, in-profile 4083 unchanged). Four-case battery (`add(2,3)` literal args / unused scalar result / nested `g(f())` / mixed scalar+heap `mix(5,"hi")`) produces **valid wat → wasmtime → matching v0 exit/output on v1** (no print ⇒ values unobserved, structure executes). **Side effect: verification coverage UP**—materialized deferred calls improve caps 3528→**3582** (+54 functions TAINTED→VERIFIED), ownership 13007→**13153** (+146, Alloc+Drop balance of heap args).
  **3-agent adversarial verification SOUND** (sound caps recovery in brick #56 series—zero unsound flips; de-taint exposed true Stdout reachability in 2 functions, correctly still unverified; names preserved 1:1, caps set unchanged, double-count gate 0, proven-checker backstop). Regression `scalar_user_call_lowers_to_executable_callfn` (from-source lowering, cargo test 94/0).
  **Remaining: module scalar calls dangle at execution because `$string.len` is undefined (= same self-host gap as heap module calls; verification improves). Existing harness flake in corpus-wall.sh caps step (3582 successive checker spawns, OS-OOM/shared-scratch; deterministic recheck 0-reject—future batching/in-process).**

  **⚠ Design core—v0-reuse is the v0 trap (v1-mir-architecture.md §4 and ⚠ note). The right route is SELF-HOST RUNTIME**: **write the runtime in Almide**, pass through the same Core→MIR→target, and let v1 compile itself (dogfooding; rt-oracle/drift class disappears). **Only minimal alloc/RC primitives remain in MIR; Push/IndexSet/Print/string/list/json/… become Calls to self-host runtime functions**, not baked ops. Current handwritten WAT in render_wasm.rs (list_copy/itoa/print baked into ops) is “the v0 trap itself,” scaffolding showing “it runs.” **Do not grow handwritten WAT** is the discipline. Convergence: (a) replace baked ops with runtime-function Calls → (b) shrink MIR ops to primitives → (c) write runtime in Almide (Phase 3, v1-mir-architecture.md §4 / #575/#576).

  PUNCH-LIST (in order): (1) define MIR op set as primitives (alloc/RC + indivisibles), replace Push/Print etc. with runtime-function Calls; (2) **write minimal stdlib in Almide** (direction of old `research/selfhost/`, now `research/spike/v1-mir/`) → v1-compile program+runtime via render_program → match v0 bytes; (3) execute faithful Module/Computed calls → control flow → closures → nested heap.
  Self-host design is substantial: do not squeeze it in while fatigued; build in a fresh session after de-risking via verification workflow. “All stdlib works” = building the self-host runtime (not yet).

---

<a id="kgi-と-kpi-の関係運用ルール--この-1-行"></a>

## KGI–KPI relationship (operating rule in one line)

> **KGI = max(offensive set) subject to pinning every defensive invariant.** Offensive gains bought by breaking defense are **subtractions**, not additions (they erode the trusted core).

So **before** asking how much offense grew that week, ask whether defense stayed entirely green. One defensive failure makes that week **regression**, not progress.

---

<a id="週次記録最新を上に追記"></a>

## Weekly record (prepend newest)

<a id="2026-06-21--③実行-path-で実-repo-2本-full-conquestcsv--svg"></a>

### 2026-06-21 — ③ execution path fully conquers 2 real repos (csv + svg)
- **Defense**: all green maintained. corpus-wall **ACCEPT every commit** (4 properties: ownership 16556 / names 3850 / caps 3141 / caps-transitive 192, no leaks/double-free). `cargo test -p almide-mir` green (7 added regressions). Checker size, TB, axiom purity, claim drift unchanged. Silent acceptance remains 🔄.
  **Discipline caught itself**: after committing byte-identical svg once, **a 10⁴ leak loop found OOM** (flat drop of map.entries' resulting tuple-list) → immediate root fix (`DropListStrStr`), passing the next commit's gate. Nothing shipped with the leak (② discipline).
- **Offense (③ execution path)**: **2 real repos EXECUTE and match v0 bytes**—`csv` (4/4 public functions) + `svg` (records-based renderer: rect/text/group/doc/nested children + `map.entries` attributes). Both leak-free over 10⁴ loops. **Real-program coverage 1→2 full repo conquests + many fixtures**.
  **Language coverage**: subset → **call / control flow / heap-result if·match / closure (defunc inline) / nested-heap list / all records / `Map[String,String]` entries**. New mechanisms: recursive record drop, record-argument drop, List[Record] literal+concat, `(String,String)` tuple-list (`map_entries_str` / `DropListStrStr`), self-recursion allowed inside defunc-map (`in_defunc_body`), lower_bind UnOp arm (fix missing `let hc = not <call>`). Details: [[v1-records-svg]](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/STATUS%206) / [[v1-parser-tco-lever]].
- **Property coverage remains 4/8**: leak-freedom is **empirically clean (10⁴ loops), but Perceus proof remains absent**—no increase in proven properties (progress in emit/execution, not proof). Gap 1 (byte binding) / Gap 2 (frees proof) unchanged.
- **Reading**: ③'s new “build execution” axis **has reached 2 real repos** (execution narrowed the gap behind verification). Remaining language gaps: **Set / guards / general Map (non-String values)**. Next: ④ MSR (pillar A, unmeasured) and Gap 1/2 proof axes, or more real repos with little Map/Set dependency. Progress without dropping any defense.

<a id="2026-06-14--lower-被覆ほぼ天井--2-つの潜在-accept-but-unsafe-を発見封鎖"></a>

### 2026-06-14 — lowering coverage near ceiling + 2 latent accept-but-unsafe cases found and closed
- **Defense**: all green maintained. **Silent acceptance (hole-hunt) advanced**—verification workflow (adversarial refutation) **found 2 latent accept-but-unsafe cases and closed both**:
  (1) **Try/Unwrap/Fan early-return wasm leak**—deferred-continue certificate balances (no_leak), but v0 wasm Err-path `return_` jumps over Perceus final rc_dec, leaking live heap locals. **Fixed the core v0 codegen** (emit_early_return_decs, real bug fix rather than wall); empirically demonstrated no leak (100k×100KB err-loop completes without OOM) + no double-free (260-file wasm corpus + wasm_gc/runtime/cross-target all green).
  (2) **Cancellation in caps gate `mir<=ir`**—FnRef/ClosureCreate marker overcount could offset Computed/Method omission undercount; structurally closed with count_ir_calls (currently unreachable, by-construction hardening). **Soundness GENUINELY maintained at 100%**.
- **Offense**: **lower+verify coverage 4022→4083 (97.3%)**. (a) Cleared small gaps (heap-tail Block / Map insertion / nested tuple destructure / top-level let global / break scalar frame / loop/branch accumulator / Computed-effect-call) + root-fixed the early-return leak class, removed walls and recovered −59. Property coverage **still 4/8** (leak fixes in emit, not proof). **Real-program coverage still 1** (③, advancing render to real programs, not started).
- **③ scope measurement correction (same day)**: verification chain **already ACCEPTs calls and control flow** (`helper()` `im` / `if` `im` / `match` `ad|im` / `for` `ad`; two_functions/transitive_caps already in gate). Stale “render is subset, no calls” does not apply to verification—checker + render patterns + V are ahead. **③'s true frontier: v1 faithfully EXECUTES real programs and matches v0 bytes** (V only matches patterns, not execution; v0 is reference/dual oracle until parity. Lowering defers calls for certs, so execution needs a new path that un-defers calls). Gap 3 section updated with details.
- **Reading**: **the quantitative axis (lowering coverage) is near its ceiling**. Two substantial new axes remain—**③ build v1 execution path and match v0** (verification ahead; new execution-building axis) and **④ MSR** (pillar A, unmeasured). Build each with focus in a fresh session. The next contest is growing them without losing any defensive invariant.

<a id="2026-06-13--基準点"></a>

### 2026-06-13 — baseline
- **Defense**: all green (small checker / declared TB / pure axioms / CI green, cross-version 9.1.1+9.2). Machine checking of claim drift and silent acceptance (hole-hunt drying) unfinished. **Structure intact**—“getting the structure right” achieved for the current subset.
- **Offense**: 4/8 properties, eager only, 1 real program, subset (no calls), byte binding still trusted, frees not started, **MSR zero**.
- **Reading**: what must be defended is defended; the content to fill remains thin. Both KGI wheels (KGI-T slices only / KGI-W unmeasured) still lie ahead. Next contest: grow offense (especially **MSR** and **Gap 1 byte binding**) without dropping any defense.

Weekly template (display only, not automatic input):
> ### YYYY-MM-DD
> - Defense: [all green? / immediately record any broken invariant → this is regression]
> - Offense: [which offensive KPI moved and its current value]
> - Reading: [advanced without weakening defense? Next move]

---

<a id="一言"></a>

## In a word

Defend **the smallness and purity of what must be trusted** (= a few-hundred-line checker + TB reduced to its floor). KGI is the conjunction “**prove all real production on every build with that small trust, and machines write it most accurately**.” **Defense is now perfect; offense is early**—the next contest is growing offense (MSR and Gap 1) without dropping any defense.
