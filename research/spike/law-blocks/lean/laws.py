#!/usr/bin/env python3
"""Try to prove every law-spec law with machine-only tactics.

T0 = automation alone (the definitions of the task may be unfolded: a compiler knows its own code).
T1 = T0 after structural induction on one List argument (a search a machine can enumerate).
Anything that needs a human-written lemma or a hand-picked induction counts as not closed.
"""
import subprocess, sys, os, json

LAWS = [
  # (task, name, binders, statement, list vars, defs to unfold)
  ("t01_rle", "decode inverts encode", "(n m : Nat) (hn : n ≥ 1) (hm : m ≥ 1)",
   "decode (encode (List.replicate n 'a' ++ List.replicate m 'b')) = List.replicate n 'a' ++ List.replicate m 'b'",
   [], "encode, decode, decodeFrom, group, splitRun"),
  ("t01_rle", "every count is a single digit", "(n : Nat) (hn : n ≥ 1) (i : Nat) (hi : 1 ≤ i) (hl : i < (encode (List.replicate n 'a')).length)",
   "¬ (((encode (List.replicate n 'a')).getD (i - 1) ' ').isDigit ∧ ((encode (List.replicate n 'a')).getD i ' ').isDigit)",
   [], "encode, group, splitRun"),
  ("t02_paginate", "pages 1..page_count rebuild the list", "(xs : List Int) (size : Int) (hs : size ≥ 1)",
   "(List.range' 1 (pageCount xs size).toNat).flatMap (fun (p : Nat) => page xs (p : Int) size) = xs",
   ["xs"], "page, pageCount"),
  ("t02_paginate", "a page below 1 is empty", "(xs : List Int) (size p : Int) (hs : size ≥ 1)",
   "p ≥ 1 ∨ (page xs p size).length = 0",
   ["xs"], "page"),
  ("t03_rotate", "rotation keeps the length", "(xs : List Int) (k : Int)",
   "(rotate xs k).length = xs.length", ["xs"], "rotate"),
  ("t03_rotate", "rotating back undoes a rotation", "(xs : List Int) (k : Int)",
   "rotate (rotate xs k) (0 - k) = xs", ["xs"], "rotate"),
  ("t03_rotate", "a full turn is the identity", "(xs : List Int)",
   "rotate xs xs.length = xs", ["xs"], "rotate"),
  ("t04_chunk", "chunks rebuild the list", "(xs : List Int) (n : Nat) (hn : n ≥ 1)",
   "(chunk xs n).flatten = xs", ["xs"], "chunk, restChunks"),
  ("t04_chunk", "no chunk is empty", "(xs : List Int) (n : Nat) (hn : n ≥ 1)",
   "∀ c ∈ chunk xs n, c.length > 0", ["xs"], "chunk, restChunks"),
  ("t04_chunk", "only the first chunk may be short", "(xs : List Int) (n : Nat) (hn : n ≥ 1)",
   "∀ c ∈ (chunk xs n).drop 1, c.length = n", ["xs"], "chunk, restChunks"),
  ("t05_merge", "result is strictly ascending", "(a b : List Int)",
   "∀ i, 1 ≤ i → i < (merge (sort a) (sort b)).length → (merge (sort a) (sort b)).getD (i - 1) 0 < (merge (sort a) (sort b)).getD i 0",
   ["a", "b"], "merge, mergeAll, dedup, sort"),
  ("t05_merge", "nothing is lost", "(a b : List Int)",
   "∀ x ∈ a ++ b, x ∈ merge (sort a) (sort b)", ["a", "b"], "merge, mergeAll, dedup, sort"),
  ("t05_merge", "nothing is invented", "(a b : List Int)",
   "∀ x ∈ merge (sort a) (sort b), x ∈ a ++ b", ["a", "b"], "merge, mergeAll, dedup, sort"),
  ("t06_top_k", "at most k entries", "(es : List Entry) (k : Int) (hk : k ≥ 0)",
   "((topK es k).length : Int) ≤ k", ["es"], "topK"),
  ("t06_top_k", "ordered by count, then name", "(es : List Entry) (k : Int) (hk : k ≥ 0)",
   "∀ i, 1 ≤ i → (hi : i < (topK es k).length) → ((topK es k)[i - 1].count > (topK es k)[i].count ∨ ((topK es k)[i - 1].count = (topK es k)[i].count ∧ (topK es k)[i - 1].name ≤ (topK es k)[i].name))",
   ["es"], "topK, keyLe"),
  ("t06_top_k", "nothing better is left out", "(es : List Entry) (k : Int) (hk : k ≥ 0)",
   "∀ (h : topK es k ≠ []), ∀ e ∈ es, e ∈ topK es k ∨ e.count ≤ ((topK es k).getLast h).count",
   ["es"], "topK, keyLe"),
]

