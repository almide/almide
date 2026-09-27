import Defs
open LawSpec
set_option maxHeartbeats 400000

-- One human-written lemma per helper, each stated over the helper with plain variables
-- (the generalization a machine did not find), each proved by the same machine-only recipe.

theorem restChunks_flatten (xs : List Int) (n : Nat) (hn : n ≥ 1) : (restChunks xs n).flatten = xs := by
  fun_induction restChunks <;> grind [restChunks, List.take_append_drop]

theorem restChunks_nonempty (xs : List Int) (n : Nat) : ∀ c ∈ restChunks xs n, c.length > 0 := by
  fun_induction restChunks <;> grind [restChunks]

theorem mergeAll_mem (a b : List Int) (x : Int) : x ∈ mergeAll a b ↔ x ∈ a ∨ x ∈ b := by
  fun_induction mergeAll <;> grind [mergeAll]

-- the laws, closed by automation once the lemma exists
theorem chunks_rebuild (xs : List Int) (n : Nat) (hn : n ≥ 1) : (chunk xs n).flatten = xs := by
  have h1 := restChunks_flatten xs n hn
  have h2 := restChunks_flatten (xs.drop (xs.length % n)) n hn
  grind [chunk, List.take_append_drop]

theorem no_chunk_empty (xs : List Int) (n : Nat) (hn : n ≥ 1) : ∀ c ∈ chunk xs n, c.length > 0 := by
  have h1 := restChunks_nonempty xs n
  have h2 := restChunks_nonempty (xs.drop (xs.length % n)) n
  grind [chunk]
