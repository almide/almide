import AlmideEditBelt.Evaluator

/-!
# The kernel-conformance program family

Stage 3's bridge between the proven kernel and the shipping backends. Each
`k*` below is a λ_almd program whose almide-surface image lives in
`spec/wasm_cross/kernel_conformance.almd`; the `#guard`s pin the
kernel-semantic observables at COMPILE TIME (by `eval_sound` + `ev_det`
these are theorems about `Ev`, not just evaluator outputs), and
`tests/kernel_conformance_test.rs` pins the real compiler's stdout to the
same strings on native, with the `wasm_cross` harness extending the claim
to wasm.

The Lean literal ↔ fixture expected-output link is by-hand and REVIEWED,
not proven — that boundary is recorded in
`docs/contracts/proven-vs-trusted.md`. A mapping note: an almide
`effect fn` body's tail is ok-LIFTED by the compiler, so its λ_almd image
wraps the body in `.ok ...`; explicit `!` sits inside that wrapper and
aborts before the lift — which is exactly how `callReify` reproduces the
surface behavior of `err(..)!`.

Coverage: sequencing + `print` (k1), pure fn returning `ok`/`err` through
`match` (k2, k3), `!` on `err` reified at the call boundary (k4), `!` on
`ok` under the tail lift (k5), callee effects ordered before caller
continuation (k6), and `??` fallback (k7 — kernel-guard only: its
observation needs integer printing, which the kernel's string-only `print`
does not model; recorded as a fixture gap, not silently skipped).
-/

namespace LambdaAlmd

/-- `fn g(x: Int) -> Result[Int, String] = ok(x)` -/
def gDef : Defn := ⟨"x", .ok (.var "x"), false⟩

/-- `fn h(x: Int) -> Result[Int, String] = err("nope")` -/
def hDef : Defn := ⟨"x", .err (.strLit "nope"), false⟩

/-- `effect fn boom(x: Int) -> Int = err("kaboom")!` — the `!` aborts
before the tail lift. -/
def boomDef : Defn := ⟨"x", .ok (.prop (.err (.strLit "kaboom"))), true⟩

/-- `effect fn five(x: Int) -> Int = { let r: Result[Int, String] = ok(5)
r! }` — the `!` unwraps, the tail lift re-wraps. (The `let` anchors the
err type at the surface; the kernel is untyped here, but the image stays
shape-exact.) -/
def fiveDef : Defn :=
  ⟨"x", .ok (.letE "r" (.ok (.intLit 5)) (.prop (.var "r"))), true⟩

/-- `effect fn shout(x: Int) -> Int = { println("inside") 1 }` -/
def shoutDef : Defn := ⟨"x", .ok (.seq (.print (.strLit "inside")) (.intLit 1)), true⟩

/-- The conformance definition table. -/
def DC : Defs :=
  upd (upd (upd (upd (upd (fun _ => none)
    "g" gDef) "h" hDef) "boom" boomDef) "five" fiveDef) "shout" shoutDef

/-- k1 — sequencing and `print` order. -/
def k1 : Expr :=
  .seq (.print (.strLit "alpha")) (.seq (.print (.strLit "beta")) (.print (.strLit "gamma")))

/-- k2 — pure `ok` through `match`, ok branch. -/
def k2 : Expr :=
  .matchR (.call "g" (.intLit 7)) "v" (.print (.strLit "got-ok")) "w" (.print (.strLit "got-err"))

/-- k3 — pure `err` through `match`, err branch. -/
def k3 : Expr :=
  .matchR (.call "h" (.intLit 0)) "v" (.print (.strLit "got-ok")) "w" (.print (.strLit "got-err"))

/-- k4 — `!` on `err`: the abort reifies at the call boundary. -/
def k4 : Expr :=
  .matchR (.call "boom" (.intLit 0)) "v" (.print (.strLit "no-abort")) "w" (.print (.strLit "reified"))

/-- k5 — `!` on `ok` under the tail lift. -/
def k5 : Expr :=
  .matchR (.call "five" (.intLit 0)) "v" (.print (.strLit "five-ok")) "w" (.print (.strLit "five-err"))

/-- k6 — callee effects are observed before the caller continues. -/
def k6 : Expr :=
  .matchR (.call "shout" (.intLit 0)) "v" (.print (.strLit "outside")) "w" (.print (.strLit "boom"))

/-- k7 — `??` fallback on an `err` (kernel-guard only, see module doc). -/
def k7 : Expr := .orElse (.call "h" (.intLit 0)) (.intLit 9)

/-- The whole family as one program — the exact λ_almd image of
`spec/wasm_cross/kernel_conformance.almd`'s `main`. -/
def kAll : Expr := .seq k1 (.seq k2 (.seq k3 (.seq k4 (.seq k5 k6))))

#guard evalE DC 32 emptyEnv k1 =
  some (.norm .vUnit, ["alpha", "beta", "gamma"], [])
#guard evalE DC 32 emptyEnv k2 = some (.norm .vUnit, ["got-ok"], ["g"])
#guard evalE DC 32 emptyEnv k3 = some (.norm .vUnit, ["got-err"], ["h"])
#guard evalE DC 32 emptyEnv k4 = some (.norm .vUnit, ["reified"], ["boom"])
#guard evalE DC 32 emptyEnv k5 = some (.norm .vUnit, ["five-ok"], ["five"])
#guard evalE DC 32 emptyEnv k6 =
  some (.norm .vUnit, ["inside", "outside"], ["shout"])
#guard evalE DC 32 emptyEnv k7 = some (.norm (.vInt 9), [], ["h"])
#guard evalE DC 64 emptyEnv kAll =
  some (.norm .vUnit,
    ["alpha", "beta", "gamma", "got-ok", "got-err", "reified", "five-ok",
     "inside", "outside"],
    ["g", "h", "boom", "five", "shout"])

/-- The pinned family observables are not merely evaluator outputs: by
`eval_sound` they are derivations of the relational semantics, and by
`ev_det` the only ones. -/
example :
    Ev DC emptyEnv kAll (.norm .vUnit)
      ["alpha", "beta", "gamma", "got-ok", "got-err", "reified", "five-ok",
       "inside", "outside"]
      ["g", "h", "boom", "five", "shout"] :=
  eval_sound 64 _ _ _ _ _ (by rfl)

end LambdaAlmd
