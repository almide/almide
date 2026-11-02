Original description metadata: Native renderer and proposed Ferrocene qualification path; abstract RC lemmas do not prove Almide-to-Rust translation
<a id="flight-keystone-い--v1-mir--rust--ferrocene忠実性束縛"></a>

# Flight Keystone (い) — v1 MIR → Rust → Ferrocene (fidelity binding)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-rust-ferrocene.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-rust-ferrocene.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-rust-ferrocene.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)

> Source caveat: this is a historical proposal, not proof of current complete translation fidelity, actual Ferrocene use or regulatory acceptance. Existing native rendering, abstract RC safety, generated-code correspondence, downstream qualification and physical target behavior are distinct obligations. The displayed theorem does not mention rust_pattern or Rust execution; textual presence/count checks do not bridge that gap. Old cost percentages and source-readability/qualification claims are recorded design judgments, not validated standards guidance. The original recommendation to render linear-memory self-host bodies contrasts with the native-trust-spine plan rejecting that route in favor of native shims; retain it as design history, not a current implementation instruction. Proposed clone/uniqueness, scope-drop, trap and ABI claims require their actual type/runtime/representation premises; repr-c alone is no full ABI or safety proof. No commands, code examples, proof checks, toolchains or measurements were executed.

> **Goal**: concrete implementation design for [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) ladder gate **G-F3** — **a proposal considering Rust → Ferrocene**. The current native renderer exists at `crates/almide-mir/src/render_native.rs`, but its existence is separate from a proof of full Almide→Rust translation fidelity. `.github/workflows/ferrocene-lane.yml` explicitly uses stock rustc before a Ferrocene contract; it is not a record of execution with a qualified compiler. Qualification of a later stage does not automatically prove semantic preservation of earlier stages; do not infer proof effort from percentages such as 75% reuse.
> **Related**: [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §4 / [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §1, §9 / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md).

---

<a id="1-設計核一行"></a>

## 1. Design core (one line)

> **Design goal**: establish grounds connecting native renderer output to explicit semantics. Existing RC event-stream theorems are reuse candidates, but a string-pattern table and `exact eager_copy_refines_safety` alone cannot prove actual Rust execution faithful. State target syntax, runtime, ABI and unproved boundaries, separately from downstream toolchain evidence.

---

<a id="2-既存証拠の分割再利用可能性と未証明の接続"></a>

## 2. Separating existing evidence (reuse potential and unproved connections)

| Category | Files | Implication for flight |
|---|---|---|
| **Target-independent (reuse as-is)** | `OwnershipChecker.v` (`check_sound` establishes the **fold's `Z` result**, `:34-36`) / certificate i/a/d/m/r/b (= MIR ownership facts) / `certificate.rs` (zero target awareness) / `RuntimeModel.v` / `ALS.v` / `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` | Abstract event-model theorems are reuse candidates; connection to Rust execution is a separate obligation |
| **Wasm-specific (the sole target layer)** | `Translation.v:28-35` (`wasm_pattern: Op→string`) + `WasmEncode`/`WasmExec`/`WasmRcDec` + V validator | `rust_pattern` is a syntax-matching candidate; depth of semantic-preservation proof not evaluated |

The eager instance of `Translation.v` is **already proved** (`eager_translation_refines_safety
:= exact eager_copy_refines_safety`, `Translation.v:52-54` ← `ALS.v:62`).

---

<a id="3-当初の-renderer-実装案a"></a>

## 3. Original renderer implementation proposal (§A)

The demos/line numbers below record the original design, not a claim that the current renderer is unstarted. Implementation entrypoints are `crates/almide-mir/src/render_native.rs` and `src/cli/emit.rs`.

**Structure (mirror `render_wasm.rs`)**: `render_rust_program(prog) -> String` drives `render_rust_fn` + `rust_preamble()` (analogue of `render_wasm.rs:648` `preamble()`) + self-host runtime linkage (same shape as `render_wasm.rs:591-644` `self_host_runtime()`). CLI entry is a sibling of `examples/render_program.rs` (currently hardcodes `render_wasm_program` at :16/126).

**Reuse from demo `render_rust.rs` (retain)**:
- Op→Rust mapping skeleton + §3.2 contract comments (`render_rust.rs:1-21`) = **intended mapping, normative design anchor**.
- `render_op` dispatch (`:120-198`), `IntBinOp` lowering (`:172-189`, already idiomatic `a + b` / `(a < b) as i64`), `CallFn`/`Call` branches (`:190-225`), `render_rust_fn` (`:32-56`, including tail-return-by-move).
- `value_reprs` inference (`:85-112`) needs heap-result extension (read `CallFn.result` repr, already present at `render_wasm.rs:339-341`).

**Rebuild for production (barriers to review-grade output)**:
1. **`rust_ty` (`:75-81`) uses `Vec<i64>` for every heap** — biggest barrier. Consult `Repr::Ptr{layout}` / layout registry to emit real types (`String`/`Vec<T>`/`Box<Node>`/named struct). Uniform `Vec<i64>` fails DO-178C “source readability.”
2. **`var` (`:114-116`) is `v{id}`** — `v0`/`v1` numbers cannot be reviewed. `ValueId(u32)` (`lib.rs:114`) has no names; **thread a `debug_name: Option<Sym>` side-table through lowering** (IR `VarId` has source names). This is a **lowering change**, not renderer-local.
3. **Control flow**: demo uses `None` for `If/Loop` markers (`:154-155`), which is wrong. **Reconstruct idiomatic `if c { } else { }` / `while c { }`** from the flat marker stream (mirror `if_stack`/`loop_stack` state machines at `render_wasm.rs:212-260`). **Largest net-new code**.
4. **Heap initializers**: `Init::Str`→real `String`, `DynStr`→owned buffer, `OptSome`→`Some(x)`.
5. **Struct/enum declarations**: absent from demo. A declaration pass from the `LayoutId` registry, targeting `walker/declarations.rs:58-62` (`#[derive(Clone)] pub struct`) and `:135` (`pub enum`).

---

<a id="4-rust_pattern-表--各-op--イディオマティック-rustb"></a>

## 4. `rust_pattern` table — each Op → idiomatic Rust (§B)

Extend §3.2's 4 contract anchors (`render_rust.rs:6-13`) — `Dup→.clone()` / `Drop→scope-end` / `Consume→move` / `MakeUnique→no-op` — to the full op set (`lib.rs:152-257`). `dst`/`src` are **resolved real names**, not `v{id}`; types are actual `Repr`/`LayoutId` types:

| Op | Idiomatic Rust | Note |
|---|---|---|
| `Alloc{Init::IntList}` | `let mut dst: Vec<T> = vec![e0,e1,…];` | Actual element type |
| `Alloc{Init::Str(s)}` | `let dst: String = "s".to_string();` | Replaces `vec![bytes]` |
| `Alloc{Init::DynStr}` | `let mut dst = String::with_capacity(len);` | |
| `Alloc{Init::OptSome{p}}` | `let dst = Some(p);` | |
| `Alloc{Init::Opaque}` | `let dst = Name { … };` | Ctor from layout |
| `Dup{dst,src}` | `let dst = src.clone();` | §3.2. **Only ownership decision = eager COW** |
| `Drop{v}` | *(emit nothing)* RAII scope-end | §3.2. Source of eager safety (same as `Translation.v:32` `Dec⇒""`) |
| `Consume{v}` | *(emit nothing)* move at use site | §3.2 |
| `Borrow{v}` | `&v` | |
| `MakeUnique{v}` | *(no-op)* already unique through `Dup`'s `.clone()` | §3.2 |
| `ConstInt{dst,value}` | `let dst: i64 = value;` | |
| `IntBinOp{Add/Sub/Mul}` | `let dst = a + b;` | demo `:172` |
| `IntBinOp{Div/Mod}` | `almide_div!(a,b)` / `almide_mod!(a,b)` | **Trap parity** (below) |
| `IntBinOp{Lt/…/Ne}` | `let dst: i64 = (a < b) as i64;` | demo `:181` |
| `IfThen/Else/EndIf` | `let dst = if cond != 0 { then } else { els };` | Net-new, mirror wasm `:232` |
| `LoopStart/BreakUnless/End` | `while cond != 0 { … }` | Net-new, mirror wasm `:214` |
| `SetLocal{local,src}` | `local = src;` | |
| `CallFn{dst,name,args}` | `let dst = name(args);` | |
| `Call{RtFn::*}` | Call self-host Rust fn (§5) | |
| `Prim{kind,…}` | Trusted floor → Rust stdlib (§5) | Net-new |
| `Pure{dst,uses}` | `let dst = a.len();` etc. | Demo punts to `0` |

**Trap parity (hard part)**: `Div/Mod` trap on zero division in MIR/wasm (`render_wasm.rs:457` `i64.div_s`). Bare Rust `a/b` panics, an observable difference. For oracle byte identity, use `almide_div!`/`almide_mod!` (`rust.toml:36-40`) — already total-checked `Error:\n` + exit-1 shape, review-grade.

**Readability strategy** (output indistinguishable from v0): equality `almide_eq!` (`rust.toml:63`), struct/enum `#[derive(Clone)]` (`declarations.rs:58`), concat `format!` (`rust.toml:53`), runtime calls `almide_rt_*` (`rust.toml:28`), real names + real types. **Litmus: `cargo fmt`-formatted output passes human review and `cargo build` emits zero warnings** (CLAUDE.md codegen rule).

---

<a id="5-runtime--mirrust-で-dogfoodstdlib-にマップしないc"></a>

## 5. Runtime — dogfood MIR→Rust, do not map to stdlib (§C)

**Recommendation: render self-host runtime through the same MIR→Rust path (dogfood); map only the prim floor to Rust stdlib.**

Rationale ([v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §4/§4.1): runtime is **written in Almide and follows the same Core→MIR→target**. Mapping list/string/print to Rust `std` would **reintroduce dual-implementation drift (~136 routines maintained twice), which v1 exists to eliminate**. Runtime is already self-hosted in Almide and auto-linked (`render_wasm.rs:591-644` lowers `stdlib/*.almd`; `print_str.almd` is dogfood `println` over the prim floor). Rust renderer reuses **the same registry** — same `.almd`, same `lower_function`, only substitute `render_rust_fn`. **Only the prim floor** (`PrimKind`, `lib.rs:262-273`) maps to Rust primitives: each target's target-specific trusted floor (`§4.1`). The `print_str` body lowers to dogfood Rust, with `prim.fd_write` reaching `io::stdout().write_all`.

**But the prim floor itself splits by target** (§7 hard part 3): Rust has no shared linear memory, so `prim.handle(s)` (assumes a linear-memory address) does not hold. **Everything above the prim floor dogfoods; the prim floor itself branches per target** (the architectural trust boundary, the only place permitting “no dogfood”).

---

<a id="6-忠実性証明d"></a>

## 6. Fidelity proof (§D)

<a id="61-抽象-eager-事象モデルの再利用例rust-翻訳証明ではない"></a>

### 6.1 Abstract eager event-model reuse example (not a Rust translation proof)

Add `rust_pattern : Op -> string` (structurally isomorphic to `wasm_pattern` `:28-35`):

```coq
Definition rust_pattern (o : Op) : string :=
  match o with
  | Inc | Alias => ".clone()"   (* 新規取得 / alias = clone *)
  | Dec | MoveOut | Reuse => "" (* eager: scope-end drop / move、明示 op 無し *)
  end.

Theorem eager_rust_translation_refines_safety :
  forall ops, increments_only ops -> no_double_free ops.
Proof. exact eager_copy_refines_safety. Qed.
```

Translation of the retained Coq comments: new acquisition / alias = clone; eager scope-end drop / move emits no explicit op. The theorem statement is preserved exactly; it does not mention Rust execution.

`Dec/MoveOut/Reuse` are empty strings **for the same reason as wasm** (RAII scope-end emits no explicit instruction; neither does eager wasm). The release-empty lemma (`is_release o → rust_pattern o = ""`) transfers unchanged too.

- **Scope of existing theorem**: `eager_copy_refines_safety` establishes a property of abstract event streams. Neither `rust_pattern` nor Rust execution semantics appears in the conclusion above, so Rust-output correctness does not follow.
- **Separately needed grounds**: correspondence showing actual generated code, runtime and ABI realize the model. String matching or `Print Assumptions` alone does not close this connection.
- **Remaining connection obligation**: evidence that Rust fragments implement abstract ops. Adopting Ferrocene downstream does not permit omitting correspondence of upstream generated Rust with the model. Rather than requiring a particular formalization file, determine evidence according to claim scope.

<a id="62-per-build-v-validatortranslation_validationrs-の-rust_pattern-兄弟"></a>

### 6.2 Per-build V validator (`rust_pattern` sibling of `translation_validation.rs`)

Mirror `wasm_pattern` (`:49-98`) + `validate_translation_perceus` (`:122-126`): `rust_pattern(op)->Option<String>` + `validate_translation_rust(rust_src, mir)`. Scan **output Rust source** per op:

| Op | wasm V (`:49-97`) | Rust V |
|---|---|---|
| `Dup`/`Alias` | `call $rc_inc` | `.clone()` |
| `Drop` | `call $rc_dec` | *(eager: absent = leak-count check)* / *(perceus: scope-end `}` or `drop(x)`)* |
| `Alloc{IntList}` | `call $list_new` | `vec!` / `Vec::` |
| `CallFn{name}` | `call $name` | `name(` |
| `IntBinOp{Add}` | `i64.add` | ` + ` |
| `Call{PrintInt}` | `call $print_int` | `print_int(` |
| `Consume` | None | None (move) |

Leak-count check (`:122-126` `rc_decs >= drop_count`) transfers unchanged. Directly port nonvacuity test (`:185`: remove a pattern and V fails). Add `validate_safety_rust` sibling to `validate_safety`.

---

<a id="7-信頼基底の台帳変更--9-和解e"></a>

## 7. Trusted-base ledger changes + reconciliation with §9 (§E)

**Enters the trusted base (newly trusted, unproved)**:
- **Ferrocene / qualified `rustc`** owns Rust source→machine code (ISO 26262 ASIL D / IEC 61508). **Qualified** (not proved by us) = SCADE KCG analogy (`certification-grade.md:135`).
- **Rust prim floor** (`PrimKind`→Rust stdlib `io::Write`/ptr) = small closed trusted surface, Rust analogue of wasm prim floor.

**Unproved boundary**: full Almide→Rust translation fidelity is not established by abstract RC theorems or `rust_pattern` syntax matching. The native route has a different verification target from Wasm byte binding, but correspondence obligations for generated Rust remain. Using Ferrocene qualification does not remove upstream obligations.

**Original §9 design goal** (the “proof-bound renderer” below is not an achieved fact): preserve the intent of establishing generated Rust's correspondence to the MIR model.

**Original reconciliation proposal** (respect MIR sovereignty): `v1-mir-architecture.md §9` rejected “**Rust as the sole truth**” (building on formal Rust semantics that do not exist). This design **does not do that** — it does not build on Rust semantics, but binds to certificate-checked **MIR facts**; Rust remains just a renderer, **MIR remains the sole truth**. The only change promotes the Rust renderer from “almost trivial lowering stepping stone” (§1) to “**a second proof-bound production target**.” §9's rejected “Rust = truth” differs from this design's “Rust = proof-bound renderer.” Sovereignty (wasm canonical) remains; Rust is the second consumer of the same witnesses.

---

<a id="8-ferrocene-実証経路--難所f"></a>

## 8. Ferrocene demonstration route + hard parts (§F)

<a id="de-risking-順最小の端から端スライス先"></a>

### De-risking order (smallest end-to-end slice first)

**Slice 0 — `add` program (already on both targets)**. `render_rust.rs:245-286` `add_program` (`fn add(a,b)=a+b` + `main` prints 5) already compiles with `rustc` and prints `5` (`:278-286`). Required ops: `IntBinOp{Add}`/`CallFn`/`Call{PrintInt}`/`ConstInt`. **First replace `rustc` with Ferrocene `rustc` and check byte identity**. No new renderer code; pure toolchain wiring.

**Slice 1 — value-semantic lists (ownership keystone)**. `render_rust.rs:313-342` (`var a=[1,2,3]; var b=a; a[0]=9; print a,b`) = `Alloc{IntList}`/`Dup→.clone()`/`MakeUnique→no-op`/`Drop`/`Call{ListSet,PrintList}`. Already outputs `a=9,2,3 / b=1,2,3`. **Demonstrates §3.2 contract** (1 `Dup` → 1 `.clone()` → value semantics correct by construction). Add V checking.

**Slice 2 — heap types + control flow**. Change `Vec<i64>` to real `String`/`Vec<T>` (rebuild `rust_ty`), thread real names (lowering side-table), add `if`/`while` marker reconstruction. First program = counted list-sum loop (`Loop*`/`SetLocal`/`IntBinOp`), **intersecting keystone (あ)'s counted-loop subset** ([flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)); the 2 keystones share fixtures.

**Subset needed for the first trustworthy Ferrocene demo**: `Alloc{IntList,Str}`/`Dup`/`Drop`/`Consume`/`MakeUnique`/`ConstInt`/`IntBinOp` (all)/`CallFn`/`Call{PrintInt,PrintList,PrintStr}`/`SetLocal`/`If*`/`Loop*` + `print_str` prim floor — exactly the flight subset in [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7. G-F4 takes a real safety-critical module (control law/state machine/watchdog) end to end over it.

<a id="正直な難所"></a>

### Honest hard parts

1. **Disjoint paths (deepest)**: 2 Rust routes — demo `render_rust.rs` (op↔Rust correspondence, but `Vec<i64>`, not review-grade, not CLI) and `walker/`+`rust.toml` (review-grade but **no op correspondence object**, not v1-MIR-driven). G-F3 cannot simply take walker (no op↔fragment binding to validate) or ship the demo (not review-grade). **Build a third thing**: production v1-MIR→Rust renderer emitting walker-grade Rust while retaining demo per-op correspondence. `flight-profile.md:144` honestly sizes it as “full-language render_wasm.rs, about 5x.” **Real cost = engineering, not proof.**
2. **Variable naming = lowering change**: `ValueId(u32)` has no names; review-grade needs real ones. IR→MIR lowering threads a `debug_name: Option<Sym>` side-table. Touching MIR data model affects both renderers + verifier; make it an `Option` wasm may ignore.
3. **Runtime rendering exposes prim-floor-on-Rust gap**: `print_str.almd` uses `prim.handle/load32/store32/fd_write` (raw linear-memory addresses). Rust lacks a shared linear-memory model. The prim floor must (a) emulate a byte-addressed arena (faithful but ugly, harms readability), or (b) map `print_str` to native Rust `String` write at `RtFn::PrintStr`. **Recommendation: all above prim floor dogfoods, prim floor itself branches per target** (“no dogfood” allowed only at the design trust boundary).
4. **Repr/ABI determinism**: real `String`/`Vec<T>`/struct delegate layout to rustc — sufficient for **observational** equivalence (stdout bytes), but not **byte-layout** identity. Observational equivalence is the contract (CLAUDE.md Behavior Contracts), so allowed; `almide_div!`/`almide_mod!` handle the sole trap difference. If later claiming FFI/ABI guarantees, `--repr-c` (`rust.toml:56`) helps.

**Net**: existing native renderer and abstract-model evidence are a foundation, but full translation fidelity, actual qualified-toolchain use and authority acceptance are separately unestablished. Do not use old ~5x/~75% estimates or “trivial with one table” explanations as evidence of present effort or proof completion.
