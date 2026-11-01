(* TRUSTED GLUE, reduced to file I/O. The parsers + checkers are INSIDE the
   proof (`Checker.check_cert` / `Checker.check_names_cert`, each carrying its
   soundness theorem). This driver reads the witness file and dispatches to the
   proven checker for the requested property. accept ⟹ the proven property holds. *)

let read_file path =
  let ic = open_in_bin path in
  let n = in_channel_length ic in
  let s = really_input_string ic n in
  close_in ic;
  s

let () =
  if Array.length Sys.argv < 3 then (
    prerr_endline "usage: checker <ownership|names|caps|caps-transitive|call-modes|bounds|init|fields|rc-sig> <witness-file>";
    exit 2);
  let mode = Sys.argv.(1) in
  let bytes = read_file Sys.argv.(2) in
  let accepted =
    match mode with
    | "ownership" -> Checker.check_xc bytes  (* exit- and abort-aware (v6); flat + CLoop + CCondLoop + CBranch + v5 certs unchanged *)
    | "names" -> Checker.check_names_cert bytes
    | "caps" -> Checker.check_caps_cert bytes
    | "caps-transitive" -> Checker.check_prog_cert bytes  (* call-graph: transitive reach ⊆ declared *)
    | "call-modes" -> Checker.check_modes_cert bytes  (* per-call-site param modes = callee's declared signature *)
    | "bounds" -> Checker.check_lbounds_cert bytes  (* every elided bounds check re-derived, routine stretches by their contracts, self-host procedures by theirs (counted or linear); accept ⟹ Spatial (#4005) *)
    | "init" -> Checker.check_init_cert bytes  (* every construction site fills its block; accept ⟹ initialized (#4013) *)
    | "fields" -> Checker.check_fields_cert bytes  (* every field held to its kind's layout; accept ⟹ in bounds and initialized (#4279) *)
    | "rc-sig" -> Checker.check_sig_cert bytes  (* a helper's body nets each heap param to its declared mode on every path (#4278) *)
    | m ->
      prerr_endline ("unknown property: " ^ m ^ " (try: ownership | names | caps | caps-transitive | call-modes | bounds | init | fields | rc-sig)");
      exit 2
  in
  if accepted then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
