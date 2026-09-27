#!/usr/bin/env python3
"""Per-function laws: can a fixed machine recipe prove the task laws bottom-up?

The discipline under test ("theorem proving without the mathematics"):
  * a person writes only law STATEMENTS, one small set per function, helpers included;
  * the machine proves each law with one fixed recipe, bottom-up over the call graph,
    using only: the subject function's definition, laws already proved in the task,
    and a fixed stdlib law list chosen once for every task (STD below).
No law gets a hand-written tactic, lemma name or proof term.

Laws marked `target` are the 16 law-spec laws (some restated in list-predicate form, see
RESTATED). Lean is the measuring instrument here, not part of the proposed mechanism.
"""
import subprocess, os, sys, json

STD = ("List.take_append_drop, List.length_take, List.length_drop, List.length_append, "
       "List.flatten_cons, List.flatten_nil, List.drop_drop, List.mem_mergeSort, List.pairwise_mergeSort, "
       "List.mergeSort_perm, List.take_sublist, List.length_replicate, List.range'_succ, "
       "List.flatten_append, List.take_of_length_le, List.Pairwise.sublist")

# Helpers a law writer introduced to name a piece of arithmetic the code inlines (t03 only).
EXTRA_DEFS = """
namespace LawSpec
def rotl (xs : List α) (s : Nat) : List α := xs.drop s ++ xs.take s
def shiftOf (n k : Int) : Nat := (((k % n) + n) % n).toNat
end LawSpec
"""

