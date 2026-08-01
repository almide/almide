/-!
# The decisive-event selection algebra of `fan.race`

Machine-checked core of docs/roadmap/active/logical-time-async.md (semantics)
and docs/roadmap/active/logical-time-proofs.md (the proof ledger).

The setting: every branch of a `fan.race` is a deterministic charge trace; its
terminal (completion or trap) is an *event* `⟨time, idx⟩` where `time` is the
branch's cumulative fuel at the terminal and `idx` its source position. The
race's outcome is the merge-order minimum — the *decisive event* — of the
candidate set. Implementations (sequential scan, parallel with pruning, lazy
cap checks) differ only in WHICH candidates they record; these theorems show
the decision cannot depend on that:

* `decisive_unique`  — the decisive event is unique (determinism of selection).
* `decisive_subset`  — minimality survives restriction to any recorded subset.
* `cap_admits_decisive` / `cap_admits_window` — the pruning-cap rule never
  blocks the decisive event, nor any event that semantically precedes it
  (the trap visibility window).
* `decide_stable`    — the confluence theorem: any implementation that records
  a subset of the true candidates containing the decisive event, and returns
  a minimal recorded element, returns THE decisive event.

What is *not* mechanized here (and where it lives instead): that each branch's
trace is a function of (program, input, budget) — definitional on the pure
fragment, argued in logical-time-proofs.md; and that the two renderers
preserve charge traces — an implementation obligation gated by the
charge-trace validator and the `spec/wasm_cross` fixtures, per
docs/contracts/proven-vs-trusted.md.
-/

/-- A terminal event of one race branch: the branch's cumulative fuel at its
terminal, and the branch's source index. Branch indices are pairwise distinct
in any candidate set (a branch terminates at most once). -/
structure Ev where
  time : Nat
  idx  : Nat
deriving DecidableEq, Repr

namespace Ev

/-- Strict merge-order precedence: earlier logical time first; equal times
resolve by source order. This is the lockstep order — "1 fuel per tick,
simultaneous events in source order" — with the scheduler nowhere in sight. -/
def prec (a b : Ev) : Prop :=
  a.time < b.time ∨ (a.time = b.time ∧ a.idx < b.idx)

theorem prec_irrefl (a : Ev) : ¬ prec a a := by
  intro h
  cases h with
  | inl h => omega
  | inr h => omega

/-- Events of distinct branches are always ordered (totality). -/
theorem prec_total (a b : Ev) (hne : a.idx ≠ b.idx) : prec a b ∨ prec b a := by
  unfold prec
  omega

/-- `d` is the decisive event of candidate set `C`: a member no candidate
strictly precedes. -/
def IsDecisive (d : Ev) (C : List Ev) : Prop :=
  d ∈ C ∧ ∀ c ∈ C, ¬ prec c d

/-- **Determinism of selection.** With pairwise-distinct branch indices the
decisive event is unique — there is exactly one answer for "who won",
independent of how the candidate set was traversed. -/
theorem decisive_unique {C : List Ev}
    (hinj : ∀ a ∈ C, ∀ b ∈ C, a.idx = b.idx → a = b)
    {d₁ d₂ : Ev} (h₁ : IsDecisive d₁ C) (h₂ : IsDecisive d₂ C) : d₁ = d₂ := by
  have hn₁ : ¬ prec d₁ d₂ := h₂.2 d₁ h₁.1
  have hn₂ : ¬ prec d₂ d₁ := h₁.2 d₂ h₂.1
  have hidx : d₁.idx = d₂.idx := by
    unfold prec at hn₁ hn₂
    omega
  exact hinj d₁ h₁.1 d₂ h₂.1 hidx

/-- **Restriction stability.** The decisive event of the full candidate set is
still decisive in any subset that contains it. Read outward: extra recorded
candidates (from lazy cap checks that overran) can never change the decision.
Read inward with `Cc` = the completions: when the decisive event is a
completion, it is the (spend, index)-lexicographic minimum of the completions
— the "winner = lexmin" characterisation. -/
theorem decisive_subset {C R : List Ev} {d : Ev}
    (hsub : ∀ e ∈ R, e ∈ C) (hd : IsDecisive d C) (hdR : d ∈ R) :
    IsDecisive d R := by
  refine ⟨hdR, ?_⟩
  intro c hc
  exact hd.2 c (hsub c hc)

/-- The pruning cap a recorded candidate `dcur` imposes on branch `k`: an
implementation may stop branch `k` once its next charge would pass this time.
A branch after `dcur`'s source position must be strictly earlier to precede
it, hence the `- 1`. -/
def cap (dcur : Ev) (k : Nat) : Nat :=
  if dcur.idx < k then dcur.time - 1 else dcur.time

/-- **The cap never hides the decisive event.** Whatever candidate the cap was
computed from — the interim best of a sequential scan, or any candidate some
parallel schedule happened to record first — the true decisive branch is
allowed to run at least to its decisive time. Hence every completed schedule
records the decisive event. -/
theorem cap_admits_decisive {C : List Ev} {d dcur : Ev}
    (hd : IsDecisive d C) (hcur : dcur ∈ C) :
    d.time ≤ cap dcur d.idx := by
  have hn : ¬ prec dcur d := hd.2 dcur hcur
  unfold prec at hn
  unfold cap
  split <;> omega

/-- **The cap never hides the visibility window.** Any event that semantically
precedes the decisive event — in particular a trap inside the visibility
window — survives every admissible cap: pruning cannot make an observable
trap unobservable. -/
theorem cap_admits_window {C : List Ev} {d dcur : Ev} (e : Ev)
    (hd : IsDecisive d C) (hcur : dcur ∈ C) (he : prec e d) :
    e.time ≤ cap dcur e.idx := by
  have hn : ¬ prec dcur d := hd.2 dcur hcur
  unfold prec at hn
  cases he with
  | inl h =>
    unfold cap
    split <;> omega
  | inr h =>
    unfold cap
    split <;> omega

/-- **Confluence.** Any implementation that (i) records a subset of the true
candidates, (ii) records at least the decisive event — guaranteed by
`cap_admits_decisive` for every schedule that runs to quiescence — and
(iii) answers with a minimal recorded element, answers with THE decisive
event. Sequential scan, parallel execution, pruning, and lazy cap checks all
fall inside (i)–(iii); this is the model-checked ADV confluence, proved. -/
theorem decide_stable {C R : List Ev} {d dR : Ev}
    (hinj : ∀ a ∈ C, ∀ b ∈ C, a.idx = b.idx → a = b)
    (hsub : ∀ e ∈ R, e ∈ C)
    (hd : IsDecisive d C) (hdR : d ∈ R)
    (hR : IsDecisive dR R) : dR = d := by
  have hdR' : IsDecisive d R := decisive_subset hsub hd hdR
  exact decisive_unique
    (fun a ha b hb => hinj a (hsub a ha) b (hsub b hb)) hR hdR'

end Ev
