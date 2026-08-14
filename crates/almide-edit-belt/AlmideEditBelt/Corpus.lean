import AlmideEditBelt.Evaluator
import AlmideEditBelt.Typing

/-!
# The generated kernel-conformance corpus

Stage 3 completion: the hand-written seven-program family
(`Conformance.lean`) scaled to the λ_almd-expressible fragment. A typed,
deterministic generator produces surface programs; the REVIEWED SEAM is the
pair `eraseE`/`eraseChain` (surface → λ_almd, the term whose kernel
observables `evalE` computes — theorems by `eval_sound` + `ev_det`) and
`render*` (surface → almide text, what the compiler actually eats). Both
walk the same tiny `SExpr`/`SStmt` grammar constructor-by-constructor;
reviewing that they agree IS the trust obligation, and it is ~80 lines.

The corpus itself is committed under `proofs/kernel-conformance/` —
`gen_NN.almd` plus `gen_NN.expected` (the kernel trace). Drift is caught
twice: the `conformancegen --check` CI step regenerates and byte-compares,
and `tests/kernel_conformance_test.rs` runs every program on native (and
wasm where a runtime exists) against the expected trace.

Generation is type-directed and closed by construction: no recursion (a
definition calls only earlier definitions), every variable reference is
bound, `print` takes strings only, `!` never appears in `main`'s own body
(so `main` cannot abort and the expected exit code is always 0), and
`seq`-chains put only unit-typed statements before the tail. Under those
invariants the evaluator returns `some` for every program — `#guard
corpusOK` checks it at Lean compile time.
-/

namespace LambdaAlmd

/-! ## The surface mini-AST (annotated where almide needs annotations) -/

/-- Inline surface expressions — the printable/erasable core. -/
inductive SExpr where
  | sInt (n : Nat)
  | sStr (s : String)
  | sVar (x : Name)
  | sCall (f : Name) (a : SExpr)
  | sOk (e : SExpr)
  | sErr (e : SExpr)
  | sProp (e : SExpr)
  | sOrElse (e d : SExpr)
deriving Repr

/-- Surface statements (unit-position forms). `sMatchUnit` fixes both arms
to `println` of an inline expression — the shape every generated match
takes. `sLet` carries the type annotation almide wants on Result-typed
bindings. -/
inductive SStmt where
  | sPrintln (e : SExpr)
  | sLet (x : Name) (ty : Ty) (rhs : SExpr)
  | sMatchUnit (subj : SExpr) (xv yv : Name) (okMsg errMsg : SExpr)
deriving Repr

/-- An effect-fn body: statements, then a value tail (ok-lifted by the
compiler; the erasure adds the matching `.ok`). -/
structure SEffBody where
  stmts : List SStmt
  tail  : SExpr
deriving Repr

inductive SDefBody where
  | pureB (e : SExpr)
  | effB (b : SEffBody)
deriving Repr

/-- One surface definition. All definitions are unary with parameter `x`
(the kernel is unary; the generator never needs a second name). `retTy` is
the SURFACE return annotation — for an effect fn that is the payload type,
per the tail-lift convention. -/
structure SDef where
  name  : Name
  argTy : Ty
  retTy : Ty
  body  : SDefBody
deriving Repr

/-- A program: definitions (each may call only earlier ones) and `main`'s
statements (never `sLet`-final, never containing `!`). -/
structure SProg where
  defs      : List SDef
  mainStmts : List SStmt
deriving Repr

/-! ## Erasure: surface → λ_almd (reviewed seam, half one) -/

def eraseE : SExpr → Expr
  | .sInt n => .intLit n
  | .sStr s => .strLit s
  | .sVar x => .var x
  | .sCall f a => .call f (eraseE a)
  | .sOk e => .ok (eraseE e)
  | .sErr e => .err (eraseE e)
  | .sProp e => .prop (eraseE e)
  | .sOrElse e d => .orElse (eraseE e) (eraseE d)

/-- The unit-valued kernel image of one statement. (`sLet` is never a
standalone statement by generator invariant — the chain functions bind it
properly; the total fallback here is inert.) -/
def eraseStmtE : SStmt → Expr
  | .sPrintln e => .print (eraseE e)
  | .sLet x _ rhs => .letE x (eraseE rhs) (.strLit "")
  | .sMatchUnit subj xv yv okM errM =>
      .matchR (eraseE subj) xv (.print (eraseE okM)) yv (.print (eraseE errM))

