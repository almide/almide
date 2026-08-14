import AlmideEditBelt.Corpus

/-!
`conformancegen` — write or verify the committed kernel-conformance corpus.

* `lake exe conformancegen -- --write <dir>`  regenerate `gen_NN.almd` +
  `gen_NN.expected` into `<dir>` (used once per deliberate corpus bump).
* `lake exe conformancegen -- --check <dir>`  regenerate in memory and
  byte-compare against the committed files — the CI drift gate.

The corpus is a pure function of `Corpus.lean` (seeded LCG, no ambient
state), so `--check` failing means exactly one thing: generator and
committed corpus disagree, and one of them was edited without the other.
-/

open LambdaAlmd

def fileStem (i : Nat) : String :=
  if i < 10 then s!"gen_0{i}" else s!"gen_{i}"

def entries : List (String × String × String) :=
  corpus.zipIdx.map fun (p, i) =>
    let stem := fileStem i
    let expected :=
      match expectedOf p with
      | some o => expectedText o
      | none => "<GENERATOR INVARIANT VIOLATION>"
    (stem, renderProg p, expected)

def main (args : List String) : IO UInt32 := do
  match args with
  | ["--write", dir] =>
      IO.FS.createDirAll dir
      for (stem, source, expected) in entries do
        IO.FS.writeFile (System.mkFilePath [dir, stem ++ ".almd"]) source
        IO.FS.writeFile (System.mkFilePath [dir, stem ++ ".expected"]) expected
      IO.println s!"wrote {corpus.length} programs to {dir}"
      pure 0
  | ["--check", dir] =>
      let mut bad : Nat := 0
      for (stem, source, expected) in entries do
        for (name, want) in [(stem ++ ".almd", source), (stem ++ ".expected", expected)] do
          let path := System.mkFilePath [dir, name]
          let got ← try IO.FS.readFile path
            catch _ => pure "<MISSING FILE>"
          if got ≠ want then
            IO.eprintln s!"conformancegen: DRIFT in {name}"
            bad := bad + 1
      if bad == 0 then
        IO.println s!"conformancegen: {corpus.length} programs in sync"
        pure 0
      else
        IO.eprintln s!"conformancegen: {bad} file(s) out of sync — regenerate with --write or fix Corpus.lean"
        pure 1
  | _ =>
      IO.eprintln "usage: conformancegen (--write | --check) <dir>"
      pure 2
