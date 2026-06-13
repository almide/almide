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
    prerr_endline "usage: checker <ownership|names> <witness-file>";
    exit 2);
  let mode = Sys.argv.(1) in
  let bytes = read_file Sys.argv.(2) in
  let accepted =
    match mode with
    | "ownership" -> Checker.check_cert bytes
    | "names" -> Checker.check_names_cert bytes
    | m ->
      prerr_endline ("unknown property: " ^ m ^ " (try: ownership | names)");
      exit 2
  in
  if accepted then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
