(* Almide v1 trust spine — TRANSITIVE capability bound (the composition law).

   CapabilityBound.v proves the PER-FUNCTION check: a function's DIRECT used caps ⊆ its
   declared allowlist. But a function also reaches the caps of everything it CALLS, so the
   sandbox promise ("no network, even via a helper") is a property of the WHOLE call graph,
   not one function. The v1 classifier computes that transitive reach in UNTRUSTED Rust
   (corpus-wall's `reaches_capability_or_unknown` fold). This file proves the COMPOSITION
   that justifies it, moving the transitive reasoning into the kernel-proven base:

     if every function declares at least its OWN direct caps AND at least each CALLEE's
     declared caps, then its FULL transitive reach stays within its declared bound.

   So `accept ⟹ no undeclared capability is reached, even transitively through callees`.
   Built on the shared subset law (Subset.v). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Arith.
From Stdlib Require Import Bool.
From Stdlib Require Import String Ascii.
From Stdlib Require Import Lia.
From AlmideTrust Require Import Subset.

(* A function node: its declared allowlist, its DIRECT used caps, and the indices (into the
   program list) of the functions it calls. *)
Record Fn := { fallowed : list nat; fdirect : list nat; fcallees : list nat }.

Definition empty_fn : Fn := {| fallowed := []; fdirect := []; fcallees := [] |}.

(* A program is a list of function nodes; a callee index selects one (out-of-range ⟶ the
   empty node, which reaches nothing — a total, conservative lookup). *)
Definition lookup (prog : list Fn) (i : nat) : Fn := nth i prog empty_fn.

(* The TRANSITIVE used caps of function i: its direct caps plus, recursively, every callee's
   transitive caps. `fuel` bounds the recursion depth (a cap on call-chain length; the
   classifier uses the function count, which bounds any acyclic chain). *)
Fixpoint reaches (prog : list Fn) (fuel : nat) (i : nat) : list nat :=
  match fuel with
  | O => fdirect (lookup prog i)
  | S f => fdirect (lookup prog i) ++ flat_map (reaches prog f) (fcallees (lookup prog i))
  end.

(* THE PER-FUNCTION CHECK: its direct caps are declared, AND each callee declares no more
   than it does (caps are monotone up the call graph — a caller must cover its callees). *)
Definition fn_ok (prog : list Fn) (f : Fn) : bool :=
  andb (subset_check (fallowed f) (fdirect f))
       (forallb (fun j => subset_check (fallowed f) (fallowed (lookup prog j))) (fcallees f)).

(* THE PROGRAM CHECK: every function passes. *)
Definition prog_ok (prog : list Fn) : bool := forallb (fn_ok prog) prog.

(* Every looked-up node is fn_ok: an in-range index is a member (prog_ok), an out-of-range
   one is the empty node (fn_ok empty = true). *)
Lemma lookup_fn_ok :
  forall prog, prog_ok prog = true -> forall i, fn_ok prog (lookup prog i) = true.
Proof.
  intros prog Hprog i. unfold lookup.
  destruct (Nat.ltb i (List.length prog)) eqn:Hlt.
  - apply Nat.ltb_lt in Hlt.
    unfold prog_ok in Hprog. rewrite forallb_forall in Hprog.
    apply Hprog. apply nth_In. exact Hlt.
  - apply Nat.ltb_ge in Hlt.
    rewrite nth_overflow by exact Hlt. reflexivity.
Qed.

(* SOUNDNESS: under prog_ok, the FULL transitive reach of any function (at any fuel) stays
   within its declared allowlist — the transitive sandbox promise. *)
Theorem reaches_sound :
  forall prog, prog_ok prog = true ->
  forall fuel i, subset_prop (fallowed (lookup prog i)) (reaches prog fuel i).
Proof.
  intros prog Hprog.
  pose proof (lookup_fn_ok prog Hprog) as Hlook.
  induction fuel as [|f IH]; intros i.
  - (* fuel 0: reaches = direct ⊆ allowed (the per-function check). *)
    simpl.
    pose proof (Hlook i) as Hi. unfold fn_ok in Hi.
    apply andb_prop in Hi. destruct Hi as [Hd _].
    apply (subset_check_sound _ _ Hd).
  - (* fuel S f: direct ∪ ⋃ callees' reaches, each bounded by the per-edge monotone check. *)
    simpl. intros x Hx.
    apply in_app_or in Hx. destruct Hx as [Hxd | Hxc].
    + pose proof (Hlook i) as Hi. unfold fn_ok in Hi.
      apply andb_prop in Hi. destruct Hi as [Hd _].
      exact (subset_check_sound _ _ Hd x Hxd).
    + apply in_flat_map in Hxc. destruct Hxc as [j [Hjc Hxj]].
      (* IH: x reached by callee j ⟹ x ∈ allowed(j). *)
      pose proof (IH j x Hxj) as Hxaj.
      (* per-edge: allowed(j) ⊆ allowed(i). *)
      pose proof (Hlook i) as Hi. unfold fn_ok in Hi.
      apply andb_prop in Hi. destruct Hi as [_ Hcall].
      rewrite forallb_forall in Hcall.
      pose proof (Hcall j Hjc) as Hedge.
      exact (subset_check_sound _ _ Hedge x Hxaj).
Qed.

(* non-vacuous: a 2-function program where main (caps {1}) calls helper (caps {1}) — main
   declares {1,2}, covering both — is ACCEPTED, and main's transitive reach ⊆ {1,2}. *)
(* main: declares {1,2}, uses 2, calls helper (index 1); helper: declares {1}, uses 1. *)
Definition demo_ok : list Fn :=
  [ {| fallowed := [1;2]; fdirect := [2]; fcallees := [1] |}
  ; {| fallowed := [1];   fdirect := [1]; fcallees := []   |} ].
Example demo_ok_accepts : prog_ok demo_ok = true.
Proof. reflexivity. Qed.
Example demo_ok_reaches : reaches demo_ok 8 0 = [2; 1].
Proof. reflexivity. Qed.

(* rejected: helper reaches network (cap 0) that main does NOT declare — the per-edge check
   `allowed(helper) ⊆ allowed(main)` fails, so prog_ok is false (the leak is caught). *)
Definition demo_bad : list Fn :=
  [ {| fallowed := [1;2]; fdirect := [2]; fcallees := [1] |}
  ; {| fallowed := [0];   fdirect := [0]; fcallees := []   |} ].
Example demo_bad_rejects : prog_ok demo_bad = false.
Proof. reflexivity. Qed.

(* ─── witness parsing, INTERNALIZED INTO COQ (end-to-end like check_cert) ───
   A program witness is the call graph the compiler emits: functions separated by ';',
   each function `<allowed ids>|<direct ids>|<callee indices>` — three whitespace-separated
   decimal-nat lists (reusing Subset's `pnats`/`split_bar`). Callee entries are 0-based
   INDICES into the function list (line order). The parser is total; what it produces is
   what `prog_within` validates, so the whole "witness bytes ⟶ accept/reject" pipeline is the
   single extracted proven function `check_prog_cert` (no untrusted transitive fold). *)

(* `split_semi` is the shared `;`-segment parser (Subset.v).
   one function: split off `allowed`, then `direct`, then the rest is `callees`. *)
Definition parse_fn (s : string) : Fn :=
  let (a, rest) := split_bar s EmptyString in
  let (d, c) := split_bar rest EmptyString in
  {| fallowed := pnats a None []; fdirect := pnats d None []; fcallees := pnats c None [] |}.

Definition parse_prog (s : string) : list Fn := map parse_fn (split_semi s EmptyString []).

(* THE GATE CHECKER: for EVERY function, check its transitive reach (the fold computed
   INSIDE the checker, at fuel = the function count — which bounds any simple call chain in
   a graph of that many nodes) stays within its declared bound. This is exactly the gate's
   `reach ⊆ declared` semantics, with the transitive FOLD now done by the proven checker
   rather than the untrusted Rust reachability fold — and, unlike the `prog_ok` composition
   law, it accepts a callee that over-declares (no per-edge monotonicity requirement). *)
Definition prog_within (prog : list Fn) : bool :=
  forallb (fun i => subset_check (fallowed (lookup prog i)) (reaches prog (List.length prog) i))
          (List.seq 0 (List.length prog)).

(* THE END-TO-END CHECKER: parse the call-graph witness, then run the program check. *)
Definition check_prog_cert (s : string) : bool := prog_within (parse_prog s).

(* SOUNDNESS, end-to-end: acceptance of the witness BYTES guarantees every function's
   transitive capability reach (at the checked fuel = function count, which covers the full
   reachable set) stays within its declared bound. The transitive sandbox promise is now
   decided by the kernel-proven checker over the emitted witness, not by an untrusted fold. *)
Theorem check_prog_cert_sound :
  forall s, check_prog_cert s = true ->
  forall i, i < List.length (parse_prog s) ->
    subset_prop (fallowed (lookup (parse_prog s) i))
                (reaches (parse_prog s) (List.length (parse_prog s)) i).
Proof.
  intros s H i Hi.
  unfold check_prog_cert, prog_within in H. rewrite forallb_forall in H.
  apply subset_check_sound. apply H. apply in_seq. lia.
Qed.

(* non-vacuous, end-to-end: the demo_ok call graph as a witness string is ACCEPTED; the
   demo_bad one (helper reaches undeclared network) is REJECTED. *)
Example cert_ok : check_prog_cert "1 2|2|1;1|1|" = true.
Proof. reflexivity. Qed.
Example cert_bad : check_prog_cert "1 2|2|1;0|0|" = false.
Proof. reflexivity. Qed.

Print Assumptions reaches_sound.
Print Assumptions check_prog_cert_sound.
