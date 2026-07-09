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
    prerr_endline "usage: checker <ownership|names|caps|caps-transitive> <witness-file>";
    exit 2);
  let mode = Sys.argv.(1) in
  let bytes = read_file Sys.argv.(2) in
  let accepted =
    match mode with
    | "ownership" -> Checker.check_bc bytes  (* branch-aware (v4); flat + CLoop + CCondLoop certs unchanged *)
    | "names" -> Checker.check_names_cert bytes
    | "caps" -> Checker.check_caps_cert bytes
    | "caps-transitive" -> Checker.check_prog_cert bytes  (* call-graph: transitive reach ⊆ declared *)
    | "call-modes" -> Checker.check_modes_cert bytes  (* per-call-site param modes = callee's declared signature *)
    | m ->
      prerr_endline ("unknown property: " ^ m ^ " (try: ownership | names | caps | caps-transitive | call-modes)");
      exit 2
  in
  if accepted then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
