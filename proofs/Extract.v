(* Extract the KERNEL-PROVEN checker to OCaml. With the tokenizer now INSIDE the
   proof (`check_cert` parses bytes AND checks), the whole "certificate bytes ⟶
   accept/reject" pipeline is the extracted proven function `check_cert`. The
   driver only reads the file — the untrusted glue shrinks to I/O. *)

From AlmideTrust Require Import OwnershipChecker.
From AlmideTrust Require Import NameTotality.
From AlmideTrust Require Import CapabilityBound.
From AlmideTrust Require Import CapabilityReach.
From AlmideTrust Require Import CallModes.
From Stdlib Require Import Extraction.
From Stdlib Require Import ExtrOcamlBasic ExtrOcamlNativeString.

Extraction Language OCaml.
Extract Inductive bool => "bool" [ "true" "false" ].
Extract Inductive list => "list" [ "[]" "(::)" ].

Set Extraction Output Directory ".".
(* `check_cert_lc` is the loop-aware ownership checker (format v2, backward-compatible
   with the flat `check_cert`); the driver dispatches ownership to it so loop certs
   (heap-loop-carried accumulators) are accepted on the same proven spine. *)
(* `check_prog_cert` (CapabilityReach) is the TRANSITIVE capability checker: it parses the
   emitted call-graph witness and decides prog_ok, so `accept ⟹ every function's full
   transitive reach ⊆ its declared bound` — the gate consumes the proof instead of the
   untrusted transitive fold. *)
(* `check_clc` is the conditional-loop-aware ownership checker (format v3, a SUPERSET
   of `check_cert_lc`: it also parses `[ then | else ]` filter slots; flat + CLoop
   certs parse identically, so it is fully backward-compatible). The driver dispatches
   ownership to it. *)
(* `check_modes_cert` (CallModes) is the CALL-MODE SIGNATURE checker (brick 2c):
   it parses the `<sigs>|<sites>` witness and decides per-call-site agreement, so
   `accept ⟹ every call site used exactly its callee's declared param modes` —
   the ground fact that makes per-function ownership certs compose
   (CallModes.check_fill_sound). *)
Extraction "checker.ml" check_cert check_cert_lc check_clc check_names_cert check_caps_cert check_prog_cert check_modes_cert.
