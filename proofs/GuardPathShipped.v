(* Almide trust spine — paths as they ship, judged by GuardPath.v.

   The byte strings below are cut from modules the compiler emitted for the
   reproductions of three memory defects that reached releases (issues 2474,
   2685 and 1909), once by the compiler as it is and once with the defect
   put back. Each fixed path's statement is proved; each defect's is not
   (`Fail`), and the range the defect leaves alone still is. *)

From Stdlib Require Import ZArith List Lia Bool.
From AlmideTrust Require Import GuardWindow GuardFresh GuardPath.
Import ListNotations.
Open Scope Z_scope.

(* ── `list.copy_within` as it ships (the reproduction of issue 2474,
     function 15 of its module): the same guard, the block reached through
     two locals. Both sides are proved. ── *)
Example shipped_copy_within_written :
  path_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] [] true (WLen 3 0).
Proof. path_auto. Qed.

Example shipped_copy_within_read :
  path_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 66; 0; 89; 113; 32; 37; 32; 11; 40; 2; 4; 173; 32;
      36; 32; 35; 125; 125; 87; 113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11;
      65; 12; 106; 32; 35; 167; 106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] [] false (WLen 3 0).
Proof. path_auto. Qed.

(* ── Control: the same function as the compiler emitted it with the defect
     of issue 2474 put back. Its test is `(dst + end) - start <=u len`, an
     addition that wraps for a destination near the top of the 64-bit range,
     and the copy then writes below the payload. The statement for the bytes
     written is not proved; the bytes read are still inside (`start < end <=
     len` is tested apart). ── *)
Goal path_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] [] true (WLen 3 0).
Proof. Fail path_auto. Abort.

Example defect_2474_still_reads_inside :
  path_vc 7
    [ 32; 8; 33; 3; 32; 8; 33; 11; 32; 0; 33; 35; 32; 1; 33; 36; 32; 2; 33; 37;
      32; 36; 32; 11; 40; 2; 4; 173; 32; 36; 32; 11; 40; 2; 4; 173; 84; 27; 33; 36;
      32; 35; 32; 36; 84; 32; 37; 32; 36; 124; 32; 35; 125; 32; 11; 40; 2; 4; 173; 88;
      113; 4; 64; 32; 11; 65; 12; 106; 32; 37; 167; 106; 32; 11; 65; 12; 106; 32; 35; 167;
      106; 32; 36; 32; 35; 125; 167; 252; 10; 0; 0 ] [] false (WLen 3 0).
Proof. path_auto. Qed.

(* ── `list.take` as it ships (the reproduction of issue 2685, function 13 of
     its module, the call with a count near the top of the 64-bit range):
     the byte count is `count * 8` only when `0 <= count < len / 8`, and the
     length otherwise; the new block is asked for that many bytes, and that
     many are copied from the old payload. Both sides are proved, the bytes
     written against the new block, the bytes read against the old one's
     length as read before the allocation. ── *)
Example shipped_take_written :
  path_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 33; 66; 8; 126; 167;
      32; 33; 66; 0; 83; 32; 33; 32; 9; 40; 2; 4; 65; 8; 110; 173; 89; 114; 27; 33;
      10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9; 65; 12; 106; 32; 10; 252;
      10; 0; 0 ] [] true (WFresh 0).
Proof. path_auto. Qed.

Example shipped_take_read :
  path_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 33; 66; 8; 126; 167;
      32; 33; 66; 0; 83; 32; 33; 32; 9; 40; 2; 4; 65; 8; 110; 173; 89; 114; 27; 33;
      10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9; 65; 12; 106; 32; 10; 252;
      10; 0; 0 ] [] false (WLen 0 0).
Proof. path_auto. Qed.

(* ── Control: the same call as the compiler emitted it with the defect of
     issue 2685 put back. The byte count is `min(count * 8, len)` taken
     after the multiplication, which wraps: the bytes read are not proved
     inside the old block. The bytes written still are (the new block is
     asked for exactly the count copied). ── *)
Goal path_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 9; 40; 2; 4; 173;
      32; 33; 66; 8; 126; 32; 33; 66; 8; 126; 32; 9; 40; 2; 4; 173; 85; 27; 167; 32;
      33; 66; 0; 83; 27; 33; 10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9;
      65; 12; 106; 32; 10; 252; 10; 0; 0 ] [] false (WLen 0 0).
Proof. Fail path_auto. Abort.

