import Defs
open LawSpec
-- the hidden tests of each task, evaluated on the translation
#eval (encode (List.replicate 12 'a') == "9a3a".toList, encode (List.replicate 10 'a' ++ ['b']) == "9a1a1b".toList, decode (encode (List.replicate 27 'z')) == List.replicate 27 'z')
#eval (page [1,2,3,4,5,6,7] 3 3, page [1,2,3,4,5,6,7] 0 3, page [1,2,3,4,5,6,7] (-1) 3, pageCount [1,2,3,4,5,6] 3)
#eval (rotate [1,2,3] (-1), rotate [1,2,3] 4, rotate [1,2,3] (-4), rotate ([] : List Nat) 5)
#eval (chunk [1,2,3,4,5] 2, chunk [1,2,3,4] 2, chunk [1,2,3,4,5,6,7] 3, chunk [1,2] 5)
#eval (merge [1,1,2] [2,3], merge [] [4,4], merge [5,5,6] [], merge [1,3] [2,3,3])
#eval (topK [⟨"b",2⟩, ⟨"a",2⟩, ⟨"c",9⟩] 3).map (·.name)
#eval (topK [⟨"z",1⟩, ⟨"m",1⟩, ⟨"a",1⟩] 2).map (·.name)
