(* UNTRUSTED thin glue. It tokenizes a certificate file into the op list that
   the KERNEL-PROVEN `Checker.check` consumes, then runs that proven function.
   The trust lives entirely in `Checker.check` (extracted from the Coq proof,
   carrying `check_sound`). This tokenizer is the one item in known-limitations
   to internalize into Coq for full qualification.

   Certificate format v0 (Metamath-simple, deliberately): a stream of tokens,
   `i`/`I` = an ownership +1 (Alloc/Dup), `d`/`D` = a −1 (Drop/Consume),
   whitespace ignored, anything else = malformed (reject). *)

let () =
  if Array.length Sys.argv < 2 then (prerr_endline "usage: checker <cert>"; exit 2);
  let ic = open_in Sys.argv.(1) in
  let ops = ref [] in
  (try
     while true do
       let line = input_line ic in
       String.iter
         (fun c ->
           match c with
           | 'i' | 'I' -> ops := Checker.Inc :: !ops
           | 'd' | 'D' -> ops := Checker.Dec :: !ops
           | ' ' | '\t' | '\r' -> ()
           | _ ->
             prerr_endline ("malformed certificate token: " ^ String.make 1 c);
             exit 2)
         line
     done
   with End_of_file -> close_in ic);
  let ops = List.rev !ops in
  (* The proven checker decides. accept ⟹ no double-free ∧ no leak (check_sound). *)
  if Checker.check ops then (print_endline "ACCEPT"; exit 0)
  else (print_endline "REJECT"; exit 1)
