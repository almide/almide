-- Hand translations of the law-spec reference solutions
-- (research/grammar-lab/experiments/law-spec/reference/*.almd).
-- Deviations from the Almide source are marked DEVIATION.

set_option maxHeartbeats 400000

namespace LawSpec

-- ── t01 rle ── DEVIATION: String is modelled as List Char; digits are '0'..'9'.
def group : List Char → List (Char × Nat)
  | [] => []
  | c :: cs =>
    match group cs with
    | (d, n) :: rest => if c = d then (c, n + 1) :: rest else (c, 1) :: (d, n) :: rest
    | [] => [(c, 1)]

def splitRun (c : Char) : Nat → List Char
  | n => if n ≤ 9 then (Nat.toDigits 10 n) ++ [c] else ['9', c] ++ splitRun c (n - 9)
termination_by n => n
decreasing_by omega

def encode (s : List Char) : List Char := (group s).flatMap fun (c, n) => splitRun c n

def decodeFrom : List Char → Nat → List Char
  | [], _ => []
  | c :: rest, count =>
    if c.isDigit then decodeFrom rest (count * 10 + (c.toNat - '0'.toNat))
    else List.replicate count c ++ decodeFrom rest 0

def decode (s : List Char) : List Char := decodeFrom s 0

-- ── t02 paginate ──
def page (items : List α) (n size : Int) : List α :=
  if n < 1 then [] else (items.drop ((n - 1) * size).toNat).take size.toNat

def pageCount (items : List α) (size : Int) : Int := (items.length + size - 1) / size

-- ── t03 rotate ──
def rotate (xs : List α) (k : Int) : List α :=
  let n : Int := xs.length
  if n = 0 then xs
  else
    let s := (((k % n) + n) % n).toNat
    xs.drop s ++ xs.take s

-- ── t04 chunk ── DEVIATION: the Almide source loops forever for n ≤ 0;
-- Lean demands termination, so n ≤ 0 returns [] here (the laws assume n ≥ 1).
def restChunks (xs : List α) (n : Nat) : List (List α) :=
  if h : xs.length = 0 ∨ n = 0 then [] else xs.take n :: restChunks (xs.drop n) n
termination_by xs.length
decreasing_by simp only [List.length_drop]; omega

def chunk (xs : List α) (n : Nat) : List (List α) :=
  let head := xs.length % n
  if xs.length = 0 ∨ n = 0 then []
  else if head = 0 then restChunks xs n
  else xs.take head :: restChunks (xs.drop head) n

-- ── t05 merge ──
def mergeAll : List Int → List Int → List Int
  | [], b => b
  | a, [] => a
  | x :: xs, y :: ys => if x ≤ y then x :: mergeAll xs (y :: ys) else y :: mergeAll (x :: xs) ys

def dedup (xs : List Int) : List Int :=
  xs.foldl (fun acc x => if acc.getLast? = some x then acc else acc ++ [x]) []

def merge (a b : List Int) : List Int := dedup (mergeAll a b)

def sort (xs : List Int) : List Int := xs.mergeSort (· ≤ ·)

-- ── t06 top_k ── DEVIATION: sort_by on the key (-count, name) is a stable mergeSort
-- with the lexicographic comparator; String order is Lean's (lexicographic on chars).
structure Entry where
  name : String
  count : Int
deriving DecidableEq, Repr

def keyLe (a b : Entry) : Bool :=
  a.count > b.count || (a.count == b.count && decide (a.name ≤ b.name))

def topK (es : List Entry) (k : Int) : List Entry := (es.mergeSort keyLe).take k.toNat

end LawSpec
