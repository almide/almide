/// The fixed WAT runtime: WASI import, memory, bump allocator, list ops, integer
/// formatting, and line printing. Addresses are the named constants above.
fn preamble() -> String {
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
  (import "wasi_snapshot_preview1" "path_open"
    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_read"
    (func $fd_read (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_close"
    (func $fd_close (param i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_filestat_get"
    (func $fd_filestat_get (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  ;; the fs.read_text path_open error message — a CONST byte run the Err arm copies.
  (data (i32.const {RTF_NOTFOUND_ADDR}) "file not found")
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

  ;; env.args() — build a fresh OWNED `List[String]` of the program arguments
  ;; argv[1..] (SKIP argv[0] = program path, mirroring native `env.args`). The
  ;; WASI floor: `args_sizes_get` gives argc + the flat NUL-terminated argv buffer
  ;; size; `args_get` fills a pointer array + that buffer. We then build the
  ;; canonical `[rc][len][cap][data:i64…]` list of `argc-1` Strings, each a
  ;; canonical `[rc][len][cap][bytes…]` String copied from the argv C-string. The
  ;; result is the third sandbox exit (Capability::CliArgs) — its dst is an owned
  ;; heap handle the caller's scope-end DropListStr balances.
  (func $args_get_list (result i32)
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
    ;; count = max(argc - 1, 0): drop argv[0]. Clamp so a degenerate argc 0 never
    ;; underflows the unsigned loop bound below.
    (local.set $count
      (select (i32.sub (local.get $argc) (i32.const 1)) (i32.const 0)
              (i32.ge_u (local.get $argc) (i32.const 1))))
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
      ;; cstr = argv[$i + 1]
      (local.set $cstr (i32.load (i32.add (local.get $argv)
                                          (i32.mul (i32.add (local.get $i) (i32.const 1)) (i32.const 4)))))
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
    (local $j i32) (local $msg i32)
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
    ;; On a path_open error build Err("file not found").
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        (local.set $msg (call $rtf_str (i32.const {RTF_NOTFOUND_ADDR}) (i32.const {RTF_NOTFOUND_LEN})))
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

"#
    )
}
