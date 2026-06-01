<!-- description: Closure Architecture v2 — one identity, one capture-set, lifting is lowering; separates closure REPRESENTATION from the inlining OPTIMIZATION -->
# Closure Architecture v2

> **One identity, one capture-set, lifting is lowering.** A closure is a single
> canonical value with a program-unique identity and an explicit, once-computed
> capture set; *inlining* is a separate, escape-proven rewrite — never a
> representational shortcut. This is the closure analogue of how `Verified` /
> `Canonical` gate emit: correctness by construction, not by guessing.

## Why

Today a single source lambda `(x) => f(x)` is represented up to **five** different
ways depending on use-site syntax, capture analysis, mutability, and target:

1. an inline iterator-adapter callback (`IterStep`/`IterCollector`, Rust);
2. a surviving raw `IrExprKind::Lambda` value;
3. a lifted `__closure_N` function + `ClosureCreate` + `EnvLoad` triple (WASM, immutable captures);
4. a heap-cell mutable-capture raw `Lambda` (WASM);
5. an eta-expanded wrapper / `FnRef` (named functions).

Captures are **not** stored on the `Lambda` node; they are re-derived by **three**
independent free-variable analyses (`FreeVarCollector` in closure conversion,
`ClosureScanner` in `emit_wasm`, and Rust's implicit `move`), feeding **two**
near-duplicate WASM env builders (`emit_lambda_closure` vs `emit_closure_create`).
`Ty::Fn { params, ret }` is the *only* callable type — it carries no closure /
capture / calling-convention tag, so every operational distinction lives
implicitly in *which node kind* and *which pipeline branch* was chosen.

The conceptual-integrity defect: **there is no canonical "function value."** The
representation is *guessed* from the lambda's shape (`free.is_empty()` ⟹ "this
will be inlined at a HOF call site"), a property the pass cannot actually see.

### The verified bug this produces

`ClosureConversionPass` keeps non-capturing lambdas raw (`pass_closure_conversion.rs`
`free.is_empty()` branch). Raw lambdas are correlated to their pre-scanned
`LambdaInfo` at emit by `lambda_id` — but `lambda_id` **resets to 0 per module**
(`LowerCtx::new`) and module lambdas are **never registered** (the WASM pre-scan
only walks `program.functions`). Result, reproduced on 0.23.14 with an 8-line
two-module program:

```almide
// src/lib.almd
pub fn neg() -> (Int) -> Int = (n) => 0 - n
// main.almd
import self.lib
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)
fn add() -> (Int) -> Int = (n) => n + 1000
effect fn main() -> Unit = {
  println(int.to_string(apply(add(), 5)))     // 1005  (correct on both)
  println(int.to_string(apply(lib.neg(), 5))) // want -5; WASM printed 1005
}
```

`lib.neg()(5)` returns `1005` (main's `add` body) on WASM — a silent wrong result.
A variant with no main-program lambda emits invalid WASM
(`unknown table 0: table index out of bounds`). Native is correct because the
Rust target uses native closures and never runs the WASM id-matching path.

## The principle: separate three conflated concerns

| Concern | Today (broken) | v2 |
|---|---|---|
| **Identity** — *which* closure | per-module `lambda_id`, collides across modules | program-unique `ClosureId` |
| **Capture set** — *what* it closes over | re-derived by 3 analyses, can diverge | one shared analysis, attached to the node, with explicit `mode` + precise `Ty` |
| **Representation** — *how* it is laid out & called | guessed from capture-emptiness → 5 shapes | the `Lambda` value node is canonical through the shared pipeline; *lifting* is a target lowering; *inlining* is the proven absence of lifting |

The inversion: **today the representation is guessed from the lambda's shape;
in v2 it is proven from the use** (escape analysis decides per call-site), while
identity and captures are computed once and shared by all targets.

## The boundary: shared semantic core vs target lowering

The redesign's single most important decision (a uniform *lifted* `Closure{code,env}`
node is the wrong altitude — it is a WASM lowering form; pushing it through Rust
forces a backwards reconstruction of native closures that defeats `rustc`, and
WGSL cannot represent it at all):

**SHARED (in `almide-ir`, computed once over functions *and* modules, before the
pipeline splits):**

- `ClosureId(u32)` — program-unique identity, replaces `lambda_id: Option<u32>`.
- The capture set, attached to the node:
  `Lambda { params, body, id: ClosureId, captures: Vec<Capture>, escape: EscapeVerdict }`,
  `Capture { var, ty: Ty /* Almide owned type */, mode: ByVal | ByMutCell }`.
  Non-capturing ⟹ `captures: []`. No `free.is_empty()` bypass.
- A program-wide `ClosureTable` keyed by `ClosureId` (functions + modules).
- One shared free-var + escape analysis fills all of the above.

`Lambda` (carrying id + captures + escape) **stays the canonical closure VALUE
form through the entire shared pipeline.** There is no shared lifted
`Closure{code,env}` node.

**TARGET LOWERING (each reads the shared capture-set + escape verdict):**

- **Rust** — never lifts. Emits native `move |params| body`, driving
  `CaptureClonePass` deterministically from `captures` (delete the
  `needs_clone_type` allowlist). `ParamBorrow` materialization (`to_vec`/`to_owned`)
  is a Rust-representation derivation. Escape-required stored forms → `Rc<dyn Fn>`.
  Lets `rustc` optimize.
- **WASM** — a WASM-only `ClosureLowering` pass lifts **only the closures that
  survive `InlineClosures`** (the escaping ones) to `__closure_N` + a `Ty::EnvPtr`
  env + `EnvLoad`, registered in the `ClosureTable` → func-table **by name**
  (order-independent, module-safe). One env builder; one `valtype_and_load(ty, mode)`
  helper centralizing ValType selection, mut-cell-as-i32, and narrow-int
  sign-extension. `call_indirect` type index comes from the lifted fn's registered
  `closure_type_idx` via `ClosureId` — **one source of truth**.
- **WGSL** — a *closure-free* target. `InlineClosures` runs unconditionally inside
  `@gpu` bodies; any residual escaping closure → a hard compile error. (This
  validates the thesis from the other side: WGSL is where "inline or error" is
  forced rather than optional.)

The elegant payoff: **shared = identity + capture-set + escape-verdict; lifting,
sign-extension, `to_vec`/`to_owned`, `Rc<dyn Fn>`, `call_indirect`, and
"inline-or-error" are all per-target derivations of that one shared truth.**
Neither backend is contorted.

## InlineClosures: "prove the use"

A recognizer pass that runs **before** lifting. A closure may be inlined iff a
**two-sided** proof holds:

- **(a) the use site**: the closure is *anonymous* (not the RHS of any `Bind`),
  *single-use*, and does not escape its scope; **and**
- **(b) the sink**: the callee is on a curated allowlist of *consume-only*
  combinators carrying `@fn_arg_consumed(idx)` in `stdlib/defs`, verified once
  against the runtime impl — so the closure cannot escape *through* the call (a
  `fold` whose reducer captures the fn into the accumulator would otherwise
  escape it).

When both hold, rewrite `Call(combinator, [.., literal-closure])` → an
`InlinedHOF` node (WASM: spliced body, no alloc / no `call_indirect`; Rust:
`IterChain` adapter). Egg fusion composes `InlinedHOF`s. **A bound closure is
never inlined** ⟹ "computed once" is literally true. The WASM emitter's fast path
keys on `InlinedHOF`, not on raw-`Lambda` shape.

## The hard correctness pieces (designed in, not glossed)

- **Recursion.** Self / mutual-recursive *local* closures cannot capture
  themselves into their own env (a cycle / a not-yet-allocated value). A pre-pass
  computes SCCs over `let`-bound lambda groups; self/sibling references are
  rewritten to the `ClosureId` code symbol and the lifted fn's own `__env` param
  is threaded into the recursive call (the standard "recursive closures pass their
  own env"). Postcondition: no `env` captures the closure being constructed.
- **Mutable capture is a *lifetime* bug, not a representation bug.** `ByMutCell`
  is necessary but insufficient alone (the current code silently stores `0` into
  the env when the captured local is out of `var_map`). Fix: a `CellAlloc` node at
  the variable's *binding* site; redirect **every** read/write of that var — in
  the enclosing function too, not just the lifted body — to `CellRead`/`CellWrite`;
  the cell (`Ty::Cell(T)`, a 1-slot heap record) is RC-tracked so Perceus keeps it
  alive across the closure's escape. Delete the zero-store fallback → a
  postcondition error.
- **Perceus RC through env.** Today capture-inc fires only when `ClosureCreate` is
  a `VDecl` value — so a closure created as a call-arg or tail (exactly the verified
  bug's shape) gets **no** inc → under-count. Fix: bind inc/dec to the **Closure
  node** in *every* syntactic position (decl value, call arg, tail, list element);
  inc each heap env capture where the value is materialized, dec each on the
  Closure's own drop. `Ty::EnvPtr` is heap **and** borrowed-not-owned; EnvLoad-bound
  captures are structural borrows (no inc/dec). This gives the L5 Lean proof a clean
  handle — `inc_count(cap) == dec_count(cap)` over every path because both ends bind
  to one node — **closing the "Perceus → binary" proof chain.**
- **Type identity, one source of truth.** `Ty::EnvPtr` is a *real sealed scalar
  variant* (not a `Ty::String` alias — else Perceus `RcDec`s the env block and
  corrupts the heap); `ty_to_valtype = I32` today, target-parametrized for a future
  GC backend (`(ref $env)` + `call_ref`). `call_indirect` type comes from
  `closure_type_idx` via `ClosureId`, not re-derived from `callee.ty` per site (the
  structural `register_type` dedup is a latent second collision — the no-main
  "table OOB" variant). **Keep one `Ty::Fn`** — bifurcating the callable type
  metastasizes through unification and every `Type[Fn]` container; the convention
  lives on the node, enforced by "every callable value is a Closure after
  conversion; sites always emit the closure ABI, never sniff `.ty`."

## Phasing (every boundary perf-neutral AND invariant-satisfiable)

The red-team's rule: **totality is atomic with its dependents.** "Zero raw Lambda
after conversion" is *unsatisfiable* while egg fusion (runs before conversion,
re-emits raw lambdas) and the WASM inline fast path (`is_inline_lambda` matches
raw `Lambda`) still need raw lambdas. Never split that across releases.

| Phase | Content | Property |
|---|---|---|
| **P0** | program-unique closure identity (`GlobalizeClosureIdsPass`) + register module lambdas in the WASM scan | **fixes the verified bug, perf-neutral** (fast path untouched). 8-line repro as a native==wasm regression test |
| **P1** | shared core: `ClosureId` + one free-var/escape analysis over functions+modules; attach `captures`/`escape` to the node; `ClosureTable` | representation unchanged; Rust/WASM *read* the shared set; assert old analyses agree |
| **P2** | `InlineClosures` (`InlinedHOF`) + **atomic totality**: move egg onto identified closures; rewrite the 3 WASM fast-paths to key on `InlinedHOF`; total WASM `ClosureLowering` over survivors; delete the `lambda_id`/`param_id`/counter resolver + one env builder; `Ty::EnvPtr`; `EnvLoad` via `emit_load_at`; `closure_type_idx` via `ClosureId` | "zero raw Lambda" finally satisfiable; the `unreachable`s become postcondition errors |
| **P3** | `ByMutCell` (`CellAlloc`/`CellRead`/`CellWrite`) + SCC recursion + Perceus-RC-through-env | the three correctness blockers, verified at `PerceusVerify` |
| **P4** | Rust reads the shared capture set fully; retire `needs_clone_type` + the third (implicit-move) analysis | one analysis, both targets |
| **P5** | Lean: certify `inc == dec` over env | the proof chain closes |

## Honest limits

- **P0 is a targeted fix, not the redesign.** It makes `lambda_id` program-unique
  and registers module lambdas; the `free.is_empty()` representational bypass and
  the parallel raw-Lambda emit path remain until P2. It does not improve
  mutable-capture lifetime (P3) or the duplicate analyses (P1/P4).
- **The escape allowlist (`@fn_arg_consumed`) is hand-curated and verified once
  against each runtime impl** — a maintenance obligation, not an inferred property.
  A wrong annotation can inline an escaping closure; treat it as a trusted base.
- **GC migration is localized but not free**: `Ty::EnvPtr` and the Closure layout
  are target-parametrized, so a `gc` backend swaps `emit_wasm` + `ty_to_valtype`
  only — but `call_ref`/typed-funcref bring their own type-identity model to design.
- **L3 determinism interaction**: `GlobalizeClosureIdsPass` runs before
  `CanonicalizePass` and mutates only `lambda_id` (not function order), so the
  `Canonical` certificate is unaffected; closure table slots are assigned in scan
  order, independent of these ids, so emitted bytes stay host-deterministic.
