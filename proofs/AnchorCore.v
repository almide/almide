(* Almide trust spine — the byte-level check's core, mechanized (first brick).

   almide-anchor-check holds each located bounds access to its check on the
   shipped bytes: the compare tests the access's own index against the count
   of the block the access addresses, in the access's slot size, and nothing
   between them changes either. What that buys is arithmetic, and this file
   proves the arithmetic with the machine's own word sizes:

     - `unsigned_guard_excludes_negatives`: the ONE unsigned 64-bit compare
       the emitter emits (`idx >=u count` aborting) leaves a signed index in
       [0, count) — a negative i64 reads above any count;
     - `guarded_access_in_payload`: an index below `len / S` addresses, in
       32-bit address arithmetic that does not wrap, bytes inside the block's
       payload [B + 12, B + 12 + len), for any access of at most one slot.

   Not here yet: the instruction semantics that carry "nothing between
   changes either" from the bytes to these hypotheses. *)

From Stdlib Require Import ZArith Lia.
Open Scope Z_scope.

(* 2^32 and 2^64, written out so `lia` sees constants. *)
Definition M32 : Z := 4294967296.
Definition M64 : Z := 18446744073709551616.

Definition wrap32 (x : Z) : Z := x mod M32.

(* The block header (rc / len / cap): the payload starts 12 bytes in. *)
Definition PAYLOAD : Z := 12.

Theorem unsigned_guard_excludes_negatives : forall v cnt,
  - 9223372036854775808 <= v < 9223372036854775808 ->
  0 <= cnt < M32 ->
  v mod M64 < cnt ->
  0 <= v < cnt.
Proof.
  unfold M32, M64. intros v cnt Hv Hc Hlt.
  destruct (Z_lt_le_dec v 0) as [Hneg | Hpos].
  - assert (E : v + 18446744073709551616 = v mod 18446744073709551616).
    { apply Z.mod_unique with (q := -1); lia. }
    lia.
  - rewrite Z.mod_small in Hlt by lia. lia.
Qed.

Lemma slot_below_len : forall u len S,
  0 < S -> 0 <= u -> u < len / S -> S * u + S <= len.
Proof.
  intros u len S HS Hu Hlt.
  assert (H1 : u + 1 <= len / S) by lia.
  assert (H2 : S * (u + 1) <= S * (len / S)) by (apply Z.mul_le_mono_nonneg_l; lia).
  assert (H3 : S * (len / S) <= len) by (apply Z.mul_div_le; lia).
  lia.
Qed.

Theorem guarded_access_in_payload : forall B u len S W a,
  0 < S -> 0 < W -> W <= S ->
  0 <= B -> 0 <= len -> B + PAYLOAD + len <= M32 ->
  0 <= u -> u < len / S ->
  a = wrap32 (B + wrap32 u * S) ->
  a = B + u * S /\
  B + PAYLOAD <= a + PAYLOAD /\
  a + PAYLOAD + W <= B + PAYLOAD + len.
Proof.
  intros B u len S W a HS HW HWS HB Hlen Hfit Hu Hlt Ha0.
  unfold PAYLOAD, M32 in *. unfold wrap32, M32 in Ha0.
  pose proof (slot_below_len u len S HS Hu Hlt) as Hs.
  assert (Hus : u <= S * u) by nia.
  assert (Hu32 : u mod 4294967296 = u) by (apply Z.mod_small; lia).
  rewrite Hu32 in Ha0.
  assert (Ha : a = B + u * S).
  { rewrite Ha0. apply Z.mod_small. nia. }
  rewrite Ha. repeat split; nia.
Qed.

Print Assumptions unsigned_guard_excludes_negatives.
Print Assumptions guarded_access_in_payload.
