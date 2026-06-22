/// The fixed WAT runtime: WASI import, memory, bump allocator, list ops, integer
/// formatting, and line printing. Addresses are the named constants above.
fn preamble() -> String {
    format!(
        r#"(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "random_get"
    (func $random_get (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const {HEAP_BASE}))
  ;; the free-list head (0 = empty) — physical reclamation (A1.2-render), the
  ;; realization of proofs/FreeList.v. A freed block is pushed here; $alloc reuses
  ;; the head when it is EXACTLY the requested size. The link is stored in the dead
  ;; LEN field (offset 4), NOT the rc cell (offset 0), so the rc cell stays 0 and
  ;; the $rc_dec double-free sentinel still fires on a re-release of a freed block.
  (global $freelist (mut i32) (i32.const 0))

  (func $alloc (param $n i32) (result i32)
    (local $p i32) (local $prev i32)
    ;; FIRST-FIT reuse: SEARCH the free-list for ANY block of exactly n bytes and unlink it
    ;; (FreeList.alloc: a valid allocation is the fresh frontier OR a block currently on the free-
    ;; list — searching the list, not just its head, still returns ONLY a free-list block, so the
    ;; proven no-double-free / bounded-reuse properties hold; head-only reuse LEAKED whenever
    ;; heterogeneous sizes interleaved — a smaller block stuck at the head shadowed a size match
    ;; deeper in the list, forcing a fresh bump every iteration). The link lives in the dead LEN
    ;; field. prev==0 marks the head.
    (local.set $prev (i32.const 0))
    (local.set $p (global.get $freelist))
    (block $done
      (loop $scan
        (br_if $done (i32.eqz (local.get $p)))
        (if (i32.eq (i32.add (i32.const {LIST_HEADER})
                             (i32.mul (i32.load (i32.add (local.get $p) (i32.const {LIST_CAP_OFFSET})))
                                      (i32.const {ELEM_SIZE})))
                    (local.get $n))
          (then
            ;; unlink p: head → freelist = p.next; else prev.next = p.next
            (if (i32.eqz (local.get $prev))
              (then (global.set $freelist (i32.load (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET})))))
              (else (i32.store (i32.add (local.get $prev) (i32.const {LIST_LEN_OFFSET}))
                              (i32.load (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET}))))))
            (return (local.get $p))))
        (local.set $prev (local.get $p))
        (local.set $p (i32.load (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET}))))
        (br $scan)))
    ;; not found: bump the frontier (a genuinely fresh block)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (local.get $p) (local.get $n)))
    (local.get $p))

  (func $list_new (param $len i32) (param $cap i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (i32.const {LIST_HEADER})
                                        (i32.mul (local.get $cap) (i32.const {ELEM_SIZE})))))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET})) (local.get $len))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_CAP_OFFSET})) (local.get $cap))
    (local.get $p))

  ;; release one reference (RuntimeModel.v's rt_dec): trap if the cell is already
  ;; 0 (double-free / use-after-free sentinel), else decrement. At 0 the block is
  ;; FREED — returned to the free-list for physical reuse (A1.2-render, refining
  ;; FreeList.v). The link goes in the dead LEN field; the rc cell stays 0 so a
  ;; re-release of the freed block still hits the sentinel above.
  (func $rc_dec (param $p i32)
    (local $rc i32)
    (local.set $rc (i32.load (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET}))))
    (if (i32.eqz (local.get $rc)) (then (unreachable)))
    (local.set $rc (i32.sub (local.get $rc) (i32.const 1)))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (local.get $rc))
    (if (i32.eqz (local.get $rc))
      (then
        (i32.store (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET})) (global.get $freelist))
        (global.set $freelist (local.get $p)))))

  ;; acquire one reference (RuntimeModel.v's rt_inc): the shared-Dup primitive
  ;; (A1.3-render). Realizes WasmRcDec.rc_inc_prog — proven to compute rt_inc.
  (func $rc_inc (param $p i32)
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET}))
               (i32.add (i32.load (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})))
                        (i32.const 1))))

  (func $elem_addr (param $list i32) (param $idx i32) (result i32)
    ;; SAFETY WALL: an out-of-range index would compute an address OUTSIDE the
    ;; block (idx < 0 below it, idx >= cap beyond it) and a $list_set there would
    ;; corrupt memory — the ownership checker accepts (it tracks RC, not bounds),
    ;; so this would be accept-but-unsafe. Trap instead, so OOB is a WALL (a
    ;; controlled halt), never silent corruption (the index-bounds memory-safety
    ;; gate; cap is the block's allocated slot count).
    (if (i32.or (i32.lt_s (local.get $idx) (i32.const 0))
                (i32.ge_s (local.get $idx)
                          (i32.load (i32.add (local.get $list) (i32.const {LIST_CAP_OFFSET})))))
      (then (unreachable)))
    (i32.add (i32.add (local.get $list) (i32.const {LIST_HEADER}))
             (i32.mul (local.get $idx) (i32.const {ELEM_SIZE}))))

  (func $list_set (param $list i32) (param $idx i32) (param $val i64)
    (i64.store (call $elem_addr (local.get $list) (local.get $idx)) (local.get $val)))

  (func $list_get (param $list i32) (param $idx i32) (result i64)
    (i64.load (call $elem_addr (local.get $list) (local.get $idx))))

  (func $list_len (param $list i32) (result i32)
    (i32.load (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))))

  (func $list_copy (param $src i32) (result i32)
    (local $len i32) (local $cap i32) (local $dst i32) (local $i i32)
    (local.set $len (i32.load (i32.add (local.get $src) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $cap (i32.load (i32.add (local.get $src) (i32.const {LIST_CAP_OFFSET}))))
    (local.set $dst (call $list_new (local.get $len) (local.get $cap)))
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ge_s (local.get $i) (local.get $len)))
      (call $list_set (local.get $dst) (local.get $i)
                      (call $list_get (local.get $src) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $loop)))
    (local.get $dst))

  (func $list_push (param $list i32) (param $val i64) (result i32)
    (local $len i32)
    (local.set $len (i32.load (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))))
    (call $list_set (local.get $list) (local.get $len) (local.get $val))
    (i32.store (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))
               (i32.add (local.get $len) (i32.const 1)))
    (local.get $list))

  ;; append the decimal digits of a non-negative i64 at $cur; return new cursor
  (func $itoa_append (param $cur i32) (param $v i64) (result i32)
    (local $n i32)
    (if (i64.eqz (local.get $v))
      (then
        (i32.store8 (local.get $cur) (i32.const {ASCII_ZERO}))
        (return (i32.add (local.get $cur) (i32.const 1)))))
    (local.set $n (i32.const 0))
    (block $ddone (loop $dloop
      (br_if $ddone (i64.eqz (local.get $v)))
      (i32.store8 (i32.add (i32.const {ITOA_TMP_ADDR}) (local.get $n))
                  (i32.add (i32.const {ASCII_ZERO})
                           (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const {DECIMAL_BASE})))))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (local.set $v (i64.div_u (local.get $v) (i64.const {DECIMAL_BASE})))
      (br $dloop)))
    (block $cdone (loop $cloop
      (br_if $cdone (i32.eqz (local.get $n)))
      (local.set $n (i32.sub (local.get $n) (i32.const 1)))
      (i32.store8 (local.get $cur)
                  (i32.load8_u (i32.add (i32.const {ITOA_TMP_ADDR}) (local.get $n))))
      (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
      (br $cloop)))
    (local.get $cur))

  ;; print "<label>=<e0>,<e1>,...\n" to stdout
  (func $print_list (param $list i32) (param $lblptr i32) (param $lbllen i32)
    (local $cur i32) (local $i i32) (local $len i32)
    (local.set $cur (i32.const {SCRATCH_ADDR}))
    (local.set $i (i32.const 0))
    (block $lbldone (loop $lblloop
      (br_if $lbldone (i32.ge_s (local.get $i) (local.get $lbllen)))
      (i32.store8 (local.get $cur)
                  (i32.load8_u (i32.add (local.get $lblptr) (local.get $i))))
      (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $lblloop)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_EQUALS}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (local.set $len (call $list_len (local.get $list)))
    (local.set $i (i32.const 0))
    (block $eldone (loop $elloop
      (br_if $eldone (i32.ge_s (local.get $i) (local.get $len)))
      ;; SAFETY WALL: appending an element writes up to a comma + 20 digits; if
      ;; that would cross HEAP_BASE (the line buffer's end), trap rather than
      ;; overflow the buffer into the heap (the print-buffer-overflow gate).
      (if (i32.gt_u (i32.add (local.get $cur) (i32.const {MAX_ELEM_PRINT_BYTES}))
                    (i32.const {HEAP_BASE}))
        (then (unreachable)))
      (if (i32.gt_s (local.get $i) (i32.const 0))
        (then
          (i32.store8 (local.get $cur) (i32.const {ASCII_COMMA}))
          (local.set $cur (i32.add (local.get $cur) (i32.const 1)))))
      (local.set $cur (call $itoa_append (local.get $cur)
                                         (call $list_get (local.get $list) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $elloop)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_NEWLINE}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {SCRATCH_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
               (i32.sub (local.get $cur) (i32.const {SCRATCH_ADDR})))
    (drop (call $fd_write (i32.const {STDOUT_FD}) (i32.const {IOVEC_ADDR})
                          (i32.const {IOVS_COUNT}) (i32.const {NWRITTEN_ADDR}))))

  ;; print a scalar integer followed by a newline
  (func $print_int (param $v i64)
    (local $cur i32)
    (local.set $cur (call $itoa_append (i32.const {SCRATCH_ADDR}) (local.get $v)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_NEWLINE}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {SCRATCH_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
               (i32.sub (local.get $cur) (i32.const {SCRATCH_ADDR})))
    (drop (call $fd_write (i32.const {STDOUT_FD}) (i32.const {IOVEC_ADDR})
                          (i32.const {IOVS_COUNT}) (i32.const {NWRITTEN_ADDR}))))

"#
    )
}