REC = {"t01_rle": ["group", "splitRun", "decodeFrom"], "t04_chunk": ["restChunks"], "t05_merge": ["mergeAll"]}

WRAP = {"t01_rle": "encode, decode", "t04_chunk": "chunk", "t05_merge": "merge, dedup, sort"}

HEAD = """import Defs
open LawSpec
set_option maxHeartbeats 400000
macro "auto" : tactic => `(tactic| first
  | omega
  | decide
  | (simp_all [%(defs)s]; done)
  | (simp_all [%(defs)s] <;> omega)
  | grind [%(defs)s])
"""

def attempt(idx, law, tactic, tag):
    task, name, binders, stmt, _, defs = law
    src = HEAD % {"defs": defs} + f"\ntheorem law_{idx} {binders} :\n    {stmt} := by\n  {tactic}\n"
    path = f"/tmp/lawspec_{idx}_{tag}.lean"
    open(path, "w").write(src)
    try:
        r = subprocess.run(["perl", "-e", "alarm shift; exec @ARGV", "120", "lean", path],
                           capture_output=True, text=True, env={**os.environ, "LEAN_PATH": "."})
        out = r.stdout + r.stderr
    except Exception as e:
        return False, str(e)
    ok = r.returncode == 0 and "error" not in out and "sorry" not in out
    return ok, out

def statement_checks(idx, law):
    ok, out = attempt(idx, law, "sorry", "stmt")
    return "error" not in out, out

results = []
ONLY = set(int(a) for a in sys.argv[1:])
for idx, law in enumerate(LAWS):
    if ONLY and idx not in ONLY: continue
    task, name, _, _, lists, _ = law
    good, out = statement_checks(idx, law)
    if not good:
        print(f"{task:13} {name:40} STATEMENT ERROR\n{out[:600]}"); results.append((task, name, "stmt-error")); continue
    verdict = "human"
    ok, _ = attempt(idx, law, "auto", "t0")
    if ok: verdict = "T0"
    else:
        tries = [(f"induction {v} <;> auto", f"induction {v}") for v in lists]
        tries += [(f"induction {v} generalizing {' '.join(o for o in lists if o != v)} <;> auto", f"induction {v} generalizing")
                  for v in lists if len(lists) > 1]
        tries += [(f"fun_induction {f} <;> auto", f"fun_induction {f}") for f in REC.get(task, [])]
        # the law names a wrapper; unfold it first so the recursive helper's call is in the goal
        tries += [(f"simp only [{WRAP[task]}]; fun_induction {f} <;> auto", f"unfold + fun_induction {f}")
                  for f in REC.get(task, []) if task in WRAP]
        for tac, label in tries:
            ok, _ = attempt(idx, law, tac, "t1_" + label.replace(" ", "_"))
            if ok: verdict = f"T1 ({label})"; break
    print(f"{task:13} {name:40} {verdict}", flush=True)
    results.append((task, name, verdict))
json.dump(results, open("results.json" if not ONLY else "results_partial.json", "w"), indent=1)
