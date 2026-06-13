(* TRUSTED GLUE, now reduced to file I/O. The byte→op tokenizer is no longer
   here — it is INSIDE the proof (`Checker.check_cert` parses the bytes AND
   checks them, carrying `check_cert_sound`). This driver reads the certificate
   file and hands the raw bytes to the proven checker. accept ⟹ every object is
   free of double-free and leak. *)

let read_file path =
  let ic = open_in_bin path in
  let n = in_channel_length ic in
  let s = really_input_string ic n in
  close_in ic;
  s

let () =
  if Array.length Sys.argv < 2 then (prerr_endline "usage: checker <cert>"; exit 2);
  let bytes = read_file Sys.argv.(1) in
  if Checker.check_cert bytes then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
