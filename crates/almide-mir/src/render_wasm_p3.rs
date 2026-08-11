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
///
/// Every `(func $…)` below is HAND-WRITTEN wasm — the one part of the wasm leg
/// that is not rendered from the shared MIR, so the 3-way oracle has no native
/// counterpart to differ it against. Each one therefore carries a row in
/// `proofs/wat-prelude-audit.toml` naming the evidence that reaches it, gated by
/// `scripts/check-wat-prelude-audit.sh`. Add a function here ⇒ add its row.
///
/// `$__chk_div`/`$__chk_rem` sat here until that audit: #806 inline-expanded the
/// checked division/remainder at every `/`/`%` site (`render_wasm_calls.rs`) and
/// left the two functions behind with zero callers repo-wide. DCE already dropped
/// their BODIES from every shipped module, so removing them is instruction-identical
/// on all 367 corpus fixtures; the only textual delta is the three dead comment
/// lines above them, which DCE keeps (it removes `(func …)` blocks, and the text
/// between blocks is fixed). What they cost was audit surface: hand-written wasm no
/// evidence could reach. The divisor-0 / MIN÷-1 abort bytes (C-001/C-035) are
/// carried by the inline expansion and its fixtures, not by these two.
pub(crate) fn preamble_with_bump_base(bump_base: u32) -> String {
    // Stage 2 fuel core: present when the program uses fan.bounded OR the
    // probe is on. Counters count DOWN from i64::MAX (consumed = MAX - fuel).
    let fuel_core = if crate::charge_probe::probe_enabled() || crate::charge_probe::budget_used() || crate::charge_probe::timeout_used() {
        "  (global $__fuel (export \"__fuel\") (mut i64) (i64.const 9223372036854775807))\n  (global $__fuel_entry (mut i64) (i64.const 0))\n  (global $__b_verdict (mut i64) (i64.const 0))\n  (global $__b_spend (mut i64) (i64.const 0))\n"
    } else {
        ""
    };
    // Stage 1 probe extras: the order-sensitive trace + the in-guest printer.
    let probe_globals = if crate::charge_probe::probe_enabled() {
        "  (global $__trace (export \"__trace\") (mut i64) (i64.const 0))\n  ;; u64 decimal digits, written BACKWARDS ending at $p (exclusive); returns the start.\n  (func $__probe_digits (param $v i64) (param $p i32) (result i32)\n    (loop $l\n      (local.set $p (i32.sub (local.get $p) (i32.const 1)))\n      (i32.store8 (local.get $p)\n        (i32.add (i32.const 48) (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const 10)))))\n      (local.set $v (i64.div_u (local.get $v) (i64.const 10)))\n      (br_if $l (i64.ne (local.get $v) (i64.const 0))))\n    (local.get $p))\n  ;; `__ALMD_PROBE <fuel> <trace>\\n` on STDERR — the exact native-shim format.\n  ;; The buffer sits on the untouched bump frontier (probe runs at _start exit).\n  (func $__probe_print\n    (local $end i32) (local $cur i32)\n    (local.set $end (i32.add (global.get $bump) (i32.const 128)))\n    (local.set $cur (local.get $end))\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 10))\n    (local.set $cur (call $__probe_digits (global.get $__trace) (local.get $cur)))\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 32))\n    (local.set $cur (call $__probe_digits (i64.sub (i64.const 9223372036854775807) (global.get $__fuel)) (local.get $cur)))\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 32))  ;; ' '\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 69))  ;; 'E'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 66))  ;; 'B'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 79))  ;; 'O'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 82))  ;; 'R'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 80))  ;; 'P'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 95))  ;; '_'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 68))  ;; 'D'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 77))  ;; 'M'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 76))  ;; 'L'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 65))  ;; 'A'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 95))  ;; '_'\n    (local.set $cur (i32.sub (local.get $cur) (i32.const 1)))\n    (i32.store8 (local.get $cur) (i32.const 95))  ;; '_'\n    (i32.store (i32.const 8) (local.get $cur))\n    (i32.store (i32.const 12) (i32.sub (local.get $end) (local.get $cur)))\n    (drop (call $fd_write (i32.const 2) (i32.const 8) (i32.const 1) (i32.const 0))))"
    } else {
        ""
    };
    // T5-1 wall-deadline support (fan.timeout): deadline/hit/verdict/ordinal
    // globals + the $__wall_hit helper each charge site calls. In REPLAY mode
    // (ALMIDE_OMEGA baked at compile time) the clock is never read — the
    // baked ordinal decides the cut, which is what makes an observed omega
    // reproducible on ANY host (T5-2). Clock scratch: the i64 at address 24
    // (the 0..16 iovec area is the printer's).
    let timeout_support = if crate::charge_probe::timeout_used() {
        let omega = crate::charge_probe::omega_replay();
        format!(
            "  (global $__t_deadline (mut i64) (i64.const 9223372036854775807))\n  (global $__t_hit (mut i32) (i32.const 0))\n  (global $__t_verdict (mut i64) (i64.const 0))\n  (global $__t_ord (mut i64) (i64.const 0))\n  (func $__wall_now (result i64)\n    (drop (call $clock_time_get (i32.const 1) (i64.const 1) (i32.const 24)))\n    (i64.load (i32.const 24)))\n  (func $__wall_hit (result i32)\n    (if (i64.eq (global.get $__t_deadline) (i64.const 9223372036854775807)) (then (return (i32.const 0))))\n    (if (i32.ne (global.get $__t_hit) (i32.const 0)) (then (return (i32.const 1))))\n    (global.set $__t_ord (i64.add (global.get $__t_ord) (i64.const 1)))\n    (if (i64.ge_s (i64.const {omega}) (i64.const 0))\n      (then\n        (if (i64.ge_s (global.get $__t_ord) (i64.const {omega}))\n          (then (global.set $__t_hit (i32.const 1))))\n        (return (global.get $__t_hit))))\n    (if (i64.ge_s (call $__wall_now) (global.get $__t_deadline))\n      (then (global.set $__t_hit (i32.const 1))))\n    (global.get $__t_hit))\n"
        )
    } else {
        String::new()
    };
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
  (data (i32.const {OOM_MSG_ADDR}) "Error: out of memory\n")
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
{fuel_core}{timeout_support}{probe_globals}
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
  ;; $oom: the DEFINED out-of-memory abort (C-197). A failed memory.grow means the
  ;; wasm32 linear-memory resource is exhausted; print the named line and exit 1
  ;; instead of letting the caller store past the old end — an OOB fault reads as a
  ;; memory-safety bug, which the trust surface cannot afford (Wave 4 L5). Uses only
  ;; pre-reserved low memory (the iovec scratch + a data segment), safe under OOM.
  (func $oom (call $__div_trap (i32.const {OOM_MSG_ADDR}) (i32.const 21)) (unreachable))
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
    ;; A single request that OVERFLOWS the i32 frontier (p + n ≥ 2^32) can never be
    ;; satisfied on wasm32 — and the wrapped bump would SKIP the grow check below and
    ;; hand out a block whose writes run past the end (Wave 4 L5 layer 2: the ~34 GB
    ;; push probe faulted at exactly the memory boundary). Unsigned wrap test.
    (if (i32.lt_u (global.get $bump) (local.get $p))
      (then (call $oom)))
    ;; GROW the linear memory if the new frontier passed the last allocated page. The wasm memory
    ;; starts at 1 page (64 KiB) with no max; a program that allocates more (a deep recursive
    ;; List-accumulator, a large file read) MUST grow it or the next store traps OOB. `memory.size`
    ;; returns the current page count; grow by exactly enough whole pages to cover `$bump`. This
    ;; touches ONLY the page count — no rc cell, no free-list link, no allocation identity — so the
    ;; FreeList.v / ownership accounting is unchanged (the proof surface is byte addresses below the
    ;; frontier, which growing only extends). `memory.grow` returning -1 (host refused: the wasm32
    ;; linear-memory ceiling) is a DEFINED abort — `$oom` prints "Error: out of memory" and exits 1
    ;; — never a store past the old end (the OOB fault Wave 4 L5 observed; C-197 contracts memory
    ;; exhaustion as a resource limit, and the abort is its honest failure shape).
    (if (i32.gt_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (if (i32.eq (memory.grow
          (i32.add
            (i32.div_u (i32.sub (i32.sub (global.get $bump) (i32.const 1))
                                (i32.mul (memory.size) (i32.const 65536)))
                       (i32.const 65536))
            (i32.const 1))) (i32.const -1))
          (then (call $oom)))))
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
    ;; Same frontier-overflow guard as $alloc (Wave 4 L5 layer 2).
    (if (i32.lt_u (global.get $bump) (local.get $p))
      (then (call $oom)))
    ;; Grow the linear memory past the last page if this (possibly large — a 4 KiB readdir buffer, a
    ;; file-content buffer) scratch alloc crossed it. Same page-count-only grow as `$alloc`, and the
    ;; same C-197 discipline: a refused grow is the defined `$oom` abort, never an OOB store.
    (if (i32.gt_u (global.get $bump) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (if (i32.eq (memory.grow
          (i32.add
            (i32.div_u (i32.sub (i32.sub (global.get $bump) (i32.const 1))
                                (i32.mul (memory.size) (i32.const 65536)))
                       (i32.const 65536))
            (i32.const 1))) (i32.const -1))
          (then (call $oom)))))
    (local.get $p))

  (func $list_new (param $len i32) (param $cap i32) (result i32)
    (local $p i32)
    ;; A cap whose byte size (header + cap*8) would WRAP the i32 multiply is an
    ;; unsatisfiable single block on wasm32 — without this guard the wrapped size
    ;; under-allocates and the element stores run past the block (the mul-wrap
    ;; sibling of the frontier-overflow guard in $alloc; Wave 4 L5 layer 2).
    (if (i32.gt_u (local.get $cap) (i32.const 268435454))
      (then (call $oom)))
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

  ;; MakeUnique's clone: a RAW byte copy of the whole data region (cap*ELEM_SIZE
  ;; bytes). Both COW'able layouts store cap@8 in ELEM_SIZE units — a DynList's
  ;; data is len*8 <= cap*8, a DynStr/Bytes block's is len BYTES <= cap*8 — so the
  ;; raw copy is exact for both. The old per-ELEMENT $list_get/$list_set loop read
  ;; len (BYTES for a Bytes block) as an element count and trapped in $elem_addr
  ;; the moment a shared Bytes was COW'd (`var b = a; bytes.set_at(b, …)`, #794).
  (func $list_copy (param $src i32) (result i32)
    (local $len i32) (local $cap i32) (local $dst i32) (local $i i32) (local $nbytes i32)
    (local.set $len (i32.load (i32.add (local.get $src) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $cap (i32.load (i32.add (local.get $src) (i32.const {LIST_CAP_OFFSET}))))
    (local.set $dst (call $list_new (local.get $len) (local.get $cap)))
    (local.set $nbytes (i32.mul (local.get $cap) (i32.const {ELEM_SIZE})))
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ge_s (local.get $i) (local.get $nbytes)))
      (i32.store8 (i32.add (i32.add (local.get $dst) (i32.const {LIST_HEADER})) (local.get $i))
                  (i32.load8_u (i32.add (i32.add (local.get $src) (i32.const {LIST_HEADER})) (local.get $i))))
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
    ;; SIGN (#1208's execution pin found this missing: -42 printed as the u64
    ;; 18446744073709551574): emit '-' and continue on the wrapped negation —
    ;; for i64::MIN the wrap IS the correct 2^63 magnitude read unsigned, so
    ;; the u64 digit loop below is exact for every negative including MIN.
    (if (i64.lt_s (local.get $v) (i64.const 0))
      (then
        (i32.store8 (local.get $cur) (i32.const {ASCII_MINUS}))
        (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
        (local.set $v (i64.sub (i64.const 0) (local.get $v)))))
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
    (local $str i32)
    ;; Phase 1: argc + total argv buffer size (two i32 out-params from the bump heap).
    ;; The WASI ABI validates POINTER ALIGNMENT on its out-params and the bump
    ;; allocator packs bytes tightly, so round the block up to 4 (over-alloc 3
    ;; slack bytes; both i32 cells live in one aligned 8-byte region).
    (local.set $argc_ptr (i32.and (i32.add (call $alloc (i32.const 11)) (i32.const 3)) (i32.const -4)))
    (local.set $bufsz_ptr (i32.add (local.get $argc_ptr) (i32.const 4)))
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
    ;; argv is an i32 pointer array — args_get validates its alignment too.
    (local.set $argv (i32.and (i32.add (call $alloc (i32.add (i32.mul (local.get $argc) (i32.const 4)) (i32.const 7))) (i32.const 3)) (i32.const -4)))
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
      ;; build the canonical String through the ONE host-floor constructor — an inline
      ;; copy here is what broke the allocator's size invariant in #892 (see `$rtf_str`)
      (local.set $str (call $rtf_str (local.get $cstr) (local.get $slen)))
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
        (local.set $envbuf (call $alloc (i32.add (local.get $bufsz) (i32.const 16))))
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
          ;; Build the canonical value String through the ONE host-floor constructor —
          ;; an inline copy here is what broke the allocator's size invariant in #892.
          (local.set $str (call $rtf_str (local.get $val) (local.get $vlen)))
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

"#
    ) + &preamble_wasi_fs_wat()
}

include!("render_wasm_fs_wat.rs");
