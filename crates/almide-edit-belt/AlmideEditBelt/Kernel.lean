/-!
# λ_almd — the edit-locality kernel calculus

Machine-checked core of `docs/specs/edit-locality.md` §1 (the invariant) and
Stage 2 of `docs/roadmap/active/edit-locality-theory.md`.

The setting: an almide program is a table of top-level definitions, each with
a DECLARED signature (`fn` vs `effect fn` included), plus an expression to
run. λ_almd keeps exactly the features the L1 statement quantifies over —
`let`, calls, `Result` construction and `match`, explicit propagation `!`
(ADR-0008), fallback `??`, sequencing, and one observable effect (`print`,
the kernel's stdout) — and nothing that could smuggle non-local resolution:
no overloading, no instances, no macros, no dynamically scoped handlers.
Calls resolve by NAME ALONE (`Ev.callNorm` reads `D f`, nothing else); that
one design fact is what the Stage-2 theorems turn into mathematics.

Evaluation is a big-step judgment instrumented with two observability
ledgers:

* a `Trace` — the strings printed, in order (the kernel's observable, the
  contract ledger's "stdout bytes"), and
* a `Calls` list — every definition the derivation entered, the formal
  meaning of L1's "executions that pass through the edited definition".

`!` is modeled by splitting outcomes into `Res.norm` (the expression's
value) and `Res.abrupt` (a `!` fired on `err v` and aborts the enclosing
BODY; the caller reifies it as the ordinary value `err v` — `Ev.callReify`,
ADR-0008's explicit propagation with `?` at the call boundary).

What is *not* mechanized here (and where it lives instead): that the two
backends implement this semantics — a refinement obligation gated by the
`spec/wasm_cross` fixtures and the contract ledger (Stage 3, per
`docs/contracts/proven-vs-trusted.md`); and the compiler-pass violations the
2026-08-15 hunt fixed at the implementation level (#1424–#1426).
-/

namespace LambdaAlmd

/-- Names of variables and definitions. Resolution never uses anything but
the name itself — the kernel has no scopes-of-scopes, no overload sets. -/
abbrev Name := String

/-- Values. `vOk`/`vErr` are the `Result` constructors; `vUnit` is what
`print` returns. -/
inductive Val where
  | vInt  : Int → Val
  | vStr  : String → Val
  | vUnit : Val
  | vOk   : Val → Val
  | vErr  : Val → Val
deriving DecidableEq, Repr

/-- Expressions. `prop e` is `e!`; `orElse e d` is `e ?? d`;
`matchR s x e₁ y e₂` is `match s { ok(x) => e₁, err(y) => e₂ }`. -/
inductive Expr where
  | intLit : Int → Expr
  | strLit : String → Expr
  | var    : Name → Expr
  | letE   : Name → Expr → Expr → Expr
  | call   : Name → Expr → Expr
  | ok     : Expr → Expr
  | err    : Expr → Expr
  | matchR : Expr → Name → Expr → Name → Expr → Expr
  | prop   : Expr → Expr
  | orElse : Expr → Expr → Expr
  | print  : Expr → Expr
  | seq    : Expr → Expr → Expr
deriving Repr

/-- A top-level definition: one parameter, a body, and the DECLARED effect
flag. The flag is part of the signature — a body edit cannot change it,
which is exactly the almide rule (`effect` is syntax, not inference). -/
structure Defn where
  param    : Name
  body     : Expr
  isEffect : Bool
deriving Repr

/-- The definition table. "The program" for the kernel's purposes. -/
abbrev Defs := Name → Option Defn

/-- Runtime environments (variable bindings). -/
abbrev Env := Name → Option Val

/-- Finite-map update, shared by environments, definition tables, and typing
contexts. `upd m x v` is the table that answers `v` at `x` and defers to `m`
everywhere else — the formal meaning of "an edit to one definition". -/
def upd {α : Type} (m : Name → Option α) (x : Name) (v : α) : Name → Option α :=
  fun y => if y = x then some v else m y

@[simp] theorem upd_same {α : Type} (m : Name → Option α) (x : Name) (v : α) :
    upd m x v x = some v := by simp [upd]

@[simp] theorem upd_other {α : Type} (m : Name → Option α) (x y : Name) (v : α)
    (h : y ≠ x) : upd m x v y = m y := by simp [upd, h]

/-- The empty environment: call bodies see their parameter and nothing else
(top-level definitions capture nothing). -/
def emptyEnv : Env := fun _ => none

/-- Outcome of evaluating an expression inside a body: `norm v` is the
value; `abrupt v` means a `!` fired on `err v` and aborts the enclosing
body. -/
inductive Res where
  | norm   : Val → Res
  | abrupt : Val → Res
deriving DecidableEq, Repr

/-- The printed strings, in order — the kernel's observable set. -/
abbrev Trace := List String

/-- Every definition the derivation entered, in call order — the formal
"passed through" of L1. -/
abbrev Calls := List Name

/-- Big-step evaluation, instrumented with the trace and the call ledger.

The single load-bearing design fact: the ONLY rules that consult the
definition table `D` are the two call rules, and they consult it at exactly
the called name. Everything the Stage-2 theorems say follows from that
shape being an inductive invariant. -/
inductive Ev (D : Defs) : Env → Expr → Res → Trace → Calls → Prop where
  | intLit {ρ : Env} {n : Int} :
      Ev D ρ (.intLit n) (.norm (.vInt n)) [] []
  | strLit {ρ : Env} {s : String} :
      Ev D ρ (.strLit s) (.norm (.vStr s)) [] []
  | var {ρ : Env} {x : Name} {v : Val} :
      ρ x = some v →
      Ev D ρ (.var x) (.norm v) [] []
  | letNorm {ρ : Env} {x : Name} {e₁ e₂ : Expr} {v : Val} {r : Res}
      {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ e₁ (.norm v) t₁ c₁ →
      Ev D (upd ρ x v) e₂ r t₂ c₂ →
      Ev D ρ (.letE x e₁ e₂) r (t₁ ++ t₂) (c₁ ++ c₂)
  | letAbrupt {ρ : Env} {x : Name} {e₁ e₂ : Expr} {v : Val}
      {t : Trace} {c : Calls} :
      Ev D ρ e₁ (.abrupt v) t c →
      Ev D ρ (.letE x e₁ e₂) (.abrupt v) t c
  | callNorm {ρ : Env} {f : Name} {a : Expr} {d : Defn} {va v : Val}
      {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ a (.norm va) t₁ c₁ →
      D f = some d →
      Ev D (upd emptyEnv d.param va) d.body (.norm v) t₂ c₂ →
      Ev D ρ (.call f a) (.norm v) (t₁ ++ t₂) (c₁ ++ f :: c₂)
  | callReify {ρ : Env} {f : Name} {a : Expr} {d : Defn} {va v : Val}
      {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ a (.norm va) t₁ c₁ →
      D f = some d →
      Ev D (upd emptyEnv d.param va) d.body (.abrupt v) t₂ c₂ →
      Ev D ρ (.call f a) (.norm (.vErr v)) (t₁ ++ t₂) (c₁ ++ f :: c₂)
  | callArgAbrupt {ρ : Env} {f : Name} {a : Expr} {v : Val}
      {t : Trace} {c : Calls} :
      Ev D ρ a (.abrupt v) t c →
      Ev D ρ (.call f a) (.abrupt v) t c
  | okNorm {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm v) t c →
      Ev D ρ (.ok e) (.norm (.vOk v)) t c
  | okAbrupt {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.abrupt v) t c →
      Ev D ρ (.ok e) (.abrupt v) t c
  | errNorm {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm v) t c →
      Ev D ρ (.err e) (.norm (.vErr v)) t c
  | errAbrupt {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.abrupt v) t c →
      Ev D ρ (.err e) (.abrupt v) t c
  | matchOk {ρ : Env} {s : Expr} {x y : Name} {e₁ e₂ : Expr} {v : Val}
      {r : Res} {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ s (.norm (.vOk v)) t₁ c₁ →
      Ev D (upd ρ x v) e₁ r t₂ c₂ →
      Ev D ρ (.matchR s x e₁ y e₂) r (t₁ ++ t₂) (c₁ ++ c₂)
  | matchErr {ρ : Env} {s : Expr} {x y : Name} {e₁ e₂ : Expr} {v : Val}
      {r : Res} {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ s (.norm (.vErr v)) t₁ c₁ →
      Ev D (upd ρ y v) e₂ r t₂ c₂ →
      Ev D ρ (.matchR s x e₁ y e₂) r (t₁ ++ t₂) (c₁ ++ c₂)
  | matchAbrupt {ρ : Env} {s : Expr} {x y : Name} {e₁ e₂ : Expr} {v : Val}
      {t : Trace} {c : Calls} :
      Ev D ρ s (.abrupt v) t c →
      Ev D ρ (.matchR s x e₁ y e₂) (.abrupt v) t c
  | propOk {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm (.vOk v)) t c →
      Ev D ρ (.prop e) (.norm v) t c
  | propErr {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm (.vErr v)) t c →
      Ev D ρ (.prop e) (.abrupt v) t c
  | propAbrupt {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.abrupt v) t c →
      Ev D ρ (.prop e) (.abrupt v) t c
  | orElseOk {ρ : Env} {e d' : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm (.vOk v)) t c →
      Ev D ρ (.orElse e d') (.norm v) t c
  | orElseErr {ρ : Env} {e d' : Expr} {w : Val} {r : Res}
      {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ e (.norm (.vErr w)) t₁ c₁ →
      Ev D ρ d' r t₂ c₂ →
      Ev D ρ (.orElse e d') r (t₁ ++ t₂) (c₁ ++ c₂)
  | orElseAbrupt {ρ : Env} {e d' : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.abrupt v) t c →
      Ev D ρ (.orElse e d') (.abrupt v) t c
  | printNorm {ρ : Env} {e : Expr} {s : String} {t : Trace} {c : Calls} :
      Ev D ρ e (.norm (.vStr s)) t c →
      Ev D ρ (.print e) (.norm .vUnit) (t ++ [s]) c
  | printAbrupt {ρ : Env} {e : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e (.abrupt v) t c →
      Ev D ρ (.print e) (.abrupt v) t c
  | seqNorm {ρ : Env} {e₁ e₂ : Expr} {v : Val} {r : Res}
      {t₁ t₂ : Trace} {c₁ c₂ : Calls} :
      Ev D ρ e₁ (.norm v) t₁ c₁ →
      Ev D ρ e₂ r t₂ c₂ →
      Ev D ρ (.seq e₁ e₂) r (t₁ ++ t₂) (c₁ ++ c₂)
  | seqAbrupt {ρ : Env} {e₁ e₂ : Expr} {v : Val} {t : Trace} {c : Calls} :
      Ev D ρ e₁ (.abrupt v) t c →
      Ev D ρ (.seq e₁ e₂) (.abrupt v) t c

end LambdaAlmd
