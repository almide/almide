(* Extract the KERNEL-PROVEN checker to OCaml. With the tokenizer now INSIDE the
   proof (`check_cert` parses bytes AND checks), the whole "certificate bytes ⟶
   accept/reject" pipeline is the extracted proven function `check_cert`. The
   driver only reads the file — the untrusted glue shrinks to I/O. *)

From AlmideTrust Require Import OwnershipChecker.
From AlmideTrust Require Import NameTotality.
From AlmideTrust Require Import CapabilityBound.
From Stdlib Require Import Extraction.
From Stdlib Require Import ExtrOcamlBasic ExtrOcamlNativeString.

Extraction Language OCaml.
Extract Inductive bool => "bool" [ "true" "false" ].
Extract Inductive list => "list" [ "[]" "(::)" ].

Set Extraction Output Directory ".".
Extraction "checker.ml" check_cert check_names_cert check_caps_cert.
