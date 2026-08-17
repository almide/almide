import AlmideEditBelt.Kernel

/-!
# Signature-only typing for λ_almd

The typing judgment consults the SIGNATURE table `S` and never a body —
`HasTy.call` reads `S f`, full stop. That single structural fact is the
typed half of edit locality:

* `typing_modular` — replace `f`'s body with any new body that checks
  against `f`'s OWN signature, and the whole program stays well-typed. The
  other definitions' typing derivations are not "re-checked and found
  unchanged"; they are the SAME proof objects, because nothing in them
  mentions the old body. This is `docs/specs/edit-locality.md` §2's
  "signatures are text" row, stated as mathematics.

The effect discipline is the kernel form of E006/ADR-0008: `print` and `!`
type only at `eff = true` (inside an `effect fn`), and a call to an
effectful definition requires an effectful context. `Purity.lean` turns
that into the semantic guarantee that pure code is silent.
-/

namespace LambdaAlmd

/-- Types: `res τ` is `Result[τ, Str]` — the error payload is fixed at
`Str`, which is all the kernel needs (`error` is stringly-typed at the
almide surface too, ADR-0003 notwithstanding for richer refinements). -/
inductive Ty where
  | int  : Ty
  | str  : Ty
  | unit : Ty
  | res  : Ty → Ty
deriving DecidableEq, Repr

/-- A declared signature: argument type, return type, and the `effect`
flag. This — not the body — is everything a caller's typing may consult. -/
structure Sig where
  arg      : Ty
  ret      : Ty
  isEffect : Bool
deriving DecidableEq, Repr

/-- The signature table. -/
abbrev Sigs := Name → Option Sig

/-- Typing contexts for variables. -/
abbrev TyEnv := Name → Option Ty

/-- The empty typing context: a body is checked under its parameter alone. -/
def emptyTyEnv : TyEnv := fun _ => none

/-- Expression typing. `eff` says whether we are inside an `effect fn`
body; `print` and `prop` (`!`) demand it, and calling an effectful
definition demands it (E006). The `call` rule consults `S` at the called
name and nothing else — the design fact `typing_modular` runs on. -/
inductive HasTy (S : Sigs) : Bool → TyEnv → Expr → Ty → Prop where
  | intLit {eff : Bool} {Γ : TyEnv} {n : Int} :
      HasTy S eff Γ (.intLit n) .int
  | strLit {eff : Bool} {Γ : TyEnv} {s : String} :
      HasTy S eff Γ (.strLit s) .str
  | var {eff : Bool} {Γ : TyEnv} {x : Name} {τ : Ty} :
      Γ x = some τ →
      HasTy S eff Γ (.var x) τ
  | letE {eff : Bool} {Γ : TyEnv} {x : Name} {e₁ e₂ : Expr} {τ₁ τ₂ : Ty} :
      HasTy S eff Γ e₁ τ₁ →
      HasTy S eff (upd Γ x τ₁) e₂ τ₂ →
      HasTy S eff Γ (.letE x e₁ e₂) τ₂
  | call {eff : Bool} {Γ : TyEnv} {f : Name} {a : Expr} {sig : Sig} :
      S f = some sig →
      (sig.isEffect = true → eff = true) →
      HasTy S eff Γ a sig.arg →
      HasTy S eff Γ (.call f a) sig.ret
  | ok {eff : Bool} {Γ : TyEnv} {e : Expr} {τ : Ty} :
      HasTy S eff Γ e τ →
      HasTy S eff Γ (.ok e) (.res τ)
  | err {eff : Bool} {Γ : TyEnv} {e : Expr} {τ : Ty} :
      HasTy S eff Γ e .str →
      HasTy S eff Γ (.err e) (.res τ)
  | matchR {eff : Bool} {Γ : TyEnv} {s : Expr} {x y : Name} {e₁ e₂ : Expr}
      {τ σ : Ty} :
      HasTy S eff Γ s (.res τ) →
      HasTy S eff (upd Γ x τ) e₁ σ →
      HasTy S eff (upd Γ y .str) e₂ σ →
      HasTy S eff Γ (.matchR s x e₁ y e₂) σ
  | prop {Γ : TyEnv} {e : Expr} {τ : Ty} :
      HasTy S true Γ e (.res τ) →
      HasTy S true Γ (.prop e) τ
  | orElse {eff : Bool} {Γ : TyEnv} {e d : Expr} {τ : Ty} :
      HasTy S eff Γ e (.res τ) →
      HasTy S eff Γ d τ →
      HasTy S eff Γ (.orElse e d) τ
  | print {Γ : TyEnv} {e : Expr} :
      HasTy S true Γ e .str →
      HasTy S true Γ (.print e) .unit
  | seq {eff : Bool} {Γ : TyEnv} {e₁ e₂ : Expr} {τ₁ τ₂ : Ty} :
      HasTy S eff Γ e₁ τ₁ →
      HasTy S eff Γ e₂ τ₂ →
      HasTy S eff Γ (.seq e₁ e₂) τ₂

/-- One definition checks against its own declared signature: the declared
effect flag matches, and the body types under the parameter alone, at the
definition's OWN effect level, at the declared return type. -/
def DefWT (S : Sigs) (f : Name) (d : Defn) : Prop :=
  ∃ sig, S f = some sig ∧ d.isEffect = sig.isEffect ∧
    HasTy S d.isEffect (upd emptyTyEnv d.param sig.arg) d.body sig.ret

/-- A whole program is well-typed when every definition checks. -/
def WT (S : Sigs) (D : Defs) : Prop :=
  ∀ f d, D f = some d → DefWT S f d

/-- **Typing is modular in bodies.** Replace `f` with any definition that
checks against `f`'s existing signature; the program stays well-typed, and
every OTHER definition's derivation is literally the same proof object —
typing never read the old body, so there is nothing to re-check. A language
whose call typing consulted bodies (or resolved overloads against the whole
table) could not state this without re-deriving every caller. -/
theorem typing_modular {S : Sigs} {D : Defs} {f : Name} {d' : Defn}
    (hwt : WT S D) (hd' : DefWT S f d') :
    WT S (upd D f d') := by
  intro g dg hg
  by_cases hgf : g = f
  · subst hgf
    rw [upd_same] at hg
    cases hg
    exact hd'
  · rw [upd_other _ _ _ _ hgf] at hg
    exact hwt g dg hg

end LambdaAlmd