/-- Statements-then-tail, as one kernel expression. -/
def eraseChain : List SStmt → SExpr → Expr
  | [], tail => eraseE tail
  | .sLet x _ rhs :: rest, tail => .letE x (eraseE rhs) (eraseChain rest tail)
  | s :: rest, tail => .seq (eraseStmtE s) (eraseChain rest tail)

/-- `main`'s statements as one kernel expression — the LAST statement is
the tail (generator never ends `main` on `sLet`). -/
def eraseMain : List SStmt → Expr
  | [] => .print (.strLit "")
  | [s] => eraseStmtE s
  | .sLet x _ rhs :: rest => .letE x (eraseE rhs) (eraseMain rest)
  | s :: rest => .seq (eraseStmtE s) (eraseMain rest)

def defnOf (d : SDef) : Defn :=
  match d.body with
  | .pureB e => ⟨"x", eraseE e, false⟩
  | .effB b => ⟨"x", .ok (eraseChain b.stmts b.tail), true⟩

def defsOf (p : SProg) : Defs :=
  p.defs.foldl (fun D d => upd D d.name (defnOf d)) (fun _ => none)

/-- The kernel observables of a program — by `eval_sound` + `ev_det`, THE
observables. -/
def expectedOf (p : SProg) : Option Out :=
  evalE (defsOf p) 4096 emptyEnv (eraseMain p.mainStmts)

/-! ## Rendering: surface → almide text (reviewed seam, half two) -/

def rTy : Ty → String
  | .int => "Int"
  | .str => "String"
  | .unit => "Unit"
  | .res t => s!"Result[{rTy t}, String]"

/-- Postfix `!` and infix `??` need parentheses when nested in each other;
everything else renders atomically. -/
def needsParens : SExpr → Bool
  | .sProp _ | .sOrElse _ _ => true
  | _ => false

def rE : SExpr → String
  | .sInt n => toString n
  | .sStr s => "\"" ++ s ++ "\""
  | .sVar x => x
  | .sCall f a => s!"{f}({rE a})"
  | .sOk e => s!"ok({rE e})"
  | .sErr e => s!"err({rE e})"
  | .sProp e =>
      let inner := rE e
      if needsParens e then s!"({inner})!" else s!"{inner}!"
  | .sOrElse e d =>
      let l := rE e
      let r := rE d
      let l := if needsParens e then s!"({l})" else l
      let r := if needsParens d then s!"({r})" else r
      s!"{l} ?? {r}"

def rStmt : SStmt → List String
  | .sPrintln e => [s!"println({rE e})"]
  | .sLet x ty rhs => [s!"let {x}: {rTy ty} = {rE rhs}"]
  | .sMatchUnit subj xv yv okM errM =>
      [s!"match {rE subj} \{",
       s!"  ok({xv}) => println({rE okM}),",
       s!"  err({yv}) => println({rE errM}),",
       "}"]

def indent (lines : List String) : List String := lines.map ("  " ++ ·)

def rDef (d : SDef) : List String :=
  match d.body with
  | .pureB e =>
      [s!"fn {d.name}(x: {rTy d.argTy}) -> {rTy d.retTy} = {rE e}", ""]
  | .effB b =>
      if b.stmts.isEmpty then
        [s!"effect fn {d.name}(x: {rTy d.argTy}) -> {rTy d.retTy} = {rE b.tail}", ""]
      else
        [s!"effect fn {d.name}(x: {rTy d.argTy}) -> {rTy d.retTy} = \{"]
          ++ indent (b.stmts.flatMap rStmt)
          ++ [s!"  {rE b.tail}", "}", ""]

def renderProg (p : SProg) : String :=
  let header :=
    ["// GENERATED by `lake exe conformancegen` (crates/almide-edit-belt) — DO NOT EDIT.",
     "// The kernel-expected observables live in the sibling .expected file;",
     "// they are Lean-compile-time theorems (Corpus.lean, #guard + eval_sound + ev_det).",
     ""]
  let mainLines :=
    ["effect fn main() -> Unit = {"] ++ indent (p.mainStmts.flatMap rStmt) ++ ["}"]
  String.intercalate "\n" (header ++ p.defs.flatMap rDef ++ mainLines) ++ "\n"

