/// The fixed WAT runtime: WASI import, memory, bump allocator, list ops, integer
/// formatting, and line printing. Addresses are the named constants above.
/// The bump allocator starts at [`HEAP_BASE`]; [`preamble_with_bump_base`] shifts
/// it past the mutable-global slot region.
pub(crate) fn preamble() -> String {
    preamble_with_bump_base(HEAP_BASE)
}

/// [`preamble`] with the bump allocator starting at `bump_base` (`HEAP_BASE +
/// 8*mutable_global_count`), so the mutable-global slots `[HEAP_BASE, bump_base)`
/// are never allocated over. With no mutable globals this IS `preamble()`.
pub(crate) fn preamble_with_bump_base(bump_base: u32) -> String {
    format!(
        r#"(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "random_get"
    (func $random_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "args_sizes_get"
    (func $args_sizes_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "args_get"
    (func $args_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "environ_sizes_get"
    (func $environ_sizes_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "environ_get"
    (func $environ_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_open"
    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_read"
    (func $fd_read (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_close"
    (func $fd_close (param i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_filestat_get"
    (func $fd_filestat_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_readdir"
    (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_filestat_get"
    (func $path_filestat_get (param i32 i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_create_directory"
    (func $path_create_directory (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_remove_directory"
    (func $path_remove_directory (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_unlink_file"
    (func $path_unlink_file (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "clock_time_get"
    (func $clock_time_get (param i32 i64 i32) (result i32)))
  (import "wasi_snapshot_preview1" "proc_exit"
    (func $proc_exit (param i32)))
  (memory (export "memory") 1)
  ;; integer div/mod abort messages (C-001/C-035: identical stderr + exit 1 on
  ;; BOTH targets — the native almide_div!/almide_mod! and v0-wasm __div_trap twins).
  (data (i32.const {DIVZERO_MSG_ADDR}) "Error: division by zero\n")
  (data (i32.const {OVERFLOW_MSG_ADDR}) "Error: integer overflow\n")
  (data (i32.const {BOUNDS_MSG_ADDR}) "Error: index out of bounds\n")
  ;; the fs.read_text path_open error message — a CONST byte run the Err arm copies.
  (data (i32.const {RTF_NOTFOUND_ADDR}) "file not found")
  (data (i32.const {FS_ERR_NOENT_ADDR}) "No such file or directory (os error 2)")
  (data (i32.const {FS_ERR_ACCES_ADDR}) "Permission denied (os error 13)")
  (data (i32.const {FS_ERR_NOTDIR_ADDR}) "Not a directory (os error 20)")
  (data (i32.const {FS_ERR_ISDIR_ADDR}) "Is a directory (os error 21)")
  ;; the fs.list_dir path_open(O_DIRECTORY) error message — a CONST byte run the Err arm copies.
  (data (i32.const {RDIR_ERR_ADDR}) "directory not found")
  ;; the fs.write path_open/fd_write error message — a CONST byte run the Err arm copies.
  (data (i32.const {WRITE_ERR_ADDR}) "write failed")
  ;; the fs.mkdir_p path_create_directory error message — a CONST byte run the Err arm copies.
  (data (i32.const {MKDIR_ERR_ADDR}) "mkdir failed")
  ;; the fs.remove_all path_remove_directory/path_unlink_file error message — a CONST byte run.
  (data (i32.const {REMOVE_ERR_ADDR}) "remove failed")
  (global $bump (mut i32) (i32.const {bump_base}))
  ;; env.get's ONE-TIME environ snapshot (the environment is immutable for the
  ;; guest's lifetime): 0 = not yet read; else the pointer array + entry count.
  ;; Caching bounds the WASI scratch to one allocation (a per-call re-read leaked
  ;; envp/envbuf each call — the env.get leak-loop OOM).
  (global $env_envp (mut i32) (i32.const 0))
  (global $env_cnt (mut i32) (i32.const 0))
  ;; __div_trap(msg,len): write the interned abort line to STDERR and proc_exit(1)
  ;; — the render-path twin of v0-wasm's __div_trap (§13 termination convention).
  ;; Uses the fd_write iovec scratch; never returns.
  (func $__div_trap (param $msg i32) (param $len i32)
    (i32.store (i32.const {IOVEC_ADDR}) (local.get $msg))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
      (local.get $len))
    (drop (call $fd_write (i32.const 2) (i32.const {IOVEC_ADDR})
      (i32.const 1) (i32.const {NWRITTEN_ADDR})))
    (call $proc_exit (i32.const 1))
    (unreachable))
  ;; __main_err(s): the explicit-Result main Err protocol — v0 prints `Error: <msg>` to
  ;; STDERR and exits 1 (the native main wrapper); this writes the same three spans
  ;; (prefix / payload bytes / newline) and proc_exit(1). The prefix + newline reuse the
  ;; div-zero line's bytes ("Error: " head, "\n" tail) — no new data segment.
  (func $__main_err (param $s i32)
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {DIVZERO_MSG_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
      (i32.const {MAIN_ERR_PREFIX_LEN}))
    (drop (call $fd_write (i32.const 2) (i32.const {IOVEC_ADDR})
      (i32.const 1) (i32.const {NWRITTEN_ADDR})))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.add (local.get $s) (i32.const {LIST_HEADER})))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
      (i32.load (i32.add (local.get $s) (i32.const {LIST_LEN_OFFSET}))))
    (drop (call $fd_write (i32.const 2) (i32.const {IOVEC_ADDR})
      (i32.const 1) (i32.const {NWRITTEN_ADDR})))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {MAIN_ERR_NL_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET})) (i32.const 1))
    (drop (call $fd_write (i32.const 2) (i32.const {IOVEC_ADDR})
      (i32.const 1) (i32.const {NWRITTEN_ADDR})))
    (call $proc_exit (i32.const 1))
    (unreachable))
  ;; __die(s): abort with the String block s as the STDERR message + exit 1 —
  ;; the prim.die self-host abort (message bytes at s+12, byte length at s+4).
  (func $__die (param $s i32)
    (call $__div_trap (i32.add (local.get $s) (i32.const 12))
      (i32.load (i32.add (local.get $s) (i32.const 4))))
    (unreachable))
  ;; CHECKED i64 division/remainder: divisor 0 and the MIN/-1 overflow abort with
  ;; the SAME bytes + exit code as native (never a bare wasm hard trap = exit 134).
  (func $__chk_div (param $a i64) (param $b i64) (result i64)
    (if (i64.eqz (local.get $b))
      (then (call $__div_trap (i32.const {DIVZERO_MSG_ADDR}) (i32.const 24))))
    (if (i32.and (i64.eq (local.get $a) (i64.const -9223372036854775808))
                 (i64.eq (local.get $b) (i64.const -1)))
      (then (call $__div_trap (i32.const {OVERFLOW_MSG_ADDR}) (i32.const 24))))
    (i64.div_s (local.get $a) (local.get $b)))
  (func $__chk_rem (param $a i64) (param $b i64) (result i64)
    (if (i64.eqz (local.get $b))
      (then (call $__div_trap (i32.const {DIVZERO_MSG_ADDR}) (i32.const 24))))
    (if (i32.and (i64.eq (local.get $a) (i64.const -9223372036854775808))
                 (i64.eq (local.get $b) (i64.const -1)))
      (then (call $__div_trap (i32.const {OVERFLOW_MSG_ADDR}) (i32.const 24))))
    (i64.rem_s (local.get $a) (local.get $b)))
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
    ;; GROW the linear memory if the new frontier passed the last allocated page. The wasm memory
    ;; starts at 1 page (64 KiB) with no max; a program that allocates more (a deep recursive
    ;; List-accumulator, a large file read) MUST grow it or the next store traps OOB. `memory.size`
    ;; returns the current page count; grow by exactly enough whole pages to cover `$bump`. This
    ;; touches ONLY the page count — no rc cell, no free-list link, no allocation identity — so the
    ;; FreeList.v / ownership accounting is unchanged (the proof surface is byte addresses below the
    ;; frontier, which growing only extends). `memory.grow` returning -1 (host refused) leaves the
    ;; trap-on-OOB behavior exactly as before — never a silent wrong value.
    (if (i32.gt_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (drop (memory.grow
          (i32.add
            (i32.div_u (i32.sub (i32.sub (global.get $bump) (i32.const 1))
                                (i32.mul (memory.size) (i32.const 65536)))
                       (i32.const 65536))
            (i32.const 1))))))
    (local.get $p))

  ;; 8-byte-ALIGNED bump alloc for TRANSIENT WASI out-param scratch (fd_out/stat/iov/
  ;; nread/read-buffer) — the host's `fd_filestat_get` writes an i64 at stat+32, which
  ;; traps unless the buffer is 8-aligned (the `$alloc` byte-sized String frontier leaves
  ;; the bump at arbitrary parity). This NEVER frees (scratch is immortal, like the emit
  ;; backend's `__alloc_pinned`), so it is OUTSIDE the free-list / `$alloc` proof surface:
  ;; it only rounds `$bump` up to 8 and advances — no rc cell, no free-list link, the
  ;; FreeList.v-realizing `$alloc` is untouched.
  (func $alloc8 (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.const 7)) (i32.const -8)))
    (global.set $bump (i32.add (local.get $p) (local.get $n)))
    ;; Grow the linear memory past the last page if this (possibly large — a 4 KiB readdir buffer, a
    ;; file-content buffer) scratch alloc crossed it. Same page-count-only grow as `$alloc`.
    (if (i32.gt_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (drop (memory.grow
          (i32.add
            (i32.div_u (i32.sub (i32.sub (global.get $bump) (i32.const 1))
                                (i32.mul (memory.size) (i32.const 65536)))
                       (i32.const 65536))
            (i32.const 1))))))
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

  ;; USER-FACING checked element address: bounds against LEN (not cap — a slot
  ;; between len and cap is uninitialized), aborting with the native-identical
  ;; "Error: index out of bounds" + exit 1 (never a bare unreachable = exit 134).
  ;; Internal fill paths (writes at idx == len during construction) keep the
  ;; cap-checked $elem_addr above.
  (func $elem_addr_chk (param $list i32) (param $idx i32) (result i32)
    (if (i32.or (i32.lt_s (local.get $idx) (i32.const 0))
                (i32.ge_s (local.get $idx)
                          (i32.load (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET})))))
      (then (call $__div_trap (i32.const {BOUNDS_MSG_ADDR}) (i32.const 27))))
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

  ;; env.args() — build a fresh OWNED `List[String]` of the program arguments
  ;; argv[1..] (SKIP argv[0] = program path, mirroring native `env.args`). The
  ;; WASI floor: `args_sizes_get` gives argc + the flat NUL-terminated argv buffer
  ;; size; `args_get` fills a pointer array + that buffer. We then build the
  ;; canonical `[rc][len][cap][data:i64…]` list of `argc-1` Strings, each a
  ;; canonical `[rc][len][cap][bytes…]` String copied from the argv C-string. The
  ;; result is the third sandbox exit (Capability::CliArgs) — its dst is an owned
  ;; heap handle the caller's scope-end DropListStr balances.
  ;; $skip = how many leading argv entries to drop: 1 = env.args (argv[1..], the
  ;; program args only), 0 = process.args (argv[0..] — std::env::args includes the
  ;; program path). ONE WAT bridge serves both prims (no host-floor growth).
  (func $args_get_list (param $skip i32) (result i32)
    (local $argc_ptr i32) (local $bufsz_ptr i32) (local $argc i32)
    (local $count i32) (local $bufsz i32) (local $argv i32) (local $argbuf i32)
    (local $result i32) (local $i i32) (local $cstr i32) (local $slen i32)
    (local $str i32) (local $j i32)
    ;; Phase 1: argc + total argv buffer size (two i32 out-params from the bump heap).
    (local.set $argc_ptr (call $alloc (i32.const 4)))
    (local.set $bufsz_ptr (call $alloc (i32.const 4)))
    (drop (call $args_sizes_get (local.get $argc_ptr) (local.get $bufsz_ptr)))
    (local.set $argc (i32.load (local.get $argc_ptr)))
    (local.set $bufsz (i32.load (local.get $bufsz_ptr)))
    ;; count = max(argc - $skip, 0). Clamp so a degenerate argc never underflows
    ;; the unsigned loop bound below.
    (local.set $count
      (select (i32.sub (local.get $argc) (local.get $skip)) (i32.const 0)
              (i32.ge_u (local.get $argc) (local.get $skip))))
    ;; Phase 2: alloc the pointer array (argc i32 ptrs, +4 guard) + the string buffer,
    ;; then fill them via args_get.
    (local.set $argv (call $alloc (i32.add (i32.mul (local.get $argc) (i32.const 4)) (i32.const 4))))
    (local.set $argbuf (call $alloc (i32.add (local.get $bufsz) (i32.const 4))))
    (drop (call $args_get (local.get $argv) (local.get $argbuf)))
    ;; Phase 3: build the List[String] (len = cap = count). Per result slot $i, take
    ;; argv[$i + 1], strlen-scan it, alloc a canonical String, copy the bytes, store
    ;; the i64-widened String pointer into the slot.
    (local.set $result (call $list_new (local.get $count) (local.get $count)))
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ge_u (local.get $i) (local.get $count)))
      ;; cstr = argv[$i + $skip]
      (local.set $cstr (i32.load (i32.add (local.get $argv)
                                          (i32.mul (i32.add (local.get $i) (local.get $skip)) (i32.const 4)))))
      ;; slen = strlen(cstr): scan to NUL
      (local.set $slen (i32.const 0))
      (block $sdone (loop $sloop
        (br_if $sdone (i32.eqz (i32.load8_u (i32.add (local.get $cstr) (local.get $slen)))))
        (local.set $slen (i32.add (local.get $slen) (i32.const 1)))
        (br $sloop)))
      ;; alloc a canonical String [rc][len][cap][bytes] and set its header
      (local.set $str (call $alloc (i32.add (i32.const {LIST_HEADER}) (local.get $slen))))
      (i32.store (i32.add (local.get $str) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))
      (i32.store (i32.add (local.get $str) (i32.const {LIST_LEN_OFFSET})) (local.get $slen))
      (i32.store (i32.add (local.get $str) (i32.const {LIST_CAP_OFFSET})) (local.get $slen))
      ;; copy $slen bytes from cstr into str+LIST_HEADER
      (local.set $j (i32.const 0))
      (block $cdone (loop $cloop
        (br_if $cdone (i32.ge_u (local.get $j) (local.get $slen)))
        (i32.store8 (i32.add (i32.add (local.get $str) (i32.const {LIST_HEADER})) (local.get $j))
                    (i32.load8_u (i32.add (local.get $cstr) (local.get $j))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $cloop)))
      ;; result[$i] = str (i64-widened pointer in the 8-byte element slot)
      (call $list_set (local.get $result) (local.get $i) (i64.extend_i32_u (local.get $str)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $loop)))
    (local.get $result))

  ;; env.get(name) — the WASI environ lookup floor. Scans the `KEY=VALUE\0` entries
  ;; (environ_sizes_get/environ_get — the SAME two-phase discovery as $args_get_list)
  ;; for `name` followed by '=' (byte-exact against the canonical String's bytes @12;
  ;; first hit wins — std::env::var is the oracle, C-133). Returns a fresh OWNED
  ;; `Option[String]`: a len-0 block (none) or a len-1 block whose @12 slot owns the
  ;; value String (the `materialize_opt_str_some` layout) — the caller's `match`/`??`/
  ;; `DropListStr` machinery handles it identically to a self-host-built Option. The
  ;; Env profile's Capability::CliArgs sandbox exit; dst is an owned heap handle.
  (func $env_get (param $key i32) (result i32)
    (local $klen i32) (local $kdata i32)
    (local $cnt_ptr i32) (local $sz_ptr i32) (local $cnt i32) (local $bufsz i32)
    (local $envp i32) (local $envbuf i32) (local $i i32) (local $entry i32)
    (local $j i32) (local $val i32) (local $vlen i32) (local $str i32)
    (local $opt i32)
    (local.set $klen (i32.load (i32.add (local.get $key) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $kdata (i32.add (local.get $key) (i32.const {LIST_HEADER})))
    ;; Phases 1-2 run ONCE per program (the guest environment is immutable):
    ;; the snapshot's pointer array + count live in $env_envp/$env_cnt. WASI
    ;; demands 4-ALIGNED i32 out-pointers and $alloc guarantees no alignment —
    ;; over-allocate and round up (the +3 & -4 idiom), for the pointer ARRAY too.
    (if (i32.eqz (global.get $env_envp))
      (then
        (local.set $cnt_ptr (i32.and (i32.add (call $alloc (i32.const 8)) (i32.const 3)) (i32.const -4)))
        (local.set $sz_ptr (i32.and (i32.add (call $alloc (i32.const 8)) (i32.const 3)) (i32.const -4)))
        (drop (call $environ_sizes_get (local.get $cnt_ptr) (local.get $sz_ptr)))
        (local.set $cnt (i32.load (local.get $cnt_ptr)))
        (local.set $bufsz (i32.load (local.get $sz_ptr)))
        (local.set $envp (i32.and (i32.add (call $alloc (i32.add (i32.mul (local.get $cnt) (i32.const 4)) (i32.const 8))) (i32.const 3)) (i32.const -4)))
        (local.set $envbuf (call $alloc (i32.add (local.get $bufsz) (i32.const 4))))
        (drop (call $environ_get (local.get $envp) (local.get $envbuf)))
        (global.set $env_envp (local.get $envp))
        (global.set $env_cnt (local.get $cnt))))
    (local.set $envp (global.get $env_envp))
    (local.set $cnt (global.get $env_cnt))
    ;; Phase 3: scan. $opt = 0 marks "not found yet".
    (local.set $opt (i32.const 0))
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $entry (i32.load (i32.add (local.get $envp) (i32.mul (local.get $i) (i32.const 4)))))
      ;; Prefix compare: $j == $klen afterwards ⟺ the key bytes all matched.
      (local.set $j (i32.const 0))
      (block $pdone (loop $ploop
        (br_if $pdone (i32.ge_u (local.get $j) (local.get $klen)))
        (br_if $pdone (i32.ne (i32.load8_u (i32.add (local.get $entry) (local.get $j)))
                              (i32.load8_u (i32.add (local.get $kdata) (local.get $j)))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $ploop)))
      (if (i32.and (i32.eq (local.get $j) (local.get $klen))
                   (i32.eq (i32.load8_u (i32.add (local.get $entry) (local.get $klen)))
                           (i32.const 61)))  ;; '='
        (then
          ;; $val = the NUL-terminated value bytes after '='.
          (local.set $val (i32.add (i32.add (local.get $entry) (local.get $klen)) (i32.const 1)))
          (local.set $vlen (i32.const 0))
          (block $sdone (loop $sloop
            (br_if $sdone (i32.eqz (i32.load8_u (i32.add (local.get $val) (local.get $vlen)))))
            (local.set $vlen (i32.add (local.get $vlen) (i32.const 1)))
            (br $sloop)))
          ;; Build the canonical value String [rc][len][cap][bytes].
          (local.set $str (call $alloc (i32.add (i32.const {LIST_HEADER}) (local.get $vlen))))
          (i32.store (i32.add (local.get $str) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))
          (i32.store (i32.add (local.get $str) (i32.const {LIST_LEN_OFFSET})) (local.get $vlen))
          (i32.store (i32.add (local.get $str) (i32.const {LIST_CAP_OFFSET})) (local.get $vlen))
          (local.set $j (i32.const 0))
          (block $cdone (loop $cloop
            (br_if $cdone (i32.ge_u (local.get $j) (local.get $vlen)))
            (i32.store8 (i32.add (i32.add (local.get $str) (i32.const {LIST_HEADER})) (local.get $j))
                        (i32.load8_u (i32.add (local.get $val) (local.get $j))))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))
            (br $cloop)))
          ;; some(str): a len-1 block owning the String @12.
          (local.set $opt (call $list_new (i32.const 1) (i32.const 1)))
          (call $list_set (local.get $opt) (i32.const 0) (i64.extend_i32_u (local.get $str)))))
      (br_if $done (i32.ne (local.get $opt) (i32.const 0)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $loop)))
    ;; none: a len-0 block (the canonical empty Option).
    (if (i32.eqz (local.get $opt))
      (then (local.set $opt (call $list_new (i32.const 0) (i32.const 0)))))
    (local.get $opt))

  ;; fs.read_text(path) — open the file at $path and read its bytes, returning a fresh
  ;; OWNED `Result[String, String]` in the EXACT `materialize_result_str` cap-as-tag
  ;; layout: a 1-slot DynListStr `[rc][len@4=1][cap@8=1][@12 String handle][@16 tag]`
  ;; (tag 0 = Ok, 1 = Err), so the caller's `!`/`match`/`DropListStr` machinery handles
  ;; it identically to a self-host-built Result. $path is a borrowed canonical String
  ;; `[rc][len@4][cap@8][bytes@12…]`. WASI floor: `path_open` (relative to the first
  ;; preopened dir fd 3, leading '/' stripped — the absolute-path fallback the native
  ;; emit's __resolve_path uses) gives a file fd; `fd_filestat_get` its byte size;
  ;; `fd_read` the bytes; we copy them into a canonical String and wrap it Ok. On a
  ;; path_open error we wrap the message "file not found" Err. The FOURTH sandbox exit
  ;; (Capability::FsRead) — the result is an owned heap handle the caller's scope-end
  ;; DropListStr balances (frees the @12 payload String + the block).
  (func $read_text_file (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $fd_out i32) (local $errno i32)
    (local $fd i32) (local $stat i32) (local $fsize i32) (local $iov i32)
    (local $nread i32) (local $data i32) (local $str i32) (local $result i32)
    (local $j i32) (local $msg i32) (local $maddr i32) (local $mlen i32)
    ;; path bytes + length; strip a leading '/' so the path is relative to preopen fd 3.
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    ;; path_open(dirfd=3, dirflags=0, path_ptr, path_len, oflags=0,
    ;;   rights_base = fd_read(2) | fd_seek(4) = 6, rights_inheriting=0, fdflags=0, fd_out)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    (local.set $errno
      (call $path_open (i32.const 3) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 0) (i64.const 6) (i64.const 0) (i32.const 0) (local.get $fd_out)))
    ;; On a path_open error build Err(<native std::io Display>) — the WASI errno maps to
    ;; the EXACT text native std::fs emits ($fs_errno_msg), so `err(e)` byte-matches.
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        ;; errno → the EXACT native std::io Display text, INLINE (§4.1: no new wat func).
        ;; NOENT(44)/ACCES(2)/NOTDIR(54)/ISDIR(31); anything else keeps "file not found".
        (local.set $maddr (i32.const {RTF_NOTFOUND_ADDR}))
        (local.set $mlen (i32.const {RTF_NOTFOUND_LEN}))
        (if (i32.eq (local.get $errno) (i32.const 44)) (then
          (local.set $maddr (i32.const {FS_ERR_NOENT_ADDR})) (local.set $mlen (i32.const {FS_ERR_NOENT_LEN}))))
        (if (i32.eq (local.get $errno) (i32.const 2)) (then
          (local.set $maddr (i32.const {FS_ERR_ACCES_ADDR})) (local.set $mlen (i32.const {FS_ERR_ACCES_LEN}))))
        (if (i32.eq (local.get $errno) (i32.const 54)) (then
          (local.set $maddr (i32.const {FS_ERR_NOTDIR_ADDR})) (local.set $mlen (i32.const {FS_ERR_NOTDIR_LEN}))))
        (if (i32.eq (local.get $errno) (i32.const 31)) (then
          (local.set $maddr (i32.const {FS_ERR_ISDIR_ADDR})) (local.set $mlen (i32.const {FS_ERR_ISDIR_LEN}))))
        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (call $rtf_result (local.get $msg) (i32.const 1)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; fd_filestat_get → file size (i64 @ stat+32; take the low 32 bits). The stat buffer
        ;; MUST be 8-aligned (the host writes an i64 there) — `$alloc8` guarantees it.
        (local.set $stat (call $alloc8 (i32.const 64)))
        (drop (call $fd_filestat_get (local.get $fd) (local.get $stat)))
        (local.set $fsize (i32.load (i32.add (local.get $stat) (i32.const 32))))
        ;; fd_read into a fresh buffer; iov = [buf_ptr, buf_len].
        (local.set $data (call $alloc8 (i32.add (local.get $fsize) (i32.const 8))))
        (local.set $iov (call $alloc8 (i32.const 8)))
        (i32.store (local.get $iov) (local.get $data))
        (i32.store (i32.add (local.get $iov) (i32.const 4)) (local.get $fsize))
        (local.set $nread (call $alloc8 (i32.const 4)))
        (drop (call $fd_read (local.get $fd) (local.get $iov) (i32.const 1) (local.get $nread)))
        (drop (call $fd_close (local.get $fd)))
        ;; the actual byte count read (may be < the stat size) is the String length.
        (local.set $fsize (i32.load (local.get $nread)))
        ;; build the canonical String + copy the bytes, then wrap it Ok.
        (local.set $str (call $rtf_str (local.get $data) (local.get $fsize)))
        (call $rtf_result (local.get $str) (i32.const 0)))))

  ;; helper: copy $len bytes at $src into a fresh canonical String `[rc][len][cap][bytes…]`.
  (func $rtf_str (param $src i32) (param $len i32) (result i32)
    (local $str i32) (local $j i32)
    (local.set $str (call $alloc (i32.add (i32.const {LIST_HEADER}) (local.get $len))))
    (i32.store (i32.add (local.get $str) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))
    (i32.store (i32.add (local.get $str) (i32.const {LIST_LEN_OFFSET})) (local.get $len))
    (i32.store (i32.add (local.get $str) (i32.const {LIST_CAP_OFFSET})) (local.get $len))
    (local.set $j (i32.const 0))
    (block $cdone (loop $cloop
      (br_if $cdone (i32.ge_u (local.get $j) (local.get $len)))
      (i32.store8 (i32.add (i32.add (local.get $str) (i32.const {LIST_HEADER})) (local.get $j))
                  (i32.load8_u (i32.add (local.get $src) (local.get $j))))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $cloop)))
    (local.get $str))

  ;; helper: wrap a String handle into the cap-as-tag `Result[String, String]` block
  ;; `[rc][len@4=1][cap@8=1][@12 String handle][@16 tag]` (tag 0 = Ok, 1 = Err).
  (func $rtf_result (param $payload i32) (param $tag i32) (result i32)
    (local $obj i32)
    (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
    ;; @12 LOW := the String handle (zero-extended, clearing the high half / @16).
    (call $list_set (local.get $obj) (i32.const 0) (i64.extend_i32_u (local.get $payload)))
    ;; @16 := the Ok/Err tag (the slot's high 32 bits).
    (i32.store (i32.add (local.get $obj) (i32.const {RTF_TAG_OFFSET})) (local.get $tag))
    (local.get $obj))

  ;; fs.write(path, content) — the WASI file-WRITE floor. $path and $content are BORROWED
  ;; canonical Strings. Opens (creating + truncating) the file at $path (path_open with
  ;; oflags=O_CREAT(1)|O_TRUNC(8)=9, rights_base=fd_seek(4)|fd_write(64)|fd_filestat_set_size
  ;; (0x400000)=0x400044, preopen fd 3, leading '/' stripped — same resolution as
  ;; $read_text_file), writes $content's bytes via fd_write, and closes the fd. Builds a fresh
  ;; OWNED `Result[Unit, String]`: Ok(()) as a 1-slot block with len@4=0 + @12=0 + tag@16=0 (the
  ;; `materialize_result_ok` convention — the scope-end flat $drop_list_str frees nothing at @12),
  ;; or Err("write failed") via $rtf_result on a path_open error (len@4=1, @12=msg, tag@16=1). The
  ;; FIFTH host-write sandbox exit (Capability::FsWrite — DISTINCT from FsRead). The result is an
  ;; owned heap handle the caller's scope-end DropListStr balances.
  (func $write_text_file (param $path i32) (param $content i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $fd_out i32) (local $errno i32)
    (local $fd i32) (local $iov i32) (local $nwritten i32) (local $obj i32) (local $msg i32)
    ;; path bytes + length; strip a leading '/' so the path is relative to preopen fd 3.
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    ;; path_open(dirfd=3, dirflags=0, path_ptr, path_len, oflags=O_CREAT|O_TRUNC=9,
    ;;   rights_base = fd_seek|fd_write|fd_filestat_set_size = 0x400044, rights_inheriting=0,
    ;;   fdflags=0, fd_out)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    (local.set $errno
      (call $path_open (i32.const 3) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 9) (i64.const 4194372) (i64.const 0) (i32.const 0) (local.get $fd_out)))
    ;; On a path_open error build Err("write failed").
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        (local.set $msg (call $rtf_str (i32.const {WRITE_ERR_ADDR}) (i32.const {WRITE_ERR_LEN})))
        (call $rtf_result (local.get $msg) (i32.const 1)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; iov = [content_data_ptr, content_len]; write it, then close.
        (local.set $iov (call $alloc8 (i32.const 8)))
        (i32.store (local.get $iov) (i32.add (local.get $content) (i32.const {LIST_HEADER})))
        (i32.store (i32.add (local.get $iov) (i32.const 4))
                   (i32.load (i32.add (local.get $content) (i32.const {LIST_LEN_OFFSET}))))
        (local.set $nwritten (call $alloc8 (i32.const 4)))
        (drop (call $fd_write (local.get $fd) (local.get $iov) (i32.const 1) (local.get $nwritten)))
        (drop (call $fd_close (local.get $fd)))
        ;; Build Ok(()) — a 1-slot block with len@4=0 (no owned payload — the
        ;; `materialize_result_ok` convention). @12 (and its high half @16=tag) zeroed by the
        ;; i64.store so the flat DropListStr frees nothing and a `match` reads tag 0 = Ok.
        (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
        (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
        (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
        (local.get $obj))))

  ;; fs.mkdir_p(path) — the WASI directory-CREATE floor. $path is a BORROWED canonical String.
  ;; Creates the directory at $path RECURSIVELY (each '/'-delimited prefix in turn, so `a/b/c`
  ;; makes all three), relative to preopen fd 3 (leading '/' stripped — same resolution as
  ;; $write_text_file). An already-existing dir (errno 20 = EEXIST) counts as success. Builds a
  ;; fresh OWNED `Result[Unit, String]`: Ok(()) as a 1-slot block with len@4=0 + @12=0 + tag@16=0
  ;; (the `materialize_result_ok` convention, IDENTICAL to $write_text_file — the scope-end flat
  ;; $drop_list_str frees nothing at @12), or Err("mkdir failed") via $rtf_result on a
  ;; path_create_directory error (len@4=1, @12=msg, tag@16=1). A mkdir IS a filesystem write
  ;; (Capability::FsWrite — the SAME cap as fs.write). The result is an owned heap handle the
  ;; caller's scope-end DropListStr balances.
  (func $make_dir (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $seg i32) (local $errno i32)
    (local $obj i32) (local $msg i32)
    ;; path bytes + length; strip a leading '/' so the path is relative to preopen fd 3.
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    ;; Create each '/'-delimited prefix. Walk $seg; at each '/' (or the end) create
    ;; path[0..seg] and IGNORE its errno (a missing parent is made by an earlier iteration; an
    ;; existing one returns EEXIST). The full path is created here too (when $seg reaches $plen).
    (local.set $seg (i32.const 0))
    (block $souter (loop $louter
      (br_if $souter (i32.ge_u (local.get $seg) (local.get $plen)))
      (local.set $seg (i32.add (local.get $seg) (i32.const 1)))
      (block $sinner (loop $linner
        (br_if $sinner (i32.ge_u (local.get $seg) (local.get $plen)))
        (br_if $sinner (i32.eq (i32.load8_u (i32.add (local.get $pdata) (local.get $seg)))
                               (i32.const {ASCII_SLASH})))
        (local.set $seg (i32.add (local.get $seg) (i32.const 1)))
        (br $linner)))
      (drop (call $path_create_directory (i32.const 3) (local.get $pdata) (local.get $seg)))
      (br $louter)))
    ;; Final attempt: create the full path, capture errno (EEXIST = 20 here once the loop made it).
    (local.set $errno (call $path_create_directory (i32.const 3) (local.get $pdata) (local.get $plen)))
    ;; errno 0 OR 20 (EEXIST) -> Ok(()), else Err("mkdir failed").
    (if (result i32)
        (i32.or (i32.eqz (local.get $errno)) (i32.eq (local.get $errno) (i32.const 20)))
      (then
        ;; Build Ok(()) — a 1-slot block with len@4=0 (no owned payload — the
        ;; `materialize_result_ok` convention), @12/@16 zeroed by the i64.store.
        (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
        (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
        (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
        (local.get $obj))
      (else
        (local.set $msg (call $rtf_str (i32.const {MKDIR_ERR_ADDR}) (i32.const {MKDIR_ERR_LEN})))
        (call $rtf_result (local.get $msg) (i32.const 1)))))

  ;; fs.exists(path) — the WASI path-stat floor. $path is a BORROWED canonical String. Strips a
  ;; leading '/' (path relative to preopen fd 3, same resolution as $read_text_file), then queries
  ;; path_filestat_get(dirfd=3, flags=symlink_follow(1), path, path_len, stat_buf): errno 0 means a
  ;; file OR directory exists there → return 1, else 0 — matching native Path::exists(). The stat
  ;; buffer is 8-aligned $alloc8 scratch (the host writes i64 fields there). Returns a SCALAR i32
  ;; Bool (the caller i64.extend's it) — NO heap result, so no Capability beyond FsRead.
  ;; fs.stat(path) — the WASI FULL-stat floor. $buf is a CALLER-OWNED 64-byte scratch (the
  ;; self-host's Bytes data region — the host writes the WASI filestat there: filetype@16,
  ;; size@32, mtim@48); $path a BORROWED canonical String. Same resolution as $path_exists
  ;; (leading '/' stripped, preopen fd 3, symlink_follow). Returns the RAW errno (0 = ok).
  (func $path_filestat_q (param $buf i32) (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32)
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    (call $path_filestat_get (i32.const 3) (i32.const 1) (local.get $pdata) (local.get $plen)
                             (local.get $buf)))

  (func $path_exists (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $stat i32) (local $errno i32)
    ;; path bytes + length; strip a leading '/' so the path is relative to preopen fd 3.
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    (local.set $stat (call $alloc8 (i32.const 64)))
    (local.set $errno
      (call $path_filestat_get (i32.const 3) (i32.const 1) (local.get $pdata) (local.get $plen)
                               (local.get $stat)))
    (i32.eqz (local.get $errno)))

  ;; io.read_line() — the WASI stdin-line floor. Reads fd 0 BYTE-BY-BYTE into a scratch buffer
  ;; until a '\n' (EXCLUDED from the result) or EOF, strips a trailing '\r', then copies the bytes
  ;; into a fresh OWNED canonical String via $rtf_str — matching native
  ;; read_line().trim_end_matches('\n').trim_end_matches('\r'). The SEVENTH sandbox exit
  ;; (Capability::Stdin). EOF with no bytes yields the empty String. Byte-at-a-time so it never
  ;; over-reads past the newline (a later read of the stream still sees the right bytes). The 4 KiB
  ;; cap bounds a pathological line (JSON-RPC headers are short); the scratch is immortal $alloc8,
  ;; like read_text_file's out-params.
  (func $read_line (result i32)
    (local $buf i32) (local $n i32) (local $cap i32) (local $iov i32) (local $nread_p i32) (local $b i32)
    (local.set $cap (i32.const 4096))
    (local.set $buf (call $alloc8 (local.get $cap)))
    (local.set $iov (call $alloc8 (i32.const 8)))
    (local.set $nread_p (call $alloc8 (i32.const 4)))
    (local.set $n (i32.const 0))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $n) (local.get $cap)))
      ;; iov = [buf+n, 1] — read exactly one byte.
      (i32.store (local.get $iov) (i32.add (local.get $buf) (local.get $n)))
      (i32.store (i32.add (local.get $iov) (i32.const 4)) (i32.const 1))
      (drop (call $fd_read (i32.const 0) (local.get $iov) (i32.const 1) (local.get $nread_p)))
      ;; EOF (0 bytes) -> stop.
      (br_if $done (i32.eqz (i32.load (local.get $nread_p))))
      (local.set $b (i32.load8_u (i32.add (local.get $buf) (local.get $n))))
      ;; newline -> stop (do NOT include it).
      (br_if $done (i32.eq (local.get $b) (i32.const 10)))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (br $l)))
    ;; strip a trailing '\r' (CRLF line endings).
    (if (i32.and (i32.gt_u (local.get $n) (i32.const 0))
                 (i32.eq (i32.load8_u (i32.add (local.get $buf) (i32.sub (local.get $n) (i32.const 1))))
                         (i32.const 13)))
      (then (local.set $n (i32.sub (local.get $n) (i32.const 1)))))
    (call $rtf_str (local.get $buf) (local.get $n)))

  ;; io.read_n_bytes(n) -> List[Int] — the WASI stdin-N-bytes floor. Reads UP TO $want bytes from fd 0
  ;; (a chunked fd_read loop — WASI may return fewer bytes per call; stops at EOF) into a scratch byte
  ;; buffer, then builds a fresh OWNED `List[Int]` of the bytes read (each byte zero-extended to an i64
  ;; element via $list_new/$list_set). The SIBLING of $read_line; carries Capability::Stdin. A List[Int]
  ;; owns NO nested handles (flat Drop). NON-DETERMINISTIC (live stdin). EOF before $want yields fewer.
  (func $read_n_bytes (param $want i32) (result i32)
    (local $buf i32) (local $n i32) (local $iov i32) (local $nread_p i32) (local $got i32)
    (local $list i32) (local $i i32)
    (local.set $buf (call $alloc8 (i32.add (local.get $want) (i32.const 1))))
    (local.set $iov (call $alloc8 (i32.const 8)))
    (local.set $nread_p (call $alloc8 (i32.const 4)))
    (local.set $n (i32.const 0))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $n) (local.get $want)))
      ;; iov = [buf+n, want-n] — request the remaining bytes (the call may return fewer).
      (i32.store (local.get $iov) (i32.add (local.get $buf) (local.get $n)))
      (i32.store (i32.add (local.get $iov) (i32.const 4)) (i32.sub (local.get $want) (local.get $n)))
      (drop (call $fd_read (i32.const 0) (local.get $iov) (i32.const 1) (local.get $nread_p)))
      (local.set $got (i32.load (local.get $nread_p)))
      (br_if $done (i32.eqz (local.get $got)))  ;; EOF -> stop
      (local.set $n (i32.add (local.get $n) (local.get $got)))
      (br $l)))
    ;; build List[Int] of the $n bytes (each byte -> an i64 element).
    (local.set $list (call $list_new (local.get $n) (local.get $n)))
    (local.set $i (i32.const 0))
    (block $bdone (loop $bl
      (br_if $bdone (i32.ge_u (local.get $i) (local.get $n)))
      (call $list_set (local.get $list) (local.get $i)
            (i64.extend_i32_u (i32.load8_u (i32.add (local.get $buf) (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $bl)))
    (local.get $list))

  ;; helper: RECURSIVELY remove the tree at byte-path [$pdata, $pdata+$plen) relative to preopen
  ;; fd 3. Returns 0 on success or the FIRST non-zero errno. If the path opens as a directory it
  ;; removes every entry — recursing via a re-readdir-from-cookie-0 scan that removes ONE entry per
  ;; pass (so a removal never invalidates a live readdir cookie) — then path_remove_directory's the
  ;; emptied directory; otherwise it path_unlink_file's it as a file (matching native remove_dir_all
  ;; vs remove_file). All removals are issued against the preopen fd 3 with full child paths, so the
  ;; opened dir fd needs only fd_readdir rights. Used by $remove_all.
  (func $remove_path (param $pdata i32) (param $plen i32) (result i32)
    (local $fd_out i32) (local $errno i32) (local $fd i32) (local $buf i32) (local $bufused_p i32)
    (local $bufused i32) (local $off i32) (local $namlen i32) (local $nameptr i32)
    (local $child i32) (local $clen i32) (local $i i32) (local $rc i32) (local $found i32)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    ;; path_open(dirfd=3, dirflags=0, path, plen, oflags=O_DIRECTORY=2, rights=fd_readdir(16384),
    ;;   rights_inheriting=16384, fdflags=0, fd_out)
    (local.set $errno
      (call $path_open (i32.const 3) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 2) (i64.const 16384) (i64.const 16384) (i32.const 0) (local.get $fd_out)))
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        ;; not a directory (or missing) — unlink as a file; its errno is the result.
        (call $path_unlink_file (i32.const 3) (local.get $pdata) (local.get $plen)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        (local.set $rc (i32.const 0))
        (local.set $buf (call $alloc8 (i32.const 4096)))
        (local.set $bufused_p (call $alloc8 (i32.const 4)))
        (block $emptied (loop $scan
          ;; re-read from cookie 0 each pass; the buffer holds at least the first real entry
          ;; (after the leading "."/"..") of any directory.
          (drop (call $fd_readdir (local.get $fd) (local.get $buf) (i32.const 4096)
                                  (i64.const 0) (local.get $bufused_p)))
          (local.set $bufused (i32.load (local.get $bufused_p)))
          (local.set $off (i32.const 0))
          (local.set $found (i32.const 0))
          (block $entry (loop $ent
            ;; dirent header = d_next(8) d_ino(8) d_namlen(4) d_type(4) = 24 bytes, then name.
            (br_if $entry (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $bufused)))
            (local.set $namlen (i32.load (i32.add (local.get $buf) (i32.add (local.get $off) (i32.const 16)))))
            (local.set $nameptr (i32.add (local.get $buf) (i32.add (local.get $off) (i32.const 24))))
            ;; a truncated trailing name (name overflows the buffer) — stop scanning this pass.
            (br_if $entry (i32.gt_u (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen))
                                    (local.get $bufused)))
            (if (i32.eqz (call $is_dot_entry (local.get $nameptr) (local.get $namlen)))
              (then
                ;; child path = pdata + "/" + name.
                (local.set $clen (i32.add (i32.add (local.get $plen) (i32.const 1)) (local.get $namlen)))
                (local.set $child (call $alloc8 (i32.add (local.get $clen) (i32.const 1))))
                (local.set $i (i32.const 0))
                (block $c1d (loop $c1
                  (br_if $c1d (i32.ge_u (local.get $i) (local.get $plen)))
                  (i32.store8 (i32.add (local.get $child) (local.get $i))
                              (i32.load8_u (i32.add (local.get $pdata) (local.get $i))))
                  (local.set $i (i32.add (local.get $i) (i32.const 1)))
                  (br $c1)))
                (i32.store8 (i32.add (local.get $child) (local.get $plen)) (i32.const {ASCII_SLASH}))
                (local.set $i (i32.const 0))
                (block $c2d (loop $c2
                  (br_if $c2d (i32.ge_u (local.get $i) (local.get $namlen)))
                  (i32.store8 (i32.add (local.get $child)
                                       (i32.add (i32.add (local.get $plen) (i32.const 1)) (local.get $i)))
                              (i32.load8_u (i32.add (local.get $nameptr) (local.get $i))))
                  (local.set $i (i32.add (local.get $i) (i32.const 1)))
                  (br $c2)))
                ;; recurse: remove the child. Keep the FIRST non-zero errno.
                (local.set $errno (call $remove_path (local.get $child) (local.get $clen)))
                (if (i32.and (i32.eqz (local.get $rc)) (i32.ne (local.get $errno) (i32.const 0)))
                  (then (local.set $rc (local.get $errno))))
                (local.set $found (i32.const 1))
                (br $entry)))
            (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen)))
            (br $ent)))
          ;; no real entry this pass -> the directory is empty.
          (br_if $emptied (i32.eqz (local.get $found)))
          (br $scan)))
        (drop (call $fd_close (local.get $fd)))
        ;; remove the now-empty directory.
        (local.set $errno (call $path_remove_directory (i32.const 3) (local.get $pdata) (local.get $plen)))
        (if (i32.and (i32.eqz (local.get $rc)) (i32.ne (local.get $errno) (i32.const 0)))
          (then (local.set $rc (local.get $errno))))
        (local.get $rc))))

  ;; fs.remove_all(path) — the WASI recursive-remove floor. $path is a BORROWED canonical String.
  ;; Strips a leading '/' (preopen-relative, same resolution as $write_text_file), recursively
  ;; removes the tree at $path via $remove_path, and builds a fresh OWNED `Result[Unit, String]`:
  ;; Ok(()) (a 1-slot block, len@4=0 + @12=0 + tag@16=0 — the materialize_result_ok convention,
  ;; IDENTICAL to $make_dir's Ok arm, so the scope-end flat $drop_list_str frees nothing) when
  ;; $remove_path returns 0, or Err("remove failed") via $rtf_result on any non-zero errno. A
  ;; recursive remove IS a filesystem write (Capability::FsWrite — the SAME cap as fs.write). The
  ;; result is an owned heap handle the caller's scope-end DropListStr balances.
  (func $remove_all (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $errno i32) (local $obj i32) (local $msg i32)
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    (local.set $errno (call $remove_path (local.get $pdata) (local.get $plen)))
    (if (result i32) (i32.eqz (local.get $errno))
      (then
        ;; Build Ok(()) — len@4=0, @12/@16 zeroed by the i64.store.
        (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
        (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
        (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
        (local.get $obj))
      (else
        (local.set $msg (call $rtf_str (i32.const {REMOVE_ERR_ADDR}) (i32.const {REMOVE_ERR_LEN})))
        (call $rtf_result (local.get $msg) (i32.const 1)))))

  ;; helper: lexicographic LESS-THAN over two canonical String handles $a, $b (byte order =
  ;; UTF-8 code-point order for valid UTF-8 = Rust's `str` Ord). Returns 1 if $a < $b, else 0.
  ;; Compares min(len_a, len_b) bytes; on the first differing byte the smaller byte wins; if one
  ;; is a prefix of the other the shorter is less. Used by $read_dir's insertion sort to match
  ;; native fs.list_dir's `names.sort()`.
  (func $str_lt (param $a i32) (param $b i32) (result i32)
    (local $la i32) (local $lb i32) (local $n i32) (local $i i32) (local $ca i32) (local $cb i32)
    (local.set $la (i32.load (i32.add (local.get $a) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $lb (i32.load (i32.add (local.get $b) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $n (select (local.get $la) (local.get $lb) (i32.le_u (local.get $la) (local.get $lb))))
    (local.set $i (i32.const 0))
    (block $done (loop $cmp
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $ca (i32.load8_u (i32.add (i32.add (local.get $a) (i32.const {LIST_HEADER})) (local.get $i))))
      (local.set $cb (i32.load8_u (i32.add (i32.add (local.get $b) (i32.const {LIST_HEADER})) (local.get $i))))
      (if (i32.lt_u (local.get $ca) (local.get $cb)) (then (return (i32.const 1))))
      (if (i32.gt_u (local.get $ca) (local.get $cb)) (then (return (i32.const 0))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $cmp)))
    ;; common prefix equal — the shorter string is less.
    (i32.lt_u (local.get $la) (local.get $lb)))

  ;; fs.list_dir(path) — the WASI directory-listing floor. $path is a borrowed canonical String.
  ;; Opens the directory (path_open with oflags=O_DIRECTORY(2), rights=fd_readdir(0x4000),
  ;; preopen fd 3, leading '/' stripped — same resolution as $read_text_file), reads its entries
  ;; via fd_readdir into a 4 KiB buffer, parses each dirent (`d_next 8 / d_ino 8 / d_namlen 4 /
  ;; d_type 4` = 24-byte header, then name[d_namlen]) SKIPPING "." and "..", builds an owned
  ;; List[String] of the names, SORTS it lexicographically (insertion sort via $str_lt) to match
  ;; native `names.sort()`, and wraps it Ok via $rtf_result. On a path_open error wraps the
  ;; "directory not found" message Err. The FIFTH sandbox exit (Capability::FsRead) — the result
  ;; is an owned Result[List[String], String] the caller's scope-end DropResultListStr balances.
  (func $read_dir (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $fd_out i32) (local $errno i32)
    (local $fd i32) (local $buf i32) (local $bufbase i32) (local $bufused_p i32) (local $bufused i32)
    (local $off i32) (local $namlen i32) (local $skip i32) (local $count i32)
    (local $list i32) (local $ci i32) (local $name i32) (local $msg i32)
    (local $namebase i32) (local $si i32) (local $sj i32) (local $hi i64) (local $hj i64)
    ;; path bytes + length; strip a leading '/' so the path is relative to preopen fd 3.
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then
        (local.set $pdata (i32.add (local.get $pdata) (i32.const 1)))
        (local.set $plen (i32.sub (local.get $plen) (i32.const 1)))))
    ;; path_open(dirfd=3, dirflags=1, path, plen, oflags=2 [O_DIRECTORY],
    ;;   rights_base = fd_readdir(0x4000), rights_inheriting=0, fdflags=0, fd_out)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    (local.set $errno
      (call $path_open (i32.const 3) (i32.const 1) (local.get $pdata) (local.get $plen)
                       (i32.const 2) (i64.const 16384) (i64.const 16384) (i32.const 0) (local.get $fd_out)))
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        (local.set $msg (call $rtf_str (i32.const {RDIR_ERR_ADDR}) (i32.const {RDIR_ERR_LEN})))
        (call $rtf_result (local.get $msg) (i32.const 1)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; fd_readdir(fd, buf, buf_len, cookie=0, bufused_p) — one pass (4 KiB holds a typical
        ;; directory; a fuller re-read loop is a future refinement). The 4 KiB buffer is a
        ;; RECLAIMABLE $list_new block (512 i64 slots = 4096 data bytes after the header) so a
        ;; list_dir LOOP frees it each call (rc_dec below) instead of leaking immortal $alloc8
        ;; scratch (which OOMs a tight loop). The WASI write target is `$bufbase = buf + HEADER`,
        ;; keeping the rc cell @0 intact for the final $rc_dec. fd_out/bufused_p stay $alloc8
        ;; (4-byte immortal scratch, like read_text_file's out-params — negligible).
        (local.set $buf (call $list_new (i32.const 0) (i32.const 512)))
        (local.set $bufbase (i32.add (local.get $buf) (i32.const {LIST_HEADER})))
        (local.set $bufused_p (call $alloc8 (i32.const 4)))
        (drop (call $fd_readdir (local.get $fd) (local.get $bufbase) (i32.const 4096)
                                (i64.const 0) (local.get $bufused_p)))
        (local.set $bufused (i32.load (local.get $bufused_p)))
        (drop (call $fd_close (local.get $fd)))
        ;; PASS 1 — count entries (skip "." and ".."). 24-byte dirent header; d_namlen @16, name @24.
        (local.set $off (i32.const 0))
        (local.set $count (i32.const 0))
        (block $c1done (loop $c1
          ;; stop when the next header would exceed bufused (a truncated trailing record).
          (br_if $c1done (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $bufused)))
          (local.set $namlen (i32.load (i32.add (i32.add (local.get $bufbase) (local.get $off)) (i32.const 16))))
          (local.set $namebase (i32.add (i32.add (local.get $bufbase) (local.get $off)) (i32.const 24)))
          (local.set $skip (call $is_dot_entry (local.get $namebase) (local.get $namlen)))
          (if (i32.eqz (local.get $skip))
            (then (local.set $count (i32.add (local.get $count) (i32.const 1)))))
          (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen)))
          (br $c1)))
        ;; allocate the List[String] (len = cap = count).
        (local.set $list (call $list_new (local.get $count) (local.get $count)))
        ;; PASS 2 — build each entry String, store into the list (same skip logic).
        (local.set $off (i32.const 0))
        (local.set $ci (i32.const 0))
        (block $c2done (loop $c2
          (br_if $c2done (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $bufused)))
          (local.set $namlen (i32.load (i32.add (i32.add (local.get $bufbase) (local.get $off)) (i32.const 16))))
          (local.set $namebase (i32.add (i32.add (local.get $bufbase) (local.get $off)) (i32.const 24)))
          (if (i32.eqz (call $is_dot_entry (local.get $namebase) (local.get $namlen)))
            (then
              (local.set $name (call $rtf_str (local.get $namebase) (local.get $namlen)))
              (call $list_set (local.get $list) (local.get $ci) (i64.extend_i32_u (local.get $name)))
              (local.set $ci (i32.add (local.get $ci) (i32.const 1)))))
          (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen)))
          (br $c2)))
        ;; free the readdir buffer (all names are now copied into the list) — reclaimable, so a
        ;; list_dir loop reuses it instead of leaking.
        (call $rc_dec (local.get $buf))
        ;; SORT the names lexicographically (insertion sort) — match native names.sort().
        (local.set $si (i32.const 1))
        (block $sdone (loop $sloop
          (br_if $sdone (i32.ge_s (local.get $si) (local.get $count)))
          (local.set $hi (call $list_get (local.get $list) (local.get $si)))
          (local.set $sj (i32.sub (local.get $si) (i32.const 1)))
          (block $shift (loop $sin
            (br_if $shift (i32.lt_s (local.get $sj) (i32.const 0)))
            (local.set $hj (call $list_get (local.get $list) (local.get $sj)))
            ;; while list[sj] > key (i.e. key < list[sj]): shift list[sj] up.
            (br_if $shift (i32.eqz (call $str_lt (i32.wrap_i64 (local.get $hi)) (i32.wrap_i64 (local.get $hj)))))
            (call $list_set (local.get $list) (i32.add (local.get $sj) (i32.const 1)) (local.get $hj))
            (local.set $sj (i32.sub (local.get $sj) (i32.const 1)))
            (br $sin)))
          (call $list_set (local.get $list) (i32.add (local.get $sj) (i32.const 1)) (local.get $hi))
          (local.set $si (i32.add (local.get $si) (i32.const 1)))
          (br $sloop)))
        (call $rtf_result (local.get $list) (i32.const 0)))))

  ;; helper: 1 if the dirent name at $base (length $len) is "." or "..", else 0 (WASI yields
  ;; these; native std::fs::read_dir excludes them — so $read_dir skips them for byte-match).
  (func $is_dot_entry (param $base i32) (param $len i32) (result i32)
    (if (i32.eq (local.get $len) (i32.const 1))
      (then (return (i32.eq (i32.load8_u (local.get $base)) (i32.const 46)))))
    (if (i32.eq (local.get $len) (i32.const 2))
      (then (return (i32.and (i32.eq (i32.load8_u (local.get $base)) (i32.const 46))
                             (i32.eq (i32.load8_u (i32.add (local.get $base) (i32.const 1))) (i32.const 46))))))
    (i32.const 0))

"#
    )
}
