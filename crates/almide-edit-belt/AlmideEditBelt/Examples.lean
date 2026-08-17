import AlmideEditBelt.EditFrame

/-!
# Non-vacuity witnesses

Belt hygiene: concrete derivations showing the semantics runs, the edit
CAN change behavior on paths that enter the edited definition (so the
`f ∉ c` hypothesis of `edit_frame` is necessary, not decorative), and the
frame theorem transports a real execution across a real edit.
-/

namespace LambdaAlmd

/-- `effect fn f(x) { print "old" }` -/
def dOld : Defn := ⟨"x", .print (.strLit "old"), true⟩

/-- The edit: `effect fn f(x) { print "new" }` — same signature, new body. -/
def dNew : Defn := ⟨"x", .print (.strLit "new"), true⟩

/-- The one-definition program `{ f ↦ dOld }`. -/
def D₀ : Defs := upd (fun _ => none) "f" dOld

/-- The semantics runs: calling `f` prints `old` and enters `f`. -/
example : Ev D₀ emptyEnv (.call "f" (.intLit 0)) (.norm .vUnit) ["old"] ["f"] :=
  .callNorm .intLit (upd_same (fun _ => none) "f" dOld) (.printNorm .strLit)

/-- The `f ∉ c` hypothesis is necessary: the SAME expression, run after the
SAME edit, prints `new` — an execution that enters `f` is not framed. -/
example : Ev (upd D₀ "f" dNew) emptyEnv (.call "f" (.intLit 0))
    (.norm .vUnit) ["new"] ["f"] :=
  .callNorm .intLit (upd_same D₀ "f" dNew) (.printNorm .strLit)

/-- The frame theorem transports a real execution across a real edit: an
expression that never enters `f` keeps its trace verbatim under the edit. -/
example : Ev (upd D₀ "f" dNew) emptyEnv (.print (.strLit "hi"))
    (.norm .vUnit) ["hi"] [] :=
  edit_frame (D := D₀) (.printNorm .strLit) (by simp)

end LambdaAlmd