# (name, target?, binders, statement, defs-to-unfold, recursive subjects, induction vars [(var, generalizing)])
L = [
  # ── t04 chunk ──
  ("rc_flatten", False, "(xs : List Int) (n : Nat) (hn : n ≥ 1)", "(restChunks xs n).flatten = xs", "restChunks", ["restChunks"], []),
  ("rc_nonempty", False, "(xs : List Int) (n : Nat)", "∀ c ∈ restChunks xs n, c.length > 0", "restChunks", ["restChunks"], []),
  ("rc_full", False, "(xs : List Int) (n : Nat) (hn : n ≥ 1)", "xs.length % n = 0 → ∀ c ∈ restChunks xs n, c.length = n", "restChunks", ["restChunks"], []),
  ("t04 chunks rebuild the list", True, "(xs : List Int) (n : Nat) (hn : n ≥ 1)", "(chunk xs n).flatten = xs", "chunk", [], []),
  ("t04 no chunk is empty", True, "(xs : List Int) (n : Nat) (hn : n ≥ 1)", "∀ c ∈ chunk xs n, c.length > 0", "chunk", [], []),
  ("t04 only the first chunk may be short", True, "(xs : List Int) (n : Nat) (hn : n ≥ 1)", "∀ c ∈ (chunk xs n).drop 1, c.length = n", "chunk", [], []),
  # ── t05 merge ──
  ("ma_mem", False, "(a b : List Int) (x : Int)", "x ∈ mergeAll a b ↔ x ∈ a ∨ x ∈ b", "mergeAll", ["mergeAll"], []),
  ("ma_sorted", False, "(a b : List Int)", "a.Pairwise (· ≤ ·) → b.Pairwise (· ≤ ·) → (mergeAll a b).Pairwise (· ≤ ·)", "mergeAll", ["mergeAll"], []),
  ("dd_mem", False, "(xs : List Int) (x : Int)", "x ∈ dedup xs ↔ x ∈ xs", "dedup", [], [("xs", "")]),
  ("dd_strict", False, "(xs : List Int)", "xs.Pairwise (· ≤ ·) → (dedup xs).Pairwise (· < ·)", "dedup", [], [("xs", "")]),
  ("so_mem", False, "(xs : List Int) (x : Int)", "x ∈ sort xs ↔ x ∈ xs", "sort", [], []),
  ("so_sorted", False, "(xs : List Int)", "(sort xs).Pairwise (· ≤ ·)", "sort", [], []),
  ("t05 result is strictly ascending", True, "(a b : List Int)", "(merge (sort a) (sort b)).Pairwise (· < ·)", "merge", [], []),
  ("t05 nothing is lost", True, "(a b : List Int)", "∀ x ∈ a ++ b, x ∈ merge (sort a) (sort b)", "merge", [], []),
  ("t05 nothing is invented", True, "(a b : List Int)", "∀ x ∈ merge (sort a) (sort b), x ∈ a ++ b", "merge", [], []),
  # ── t06 top_k ──
  ("kl_total", False, "(a b : Entry)", "keyLe a b = true ∨ keyLe b a = true", "keyLe", [], []),
  ("kl_trans", False, "(a b c : Entry)", "keyLe a b = true → keyLe b c = true → keyLe a c = true", "keyLe", [], []),
  ("tk_sorted", False, "(es : List Entry) (k : Int)", "(topK es k).Pairwise (fun a b => keyLe a b = true)", "topK", [], []),
  ("t06 at most k entries", True, "(es : List Entry) (k : Int) (hk : k ≥ 0)", "((topK es k).length : Int) ≤ k", "topK", [], []),
  ("t06 ordered by count, then name", True, "(es : List Entry) (k : Int)", "(topK es k).Pairwise (fun a b => a.count > b.count ∨ (a.count = b.count ∧ a.name ≤ b.name))", "topK, keyLe", [], []),
  ("t06 nothing better is left out", True, "(es : List Entry) (k : Int) (hk : k ≥ 0)", "∀ (h : topK es k ≠ []), ∀ e ∈ es, e ∈ topK es k ∨ e.count ≤ ((topK es k).getLast h).count", "topK, keyLe", [], []),
  # ── t03 rotate (rotl / shiftOf are law-writer helpers, see EXTRA_DEFS) ──
  ("ro_def", False, "(xs : List Int) (k : Int)", "rotate xs k = if xs.length = 0 then xs else rotl xs (shiftOf xs.length k)", "rotate, rotl, shiftOf", [], []),
  ("rl_len", False, "(xs : List Int) (s : Nat)", "(rotl xs s).length = xs.length", "rotl", [], []),
  ("rl_zero", False, "(xs : List Int)", "rotl xs 0 = xs", "rotl", [], []),
  ("rl_back", False, "(xs : List Int) (s : Nat)", "s ≤ xs.length → rotl (rotl xs s) (xs.length - s) = xs", "rotl", [], []),
  ("sh_lt", False, "(n k : Int)", "n > 0 → shiftOf n k < n", "shiftOf", [], []),
  ("sh_neg", False, "(n k : Int)", "n > 0 → shiftOf n (0 - k) = (if shiftOf n k = 0 then 0 else n.toNat - shiftOf n k)", "shiftOf", [], []),
  ("t03 rotation keeps the length", True, "(xs : List Int) (k : Int)", "(rotate xs k).length = xs.length", "rotate", [], []),
  ("t03 rotating back undoes a rotation", True, "(xs : List Int) (k : Int)", "rotate (rotate xs k) (0 - k) = xs", "", [], []),
  ("t03 a full turn is the identity", True, "(xs : List Int)", "rotate xs xs.length = xs", "rotate", [], []),
  # ── t02 paginate ──
  ("pg_slice", False, "(xs : List Int) (size : Int) (hs : size ≥ 1) (s j : Nat) (h1 : s ≥ 1)",
   "(List.range' s j).flatMap (fun (p : Nat) => page xs (p : Int) size) = (xs.drop ((s - 1) * size.toNat)).take (j * size.toNat)",
   "page", [], [("j", "s")]),
  ("pc_cover", False, "(xs : List Int) (size : Int) (hs : size ≥ 1)", "xs.length ≤ (pageCount xs size).toNat * size.toNat", "pageCount", [], []),
  ("t02 pages 1..page_count rebuild the list", True, "(xs : List Int) (size : Int) (hs : size ≥ 1)",
   "(List.range' 1 (pageCount xs size).toNat).flatMap (fun (p : Nat) => page xs (p : Int) size) = xs", "", [], []),
  ("t02 a page below 1 is empty", True, "(xs : List Int) (size p : Int) (hs : size ≥ 1)", "p ≥ 1 ∨ (page xs p size).length = 0", "page", [], []),
  # ── t01 rle ──
  ("gr_rep", False, "(n : Nat) (c : Char) (hn : n ≥ 1)", "group (List.replicate n c) = [(c, n)]", "group", ["group"], [("n", "")]),
  ("gr_two", False, "(n m : Nat) (a b : Char) (hn : n ≥ 1) (hm : m ≥ 1) (hab : a ≠ b)",
   "group (List.replicate n a ++ List.replicate m b) = [(a, n), (b, m)]", "group", [], [("n", "")]),
  ("df_split", False, "(c : Char) (n : Nat) (rest : List Char) (hc : c.isDigit = false)",
   "decodeFrom (splitRun c n ++ rest) 0 = List.replicate n c ++ decodeFrom rest 0", "splitRun, decodeFrom", ["splitRun"], []),
  ("t01 decode inverts encode", True, "(n m : Nat) (hn : n ≥ 1) (hm : m ≥ 1)",
   "decode (encode (List.replicate n 'a' ++ List.replicate m 'b')) = List.replicate n 'a' ++ List.replicate m 'b'", "decode, encode", [], []),
  ("t01 every count is a single digit", True, "(n : Nat) (hn : n ≥ 1) (i : Nat) (hi : 1 ≤ i) (hl : i < (encode (List.replicate n 'a')).length)",
   "¬ (((encode (List.replicate n 'a')).getD (i - 1) ' ').isDigit ∧ ((encode (List.replicate n 'a')).getD i ' ').isDigit)", "encode", [], []),
]

