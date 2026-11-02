(* Almide trust spine — #4013 / #4279: record and variant FIELD accesses,
   held to the layout their block was built with.

   SpatialCert.v certifies element accesses of list blocks (uniform slots,
   a run-time index, a bounds check). A record, a tuple or a variant case is
   a different shape: a block of fixed size whose fields sit at fixed
   offsets of differing widths, read and written at a CONSTANT offset with
   no check at all. Such an access is in bounds exactly when the block was
   built with the layout the access assumes. InitCert.v's construction sites
   are uniform-slot sites (list literals, range fills); this file adds the
   LAYOUT-TYPED construction site and the field access, as a third per-build
   certificate, `fields`:

     L R c C n N z S (o O w W)*   the layout of kind (R, C): representation R
                                  (a record, tuple or variant type), case C
                                  (0 for a record or tuple; the tag for a
                                  variant case) of N cases, payload of S
                                  bytes, field i at payload offset O_i, W_i
                                  bytes wide (a variant case's field 0 is
                                  its tag word, offset 0, 4 bytes)
     k R c C (s I)*               a construction site of kind (R, C): the
                                  allocation of the payload, the allocator's
                                  header, then a store of each field I
     a R c C f I                  a load or store of field I of kind (R, C)
     t R                          a read of the tag word of a block of R,
                                  whatever its case

   What is proven:

   - `rsite_window`: the events of an accepted site, from a state where the
     block is not live, are MemorySafe, leave every other block as it was,
     and leave the block live, of the layout's size and representation,
     with every declared field initialized.
   - `ftrace_safe`: in a trace whose every allocation of a recorded kind is
     an accepted site's window and whose every load and store of such a
     block is a declared field of its kind (`ftrace`), every event on a
     recorded block is MemorySafe — given TEMPORAL, which the RC
     certificate gives (`MemorySafe.rc_check_gives_temporal`).
   - `fields_memory_safe`: that, and the other certificates' clauses for
     the events on the other blocks, is MemorySafe.
   - `check_fields_cert_sound`: the byte parser inside the proof; an
     accepted witness denotes a layout table whose every field fits its
     payload, sites that store every field of their kind, accesses that
     name a declared field, and tag reads of a representation whose every
     case starts with the tag word.

   WHAT LINKS AN ACCESS TO ITS BLOCK'S KIND. `ftrace` takes the block's kind
   as `Kd b` and asks that every access of it name that kind. For a record
   or a tuple the kind is the block's static type: the existing trusted
   REPRESENTATION link (the IR is cast-free; TRUSTED_BASE.md). For a variant
   the CASE is a run-time fact: a case field's read is emitted under the
   tag compare of its own local, and almide-anchor-check holds every such
   read to that compare on the shipped bytes; that the tag word keeps the
   case the site stored there is the trusted row this adds (the heap model
   has no values). A TAG read needs no case: every case's field 0 is the
   tag word (`ITag`), so it is in bounds of any of them.

   HONEST SCOPE. As for InitCert: the witness covers the sites and accesses
   the emitter records (crates/almide-wasm/src/witness_fields.rs: record,
   tuple and variant literals and constructors, member and positional
   reads, pattern tests and binds, spread overwrites); the runtime's own
   routines (a typed drop, copy, equality or display body) record nothing
   yet (#4278), so `ftrace` holds of a run only when they do not touch a
   recorded block. The events-to-bytes link is almide-anchor-check's
   (crates/almide-anchor-check, `check_fields`). *)

From AlmideTrust Require Import MemorySafe Subset Certified InitCert.
From Stdlib Require Import List ZArith Lia Bool Arith String Ascii.
Import ListNotations.
Open Scope Z_scope.

(* ── Layouts ────────────────────────────────────────────────────────────── *)

(* The allocator's header: rc / len / cap (StructuralAlloc.v). *)
Definition HDRF : Z := 12.

Record Shape := mkShape { SZ : nat; FLD : list (nat * nat) }.

(* A field (offset, width) fits a payload of `sz` bytes. *)
Definition fits (sz : nat) (p : nat * nat) : bool :=
  (0 <? snd p)%nat && (fst p + snd p <=? sz)%nat.

Definition shape_ok (s : Shape) : bool := forallb (fits (SZ s)) (FLD s).

Definition FKey : Type := (nat * nat)%type.

Definition key_eqb (a b : FKey) : bool := Nat.eqb (fst a) (fst b) && Nat.eqb (snd a) (snd b).

Definition FTable : Type := list (FKey * Shape).

Fixpoint flookup (tb : FTable) (k : FKey) : option Shape :=
  match tb with
  | [] => None
  | (k', s) :: r => if key_eqb k' k then Some s else flookup r k
  end.

Definition table_ok (tb : FTable) : bool := forallb (fun e => shape_ok (snd e)) tb.

Definition fsize (s : Shape) : Z := HDRF + Z.of_nat (SZ s).
Definition foff (p : nat * nat) : Z := HDRF + Z.of_nat (fst p).
Definition fwid (p : nat * nat) : Z := Z.of_nat (snd p).
Definition fld (s : Shape) (i : nat) : nat * nat := nth i (FLD s) (0%nat, 0%nat).

Lemma key_eqb_eq : forall a b, key_eqb a b = true -> a = b.
Proof.
  intros [a1 a2] [b1 b2] H. unfold key_eqb in H. simpl in H. apply andb_true_iff in H.
  destruct H as [H1 H2]. apply Nat.eqb_eq in H1. apply Nat.eqb_eq in H2. subst. reflexivity.
Qed.

Lemma flookup_in : forall tb k s, flookup tb k = Some s -> In (k, s) tb.
Proof.
  intros tb k s. induction tb as [| [k' s'] r IH]; simpl; [discriminate |].
  destruct (key_eqb k' k) eqn:E.
  - intro H. injection H as <-. left. rewrite (key_eqb_eq _ _ E). reflexivity.
  - intro H. right. exact (IH H).
Qed.

Lemma lookup_shape_ok : forall tb k s, table_ok tb = true -> flookup tb k = Some s -> shape_ok s = true.
Proof.
  intros tb k s T H. unfold table_ok in T. rewrite forallb_forall in T.
  exact (T (k, s) (flookup_in tb k s H)).
Qed.

Lemma fits_spec : forall sz p, fits sz p = true -> (0 < snd p)%nat /\ (fst p + snd p <= sz)%nat.
Proof.
  intros sz p H. unfold fits in H. apply andb_true_iff in H. destruct H as [H1 H2].
  apply Nat.ltb_lt in H1. apply Nat.leb_le in H2. lia.
Qed.

Lemma shape_fits : forall s p, shape_ok s = true -> In p (FLD s) -> fits (SZ s) p = true.
Proof.
  intros s p Ok Hp. unfold shape_ok in Ok. rewrite forallb_forall in Ok. exact (Ok p Hp).
Qed.

Lemma fits_in_bounds : forall s p x, fits (SZ s) p = true -> size x = fsize s ->
  in_bounds x (foff p) (fwid p).
Proof.
  intros s p x F Sz. destruct (fits_spec _ _ F) as [H1 H2].
  unfold in_bounds, foff, fwid. rewrite Sz. unfold fsize, HDRF. lia.
Qed.

(* ── Layout-typed construction sites ───────────────────────────────────── *)

(* Every field index below the field count is stored, and none past it. *)
Definition stored_ok (s : Shape) (st : list nat) : bool :=
  forallb (fun i => existsb (Nat.eqb i) st) (seq 0 (List.length (FLD s))) &&
  forallb (fun i => (i <? List.length (FLD s))%nat) st.

Lemma stored_ok_spec : forall s st, stored_ok s st = true ->
  (forall i, (i < List.length (FLD s))%nat -> In i st) /\
  (forall i, In i st -> (i < List.length (FLD s))%nat).
Proof.
  intros s st H. unfold stored_ok in H. apply andb_true_iff in H. destruct H as [H1 H2].
  rewrite forallb_forall in H1, H2. split.
  - intros i Hi. assert (Hs : In i (seq 0 (List.length (FLD s)))) by (apply in_seq; lia).
    specialize (H1 i Hs). apply existsb_exists in H1. destruct H1 as [j [Hj E]].
    apply Nat.eqb_eq in E. subst. exact Hj.
  - intros i Hi. apply Nat.ltb_lt. exact (H2 i Hi).
Qed.

Definition fstore (b : Blk) (r : Repr) (s : Shape) (i : nat) : Ev :=
  EStore b (foff (fld s i)) (fwid (fld s i)) r.

Definition rsite_events (b : Blk) (r : Repr) (s : Shape) (st : list nat) : list Ev :=
  EAlloc b (fsize s) r :: EStore b 0 HDRF r :: map (fstore b r s) st.

(* A block of the layout: its size and representation, and every declared
   field initialized. *)
Definition fshaped (s : Shape) (r : Repr) (x : Block) : Prop :=
  size x = fsize s /\ repr x = r /\
  forall p, In p (FLD s) -> all_init (init x) (foff p) (fwid p).

Lemma fstores_window : forall s r b l h x,
  shape_ok s = true -> (forall i, In i l -> (i < List.length (FLD s))%nat) ->
  h b = Some x -> size x = fsize s -> repr x = r -> rc x = 1 ->
  holds safe_ev h (map (fstore b r s) l) /\
  exists y, after h (map (fstore b r s) l) b = Some y /\ size y = fsize s /\ repr y = r /\ rc y = 1 /\
    (forall j, init x j = true -> init y j = true) /\
    (forall i, In i l -> all_init (init y) (foff (fld s i)) (fwid (fld s i))).
Proof.
  intros s r b l. induction l as [| i l IH]; intros h x Ok Hl Hx Sz Rp Rc.
  - split; [exact I |]. exists x. split; [exact Hx |]. split; [exact Sz |]. split; [exact Rp |].
    split; [exact Rc |]. split; [auto | intros i Hi; destruct Hi].
  - assert (Hi : (i < List.length (FLD s))%nat) by (apply Hl; left; reflexivity).
    assert (F : fits (SZ s) (fld s i) = true) by (apply shape_fits; [exact Ok | unfold fld; apply nth_In; exact Hi]).
    set (x1 := mkBlock (size x) (repr x) (rc x) (mark (init x) (foff (fld s i)) (fwid (fld s i)))).
    assert (E1 : step h (fstore b r s i) = set h b x1) by (unfold fstore; simpl; rewrite Hx; reflexivity).
    assert (Hx1 : set h b x1 b = Some x1) by (unfold set; rewrite Nat.eqb_refl; reflexivity).
    destruct (IH (set h b x1) x1 Ok (fun j Hj => Hl j (or_intror Hj)) Hx1 Sz Rp Rc)
      as [Hs [y [Hy [Szy [Rpy [Rcy [Keep Cov]]]]]]].
    split.
    + cbn [map holds]. split.
      * unfold safe_ev, fstore. cbn [spatial_ev temporal_ev init_ev type_ev]. unfold live.
        split; [exists x; split; [exact Hx | exact (fits_in_bounds s (fld s i) x F Sz)] |].
        split; [exists x; split; [exact Hx | lia] |].
        split; [exact I | exists x; split; [exact Hx | exact Rp]].
      * rewrite E1. exact Hs.
    + exists y. unfold after. cbn [map fold_left]. rewrite E1.
      fold (after (set h b x1) (map (fstore b r s) l)).
      split; [exact Hy |]. split; [exact Szy |]. split; [exact Rpy |]. split; [exact Rcy |]. split.
      * intros j Hj. apply Keep. simpl. apply mark_keeps. exact Hj.
      * intros k [Hk | Hk].
        -- subst k. intros j Hj. apply Keep. simpl. apply mark_covers. exact Hj.
        -- exact (Cov k Hk).
Qed.

Theorem rsite_window : forall s r b st h,
  shape_ok s = true -> stored_ok s st = true -> ~ live h b ->
  holds safe_ev h (rsite_events b r s st) /\
  (forall c, c <> b -> after h (rsite_events b r s st) c = h c) /\
  exists x, after h (rsite_events b r s st) b = Some x /\ fshaped s r x.
Proof.
  intros s r b st h Ok St NL. destruct (stored_ok_spec s st St) as [Cov Rng].
  assert (Hsz : HDRF <= fsize s) by (unfold fsize, HDRF; lia).
  set (x0 := mkBlock (fsize s) r 1 (fun _ => false)).
  set (h0 := set h b x0).
  assert (H0b : h0 b = Some x0) by (unfold h0, set; rewrite Nat.eqb_refl; reflexivity).
  set (x1 := mkBlock (fsize s) r 1 (mark (fun _ => false) 0 HDRF)).
  set (h1 := set h0 b x1).
  assert (E1 : step h0 (EStore b 0 HDRF r) = h1) by (simpl; rewrite H0b; reflexivity).
  assert (H1b : h1 b = Some x1) by (unfold h1, set; rewrite Nat.eqb_refl; reflexivity).
  destruct (fstores_window s r b st h1 x1 Ok Rng H1b eq_refl eq_refl eq_refl)
    as [Hs [y [Hy [Szy [Rpy [Rcy [_ Slots]]]]]]].
  assert (E0 : step h (EAlloc b (fsize s) r) = h0) by reflexivity.
  assert (Ev : after h (rsite_events b r s st) = after h1 (map (fstore b r s) st)).
  { unfold after, rsite_events. cbn [fold_left]. rewrite E0, E1. reflexivity. }
  split; [| split].
  - unfold rsite_events. cbn [holds]. rewrite E0, E1. split; [| split].
    + unfold safe_ev. simpl. tauto.
    + unfold safe_ev, live. cbn [spatial_ev temporal_ev init_ev type_ev].
      split; [exists x0; split; [exact H0b | unfold in_bounds; try unfold x0; simpl; unfold HDRF, fsize in *; lia] |].
      split; [exists x0; split; [exact H0b | try unfold x0; simpl; lia] |].
      split; [exact I | exists x0; split; [exact H0b | reflexivity]].
    + exact Hs.
  - intros c Hc. apply after_other. unfold rsite_events.
    constructor; [simpl; intro E; apply Hc; symmetry; exact E |].
    constructor; [simpl; intro E; apply Hc; symmetry; exact E |].
    apply Forall_forall. intros e He. apply in_map_iff in He. destruct He as [i [<- _]].
    unfold fstore. simpl. intro E; apply Hc; symmetry; exact E.
  - exists y. split; [rewrite Ev; exact Hy |]. split; [exact Szy |]. split; [exact Rpy |].
    intros p Hp. destruct (In_nth _ _ (0%nat, 0%nat) Hp) as [i [Hi Hpi]].
    rewrite <- Hpi. exact (Slots i (Cov i Hi)).
Qed.
Print Assumptions rsite_window.

(* ── Traces of field accesses ───────────────────────────────────────────── *)

Definition is_rc (e : Ev) : bool :=
  match e with EInc _ | EDec _ => true | _ => false end.

(* Every allocation of a block of a recorded kind is an accepted site's
   window; every load or store of it is a declared field of its kind; any
   other event on it is a reference-count change. `Kd b` is the block's
   kind (see the header: the representation link, and for a variant case
   the tag). *)
Inductive ftrace (tb : FTable) (Kd : Blk -> FKey) : list Ev -> Prop :=
  | FNil : ftrace tb Kd []
  | FSite : forall b s st t, flookup tb (Kd b) = Some s -> stored_ok s st = true ->
      ftrace tb Kd t -> ftrace tb Kd (rsite_events b (fst (Kd b)) s st ++ t)
  | FLoad : forall b s p t, flookup tb (Kd b) = Some s -> In p (FLD s) ->
      ftrace tb Kd t -> ftrace tb Kd (ELoad b (foff p) (fwid p) (fst (Kd b)) :: t)
  | FStore : forall b s p t, flookup tb (Kd b) = Some s -> In p (FLD s) ->
      ftrace tb Kd t -> ftrace tb Kd (EStore b (foff p) (fwid p) (fst (Kd b)) :: t)
  | FOther : forall e t, flookup tb (Kd (ev_blk e)) = None \/ is_rc e = true ->
      ftrace tb Kd t -> ftrace tb Kd (e :: t).

(* What the heap holds of every block of a recorded kind. *)
Definition good (tb : FTable) (Kd : Blk -> FKey) (h : Heap) : Prop :=
  forall b x s, h b = Some x -> flookup tb (Kd b) = Some s -> fshaped s (fst (Kd b)) x.

Definition on_recorded (tb : FTable) (Kd : Blk -> FKey) (h : Heap) (e : Ev) : Prop :=
  flookup tb (Kd (ev_blk e)) <> None -> safe_ev h e.

Lemma holds_impl : forall (P Q : Heap -> Ev -> Prop) t h,
  (forall h e, P h e -> Q h e) -> holds P h t -> holds Q h t.
Proof.
  intros P Q t. induction t as [| e t IH]; intros h Imp H; [exact I |].
  destruct H as [He Ht]. split; [apply Imp; exact He | apply IH; assumption].
Qed.

Lemma holds_both : forall (P Q : Heap -> Ev -> Prop) t h,
  holds P h t -> holds Q h t -> holds (fun h e => P h e /\ Q h e) h t.
Proof.
  intros P Q t. induction t as [| e t IH]; intros h HP HQ; [exact I |].
  destruct HP as [P1 P2]. destruct HQ as [Q1 Q2]. split; [split; assumption | apply IH; assumption].
Qed.

Theorem ftrace_safe : forall tb Kd t, table_ok tb = true -> ftrace tb Kd t ->
  forall h, good tb Kd h -> holds temporal_ev h t -> holds (on_recorded tb Kd) h t.
Proof.
  intros tb Kd t Tb Ft.
  induction Ft as [| b s st t' Hl Hst Ft IH | b s p t' Hl Hp Ft IH | b s p t' Hl Hp Ft IH | e t' Hc Ft IH];
    intros h G T.
  - exact I.
  - (* a site's window *)
    apply holds_app'. apply holds_app' in T. destruct T as [Tw Tt].
    assert (NL : ~ live h b) by (unfold rsite_events in Tw; simpl in Tw; tauto).
    assert (Ok : shape_ok s = true) by exact (lookup_shape_ok tb (Kd b) s Tb Hl).
    destruct (rsite_window s (fst (Kd b)) b st h Ok Hst NL) as [Hs [Oth [x [Hx Shx]]]].
    split.
    + apply (holds_impl safe_ev); [intros h' e' Se _; exact Se | exact Hs].
    + apply IH; [| exact Tt].
      intros c y s' Hy Hs'. destruct (Nat.eq_dec c b) as [-> | Ne].
      * rewrite Hx in Hy. injection Hy as <-. rewrite Hl in Hs'. injection Hs' as <-. exact Shx.
      * rewrite (Oth c Ne) in Hy. exact (G c y s' Hy Hs').
  - (* a field load *)
    cbn [holds] in T |- *. destruct T as [Te Tt]. cbn [temporal_ev] in Te. destruct Te as [x [Hx Hrc]].
    destruct (G b x s Hx Hl) as [Sz [Rp Ini]].
    assert (F : fits (SZ s) p = true) by exact (shape_fits s p (lookup_shape_ok tb (Kd b) s Tb Hl) Hp).
    split.
    + intros _. unfold safe_ev. cbn [spatial_ev temporal_ev init_ev type_ev]. unfold live.
      split; [exists x; split; [exact Hx | exact (fits_in_bounds s p x F Sz)] |].
      split; [exists x; split; [exact Hx | exact Hrc] |].
      split; [exists x; split; [exact Hx | exact (Ini p Hp)] |].
      exists x; split; [exact Hx | exact Rp].
    + apply IH; [exact G | exact Tt].
  - (* a field store *)
    cbn [holds] in T |- *. destruct T as [Te Tt]. cbn [temporal_ev] in Te. destruct Te as [x [Hx Hrc]].
    destruct (G b x s Hx Hl) as [Sz [Rp Ini]].
    assert (F : fits (SZ s) p = true) by exact (shape_fits s p (lookup_shape_ok tb (Kd b) s Tb Hl) Hp).
    split.
    + intros _. unfold safe_ev. cbn [spatial_ev temporal_ev init_ev type_ev]. unfold live.
      split; [exists x; split; [exact Hx | exact (fits_in_bounds s p x F Sz)] |].
      split; [exists x; split; [exact Hx | exact Hrc] |].
      split; [exact I |].
      exists x; split; [exact Hx | exact Rp].
    + apply IH; [| exact Tt].
      intros c y s' Hy Hs'. cbn [step] in Hy. rewrite Hx in Hy. cbv beta iota delta [set] in Hy.
      destruct (Nat.eqb c b) eqn:E.
      * apply Nat.eqb_eq in E. subst c. injection Hy as <-. rewrite Hl in Hs'. injection Hs' as <-.
        split; [exact Sz |]. split; [exact Rp |].
        intros q Hq. unfold all_init. intros j Hj. simpl. apply mark_keeps. exact (Ini q Hq j Hj).
      * exact (G c y s' Hy Hs').
  - (* any other event *)
    cbn [holds] in T |- *. destruct T as [Te Tt]. split.
    + intros Hrec. destruct Hc as [Hn | Hr]; [contradiction |].
      destruct e as [b n k | b o w k | b o w k | b | b]; cbn [is_rc] in Hr; try discriminate;
        unfold safe_ev; cbn [spatial_ev init_ev type_ev];
        (split; [exact I | split; [exact Te | split; exact I]]).
    + apply IH; [| exact Tt].
      intros c y s Hy Hs.
      destruct (Nat.eq_dec c (ev_blk e)) as [Ec | Ne].
      * destruct Hc as [Hn | Hr]; [rewrite <- Ec in Hn; rewrite Hn in Hs; discriminate |].
        destruct e as [b n k | b o w k | b o w k | b | b]; cbn [is_rc] in Hr; try discriminate;
          cbn [ev_blk] in Ec; subst c; cbn [step] in Hy.
        -- destruct (h b) as [x |] eqn:Hb; cbv beta iota in Hy.
           ++ cbv beta delta [set] in Hy. rewrite Nat.eqb_refl in Hy. injection Hy as <-.
              exact (G b x s Hb Hs).
           ++ exact (G b y s Hy Hs).
        -- destruct (h b) as [x |] eqn:Hb; cbv beta iota in Hy.
           ++ destruct (0 <? rc x).
              ** cbv beta delta [set] in Hy. rewrite Nat.eqb_refl in Hy. injection Hy as <-.
                 exact (G b x s Hb Hs).
              ** exact (G b y s Hy Hs).
           ++ exact (G b y s Hy Hs).
      * rewrite (step_other h e c Ne) in Hy. exact (G c y s Hy Hs).
Qed.
Print Assumptions ftrace_safe.

(* THE COMPOSITION: the field certificate's trace, TEMPORAL (the RC
   certificate), and the other certificates' clauses for the events on every
   other block, give MemorySafe. *)
Theorem fields_memory_safe : forall tb Kd t,
  table_ok tb = true -> ftrace tb Kd t -> Temporal t ->
  holds (fun h e => flookup tb (Kd (ev_blk e)) = None -> safe_ev h e) empty_heap t ->
  MemorySafe t.
Proof.
  intros tb Kd t Tb Ft T Rest.
  assert (G : good tb Kd empty_heap) by (intros b x s H; discriminate).
  pose proof (ftrace_safe tb Kd t Tb Ft empty_heap G T) as Rec.
  unfold MemorySafe.
  apply (holds_impl (fun h e => on_recorded tb Kd h e /\
                                (flookup tb (Kd (ev_blk e)) = None -> safe_ev h e))).
  - intros h e [A B]. destruct (flookup tb (Kd (ev_blk e))) eqn:E.
    + apply A. rewrite E. discriminate.
    + apply B. reflexivity.
  - exact (holds_both (on_recorded tb Kd) (fun h e => flookup tb (Kd (ev_blk e)) = None -> safe_ev h e)
             t empty_heap Rec Rest).
Qed.
Print Assumptions fields_memory_safe.

(* ── The certificate as bytes ───────────────────────────────────────────── *)

(* Format (whitespace between tokens is ignored; items joined by `;`):
     item ::= 'L' R 'c' C 'n' N 'z' S ('o' O 'w' W)*     a layout
            | 'k' R 'c' C ('s' I)*                       a construction site
            | 'a' R 'c' C 'f' I                          a field access
            | 't' R                                      a tag read
   A string that does not parse is REJECTED. The empty string is vacuous. *)

Inductive FItem :=
  | ILay (k : FKey) (n : nat) (s : Shape)
  | ISite (k : FKey) (st : list nat)
  | IAcc (k : FKey) (f : nat)
  | ITag (r : nat).

(* The `o O w W` pairs, to the first `;` or the end. *)
Fixpoint pairs_at (fuel : nat) (cs : list ascii) : option (list (nat * nat) * list ascii) :=
  match fuel with
  | O => None
  | S f =>
      match skip_blank cs with
      | [] => Some ([], [])
      | c :: r =>
          if is_semi c then Some ([], c :: r)
          else match field "o"%char (c :: r) with
               | Some (o, r1) =>
                   match field "w"%char r1 with
                   | Some (w, r2) =>
                       match pairs_at f r2 with
                       | Some (ps, r3) => Some ((o, w) :: ps, r3)
                       | None => None
                       end
                   | None => None
                   end
               | None => None
               end
      end
  end.

Definition item_at (cs : list ascii) : option (FItem * list ascii) :=
  match field "L"%char cs with
  | Some (r, r1) =>
      match field "c"%char r1 with
      | Some (c, r2) =>
          match field "n"%char r2 with
          | Some (n, r3) =>
              match field "z"%char r3 with
              | Some (z, r4) =>
                  match pairs_at (S (List.length r4)) r4 with
                  | Some (ps, r5) => Some (ILay (r, c) n (mkShape z ps), r5)
                  | None => None
                  end
              | None => None
              end
          | None => None
          end
      | None => None
      end
  | None =>
  match field "k"%char cs with
  | Some (r, r1) =>
      match field "c"%char r1 with
      | Some (c, r2) =>
          match stores_at (S (List.length r2)) r2 with
          | Some (st, r3) => Some (ISite (r, c) st, r3)
          | None => None
          end
      | None => None
      end
  | None =>
  match field "a"%char cs with
  | Some (r, r1) =>
      match field "c"%char r1 with
      | Some (c, r2) =>
          match field "f"%char r2 with
          | Some (i, r3) =>
              match at_end r3 with
              | Some r4 => Some (IAcc (r, c) i, r4)
              | None => None
              end
          | None => None
          end
      | None => None
      end
  | None =>
  match field "t"%char cs with
  | Some (r, r1) =>
      match at_end r1 with
      | Some r2 => Some (ITag r, r2)
      | None => None
      end
  | None => None
  end
  end
  end
  end.

Fixpoint items_at (fuel : nat) (cs : list ascii) : option (list FItem) :=
  match fuel with
  | O => None
  | S f =>
      match skip_blank cs with
      | [] => Some []
      | _ =>
          match item_at cs with
          | Some (it, []) => Some [it]
          | Some (it, sep :: r) =>
              if is_semi sep then
                match items_at f r with
                | Some its => Some (it :: its)
                | None => None
                end
              else None
          | None => None
          end
      end
  end.

Definition parse_fields (s : string) : option (list FItem) :=
  let cs := list_ascii_of_string s in items_at (S (List.length cs)) cs.

Definition table_of (its : list FItem) : FTable :=
  flat_map (fun it => match it with ILay k _ s => [(k, s)] | _ => [] end) its.

Fixpoint nodup_keys (tb : FTable) : bool :=
  match tb with
  | [] => true
  | (k, _) :: r => negb (existsb (fun e => key_eqb (fst e) k) r) && nodup_keys r
  end.

Definition tag_first (s : Shape) : bool :=
  match FLD s with
  | (o, w) :: _ => Nat.eqb o 0 && Nat.eqb w 4
  | [] => false
  end.

Definition item_ok (tb : FTable) (it : FItem) : bool :=
  match it with
  | ILay k n s => shape_ok s && (snd k <? n)%nat
  | ISite k st => match flookup tb k with Some s => stored_ok s st | None => false end
  | IAcc k f => match flookup tb k with Some s => (f <? List.length (FLD s))%nat | None => false end
  | ITag r =>
      existsb (fun e => Nat.eqb (fst (fst e)) r) tb &&
      forallb (fun e => if Nat.eqb (fst (fst e)) r then tag_first (snd e) else true) tb
  end.

Definition check_fields_cert (str : string) : bool :=
  match parse_fields str with
  | Some its => let tb := table_of its in nodup_keys tb && forallb (item_ok tb) its
  | None => false
  end.

Lemma table_of_ok : forall tb its, forallb (item_ok tb) its = true -> table_ok (table_of its) = true.
Proof.
  intros tb its H. unfold table_ok. apply forallb_forall. intros [k s] Hin.
  unfold table_of in Hin. apply in_flat_map in Hin. destruct Hin as [it [Hit Hk]].
  rewrite forallb_forall in H. specialize (H it Hit).
  destruct it as [k' n s' | k' st | k' f | r]; simpl in Hk; try contradiction.
  destruct Hk as [Hk | []]. injection Hk as <- <-. cbn [item_ok] in H. apply andb_true_iff in H.
  cbn [snd]. tauto.
Qed.

(* What an accepted item MEANS. *)
Definition item_meaning (tb : FTable) (it : FItem) : Prop :=
  match it with
  | ILay _ _ s => shape_ok s = true
  | ISite k st => exists s, flookup tb k = Some s /\ stored_ok s st = true /\
      forall b h, ~ live h b ->
        holds safe_ev h (rsite_events b (fst k) s st) /\
        exists x, after h (rsite_events b (fst k) s st) b = Some x /\ fshaped s (fst k) x
  | IAcc k f => exists s p, flookup tb k = Some s /\ nth_error (FLD s) f = Some p /\ In p (FLD s)
  | ITag r => forall c s, flookup tb (r, c) = Some s -> nth_error (FLD s) 0 = Some (0%nat, 4%nat)
  end.

(* What ACCEPT means: the string denotes a layout table whose every field
   fits its payload; each site's window, wherever it runs, is MemorySafe and
   leaves its block of the layout with every field initialized
   (`rsite_window`); each access names a declared field of its kind (the
   `FLoad` / `FStore` step of `ftrace`); each tag read is field 0 of every
   case of its representation. A trace built from them is MemorySafe with
   the other certificates (`fields_memory_safe`). *)
Theorem check_fields_cert_sound : forall str,
  check_fields_cert str = true ->
  exists its, parse_fields str = Some its /\ table_ok (table_of its) = true /\
    forall it, In it its -> item_meaning (table_of its) it.
Proof.
  intros str H. unfold check_fields_cert in H.
  destruct (parse_fields str) as [its |] eqn:Hp; [| discriminate].
  apply andb_true_iff in H. destruct H as [_ H].
  exists its. split; [reflexivity |].
  assert (Tb := table_of_ok _ _ H). split; [exact Tb |].
  intros it Hin. rewrite forallb_forall in H. specialize (H it Hin).
  destruct it as [k n s | k st | k f | r]; cbn [item_ok item_meaning] in H |- *.
  - apply andb_true_iff in H. tauto.
  - destruct (flookup (table_of its) k) as [s |] eqn:E; [| discriminate].
    exists s. split; [reflexivity | split; [exact H |]].
    intros b h NL.
    destruct (rsite_window s (fst k) b st h (lookup_shape_ok _ _ _ Tb E) H NL) as [Hs [_ Hx]].
    split; assumption.
  - destruct (flookup (table_of its) k) as [s |] eqn:E; [| discriminate].
    apply Nat.ltb_lt in H. destruct (nth_error (FLD s) f) as [p |] eqn:En.
    + exists s, p. split; [reflexivity | split; [exact En | eapply nth_error_In; exact En]].
    + apply nth_error_None in En. lia.
  - apply andb_true_iff in H. destruct H as [_ H]. intros c s E.
    rewrite forallb_forall in H. specialize (H ((r, c), s) (flookup_in _ _ _ E)). simpl in H.
    rewrite Nat.eqb_refl in H. unfold tag_first in H.
    destruct (FLD s) as [| [o w] rest]; simpl in H; [discriminate |].
    apply andb_true_iff in H. destruct H as [H1 H2].
    apply Nat.eqb_eq in H1. apply Nat.eqb_eq in H2. subst. reflexivity.
Qed.
Print Assumptions check_fields_cert_sound.

(* ── Controls ───────────────────────────────────────────────────────────── *)

(* A record of an Int and an Int32-slot field: payload 16, fields at 0 (8
   bytes) and 8 (4 bytes); built with both stored, and its second field read. *)
Example accepts_record :
  check_fields_cert "L4 c0 n1 z16 o0 w8 o8 w4 ; k4 c0 s0 s1 ; a4 c0 f1" = true.
Proof. reflexivity. Qed.

(* A two-case variant: case 0 nullary (the tag word only), case 1 a tag and
   an Int at 8. A case-1 site, a tag read, a case-1 field read. *)
Example accepts_variant :
  check_fields_cert "L6 c0 n2 z8 o0 w4 ; L6 c1 n2 z16 o0 w4 o8 w8 ; k6 c1 s0 s1 ; t6 ; a6 c1 f1" = true.
Proof. reflexivity. Qed.

Example accepts_empty : check_fields_cert "" = true.
Proof. reflexivity. Qed.

(* A field never stored. *)
Example rejects_unstored_field :
  check_fields_cert "L4 c0 n1 z16 o0 w8 o8 w4 ; k4 c0 s0" = false.
Proof. reflexivity. Qed.

(* A field past the payload: a layout the block cannot hold. *)
Example rejects_field_past_payload :
  check_fields_cert "L4 c0 n1 z12 o0 w8 o8 w8" = false.
Proof. reflexivity. Qed.

(* An access of a field the kind does not declare, and of an unknown kind. *)
Example rejects_undeclared_field :
  check_fields_cert "L4 c0 n1 z16 o0 w8 o8 w4 ; a4 c0 f2" = false /\
  check_fields_cert "L4 c0 n1 z16 o0 w8 o8 w4 ; a5 c0 f0" = false.
Proof. split; reflexivity. Qed.

(* A tag read of a representation one of whose cases does not start with
   the tag word; a case at or past its count; two layouts for one kind. *)
Example rejects_tag_case_dup :
  check_fields_cert "L6 c0 n1 z8 o0 w8 ; t6" = false /\
  check_fields_cert "L4 c1 n1 z8" = false /\
  check_fields_cert "L4 c0 n1 z8 o0 w8 ; L4 c0 n1 z16 o0 w8" = false.
Proof. repeat split; reflexivity. Qed.

Example rejects_garbage :
  check_fields_cert "L4 c0 n1 z8 o0" = false /\ check_fields_cert "a4 c0 f0 x" = false /\
  check_fields_cert "k4 s0" = false /\ check_fields_cert ";" = false.
Proof. repeat split; reflexivity. Qed.

(* The rejections matter: a field read one slot past a one-field layout —
   the shifted offset almide-anchor-check refuses on the bytes — IS out of
   bounds of the block the site built. *)
Example shifted_field_not_spatial :
  ~ Spatial (rsite_events 0%nat 0%nat (mkShape 8 [(0, 8)]%nat) [0%nat] ++
             [ELoad 0%nat (foff (8, 8)%nat) (fwid (8, 8)%nat) 0%nat]).
Proof.
  intro H. unfold Spatial, rsite_events in H. cbn in H.
  destruct H as [_ [_ [_ [[x [Hx Hb]] _]]]].
  injection Hx as <-. cbn in Hb. unfold in_bounds in Hb. cbn in Hb. lia.
Qed.
