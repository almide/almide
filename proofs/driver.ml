(* UNTRUSTED thin glue. Tokenizes a per-object certificate into the
   `op list list` that the KERNEL-PROVEN `Checker.check_all` consumes, then runs
   that proven function. The trust lives entirely in `Checker.check_all`
   (extracted from the Coq proof, carrying `check_all_sound`). This tokenizer is
   the one item in known-limitations to internalize into Coq for full
   qualification.

   Certificate format v0 (Metamath-simple): ONE reference-counted object per
   NON-EMPTY line; within a line, `i`/`I` = an ownership +1 (Alloc/Dup),
   `d`/`D` = a −1 (Drop/Consume), whitespace ignored, anything else = malformed
   (reject). Accept ⟹ every object is free of double-free and leak. *)

let parse_line (line : string) : Checker.op list =
  let ops = ref [] in
  String.iter
    (fun c ->
      match c with
      | 'i' | 'I' -> ops := Checker.Inc :: !ops
      | 'd' | 'D' -> ops := Checker.Dec :: !ops
      | ' ' | '\t' | '\r' -> ()
      | _ ->
        prerr_endline ("malformed certificate token: " ^ String.make 1 c);
        exit 2)
    line;
  List.rev !ops

let () =
  if Array.length Sys.argv < 2 then (prerr_endline "usage: checker <cert>"; exit 2);
  let ic = open_in Sys.argv.(1) in
  let objs = ref [] in
  (try
     while true do
       let line = input_line ic in
       if String.trim line <> "" then objs := parse_line line :: !objs
     done
   with End_of_file -> close_in ic);
  let objs = List.rev !objs in
  (* The proven checker decides. accept ⟹ every object: no double-free ∧ no leak. *)
  if Checker.check_all objs then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