RESTATED = {
  "t05 result is strictly ascending": "was: adjacent pairs by index with getD; now List.Pairwise (<)",
  "t06 ordered by count, then name": "was: adjacent pairs by index; now List.Pairwise of the same relation",
}

HEAD = "import Defs\nset_option maxHeartbeats 400000\n" + EXTRA_DEFS + "open LawSpec\n"

def ident(name):
    return "law_" + "".join(ch if ch.isalnum() else "_" for ch in name)

def run(src, tag):
    path = f"/tmp/modular_{tag}.lean"
    open(path, "w").write(src)
    r = subprocess.run(["perl", "-e", "alarm shift; exec @ARGV", "90", "lean", path],
                       capture_output=True, text=True, env={**os.environ, "LEAN_PATH": "."})
    out = r.stdout + r.stderr
    return r.returncode == 0 and "error" not in out and "sorry" not in out, out

proved = []          # (ident, theorem text)
report = []
for name, target, binders, stmt, defs, recs, ivars in L:
    lemmas = ", ".join([d for d in [defs] if d] + [p for p, _ in proved] + [STD])
    closers = [f"grind [{lemmas}]", f"(simp_all [{lemmas}]; done)", f"(simp_all [{lemmas}] <;> omega)"]
    recipe = list(closers)
    recipe += [f"fun_induction {f} <;> grind [{lemmas}]" for f in recs]
    recipe += [f"induction {v}{(' generalizing ' + g) if g else ''} <;> grind [{lemmas}]" for v, g in ivars]
    theorem = lambda tac: f"theorem {ident(name)} {binders} :\n    {stmt} := by\n  {tac}\n"
    context = HEAD + "".join(t for _, t in proved)
    ok_stmt, out = run(context + theorem("sorry"), ident(name) + "_stmt")
    if "error" in out:
        verdict, used = "STATEMENT ERROR", out[:400]
    else:
        verdict, used = "not closed", ""
        for i, tac in enumerate(recipe):
            ok, out = run(context + theorem(tac), f"{ident(name)}_{i}")
            if ok:
                verdict, used = "closed", tac.split(" [")[0]
                proved.append((ident(name), theorem(tac)))
                break
    mark = "TARGET" if target else "helper"
    print(f"{mark:6} {name:42} {verdict:12} {used}", flush=True)
    report.append({"law": name, "target": target, "verdict": verdict, "recipe": used, "restated": RESTATED.get(name, "")})

json.dump(report, open("modular_results.json", "w"), indent=1, ensure_ascii=False)
open("Modular.lean", "w").write(HEAD + "".join(t for _, t in proved))
t = [r for r in report if r["target"]]
print(f"\ntargets closed: {sum(r['verdict'] == 'closed' for r in t)}/{len(t)}; "
      f"helper laws closed: {sum(r['verdict'] == 'closed' for r in report if not r['target'])}/{sum(not r['target'] for r in report)}")
