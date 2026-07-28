(* Almide v1 trust spine — the SHARED MEMBERSHIP-SUBSET law.

   Two of the flight-grade properties are, structurally, the SAME decidable
   check over the SAME witness shape (two `|`-separated decimal-nat lists):

     - name totality   (NameTotality.v):    used ⊆ defined   (no dangling ref)
     - capability bound (CapabilityBound.v): used ⊆ allowed   (sandbox promise)

   So the checker, its soundness theorem, and the internalized witness parser
   live here ONCE; the two properties are thin namings of `subset_*`. One proof
   to audit instead of three near-identical copies — a smaller trusted base and
   no parser duplication (the patchwork this would otherwise be). *)

From Stdlib Require Import List.
Import ListNotations.
From Stdlib Require Import Arith.
From Stdlib Require Import String Ascii.

Definition mem (x : nat) (l : list nat) : bool := existsb (Nat.eqb x) l.

(* THE CHECKER: every element of `sub` is in `sup`. *)
Definition subset_check (sup sub : list nat) : bool :=
  forallb (fun x => mem x sup) sub.

(* THE PROPERTY: `sub` is contained in `sup`. *)
Definition subset_prop (sup sub : list nat) : Prop :=
  forall x, In x sub -> In x sup.

(* SOUNDNESS: acceptance guarantees containment. *)
Theorem subset_check_sound :
  forall sup sub, subset_check sup sub = true -> subset_prop sup sub.
Proof.
  intros sup sub H x Hx.
  unfold subset_check in H. rewrite forallb_forall in H.
  specialize (H x Hx). unfold mem in H. rewrite existsb_exists in H.
  destruct H as [y [Hin Heq]]. apply Nat.eqb_eq in Heq.
  rewrite Heq. exact Hin.
Qed.

(* ─── the SORTED fast path (arc v1-join-completeness, C3) ───
   `subset_check` is Θ(|sub|·|sup|) — quadratic in ids, which is one of the
   terms that parked the kernel oracle for 4h on the 2026-07-27 231KB witness.
   The compiler now emits both id lists SORTED + DEDUPED (certificate.rs —
   sound to do: `subset_prop` is permutation- and duplicate-invariant), so a
   two-pointer merge decides the same property in Θ(|sup|+|sub|).

   Soundness of the merge needs NO sortedness hypothesis: every accepted `sub`
   element is literally found in a suffix of `sup` (drop_lt only DROPS
   elements), so the trust theorem below is unconditional. Sortedness gates
   only COMPLETENESS — an unsorted witness could be falsely REJECTED by the
   merge, never falsely accepted — which is why `subset_check_fast` falls back
   to the proven membership check whenever either side is unsorted: every
   pre-existing witness verifies bit-identically. This is the v1→v4
   ownership-format precedent (strict-superset add, no format change) applied
   to the subset law. *)

Fixpoint sortedb (l : list nat) : bool :=
  match l with
  | [] => true
  | x :: t =>
      match t with
      | [] => true
      | y :: _ => andb (Nat.leb x y) (sortedb t)
      end
  end.

(* Drop the strict prefix of elements `< x`. On a sorted list the head of the
   result is the first candidate that could equal `x`. *)
Fixpoint drop_lt (x : nat) (l : list nat) : list nat :=
  match l with
  | [] => []
  | y :: l' => if Nat.ltb y x then drop_lt x l' else l
  end.

(* Two-pointer merge: for each `sub` element, advance `sup` to the first
   element not below it and demand equality; continue from THAT suffix (the
   head is kept so a duplicated `sub` element re-matches it). `sup` only ever
   advances, so total work is Θ(|sup|+|sub|). *)
Fixpoint merge_subset (sup sub : list nat) {struct sub} : bool :=
  match sub with
  | [] => true
  | x :: sub' =>
      match drop_lt x sup with
      | [] => false
      | y :: sup' => if Nat.eqb x y then merge_subset (y :: sup') sub' else false
      end
  end.

