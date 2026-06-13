(* Extract the KERNEL-PROVEN ownership checker to OCaml, so the very function
   `check` whose soundness is proven in OwnershipChecker.v RUNS on real
   certificate files. This is the proof-carrying-code chain made operational:
   the trusted artifact is the EXTRACTED `check` (carrying its Coq proof); the
   only untrusted glue is the ~10-line tokenizer in driver.ml (recorded in
   known-limitations, to be internalized into Coq for full qualification). *)

From AlmideTrust Require Import OwnershipChecker.
From Stdlib Require Import Extraction.

Extraction Language OCaml.
Extract Inductive bool => "bool" [ "true" "false" ].
Extract Inductive list => "list" [ "[]" "(::)" ].

Set Extraction Output Directory ".".
Extraction "checker.ml" check check_all.