Example defect_2685_still_writes_inside :
  path_vc 7
    [ 32; 0; 33; 9; 66; 255; 255; 255; 255; 255; 255; 255; 255; 255; 0; 33; 33; 32; 33; 66;
      0; 32; 33; 66; 0; 85; 27; 33; 33; 32; 9; 40; 2; 4; 32; 9; 40; 2; 4; 173;
      32; 33; 66; 8; 126; 32; 33; 66; 8; 126; 32; 9; 40; 2; 4; 173; 85; 27; 167; 32;
      33; 66; 0; 83; 27; 33; 10; 32; 10; 16; 7; 33; 11; 32; 11; 65; 12; 106; 32; 9;
      65; 12; 106; 32; 10; 252; 10; 0; 0 ] [] true (WFresh 0).
Proof. path_auto. Qed.

(* ── `matrix.from_bytes_f32_le` as it ships (the reproduction of issue 1909,
     function 36 of its module, from the clamps of the second call on): rows
     and columns are floored at zero, rows held under 2^28 and columns under
     2^28 / rows by a signed division; the offset is tested on the buffer's
     side (`0 <= off <= len` and `rows * cols * 4 <= len - off`); a block of
     `rows * cols * 8 + 8` bytes is allocated and filled; then a loop reads
     the buffer four bytes at a time and stores each as a double. The path
     enters the `if`, crosses the allocation, two stores and the fill, and
     enters the loop: its counter is forgotten there, and the read is proved
     inside the buffer for every turn, by the length word read before the
     allocation. Rows, columns and offset are whatever the locals hold. ── *)
Example shipped_from_bytes_read :
  path_vc 7
    [ 66; 0; 32; 43; 32; 43; 66; 0; 83; 27; 33; 43; 66; 0; 32; 44; 32; 44; 66; 0;
      83; 27; 33; 44; 32; 43; 66; 128; 128; 128; 128; 1; 85; 4; 64; 65; 199; 7; 33; 14;
      65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 66; 0; 85; 4; 127; 32; 44; 66; 128; 128; 128; 128; 1; 32; 43; 127; 85; 5; 65;
      0; 11; 4; 64; 65; 199; 7; 33; 14; 65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1;
      16; 41; 65; 1; 16; 1; 0; 11; 32; 42; 66; 0; 89; 32; 42; 32; 18; 40; 2; 4;
      173; 87; 113; 32; 43; 32; 44; 126; 66; 4; 126; 32; 18; 40; 2; 4; 173; 32; 42; 125;
      87; 113; 4; 127; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 66; 128; 128; 252; 255;
      7; 85; 4; 64; 65; 145; 7; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 167; 16; 7; 33; 19; 32; 19; 32; 43; 167;
      54; 2; 12; 32; 19; 66; 0; 32; 44; 32; 43; 80; 27; 167; 54; 2; 16; 32; 19; 65;
      20; 106; 65; 0; 32; 43; 32; 44; 126; 66; 8; 126; 167; 252; 11; 0; 65; 0; 33; 20;
      32; 43; 32; 44; 126; 167; 33; 21; 2; 64; 3; 64; 32; 20; 32; 21; 79; 13; 1; 32;
      19; 32; 20; 65; 8; 108; 106; 32; 18; 32; 42; 167; 106; 32; 20; 65; 4; 108; 106; 40;
      0; 12 ]
    [ 190; 187; 57; 2; 20; 32; 20; 65; 1; 106; 33; 20; 12; 0; 11 ]
    false (WLen 18 0).
Proof. path_auto. Qed.

(* The store of the same loop lies in the block the path allocated. *)
Example shipped_from_bytes_stored :
  path_vc 7
    [ 66; 0; 32; 43; 32; 43; 66; 0; 83; 27; 33; 43; 66; 0; 32; 44; 32; 44; 66; 0;
      83; 27; 33; 44; 32; 43; 66; 128; 128; 128; 128; 1; 85; 4; 64; 65; 199; 7; 33; 14;
      65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 66; 0; 85; 4; 127; 32; 44; 66; 128; 128; 128; 128; 1; 32; 43; 127; 85; 5; 65;
      0; 11; 4; 64; 65; 199; 7; 33; 14; 65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1;
      16; 41; 65; 1; 16; 1; 0; 11; 32; 42; 66; 0; 89; 32; 42; 32; 18; 40; 2; 4;
      173; 87; 113; 32; 43; 32; 44; 126; 66; 4; 126; 32; 18; 40; 2; 4; 173; 32; 42; 125;
      87; 113; 4; 127; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 66; 128; 128; 252; 255;
      7; 85; 4; 64; 65; 145; 7; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 167; 16; 7; 33; 19; 32; 19; 32; 43; 167;
      54; 2; 12; 32; 19; 66; 0; 32; 44; 32; 43; 80; 27; 167; 54; 2; 16; 32; 19; 65;
      20; 106; 65; 0; 32; 43; 32; 44; 126; 66; 8; 126; 167; 252; 11; 0; 65; 0; 33; 20;
      32; 43; 32; 44; 126; 167; 33; 21; 2; 64; 3; 64; 32; 20; 32; 21; 79; 13; 1; 32;
      19; 32; 20; 65; 8; 108; 106; 32; 18; 32; 42; 167; 106; 32; 20; 65; 4; 108; 106; 40;
      0; 12; 190; 187; 57; 2; 20 ]
    [ 32; 20; 65; 1; 106; 33; 20; 12; 0; 11 ]
    true (WFresh 0).