/-- The expected-stdout text: one line per trace entry. -/
def expectedText (o : Out) : String :=
  o.2.1.foldl (fun acc line => acc ++ line ++ "\n") ""

/-! ## Deterministic generation -/

/-- A splitmix-flavored LCG; pure, seeded, no ambient state. -/
structure Rng where
  s : UInt64
deriving Repr

def Rng.next (r : Rng) : Nat × Rng :=
  let s' := r.s * 6364136223846793005 + 1442695040888963407
  (((s' >>> 33).toNat), ⟨s'⟩)

abbrev G := StateM Rng

def below (n : Nat) : G Nat := do
  let r ← get
  let (v, r') := r.next
  set r'
  pure (v % (max n 1))

def pickL (xs : List α) (fallback : α) : G α := do
  let i ← below xs.length
  pure (xs.getD i fallback)

def words : List String :=
  ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel",
   "india", "juliet", "kilo", "lima", "mike", "november", "oscar", "papa"]

def word : G String := pickL words "alpha"

def smallInt : G Nat := below 10

/-- Available callables, tracked by kind so generation stays well-typed. -/
structure Ctx where
  pureRes : List Name  -- Int → Result[Int, String]
  strFns  : List Name  -- String → String
  effFns  : List Name  -- effect, Int → Int (call type Result[Int, String])

/-- An inline `Int`-typed expression (inside a definition body, `x : Int`
may be in scope). -/
def genInt (useX : Bool) : G SExpr := do
  let k ← smallInt
  if useX then
    let c ← below 3
    pure (if c == 0 then .sVar "x" else .sInt k)
  else
    pure (.sInt k)

/-- An inline `Result[Int, String]`-typed expression — ANY shape. Only safe
where the surface anchors the type (an annotated `let`, a declared return):
a bare `err(w)` in subject position is checker-accepted but dies in codegen
(almide#1428), so `genResInferable` exists for those positions. -/
def genRes (ctx : Ctx) (useX : Bool) : G SExpr := do
  let a ← genInt useX
  let c ← below (4 + ctx.pureRes.length + ctx.effFns.length)
  match c with
  | 0 => pure (.sOk a)
  | 1 => do let w ← word; pure (.sErr (.sStr w))
  | 2 => pure (.sOk a)
  | 3 => do let w ← word; pure (.sErr (.sStr w))
  | n =>
      let idx := n - 4
      if h : idx < ctx.pureRes.length then
        pure (.sCall ctx.pureRes[idx] a)
      else
        let j := idx - ctx.pureRes.length
        pure (.sCall (ctx.effFns.getD j "missing") a)

/-- A `Result`-typed expression whose ok payload the surface can infer
WITHOUT an annotation: `ok(intExpr)` or a call with a declared signature.
The only legal shapes for a bare `match` subject until almide#1428 lands. -/
def genResInferable (ctx : Ctx) (useX : Bool) : G SExpr := do
  let a ← genInt useX
  let calls := ctx.pureRes ++ ctx.effFns
  let c ← below (1 + calls.length)
  if c == 0 then
    pure (.sOk a)
  else
    pure (.sCall (calls.getD (c - 1) "missing") a)

/-- An inline `String`-typed expression for `println`. -/
def genStr (ctx : Ctx) : G SExpr := do
  let w ← word
  let c ← below (2 + ctx.strFns.length)
  if c < 2 then
    pure (.sStr w)
  else
    pure (.sCall (ctx.strFns.getD (c - 2) "missing") (.sStr w))

/-- One to two unit statements usable anywhere (no `!`). `uniq` keeps the
annotated-let-then-match form's binding unique in its scope. -/
def genStmts (ctx : Ctx) (useX : Bool) (uniq : String) : G (List SStmt) := do
  let c ← below 4
  match c with
  | 0 => do let e ← genStr ctx; pure [.sPrintln e]
  | 1 => do let e ← genStr ctx; pure [.sPrintln e]
  | 2 => do
      let subj ← genResInferable ctx useX
      let w1 ← word
      let w2 ← word
      pure [.sMatchUnit subj "_v" "_e" (.sStr w1) (.sStr w2)]
  | _ => do
      -- The annotated-let form anchors the payload type, so ANY Result
      -- shape (a bare err included) is legal as the bound value.
      let rhs ← genRes ctx useX
      let w1 ← word
      let w2 ← word
      let m := s!"m{uniq}"
      pure [.sLet m (.res .int) rhs,
            .sMatchUnit (.sVar m) "_v" "_e" (.sStr w1) (.sStr w2)]

/-- One pure definition `Int → Result[Int, String]`. -/
def genPureDef (ctx : Ctx) (name : Name) : G SDef := do
  let c ← below (3 + (if ctx.pureRes.isEmpty then 0 else 1))
  let body ← match c with
    | 0 => pure (SExpr.sOk (.sVar "x"))
    | 1 => do let k ← smallInt; pure (SExpr.sOk (.sInt k))
    | 2 => do let w ← word; pure (SExpr.sErr (.sStr w))
    | _ => do
        let f ← pickL ctx.pureRes "missing"
        let k ← smallInt
        pure (SExpr.sOk (.sOrElse (.sCall f (.sVar "x")) (.sInt k)))
  pure ⟨name, .int, .res .int, .pureB body⟩

/-- One effect definition, surface `Int → Int` (call type
`Result[Int, String]`). Shape A binds an annotated Result and `!`s it in
the tail; shape B is statements plus an inline tail. -/
def genEffDef (ctx : Ctx) (name : Name) : G SDef := do
  let nStmts ← below 3
  let stmtss ← (List.range nStmts).mapM (fun i => genStmts ctx true s!"{name}{i}")
  let stmts := stmtss.flatten
  let shapeA ← below 2
  if shapeA == 0 then
    let rhs ← genRes ctx true
    let tail := SExpr.sProp (.sVar "r")
    pure ⟨name, .int, .int, .effB ⟨stmts ++ [.sLet "r" (.res .int) rhs], tail⟩⟩
  else
    -- Tail is a LITERAL only: a bare-parameter tail (`= x`) splits the v1
    -- renderer's signature from its body (almide#1429) — re-admit `sVar "x"`
    -- here when that lands.
    let tail ← genInt false
    pure ⟨name, .int, .int, .effB ⟨stmts, tail⟩⟩

/-- A whole program. -/
def genProg : G SProg := do
  let nPure ← below 2
  let nEff ← below 2
  let mut ctx : Ctx := ⟨[], [], []⟩
  let mut defs : List SDef := []
  -- string passthrough, always present so println has a call form
  let sd : SDef := ⟨"s0", .str, .str, .pureB (.sVar "x")⟩
  defs := defs ++ [sd]
  ctx := { ctx with strFns := ["s0"] }
  for i in List.range (nPure + 2) do
    let name := s!"p{i}"
    let d ← genPureDef ctx name
    defs := defs ++ [d]
    ctx := { ctx with pureRes := ctx.pureRes ++ [name] }
  for i in List.range (nEff + 1) do
    let name := s!"e{i}"
    let d ← genEffDef ctx name
    defs := defs ++ [d]
    ctx := { ctx with effFns := ctx.effFns ++ [name] }
  let nMain ← below 5
  let stmtss ← (List.range (nMain + 3)).mapM (fun i => genStmts ctx false s!"{i}")
  pure ⟨defs, stmtss.flatten⟩

/-- Corpus size — bump deliberately, never silently. -/
def corpusSize : Nat := 48

/-- The committed corpus: program `i` is generated from seed `0xA1D0 + i`,
so the corpus is a pure function of this file. -/
def corpus : List SProg :=
  (List.range corpusSize).map fun i =>
    (genProg.run ⟨UInt64.ofNat (0xA1D0 + i)⟩).1

/-- Every generated program has kernel observables (the generator's typed
invariants hold) — checked at Lean compile time. -/
def corpusOK : Bool := corpus.all fun p => (expectedOf p).isSome

#guard corpusOK

end LambdaAlmd