Lemma drop_lt_incl : forall x l, incl (drop_lt x l) l.
Proof.
  intros x l; induction l as [|y l' IH]; simpl.
  - apply incl_refl.
  - destruct (Nat.ltb y x).
    + apply incl_tl. exact IH.
    + apply incl_refl.
Qed.

Lemma merge_subset_sound :
  forall sub sup, merge_subset sup sub = true -> subset_prop sup sub.
Proof.
  induction sub as [|x sub' IH]; intros sup H z Hz.
  - destruct Hz.
  - simpl in H.
    destruct (drop_lt x sup) as [|y rest] eqn:E; [discriminate|].
    destruct (Nat.eqb x y) eqn:Exy; [|discriminate].
    apply Nat.eqb_eq in Exy.
    assert (Hincl : incl (y :: rest) sup).
    { rewrite <- E. apply drop_lt_incl. }
    destruct Hz as [Hzx | Hz].
    + apply Hincl. left. congruence.
    + apply Hincl. exact (IH (y :: rest) H z Hz).
Qed.

(* THE DISPATCH: sorted witnesses take the linear merge; anything else takes
   the original quadratic membership check. Sound either way. *)
Definition subset_check_fast (sup sub : list nat) : bool :=
  if andb (sortedb sup) (sortedb sub)
  then merge_subset sup sub
  else subset_check sup sub.

Theorem subset_check_fast_sound :
  forall sup sub, subset_check_fast sup sub = true -> subset_prop sup sub.
Proof.
  intros sup sub H. unfold subset_check_fast in H.
  destruct (andb (sortedb sup) (sortedb sub)).
  - apply merge_subset_sound. exact H.
  - apply subset_check_sound. exact H.
Qed.

(* ─── witness parsing, INTERNALIZED INTO COQ (end-to-end like check_cert) ───
   Format: the SUPERSET ids, then `|`, then the SUBSET-to-check ids — each a
   whitespace-separated list of decimal nats. The parser is total; what it
   produces is what the checker validates (parse correctness = the
   cert-faithfulness obligation, tested compiler-side). The whole
   "bytes ⟶ accept/reject" pipeline is kernel-checked.

   LINEARITY (arc v1-join-completeness, C2): every accumulator below is CONS +
   one `rev_append` at the boundary, and segment strings are rebuilt once via
   `string_of_list_ascii`. The previous forms appended per element
   (`acc ++ [n]`, `left ++ String a EmptyString`) — Θ(n²) in witness BYTES, in
   both the extracted binary and the kernel's vm_compute; on the 2026-07-27
   231KB witness the parse quadratics dominated the 4h oracle park. Public
   names and signatures are unchanged; observable results are pinned by the
   Examples below and re-checked corpus-wide by the 3-way gate (extracted
   binary vs kernel oracle vs the compiler's emitters). *)

Definition is_digit (a : ascii) : bool :=
  andb (Nat.leb 48 (nat_of_ascii a)) (Nat.leb (nat_of_ascii a) 57).
Definition digit (a : ascii) : nat := nat_of_ascii a - 48.
Definition is_bar (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 124. (* '|' *)

(* Character walks run over `list ascii`, converted ONCE per public entry via
   `list_ascii_of_string` / `string_of_list_ascii` (linear fixpoints in the
   kernel; extracted to a single O(n) conversion in OCaml). Walking a Coq
   `string` directly in the EXTRACTED binary pays `String.sub` — an
   allocate-and-copy per character (ExtrOcamlNativeString's destructor),
   Θ(n²) bytes before the algorithm does anything. *)

Fixpoint pnats_rev (s : list ascii) (cur : option nat) (acc : list nat) : list nat :=
  match s with
  | [] => match cur with Some n => n :: acc | None => acc end
  | a :: r =>
      if is_digit a
      then pnats_rev r (Some (match cur with Some n => n * 10 + digit a | None => digit a end)) acc
      else match cur with Some n => pnats_rev r None (n :: acc) | None => pnats_rev r None acc end
  end.

Definition pnats (s : string) (cur : option nat) (acc : list nat) : list nat :=
  acc ++ rev_append (pnats_rev (list_ascii_of_string s) cur []) [].

Fixpoint split_bar_rev (s : list ascii) (left_rev : list ascii)
  : list ascii * list ascii :=
  match s with
  | [] => (left_rev, [])
  | a :: r => if is_bar a then (left_rev, r) else split_bar_rev r (a :: left_rev)
  end.

Definition split_bar (s : string) (left : string) : string * string :=
  let (lrev, rest) := split_bar_rev (list_ascii_of_string s) [] in
  ((left ++ string_of_list_ascii (rev_append lrev []))%string,
   string_of_list_ascii rest).

(* `;`-separated segments (shared by the multi-function witness parsers:
   CapabilityReach's call-graph nodes, CallModes' signatures/call-sites). *)
Definition is_semi (a : ascii) : bool := Nat.eqb (nat_of_ascii a) 59. (* ';' *)

Fixpoint split_semi_rev (s : list ascii) (cur_rev : list ascii) (acc_rev : list string)
  : list string :=
  match s with
  | [] => string_of_list_ascii (rev_append cur_rev []) :: acc_rev
  | a :: r =>
      if is_semi a
      then split_semi_rev r [] (string_of_list_ascii (rev_append cur_rev []) :: acc_rev)
      else split_semi_rev r (a :: cur_rev) acc_rev
  end.

Definition split_semi (s : string) (cur : string) (acc : list string) : list string :=
  acc ++ rev_append (split_semi_rev (list_ascii_of_string (cur ++ s)) [] []) [].

(* (superset ids, subset ids) *)
Definition parse_pair (s : string) : list nat * list nat :=
  let (l, r) := split_bar s EmptyString in (pnats l None [], pnats r None []).

Definition subset_cert (s : string) : bool :=
  subset_check_fast (fst (parse_pair s)) (snd (parse_pair s)).

Theorem subset_cert_sound :
  forall s, subset_cert s = true ->
    subset_prop (fst (parse_pair s)) (snd (parse_pair s)).
Proof.
  intros s H. apply subset_check_fast_sound. exact H.
Qed.

(* non-vacuous: accepts a contained witness, rejects one with an outside member. *)
Example cert_contained : subset_cert "1 2 3|1 3" = true.
Proof. reflexivity. Qed.
Example cert_outside : subset_cert "1 2|1 5" = false.
Proof. reflexivity. Qed.
(* the fast path really is the sorted path, and the fallback really fires:
   an UNSORTED superset must still accept through the membership check … *)
Example cert_unsorted_fallback : subset_cert "3 1 2|1 3" = true.
Proof. reflexivity. Qed.
(* … and the merge itself rejects an outside member on sorted input. *)
Example merge_rejects_outside : merge_subset [1; 2] [1; 5] = false.
Proof. reflexivity. Qed.
(* parser edges are unchanged by the linearization: multi-separator runs,
   a leading separator, and a trailing id parse to the same lists. *)
Example parse_edges : parse_pair "  7  8 |9" = ([7; 8], [9]).
Proof. reflexivity. Qed.

Print Assumptions subset_check_sound.
Print Assumptions subset_check_fast_sound.
Print Assumptions subset_cert_sound.