Proof. path_auto. Qed.

(* ── Control: the same call as the compiler emitted it with the defect of
     issue 1909 put back. Its test is `off + rows * cols * 4 <= len`, which
     a negative offset passes, and one near the top of the 64-bit range by
     wrapping: the read is not proved inside the buffer. The store still is
     (the block is asked for what the loop writes). ── *)
Goal path_vc 7
    [ 66; 0; 32; 43; 32; 43; 66; 0; 83; 27; 33; 43; 66; 0; 32; 44; 32; 44; 66; 0;
      83; 27; 33; 44; 32; 43; 66; 128; 128; 128; 128; 1; 85; 4; 64; 65; 199; 7; 33; 14;
      65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 66; 0; 85; 4; 127; 32; 44; 66; 128; 128; 128; 128; 1; 32; 43; 127; 85; 5; 65;
      0; 11; 4; 64; 65; 199; 7; 33; 14; 65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1;
      16; 41; 65; 1; 16; 1; 0; 11; 32; 42; 32; 43; 32; 44; 126; 66; 4; 126; 124; 32;
      18; 40; 2; 4; 173; 87; 4; 127; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 66;
      128; 128; 252; 255; 7; 85; 4; 64; 65; 145; 7; 16; 3; 65; 1; 16; 41; 65; 1; 16;
      1; 0; 11; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 167; 16; 7; 33; 19; 32;
      19; 32; 43; 167; 54; 2; 12; 32; 19; 66; 0; 32; 44; 32; 43; 80; 27; 167; 54; 2;
      16; 32; 19; 65; 20; 106; 65; 0; 32; 43; 32; 44; 126; 66; 8; 126; 167; 252; 11; 0;
      65; 0; 33; 20; 32; 43; 32; 44; 126; 167; 33; 21; 2; 64; 3; 64; 32; 20; 32; 21;
      79; 13; 1; 32; 19; 32; 20; 65; 8; 108; 106; 32; 18; 32; 42; 167; 106; 32; 20; 65;
      4; 108; 106; 40; 0; 12 ]
    [ 190; 187; 57; 2; 20; 32; 20; 65; 1; 106; 33; 20; 12; 0; 11 ]
    false (WLen 18 0).
Proof. Fail path_auto. Abort.

Example defect_1909_still_stores_inside :
  path_vc 7
    [ 66; 0; 32; 43; 32; 43; 66; 0; 83; 27; 33; 43; 66; 0; 32; 44; 32; 44; 66; 0;
      83; 27; 33; 44; 32; 43; 66; 128; 128; 128; 128; 1; 85; 4; 64; 65; 199; 7; 33; 14;
      65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1; 16; 41; 65; 1; 16; 1; 0; 11; 32;
      43; 66; 0; 85; 4; 127; 32; 44; 66; 128; 128; 128; 128; 1; 32; 43; 127; 85; 5; 65;
      0; 11; 4; 64; 65; 199; 7; 33; 14; 65; 238; 7; 32; 14; 16; 8; 16; 3; 65; 1;
      16; 41; 65; 1; 16; 1; 0; 11; 32; 42; 32; 43; 32; 44; 126; 66; 4; 126; 124; 32;
      18; 40; 2; 4; 173; 87; 4; 127; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 66;
      128; 128; 252; 255; 7; 85; 4; 64; 65; 145; 7; 16; 3; 65; 1; 16; 41; 65; 1; 16;
      1; 0; 11; 32; 43; 32; 44; 126; 66; 8; 126; 66; 8; 124; 167; 16; 7; 33; 19; 32;
      19; 32; 43; 167; 54; 2; 12; 32; 19; 66; 0; 32; 44; 32; 43; 80; 27; 167; 54; 2;
      16; 32; 19; 65; 20; 106; 65; 0; 32; 43; 32; 44; 126; 66; 8; 126; 167; 252; 11; 0;
      65; 0; 33; 20; 32; 43; 32; 44; 126; 167; 33; 21; 2; 64; 3; 64; 32; 20; 32; 21;
      79; 13; 1; 32; 19; 32; 20; 65; 8; 108; 106; 32; 18; 32; 42; 167; 106; 32; 20; 65;
      4; 108; 106; 40; 0; 12; 190; 187; 57; 2; 20 ]
    [ 32; 20; 65; 1; 106; 33; 20; 12; 0; 11 ]
    true (WFresh 0).
Proof. path_auto. Qed.
