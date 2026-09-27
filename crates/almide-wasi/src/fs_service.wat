;; The p1 fs SERVICE (#2742): the almide `fs_call` fs ops over WASI preview 1,
;; spliced into a stock-WASI artifact by `to_wasi` when the emitted op set
;; reaches an fs op. The embedded host (crates/almide-wasm-run/src/host.rs,
;; `fs_dispatch`) is the reference: every op here answers what that host
;; answers, and that host answers what native `std::fs` answers.
;;
;; Contract with the splice (fs_service.rs):
;;   - the `wasi_snapshot_preview1` imports map onto the artifact's own WASI
;;     imports (shared with the base five where the name matches);
;;   - `env.memory` is the artifact's memory 0;
;;   - `shim.plen` / `shim.ppos` are the host_read staging globals, `shim.heap`
;;     is the guest's bump head (`__heap`): everything from it to the end of
;;     memory is free while a host call runs, so it is this service's arena;
;;   - `@FSP@` is the absolute address of the service's own page (64 KiB past
;;     the park), and the generated tail supplies `$fs` (the dispatcher),
;;     `$op_name`, `$errno_text` and the `$s_*` statics.
;;
;; The fs page: 0 fd_out | 8 nread / bufused | 16 iovec | 24 prestat |
;; 32 environ sizes | 40 clock | 64 filestat (64 B) | 256 preopen table
;; (32 x 12 B) | 1024 statics | 8192 cached cwd (8 KiB) | 16384 preopen names.
;;
;; Results stage through `$stage`, which keeps them clear of the one guest
;; allocation that precedes `host_read` (the `stage_for` rule in
;; wasi_shims.rs). Error texts are `almide_base::fs_errno`'s rows; an errno the
;; table does not spell answers `filesystem operation failed (wasi errno N)`.
(module
  (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_read" (func $fd_read (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_close" (func $fd_close (param i32) (result i32)))
  (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
  (import "wasi_snapshot_preview1" "clock_time_get" (func $clock_time_get (param i32 i64 i32) (result i32)))
  (import "wasi_snapshot_preview1" "environ_sizes_get" (func $environ_sizes_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "environ_get" (func $environ_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_prestat_get" (func $fd_prestat_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_prestat_dir_name" (func $fd_prestat_dir_name (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_open" (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_filestat_get" (func $fd_filestat_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_filestat_get" (func $path_filestat_get (param i32 i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_readdir" (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_create_directory" (func $path_create_directory (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_remove_directory" (func $path_remove_directory (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_unlink_file" (func $path_unlink_file (param i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "path_rename" (func $path_rename (param i32 i32 i32 i32 i32 i32) (result i32)))
  (import "env" "memory" (memory 1))
  (import "shim" "plen" (global $plen (mut i32)))
  (import "shim" "ppos" (global $ppos (mut i32)))
  (import "shim" "heap" (global $heap (mut i32)))

  (global $fsp i32 (i32.const @FSP@))
  ;; the per-call arena head
  (global $ap (mut i32) (i32.const 0))
  ;; the preopen table's row count, -1 before the first scan
  (global $pre_n (mut i32) (i32.const -1))
  ;; the cwd cache: 0 unread, 1 cached at fsp+8192, 2 none
  (global $cwd_state (mut i32) (i32.const 0))
  (global $cwd_len (mut i32) (i32.const 0))
  ;; the per-call result list: nodes [next u32][len u32][bytes]
  (global $rh (mut i32) (i32.const 0))
  (global $rt (mut i32) (i32.const 0))
  (global $rc (mut i32) (i32.const 0))
  ;; the directory a walk failed on
  (global $ep (mut i32) (i32.const 0))
  (global $el (mut i32) (i32.const 0))
  ;; the glob pattern segments (array of [ptr u32][len u32]) and their count
  (global $gp (mut i32) (i32.const 0))
  (global $gn (mut i32) (i32.const 0))

  ;; ── primitives ─────────────────────────────────────────────────────────

  (func $prologue
    (global.set $ap (i32.and (i32.add (global.get $heap) (i32.const 7)) (i32.const -8)))
    (global.set $rh (i32.const 0))
    (global.set $rt (i32.const 0))
    (global.set $rc (i32.const 0)))

  (func $pack (param $s i32) (param $l i32) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (local.get $s)) (i64.const 32))
            (i64.extend_i32_u (local.get $l))))

  ;; C-197's line and exit 1: the arena could not grow.
  (func $oom
    (local $p i32) (local $l i32)
    (call $s_oom) (local.set $l) (local.set $p)
    (i32.store offset=16 (global.get $fsp) (local.get $p))
    (i32.store offset=20 (global.get $fsp) (local.get $l))
    (drop (call $fd_write (i32.const 2) (i32.add (global.get $fsp) (i32.const 16)) (i32.const 1)
                          (i32.add (global.get $fsp) (i32.const 8))))
    (call $proc_exit (i32.const 1))
    (unreachable))

  ;; 8-aligned bump allocation in the arena, growing memory on demand.
  (func $alloc (param $n i32) (result i32)
    (local $p i32) (local $end i64) (local $have i64)
    (local.set $p (global.get $ap))
    (local.set $end (i64.and (i64.add (i64.add (i64.extend_i32_u (local.get $p))
                                               (i64.extend_i32_u (local.get $n)))
                                      (i64.const 7))
                             (i64.const -8)))
    (local.set $have (i64.shl (i64.extend_i32_u (memory.size)) (i64.const 16)))
    (if (i64.gt_u (local.get $end) (local.get $have))
      (then
        (if (i64.ge_u (local.get $end) (i64.const 0x100000000)) (then (call $oom)))
        (if (i32.lt_s (memory.grow (i32.wrap_i64 (i64.shr_u (i64.add (i64.sub (local.get $end) (local.get $have))
                                                                      (i64.const 65535))
                                                             (i64.const 16))))
                      (i32.const 0))
          (then (call $oom)))))
    (global.set $ap (i32.wrap_i64 (local.get $end)))
    (local.get $p))

  ;; Publish (p, l) for host_read. The guest allocates the destination block
  ;; at its bump head BEFORE it calls host_read, so bytes that sit within
  ;; 2*l + 64 of that head move above it first (memory.copy is a memmove).
  (func $stage (param $p i32) (param $l i32)
    (local $min i64) (local $q i32)
    (local.set $min (i64.add (i64.add (i64.extend_i32_u (global.get $heap))
                                      (i64.shl (i64.extend_i32_u (local.get $l)) (i64.const 1)))
                             (i64.const 64)))
    (if (i64.lt_u (i64.extend_i32_u (local.get $p)) (local.get $min))
      (then
        (if (i64.ge_u (local.get $min) (i64.const 0x100000000)) (then (call $oom)))
        (if (i64.gt_u (local.get $min) (i64.extend_i32_u (global.get $ap)))
          (then (global.set $ap (i32.wrap_i64 (local.get $min)))))
        (local.set $q (call $alloc (local.get $l)))
        (memory.copy (local.get $q) (local.get $p) (local.get $l))
        (local.set $p (local.get $q))))
    (global.set $ppos (local.get $p))
    (global.set $plen (local.get $l)))

  (func $ok (param $p i32) (param $l i32) (result i64)
    (call $stage (local.get $p) (local.get $l))
    (call $pack (i32.const 0) (local.get $l)))

  (func $bool (param $b i32) (result i64)
    (call $pack (i32.const 0) (i32.ne (local.get $b) (i32.const 0))))

  (func $none (result i64)
    (call $pack (i32.const 2) (i32.const 0)))

  (func $unit (result i64)
    (call $pack (i32.const 0) (i32.const 0)))

  (func $put (param $w i32) (param $p i32) (param $l i32) (result i32)
    (memory.copy (local.get $w) (local.get $p) (local.get $l))
    (i32.add (local.get $w) (local.get $l)))

  ;; One [len u32][bytes] frame at w.
  (func $frame (param $w i32) (param $p i32) (param $l i32) (result i32)
    (i32.store (local.get $w) (local.get $l))
    (call $put (i32.add (local.get $w) (i32.const 4)) (local.get $p) (local.get $l)))

  (func $memeq (param $a i32) (param $b i32) (param $n i32) (result i32)
    (local $i i32)
    (block $no (loop $l
      (if (i32.ge_u (local.get $i) (local.get $n)) (then (return (i32.const 1))))
      (br_if $no (i32.ne (i32.load8_u (i32.add (local.get $a) (local.get $i)))
                         (i32.load8_u (i32.add (local.get $b) (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i32.const 0))

  (func $streq (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i32)
    (if (i32.ne (local.get $al) (local.get $bl)) (then (return (i32.const 0))))
    (call $memeq (local.get $a) (local.get $b) (local.get $al)))

  ;; Byte-lexicographic a < b (native `str` Ord).
  (func $lt (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i32)
    (local $i i32) (local $n i32) (local $x i32) (local $y i32)
    (local.set $n (select (local.get $al) (local.get $bl) (i32.lt_u (local.get $al) (local.get $bl))))
    (block $d (loop $l
      (br_if $d (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $x (i32.load8_u (i32.add (local.get $a) (local.get $i))))
      (local.set $y (i32.load8_u (i32.add (local.get $b) (local.get $i))))
      (if (i32.ne (local.get $x) (local.get $y)) (then (return (i32.lt_u (local.get $x) (local.get $y)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i32.lt_u (local.get $al) (local.get $bl)))

  ;; Decimal text of an unsigned 64-bit value.
  (func $utoa (param $v i64) (result i32 i32)
    (local $buf i32) (local $i i32)
    (local.set $buf (call $alloc (i32.const 24)))
    (local.set $i (i32.const 24))
    (loop $d
      (local.set $i (i32.sub (local.get $i) (i32.const 1)))
      (i32.store8 (i32.add (local.get $buf) (local.get $i))
                  (i32.add (i32.const 48) (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const 10)))))
      (local.set $v (i64.div_u (local.get $v) (i64.const 10)))
      (br_if $d (i64.ne (local.get $v) (i64.const 0))))
    (i32.add (local.get $buf) (local.get $i))
    (i32.sub (i32.const 24) (local.get $i)))

  ;; Path::join for a relative child: `d` + '/' + `n`, no doubled separator,
  ;; and an empty `d` joins to `n` alone.
  (func $join (param $d i32) (param $dl i32) (param $n i32) (param $nl i32) (result i32 i32)
    (local $buf i32) (local $w i32)
    (local.set $buf (call $alloc (i32.add (i32.add (local.get $dl) (local.get $nl)) (i32.const 1))))
    (local.set $w (call $put (local.get $buf) (local.get $d) (local.get $dl)))
    (if (i32.gt_u (local.get $dl) (i32.const 0))
      (then
        (if (i32.ne (i32.load8_u (i32.sub (local.get $w) (i32.const 1))) (i32.const 47))
          (then
            (i32.store8 (local.get $w) (i32.const 47))
            (local.set $w (i32.add (local.get $w) (i32.const 1)))))))
    (local.set $w (call $put (local.get $w) (local.get $n) (local.get $nl)))
    (local.get $buf)
    (i32.sub (local.get $w) (local.get $buf)))

  ;; ── errors: `<call>("<a>"[, "<b>"]): <text>` (the host's io_err) ────────

  (func $fail (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32)
              (param $two i32) (param $tp i32) (param $tl i32) (result i64)
    (local $np i32) (local $nl i32) (local $m i32) (local $w i32)
    (call $op_name (local.get $op)) (local.set $nl) (local.set $np)
    (local.set $m (call $alloc (i32.add (i32.add (i32.add (local.get $nl) (local.get $al))
                                                 (i32.add (local.get $bl) (local.get $tl)))
                                        (i32.const 10))))
    (local.set $w (call $put (local.get $m) (local.get $np) (local.get $nl)))
    (i32.store8 (local.get $w) (i32.const 40))
    (i32.store8 offset=1 (local.get $w) (i32.const 34))
    (local.set $w (call $put (i32.add (local.get $w) (i32.const 2)) (local.get $a) (local.get $al)))
    (i32.store8 (local.get $w) (i32.const 34))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (if (local.get $two)
      (then
        (i32.store8 (local.get $w) (i32.const 44))
        (i32.store8 offset=1 (local.get $w) (i32.const 32))
        (i32.store8 offset=2 (local.get $w) (i32.const 34))
        (local.set $w (call $put (i32.add (local.get $w) (i32.const 3)) (local.get $b) (local.get $bl)))
        (i32.store8 (local.get $w) (i32.const 34))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))))
    (i32.store8 (local.get $w) (i32.const 41))
    (i32.store8 offset=1 (local.get $w) (i32.const 58))
    (i32.store8 offset=2 (local.get $w) (i32.const 32))
    (local.set $w (call $put (i32.add (local.get $w) (i32.const 3)) (local.get $tp) (local.get $tl)))
    (call $stage (local.get $m) (i32.sub (local.get $w) (local.get $m)))
    (call $pack (i32.const 1) (i32.sub (local.get $w) (local.get $m))))

  ;; The errno's native Display text, or the bounded fallback.
  (func $errtext (param $e i32) (result i32 i32)
    (local $p i32) (local $l i32) (local $np i32) (local $nl i32) (local $m i32) (local $w i32)
    (call $errno_text (local.get $e)) (local.set $l) (local.set $p)
    (if (local.get $l) (then (return (local.get $p) (local.get $l))))
    (call $s_errno_fallback) (local.set $l) (local.set $p)
    (call $utoa (i64.extend_i32_u (local.get $e))) (local.set $nl) (local.set $np)
    (local.set $m (call $alloc (i32.add (i32.add (local.get $l) (local.get $nl)) (i32.const 1))))
    (local.set $w (call $put (local.get $m) (local.get $p) (local.get $l)))
    (local.set $w (call $put (local.get $w) (local.get $np) (local.get $nl)))
    (i32.store8 (local.get $w) (i32.const 41))
    (local.get $m)
    (i32.sub (i32.add (local.get $w) (i32.const 1)) (local.get $m)))

  (func $fail_e (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32)
                (param $two i32) (param $e i32) (result i64)
    (local $tp i32) (local $tl i32)
    (call $errtext (local.get $e)) (local.set $tl) (local.set $tp)
    (call $fail (local.get $op) (local.get $a) (local.get $al) (local.get $b) (local.get $bl)
                (local.get $two) (local.get $tp) (local.get $tl)))

  ;; ── environment: cwd (C-137) and the temp root (C-189) ──────────────────

  ;; The value of environ entry `key`, or len -1.
  (func $env_get (param $kp i32) (param $kl i32) (result i32 i32)
    (local $cnt i32) (local $arr i32) (local $buf i32) (local $i i32) (local $s i32) (local $n i32)
    (if (call $environ_sizes_get (i32.add (global.get $fsp) (i32.const 32))
                                 (i32.add (global.get $fsp) (i32.const 36)))
      (then (return (i32.const 0) (i32.const -1))))
    (local.set $cnt (i32.load offset=32 (global.get $fsp)))
    (local.set $arr (call $alloc (i32.shl (local.get $cnt) (i32.const 2))))
    (local.set $buf (call $alloc (i32.load offset=36 (global.get $fsp))))
    (if (call $environ_get (local.get $arr) (local.get $buf))
      (then (return (i32.const 0) (i32.const -1))))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $s (i32.load (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 2)))))
      (if (call $memeq (local.get $s) (local.get $kp) (local.get $kl))
        (then
          (if (i32.eq (i32.load8_u (i32.add (local.get $s) (local.get $kl))) (i32.const 61))
            (then
              (local.set $s (i32.add (i32.add (local.get $s) (local.get $kl)) (i32.const 1)))
              (block $e (loop $c
                (br_if $e (i32.eqz (i32.load8_u (i32.add (local.get $s) (local.get $n)))))
                (local.set $n (i32.add (local.get $n) (i32.const 1)))
                (br $c)))
              (return (local.get $s) (local.get $n))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $next)))
    (i32.const 0) (i32.const -1))

  ;; The launcher's cwd: ALMIDE_CWD, else PWD (#874), cached for the process;
  ;; len 0 when neither names one.
  (func $cwd (result i32 i32)
    (local $p i32) (local $l i32)
    (if (i32.eqz (global.get $cwd_state))
      (then
        (global.set $cwd_state (i32.const 2))
        (call $s_almide_cwd) (call $env_get) (local.set $l) (local.set $p)
        (if (i32.lt_s (local.get $l) (i32.const 0))
          (then (call $s_pwd) (call $env_get) (local.set $l) (local.set $p)))
        (if (i32.and (i32.gt_s (local.get $l) (i32.const 0)) (i32.le_s (local.get $l) (i32.const 8192)))
          (then
            (memory.copy (i32.add (global.get $fsp) (i32.const 8192)) (local.get $p) (local.get $l))
            (global.set $cwd_len (local.get $l))
            (global.set $cwd_state (i32.const 1))))))
    (if (i32.eq (global.get $cwd_state) (i32.const 1))
      (then (return (i32.add (global.get $fsp) (i32.const 8192)) (global.get $cwd_len))))
    (i32.const 0) (i32.const 0))

  ;; `$TMPDIR`, else "/tmp" (the self-hosted env.temp_dir's answer).
  (func $tmp (result i32 i32)
    (local $p i32) (local $l i32)
    (call $s_tmpdir) (call $env_get) (local.set $l) (local.set $p)
    (if (i32.ge_s (local.get $l) (i32.const 0)) (then (return (local.get $p) (local.get $l))))
    (call $s_slash_tmp))

  ;; ── path resolution (C-042, C-137) ────────────────────────────────────

  (func $preopens
    (local $fd i32) (local $n i32) (local $nl i32) (local $raw i32) (local $np i32) (local $rec i32) (local $c i32)
    (if (i32.ge_s (global.get $pre_n) (i32.const 0)) (then (return)))
    (local.set $fd (i32.const 3))
    (local.set $np (i32.add (global.get $fsp) (i32.const 16384)))
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $n) (i32.const 32)))
      ;; preopens are contiguous from fd 3: the first failure ends the table
      (br_if $done (call $fd_prestat_get (local.get $fd) (i32.add (global.get $fsp) (i32.const 24))))
      (if (i32.eqz (i32.load8_u offset=24 (global.get $fsp)))
        (then
          (local.set $raw (i32.load offset=28 (global.get $fsp)))
          (local.set $nl (local.get $raw))
          (if (i32.le_u (i32.add (i32.sub (local.get $np) (global.get $fsp)) (local.get $raw)) (i32.const 65536))
            (then
              (if (i32.eqz (call $fd_prestat_dir_name (local.get $fd) (local.get $np) (local.get $raw)))
                (then
                  ;; "/tmp" and "/tmp/" name the same preopen; "/" keeps its byte
                  (block $td (loop $tl
                    (br_if $td (i32.le_u (local.get $nl) (i32.const 1)))
                    (local.set $c (i32.load8_u (i32.sub (i32.add (local.get $np) (local.get $nl)) (i32.const 1))))
                    (br_if $td (i32.and (i32.ne (local.get $c) (i32.const 47)) (i32.ne (local.get $c) (i32.const 0))))
                    (local.set $nl (i32.sub (local.get $nl) (i32.const 1)))
                    (br $tl)))
                  (local.set $rec (i32.add (i32.add (global.get $fsp) (i32.const 256))
                                           (i32.mul (local.get $n) (i32.const 12))))
                  (i32.store (local.get $rec) (local.get $fd))
                  (i32.store offset=4 (local.get $rec) (local.get $np))
                  (i32.store offset=8 (local.get $rec) (local.get $nl))
                  (local.set $np (i32.add (local.get $np) (local.get $raw)))
                  (local.set $n (i32.add (local.get $n) (i32.const 1)))))))))
      (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
      (br $next)))
    (global.set $pre_n (local.get $n)))

  ;; A guest path -> (dirfd, relative remainder). A relative path joins onto
  ;; the cwd first; the longest preopen prefix at a '/' boundary wins; an
  ;; unclaimed path takes fd 3 with one leading '/' stripped (the incumbent's
  ;; `$path_norm` rule, render_wasm_fs_preopen.rs). An empty remainder is ".".
  (func $resolve (param $p i32) (param $l i32) (result i32 i32 i32)
    (local $cp i32) (local $cl i32) (local $i i32) (local $rec i32) (local $nfd i32) (local $np i32)
    (local $nl i32) (local $ok i32) (local $rem i32) (local $reml i32)
    (local $bfd i32) (local $blen i32) (local $brem i32) (local $breml i32)
    (if (i32.eqz (i32.and (i32.gt_u (local.get $l) (i32.const 0))
                          (i32.eq (i32.load8_u (local.get $p)) (i32.const 47))))
      (then
        (call $cwd) (local.set $cl) (local.set $cp)
        (if (i32.and (i32.gt_u (local.get $cl) (i32.const 0))
                     (i32.eq (i32.load8_u (local.get $cp)) (i32.const 47)))
          (then (call $join (local.get $cp) (local.get $cl) (local.get $p) (local.get $l))
                (local.set $l) (local.set $p)))))
    (call $preopens)
    (local.set $bfd (i32.const -1))
    (block $mdone (loop $m
      (br_if $mdone (i32.ge_u (local.get $i) (global.get $pre_n)))
      (local.set $rec (i32.add (i32.add (global.get $fsp) (i32.const 256)) (i32.mul (local.get $i) (i32.const 12))))
      (local.set $nfd (i32.load (local.get $rec)))
      (local.set $np (i32.load offset=4 (local.get $rec)))
      (local.set $nl (i32.load offset=8 (local.get $rec)))
      (local.set $ok (i32.const 0))
      (if (i32.and (i32.eq (local.get $nl) (i32.const 1)) (i32.eq (i32.load8_u (local.get $np)) (i32.const 47)))
        (then
          ;; the root preopen claims every absolute path
          (if (i32.and (i32.gt_u (local.get $l) (i32.const 0)) (i32.eq (i32.load8_u (local.get $p)) (i32.const 47)))
            (then
              (local.set $ok (i32.const 1))
              (local.set $rem (i32.add (local.get $p) (i32.const 1)))
              (local.set $reml (i32.sub (local.get $l) (i32.const 1))))))
        (else
          (if (i32.and (i32.eq (local.get $nl) (i32.const 1)) (i32.eq (i32.load8_u (local.get $np)) (i32.const 46)))
            (then
              ;; "." claims a path that stayed relative
              (if (i32.or (i32.eqz (local.get $l)) (i32.ne (i32.load8_u (local.get $p)) (i32.const 47)))
                (then
                  (local.set $ok (i32.const 1))
                  (local.set $rem (local.get $p))
                  (local.set $reml (local.get $l)))))
            (else
              ;; a named preopen matches only at a '/' component boundary
              (if (i32.gt_u (local.get $l) (local.get $nl))
                (then
                  (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $nl))) (i32.const 47))
                    (then
                      (if (call $memeq (local.get $p) (local.get $np) (local.get $nl))
                        (then
                          (local.set $ok (i32.const 1))
                          (local.set $rem (i32.add (i32.add (local.get $p) (local.get $nl)) (i32.const 1)))
                          (local.set $reml (i32.sub (i32.sub (local.get $l) (local.get $nl)) (i32.const 1)))))))))))))
      (if (i32.and (local.get $ok)
                   (i32.or (i32.lt_s (local.get $bfd) (i32.const 0)) (i32.gt_u (local.get $nl) (local.get $blen))))
        (then
          (local.set $bfd (local.get $nfd))
          (local.set $blen (local.get $nl))
          (local.set $brem (local.get $rem))
          (local.set $breml (local.get $reml))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $m)))
    (if (i32.lt_s (local.get $bfd) (i32.const 0))
      (then
        (local.set $bfd (i32.const 3))
        (local.set $brem (local.get $p))
        (local.set $breml (local.get $l))
        (if (i32.and (i32.gt_u (local.get $l) (i32.const 0)) (i32.eq (i32.load8_u (local.get $p)) (i32.const 47)))
          (then
            (local.set $brem (i32.add (local.get $p) (i32.const 1)))
            (local.set $breml (i32.sub (local.get $l) (i32.const 1)))))))
    (if (i32.eqz (local.get $breml)) (then (call $s_dot) (local.set $breml) (local.set $brem)))
    (local.get $bfd) (local.get $brem) (local.get $breml))

  ;; path_filestat_get into fsp+64; the errno.
  (func $stat_at (param $fd i32) (param $p i32) (param $l i32) (param $follow i32) (result i32)
    (call $path_filestat_get (local.get $fd) (local.get $follow) (local.get $p) (local.get $l)
                             (i32.add (global.get $fsp) (i32.const 64))))

  (func $stat (param $p i32) (param $l i32) (param $follow i32) (result i32)
    (local $fd i32) (local $rp i32) (local $rl i32)
    (call $resolve (local.get $p) (local.get $l)) (local.set $rl) (local.set $rp) (local.set $fd)
    (call $stat_at (local.get $fd) (local.get $rp) (local.get $rl) (local.get $follow)))

  ;; the filetype byte of the last stat (3 = directory, 4 = regular, 7 = symlink)
  (func $ftype (result i32)
    (i32.load8_u offset=80 (global.get $fsp)))

  (func $isdir_at (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (if (call $stat_at (local.get $fd) (local.get $p) (local.get $l) (i32.const 1)) (then (return (i32.const 0))))
    (i32.eq (call $ftype) (i32.const 3)))

  (func $is_dir_path (param $p i32) (param $l i32) (result i32)
    (if (call $stat (local.get $p) (local.get $l) (i32.const 1)) (then (return (i32.const 0))))
    (i32.eq (call $ftype) (i32.const 3)))

  ;; ── reading ────────────────────────────────────────────────────────────

  ;; The whole file at a guest path: (bytes, len, errno). A directory is
  ;; EISDIR (31), classified from the stat, not from fd_read's host-specific
  ;; errno (the incumbent's #1368 rule).
  (func $read_all (param $p i32) (param $l i32) (result i32 i32 i32)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $e i32) (local $h i32)
    (local $sz i32) (local $buf i32) (local $got i32) (local $n i32)
    (call $resolve (local.get $p) (local.get $l)) (local.set $rl) (local.set $rp) (local.set $fd)
    (local.set $e (call $path_open (local.get $fd) (i32.const 1) (local.get $rp) (local.get $rl)
                                   (i32.const 0) (i64.const 6) (i64.const 0) (i32.const 0) (global.get $fsp)))
    (if (local.get $e) (then (return (i32.const 0) (i32.const 0) (local.get $e))))
    (local.set $h (i32.load (global.get $fsp)))
    (local.set $e (call $fd_filestat_get (local.get $h) (i32.add (global.get $fsp) (i32.const 64))))
    (if (i32.eqz (local.get $e))
      (then
        (if (i32.eq (call $ftype) (i32.const 3))
          (then (local.set $e (i32.const 31)))
          (else
            (local.set $sz (i32.wrap_i64 (i64.load offset=96 (global.get $fsp))))
            (local.set $buf (call $alloc (i32.add (local.get $sz) (i32.const 1))))
            (block $done (loop $rd
              (br_if $done (i32.ge_u (local.get $got) (local.get $sz)))
              (i32.store offset=16 (global.get $fsp) (i32.add (local.get $buf) (local.get $got)))
              (i32.store offset=20 (global.get $fsp) (i32.sub (local.get $sz) (local.get $got)))
              (local.set $e (call $fd_read (local.get $h) (i32.add (global.get $fsp) (i32.const 16)) (i32.const 1)
                                           (i32.add (global.get $fsp) (i32.const 8))))
              (br_if $done (local.get $e))
              (local.set $n (i32.load offset=8 (global.get $fsp)))
              (br_if $done (i32.eqz (local.get $n)))
              (local.set $got (i32.add (local.get $got) (local.get $n)))
              (br $rd)))))))
    (drop (call $fd_close (local.get $h)))
    (local.get $buf) (local.get $got) (local.get $e))

  ;; Rust's `str::from_utf8` acceptance (the incumbent's #1506 loop).
  (func $utf8_ok (param $p i32) (param $n i32) (result i32)
    (local $i i32) (local $b0 i32) (local $b1 i32) (local $w i32) (local $lo i32) (local $hi i32) (local $k i32)
    (block $done (loop $next
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (local.set $b0 (i32.load8_u (i32.add (local.get $p) (local.get $i))))
      (if (i32.lt_u (local.get $b0) (i32.const 128))
        (then (local.set $i (i32.add (local.get $i) (i32.const 1))) (br $next)))
      (if (i32.or (i32.lt_u (local.get $b0) (i32.const 194)) (i32.gt_u (local.get $b0) (i32.const 244)))
        (then (return (i32.const 0))))
      (local.set $w (select (i32.const 2)
                            (select (i32.const 3) (i32.const 4) (i32.lt_u (local.get $b0) (i32.const 240)))
                            (i32.lt_u (local.get $b0) (i32.const 224))))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $w)) (local.get $n)) (then (return (i32.const 0))))
      (local.set $lo (i32.const 128))
      (local.set $hi (i32.const 191))
      (if (i32.eq (local.get $b0) (i32.const 224)) (then (local.set $lo (i32.const 160))))
      (if (i32.eq (local.get $b0) (i32.const 237)) (then (local.set $hi (i32.const 159))))
      (if (i32.eq (local.get $b0) (i32.const 240)) (then (local.set $lo (i32.const 144))))
      (if (i32.eq (local.get $b0) (i32.const 244)) (then (local.set $hi (i32.const 143))))
      (local.set $b1 (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 1)))))
      (if (i32.or (i32.lt_u (local.get $b1) (local.get $lo)) (i32.gt_u (local.get $b1) (local.get $hi)))
        (then (return (i32.const 0))))
      (local.set $k (i32.const 2))
      (block $cdone (loop $cnext
        (br_if $cdone (i32.ge_u (local.get $k) (local.get $w)))
        (if (i32.ne (i32.and (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (local.get $k))))
                             (i32.const 192))
                    (i32.const 128))
          (then (return (i32.const 0))))
        (local.set $k (i32.add (local.get $k) (i32.const 1)))
        (br $cnext)))
      (local.set $i (i32.add (local.get $i) (local.get $w)))
      (br $next)))
    (i32.const 1))

  ;; `str::lines` as frames: split on '\n', a '\r' before the '\n' dropped,
  ;; no phantom line after a trailing newline (C-225).
  (func $lines (param $p i32) (param $l i32) (result i32 i32)
    (local $i i32) (local $s i32) (local $cnt i32) (local $out i32) (local $w i32) (local $e i32)
    (block $d (loop $c
      (br_if $d (i32.ge_u (local.get $i) (local.get $l)))
      (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 10))
        (then (local.set $cnt (i32.add (local.get $cnt) (i32.const 1)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (local.set $out (call $alloc (i32.add (local.get $l) (i32.shl (i32.add (local.get $cnt) (i32.const 1)) (i32.const 2)))))
    (local.set $w (local.get $out))
    (local.set $i (i32.const 0))
    (block $d2 (loop $c2
      (br_if $d2 (i32.ge_u (local.get $i) (local.get $l)))
      (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 10))
        (then
          (local.set $e (local.get $i))
          (if (i32.gt_u (local.get $e) (local.get $s))
            (then
              (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $e) (i32.const 1)))) (i32.const 13))
                (then (local.set $e (i32.sub (local.get $e) (i32.const 1)))))))
          (local.set $w (call $frame (local.get $w) (i32.add (local.get $p) (local.get $s))
                                     (i32.sub (local.get $e) (local.get $s))))
          (local.set $s (i32.add (local.get $i) (i32.const 1)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c2)))
    (if (i32.lt_u (local.get $s) (local.get $l))
      (then (local.set $w (call $frame (local.get $w) (i32.add (local.get $p) (local.get $s))
                                       (i32.sub (local.get $l) (local.get $s))))))
    (local.get $out)
    (i32.sub (local.get $w) (local.get $out)))

  ;; The text readers: `mode` 0 = the text, 1 = its lines; `absent_none`
  ;; answers ok-none on ENOENT (the *_if_exists twins, C-215).
  (func $read_text_as (param $op i32) (param $a i32) (param $al i32) (param $mode i32) (param $absent_none i32) (result i64)
    (local $p i32) (local $l i32) (local $e i32) (local $tp i32) (local $tl i32)
    (call $read_all (local.get $a) (local.get $al)) (local.set $e) (local.set $l) (local.set $p)
    (if (local.get $e)
      (then
        (if (i32.and (local.get $absent_none) (i32.eq (local.get $e) (i32.const 44))) (then (return (call $none))))
        (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                              (i32.const 0) (local.get $e)))))
    (if (i32.eqz (call $utf8_ok (local.get $p) (local.get $l)))
      (then
        (call $s_utf8) (local.set $tl) (local.set $tp)
        (return (call $fail (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                            (i32.const 0) (local.get $tp) (local.get $tl)))))
    (if (local.get $mode) (then (call $lines (local.get $p) (local.get $l)) (local.set $l) (local.set $p)))
    (call $ok (local.get $p) (local.get $l)))

  (func $op_read_text (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $read_text_as (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)))

  (func $op_read_lines (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $read_text_as (local.get $op) (local.get $a) (local.get $al) (i32.const 1) (i32.const 0)))

  (func $op_read_lines_if_exists (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $read_text_as (local.get $op) (local.get $a) (local.get $al) (i32.const 1) (i32.const 1)))

  ;; read_text_if_exists asks Path::exists first (a path whose parent is a
  ;; file is absent too), then reads.
  (func $op_read_text_if_exists (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (if (call $stat (local.get $a) (local.get $al) (i32.const 1)) (then (return (call $none))))
    (call $read_text_as (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)))

  (func $read_bytes_as (param $op i32) (param $a i32) (param $al i32) (param $absent_none i32) (result i64)
    (local $p i32) (local $l i32) (local $e i32)
    (call $read_all (local.get $a) (local.get $al)) (local.set $e) (local.set $l) (local.set $p)
    (if (local.get $e)
      (then
        (if (i32.and (local.get $absent_none) (i32.eq (local.get $e) (i32.const 44))) (then (return (call $none))))
        (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                              (i32.const 0) (local.get $e)))))
    (call $ok (local.get $p) (local.get $l)))

  (func $op_read_bytes (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $read_bytes_as (local.get $op) (local.get $a) (local.get $al) (i32.const 0)))

  (func $op_read_bytes_if_exists (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $read_bytes_as (local.get $op) (local.get $a) (local.get $al) (i32.const 1)))

  ;; ── metadata ───────────────────────────────────────────────────────────

  (func $op_exists (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $bool (i32.eqz (call $stat (local.get $a) (local.get $al) (i32.const 1)))))

  (func $op_is_dir (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $bool (call $is_dir_path (local.get $a) (local.get $al))))

  (func $op_is_file (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (if (call $stat (local.get $a) (local.get $al) (i32.const 1)) (then (return (call $bool (i32.const 0)))))
    (call $bool (i32.eq (call $ftype) (i32.const 4))))

  (func $op_is_symlink (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (if (call $stat (local.get $a) (local.get $al) (i32.const 0)) (then (return (call $bool (i32.const 0)))))
    (call $bool (i32.eq (call $ftype) (i32.const 7))))

  ;; modified seconds of the last stat (Duration::as_secs truncation)
  (func $mtime_secs (result i64)
    (i64.div_u (i64.load offset=112 (global.get $fsp)) (i64.const 1000000000)))

  (func $op_file_size (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $e i32) (local $o i32)
    (local.set $e (call $stat (local.get $a) (local.get $al) (i32.const 1)))
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (local.set $o (call $alloc (i32.const 8)))
    (i64.store (local.get $o) (i64.load offset=96 (global.get $fsp)))
    (call $ok (local.get $o) (i32.const 8)))

  (func $op_modified_at (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $e i32) (local $o i32)
    (local.set $e (call $stat (local.get $a) (local.get $al) (i32.const 1)))
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (local.set $o (call $alloc (i32.const 8)))
    (i64.store (local.get $o) (call $mtime_secs))
    (call $ok (local.get $o) (i32.const 8)))

  ;; fs.stat: [size, is_dir, is_file, modified] as i64 LE (host op 38).
  (func $op_stat (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $e i32) (local $o i32)
    (local.set $e (call $stat (local.get $a) (local.get $al) (i32.const 1)))
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (local.set $o (call $alloc (i32.const 32)))
    (i64.store (local.get $o) (i64.load offset=96 (global.get $fsp)))
    (i64.store offset=8 (local.get $o) (i64.extend_i32_u (i32.eq (call $ftype) (i32.const 3))))
    (i64.store offset=16 (local.get $o) (i64.extend_i32_u (i32.eq (call $ftype) (i32.const 4))))
    (i64.store offset=24 (local.get $o) (call $mtime_secs))
    (call $ok (local.get $o) (i32.const 32)))

  ;; ── writing ────────────────────────────────────────────────────────────

  ;; Open (oflags, fdflags) and write every byte: the errno, or -1 when the
  ;; host accepted zero bytes (Rust's WriteZero).
  (func $write_at (param $p i32) (param $l i32) (param $d i32) (param $dl i32) (param $oflags i32) (param $fdflags i32) (result i32)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $e i32) (local $h i32) (local $done i32) (local $n i32)
    (call $resolve (local.get $p) (local.get $l)) (local.set $rl) (local.set $rp) (local.set $fd)
    (local.set $e (call $path_open (local.get $fd) (i32.const 1) (local.get $rp) (local.get $rl)
                                   (local.get $oflags) (i64.const 4194372) (i64.const 0) (local.get $fdflags)
                                   (global.get $fsp)))
    (if (local.get $e) (then (return (local.get $e))))
    (local.set $h (i32.load (global.get $fsp)))
    (block $out (loop $wr
      (br_if $out (i32.ge_u (local.get $done) (local.get $dl)))
      (i32.store offset=16 (global.get $fsp) (i32.add (local.get $d) (local.get $done)))
      (i32.store offset=20 (global.get $fsp) (i32.sub (local.get $dl) (local.get $done)))
      (local.set $e (call $fd_write (local.get $h) (i32.add (global.get $fsp) (i32.const 16)) (i32.const 1)
                                    (i32.add (global.get $fsp) (i32.const 8))))
      (br_if $out (local.get $e))
      (local.set $n (i32.load offset=8 (global.get $fsp)))
      (if (i32.eqz (local.get $n)) (then (local.set $e (i32.const -1)) (br $out)))
      (local.set $done (i32.add (local.get $done) (local.get $n)))
      (br $wr)))
    (drop (call $fd_close (local.get $h)))
    (local.get $e))

  (func $write_result (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (param $two i32) (param $e i32) (result i64)
    (local $tp i32) (local $tl i32)
    (if (i32.eqz (local.get $e)) (then (return (call $unit))))
    (if (i32.eq (local.get $e) (i32.const -1))
      (then
        (call $s_write_zero) (local.set $tl) (local.set $tp)
        (return (call $fail (local.get $op) (local.get $a) (local.get $al) (local.get $b) (local.get $bl)
                            (local.get $two) (local.get $tp) (local.get $tl)))))
    (call $fail_e (local.get $op) (local.get $a) (local.get $al) (local.get $b) (local.get $bl)
                  (local.get $two) (local.get $e)))

  ;; std::fs::write: create + truncate (ops 2 and 15)
  (func $op_write (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0)
      (call $write_at (local.get $a) (local.get $al) (local.get $b) (local.get $bl) (i32.const 9) (i32.const 0))))

  ;; write_bytes: b is the List[Int] payload, i64 LE slots, low byte each
  ;; (the host's `as_chunks::<8>` over the b_len bytes it is handed).
  (func $op_write_bytes (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $n i32) (local $d i32) (local $i i32)
    (local.set $n (i32.shr_u (local.get $bl) (i32.const 3)))
    (local.set $d (call $alloc (local.get $n)))
    (block $done (loop $c
      (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
      (i32.store8 (i32.add (local.get $d) (local.get $i))
                  (i32.load8_u (i32.add (local.get $b) (i32.shl (local.get $i) (i32.const 3)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0)
      (call $write_at (local.get $a) (local.get $al) (local.get $d) (local.get $n) (i32.const 9) (i32.const 0))))

  ;; OpenOptions create + append
  (func $op_append (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0)
      (call $write_at (local.get $a) (local.get $al) (local.get $b) (local.get $bl) (i32.const 1) (i32.const 1))))

  ;; std::fs::copy: the source's bytes into a created/truncated destination;
  ;; the message names both paths.
  (func $op_copy (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $p i32) (local $l i32) (local $e i32)
    (call $read_all (local.get $a) (local.get $al)) (local.set $e) (local.set $l) (local.set $p)
    (if (i32.eqz (local.get $e))
      (then (local.set $e (call $write_at (local.get $b) (local.get $bl) (local.get $p) (local.get $l)
                                          (i32.const 9) (i32.const 0)))))
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (local.get $b) (local.get $bl)
                        (i32.const 1) (local.get $e)))

  (func $op_rename (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $f1 i32) (local $r1 i32) (local $l1 i32) (local $f2 i32) (local $r2 i32) (local $l2 i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $l1) (local.set $r1) (local.set $f1)
    (call $resolve (local.get $b) (local.get $bl)) (local.set $l2) (local.set $r2) (local.set $f2)
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (local.get $b) (local.get $bl) (i32.const 1)
      (call $path_rename (local.get $f1) (local.get $r1) (local.get $l1)
                         (local.get $f2) (local.get $r2) (local.get $l2))))

  ;; ── directories ────────────────────────────────────────────────────────

  ;; the length of `p`'s parent: trailing '/' dropped, then the last
  ;; component, then the separators before it (0 when there is none)
  (func $parent_len (param $p i32) (param $l i32) (result i32)
    (block $d (loop $c
      (br_if $d (i32.eqz (local.get $l)))
      (br_if $d (i32.ne (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $l) (i32.const 1)))) (i32.const 47)))
      (local.set $l (i32.sub (local.get $l) (i32.const 1)))
      (br $c)))
    (block $d2 (loop $c2
      (br_if $d2 (i32.eqz (local.get $l)))
      (br_if $d2 (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $l) (i32.const 1)))) (i32.const 47)))
      (local.set $l (i32.sub (local.get $l) (i32.const 1)))
      (br $c2)))
    (block $d3 (loop $c3
      (br_if $d3 (i32.eqz (local.get $l)))
      (br_if $d3 (i32.ne (i32.load8_u (i32.add (local.get $p) (i32.sub (local.get $l) (i32.const 1)))) (i32.const 47)))
      (local.set $l (i32.sub (local.get $l) (i32.const 1)))
      (br $c3)))
    (local.get $l))

  ;; std::fs::create_dir_all over one dirfd.
  (func $mkdir_all (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $e i32)
    (if (i32.eqz (local.get $l)) (then (return (i32.const 0))))
    (local.set $e (call $path_create_directory (local.get $fd) (local.get $p) (local.get $l)))
    (if (i32.eqz (local.get $e)) (then (return (i32.const 0))))
    (if (i32.ne (local.get $e) (i32.const 44))
      (then
        (if (call $isdir_at (local.get $fd) (local.get $p) (local.get $l)) (then (return (i32.const 0))))
        (return (local.get $e))))
    (local.set $e (call $mkdir_all (local.get $fd) (local.get $p) (call $parent_len (local.get $p) (local.get $l))))
    (if (local.get $e) (then (return (local.get $e))))
    (local.set $e (call $path_create_directory (local.get $fd) (local.get $p) (local.get $l)))
    (if (i32.eqz (local.get $e)) (then (return (i32.const 0))))
    (if (call $isdir_at (local.get $fd) (local.get $p) (local.get $l)) (then (return (i32.const 0))))
    (local.get $e))

  (func $op_mkdir_p (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $fd i32) (local $rp i32) (local $rl i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $rl) (local.set $rp) (local.set $fd)
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0)
      (call $mkdir_all (local.get $fd) (local.get $rp) (local.get $rl))))

  (func $is_dot (param $p i32) (param $l i32) (result i32)
    (if (i32.eqz (local.get $l)) (then (return (i32.const 0))))
    (if (i32.ne (i32.load8_u (local.get $p)) (i32.const 46)) (then (return (i32.const 0))))
    (if (i32.eq (local.get $l) (i32.const 1)) (then (return (i32.const 1))))
    (i32.and (i32.eq (local.get $l) (i32.const 2))
             (i32.eq (i32.load8_u offset=1 (local.get $p)) (i32.const 46))))

  ;; The entry names of a directory (`.` and `..` skipped) as a node list:
  ;; (head, count, errno). fd_readdir is resumable (C-272): each pass resumes
  ;; at the last complete entry's cookie, the buffer doubles when not even one
  ;; entry fits, and a short fill ends the stream.
  (func $names_at (param $fd i32) (param $p i32) (param $l i32) (result i32 i32 i32)
    (local $e i32) (local $h i32) (local $buf i32) (local $cap i32) (local $used i32) (local $off i32)
    (local $nl i32) (local $np i32) (local $cookie i64) (local $got i32)
    (local $head i32) (local $tail i32) (local $cnt i32) (local $node i32)
    (local.set $e (call $path_open (local.get $fd) (i32.const 1) (local.get $p) (local.get $l)
                                   (i32.const 2) (i64.const 16384) (i64.const 16384) (i32.const 0) (global.get $fsp)))
    (if (local.get $e) (then (return (i32.const 0) (i32.const 0) (local.get $e))))
    (local.set $h (i32.load (global.get $fsp)))
    (local.set $cap (i32.const 16384))
    (local.set $buf (call $alloc (local.get $cap)))
    (block $done (loop $pass
      (local.set $e (call $fd_readdir (local.get $h) (local.get $buf) (local.get $cap) (local.get $cookie)
                                      (i32.add (global.get $fsp) (i32.const 8))))
      (br_if $done (local.get $e))
      (local.set $used (i32.load offset=8 (global.get $fsp)))
      (local.set $off (i32.const 0))
      (local.set $got (i32.const 0))
      (block $pdone (loop $ent
        (br_if $pdone (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $used)))
        (local.set $nl (i32.load offset=16 (i32.add (local.get $buf) (local.get $off))))
        (br_if $pdone (i32.gt_u (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $nl)) (local.get $used)))
        (local.set $np (i32.add (i32.add (local.get $buf) (local.get $off)) (i32.const 24)))
        (local.set $cookie (i64.load (i32.add (local.get $buf) (local.get $off))))
        (local.set $got (i32.const 1))
        (if (i32.eqz (call $is_dot (local.get $np) (local.get $nl)))
          (then
            (local.set $node (call $alloc (i32.add (local.get $nl) (i32.const 8))))
            (i32.store (local.get $node) (i32.const 0))
            (i32.store offset=4 (local.get $node) (local.get $nl))
            (memory.copy (i32.add (local.get $node) (i32.const 8)) (local.get $np) (local.get $nl))
            (if (local.get $tail)
              (then (i32.store (local.get $tail) (local.get $node)))
              (else (local.set $head (local.get $node))))
            (local.set $tail (local.get $node))
            (local.set $cnt (i32.add (local.get $cnt) (i32.const 1)))))
        (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $nl)))
        (br $ent)))
      (br_if $done (i32.lt_u (local.get $used) (local.get $cap)))
      (if (i32.eqz (local.get $got))
        (then
          (local.set $cap (i32.shl (local.get $cap) (i32.const 1)))
          (local.set $buf (call $alloc (local.get $cap)))))
      (br $pass)))
    (drop (call $fd_close (local.get $h)))
    (local.get $head) (local.get $cnt) (local.get $e))

  ;; append a copy of (p, l) to the per-call result list
  (func $push (param $p i32) (param $l i32)
    (local $node i32)
    (local.set $node (call $alloc (i32.add (local.get $l) (i32.const 8))))
    (i32.store (local.get $node) (i32.const 0))
    (i32.store offset=4 (local.get $node) (local.get $l))
    (memory.copy (i32.add (local.get $node) (i32.const 8)) (local.get $p) (local.get $l))
    (if (global.get $rt)
      (then (i32.store (global.get $rt) (local.get $node)))
      (else (global.set $rh (local.get $node))))
    (global.set $rt (local.get $node))
    (global.set $rc (i32.add (global.get $rc) (i32.const 1))))

  ;; A node list, byte-sorted (native `Vec<String>::sort`), as frames.
  (func $sorted_frames (param $head i32) (param $cnt i32) (result i32 i32)
    (local $arr i32) (local $i i32) (local $j i32) (local $x i32) (local $y i32) (local $tot i32) (local $out i32) (local $w i32)
    (local.set $arr (call $alloc (i32.shl (local.get $cnt) (i32.const 2))))
    (block $d (loop $c
      (br_if $d (i32.eqz (local.get $head)))
      (i32.store (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 2))) (local.get $head))
      (local.set $tot (i32.add (local.get $tot) (i32.add (i32.load offset=4 (local.get $head)) (i32.const 4))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (local.set $head (i32.load (local.get $head)))
      (br $c)))
    ;; insertion sort
    (local.set $i (i32.const 1))
    (block $sd (loop $sc
      (br_if $sd (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $x (i32.load (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 2)))))
      (local.set $j (local.get $i))
      (block $id (loop $ic
        (br_if $id (i32.eqz (local.get $j)))
        (local.set $y (i32.load (i32.add (local.get $arr) (i32.shl (i32.sub (local.get $j) (i32.const 1)) (i32.const 2)))))
        (br_if $id (i32.eqz (call $lt (i32.add (local.get $x) (i32.const 8)) (i32.load offset=4 (local.get $x))
                                      (i32.add (local.get $y) (i32.const 8)) (i32.load offset=4 (local.get $y)))))
        (i32.store (i32.add (local.get $arr) (i32.shl (local.get $j) (i32.const 2))) (local.get $y))
        (local.set $j (i32.sub (local.get $j) (i32.const 1)))
        (br $ic)))
      (i32.store (i32.add (local.get $arr) (i32.shl (local.get $j) (i32.const 2))) (local.get $x))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $sc)))
    (local.set $out (call $alloc (local.get $tot)))
    (local.set $w (local.get $out))
    (local.set $i (i32.const 0))
    (block $fd (loop $fc
      (br_if $fd (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $x (i32.load (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 2)))))
      (local.set $w (call $frame (local.get $w) (i32.add (local.get $x) (i32.const 8)) (i32.load offset=4 (local.get $x))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $fc)))
    (local.get $out) (local.get $tot))

  (func $op_list_dir (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $head i32) (local $cnt i32) (local $e i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $rl) (local.set $rp) (local.set $fd)
    (call $names_at (local.get $fd) (local.get $rp) (local.get $rl)) (local.set $e) (local.set $cnt) (local.set $head)
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (call $sorted_frames (local.get $head) (local.get $cnt))
    (call $ok))

  ;; remove: a directory (followed) is remove_dir, anything else remove_file
  (func $op_remove (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $e i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $rl) (local.set $rp) (local.set $fd)
    (if (call $isdir_at (local.get $fd) (local.get $rp) (local.get $rl))
      (then (local.set $e (call $path_remove_directory (local.get $fd) (local.get $rp) (local.get $rl))))
      (else (local.set $e (call $path_unlink_file (local.get $fd) (local.get $rp) (local.get $rl)))))
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0) (local.get $e)))

  ;; std::fs::remove_dir_all: a symlink or file is unlinked, a directory is
  ;; emptied depth-first and then removed.
  (func $rmtree (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $e i32) (local $head i32) (local $cnt i32) (local $cp i32) (local $cl i32)
    (local.set $e (call $stat_at (local.get $fd) (local.get $p) (local.get $l) (i32.const 0)))
    (if (local.get $e) (then (return (local.get $e))))
    (if (i32.ne (call $ftype) (i32.const 3))
      (then (return (call $path_unlink_file (local.get $fd) (local.get $p) (local.get $l)))))
    (call $names_at (local.get $fd) (local.get $p) (local.get $l)) (local.set $e) (local.set $cnt) (local.set $head)
    (if (local.get $e) (then (return (local.get $e))))
    (block $d (loop $c
      (br_if $d (i32.eqz (local.get $head)))
      (call $join (local.get $p) (local.get $l) (i32.add (local.get $head) (i32.const 8)) (i32.load offset=4 (local.get $head)))
      (local.set $cl) (local.set $cp)
      (local.set $e (call $rmtree (local.get $fd) (local.get $cp) (local.get $cl)))
      (if (local.get $e) (then (return (local.get $e))))
      (local.set $head (i32.load (local.get $head)))
      (br $c)))
    (call $path_remove_directory (local.get $fd) (local.get $p) (local.get $l)))

  (func $op_remove_all (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $e i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $rl) (local.set $rp) (local.set $fd)
    (if (call $isdir_at (local.get $fd) (local.get $rp) (local.get $rl))
      (then (local.set $e (call $rmtree (local.get $fd) (local.get $rp) (local.get $rl))))
      (else (local.set $e (call $path_unlink_file (local.get $fd) (local.get $rp) (local.get $rl)))))
    (call $write_result (local.get $op) (local.get $a) (local.get $al) (i32.const 0) (i32.const 0) (i32.const 0) (local.get $e)))

  ;; temp root + prefix + the wall clock's nanoseconds (the host's name)
  (func $temp_path (param $a i32) (param $al i32) (result i32 i32)
    (local $tp i32) (local $tl i32) (local $np i32) (local $nl i32) (local $name i32) (local $w i32)
    (call $tmp) (local.set $tl) (local.set $tp)
    (drop (call $clock_time_get (i32.const 0) (i64.const 1) (i32.add (global.get $fsp) (i32.const 40))))
    (call $utoa (i64.load offset=40 (global.get $fsp))) (local.set $nl) (local.set $np)
    (local.set $name (call $alloc (i32.add (local.get $al) (local.get $nl))))
    (local.set $w (call $put (local.get $name) (local.get $a) (local.get $al)))
    (local.set $w (call $put (local.get $w) (local.get $np) (local.get $nl)))
    (call $join (local.get $tp) (local.get $tl) (local.get $name) (i32.sub (local.get $w) (local.get $name))))

  (func $op_create_temp_dir (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $p i32) (local $l i32) (local $fd i32) (local $rp i32) (local $rl i32) (local $e i32)
    (call $temp_path (local.get $a) (local.get $al)) (local.set $l) (local.set $p)
    (call $resolve (local.get $p) (local.get $l)) (local.set $rl) (local.set $rp) (local.set $fd)
    (local.set $e (call $mkdir_all (local.get $fd) (local.get $rp) (local.get $rl)))
    (if (local.get $e)
      (then (return (call $write_result (local.get $op) (local.get $p) (local.get $l) (i32.const 0) (i32.const 0)
                                        (i32.const 0) (local.get $e)))))
    (call $ok (local.get $p) (local.get $l)))

  (func $op_create_temp_file (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $p i32) (local $l i32) (local $e i32)
    (call $temp_path (local.get $a) (local.get $al)) (local.set $l) (local.set $p)
    (local.set $e (call $write_at (local.get $p) (local.get $l) (i32.const 0) (i32.const 0) (i32.const 9) (i32.const 0)))
    (if (local.get $e)
      (then (return (call $write_result (local.get $op) (local.get $p) (local.get $l) (i32.const 0) (i32.const 0)
                                        (i32.const 0) (local.get $e)))))
    (call $ok (local.get $p) (local.get $l)))

  ;; ── walk / glob ────────────────────────────────────────────────────────

  ;; Depth-first: every entry's joined path, descending into directories
  ;; (followed); a directory that cannot be read names itself in $ep/$el.
  (func $walk_rec (param $d i32) (param $dl i32) (result i32)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $head i32) (local $cnt i32) (local $e i32) (local $cp i32) (local $cl i32)
    (call $resolve (local.get $d) (local.get $dl)) (local.set $rl) (local.set $rp) (local.set $fd)
    (call $names_at (local.get $fd) (local.get $rp) (local.get $rl)) (local.set $e) (local.set $cnt) (local.set $head)
    (if (local.get $e)
      (then (global.set $ep (local.get $d)) (global.set $el (local.get $dl)) (return (local.get $e))))
    (block $done (loop $n
      (br_if $done (i32.eqz (local.get $head)))
      (call $join (local.get $d) (local.get $dl) (i32.add (local.get $head) (i32.const 8)) (i32.load offset=4 (local.get $head)))
      (local.set $cl) (local.set $cp)
      (call $push (local.get $cp) (local.get $cl))
      (if (call $is_dir_path (local.get $cp) (local.get $cl))
        (then
          (local.set $e (call $walk_rec (local.get $cp) (local.get $cl)))
          (if (local.get $e) (then (return (local.get $e))))))
      (local.set $head (i32.load (local.get $head)))
      (br $n)))
    (i32.const 0))

  (func $op_walk (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $e i32)
    (local.set $e (call $walk_rec (local.get $a) (local.get $al)))
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (global.get $ep) (global.get $el) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (call $sorted_frames (global.get $rh) (global.get $rc))
    (call $ok))

  ;; Split on '/', empty segments dropped: an array of [ptr][len], and its count.
  (func $split (param $p i32) (param $l i32) (result i32 i32)
    (local $arr i32) (local $n i32) (local $i i32) (local $s i32)
    (local.set $arr (call $alloc (i32.shl (i32.add (local.get $l) (i32.const 1)) (i32.const 3))))
    (block $d (loop $c
      (if (i32.or (i32.ge_u (local.get $i) (local.get $l))
                  (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 47)))
        (then
          (if (i32.gt_u (local.get $i) (local.get $s))
            (then
              (i32.store (i32.add (local.get $arr) (i32.shl (local.get $n) (i32.const 3))) (i32.add (local.get $p) (local.get $s)))
              (i32.store offset=4 (i32.add (local.get $arr) (i32.shl (local.get $n) (i32.const 3))) (i32.sub (local.get $i) (local.get $s)))
              (local.set $n (i32.add (local.get $n) (i32.const 1)))))
          (local.set $s (i32.add (local.get $i) (i32.const 1)))))
      (br_if $d (i32.ge_u (local.get $i) (local.get $l)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (local.get $arr) (local.get $n))

  (func $has_star (param $p i32) (param $l i32) (result i32)
    (local $i i32)
    (block $d (loop $c
      (br_if $d (i32.ge_u (local.get $i) (local.get $l)))
      (if (i32.eq (i32.load8_u (i32.add (local.get $p) (local.get $i))) (i32.const 42)) (then (return (i32.const 1))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (i32.const 0))

  ;; the first index of needle in hay[from..hl), or -1
  (func $find (param $h i32) (param $hl i32) (param $from i32) (param $n i32) (param $nl i32) (result i32)
    (local $i i32)
    (local.set $i (local.get $from))
    (block $d (loop $c
      (br_if $d (i32.gt_u (i32.add (local.get $i) (local.get $nl)) (local.get $hl)))
      (if (call $memeq (i32.add (local.get $h) (local.get $i)) (local.get $n) (local.get $nl)) (then (return (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (i32.const -1))

  ;; The host's `star_match`: '*' splits the pattern into parts; the first
  ;; must prefix, the last suffix, the middle ones occur in order between.
  (func $star_match (param $pp i32) (param $pl i32) (param $sp i32) (param $sl i32) (result i32)
    (local $fe i32) (local $ls i32) (local $ll i32) (local $i i32) (local $j i32) (local $rp i32) (local $rl i32) (local $pos i32) (local $k i32)
    (if (i32.eqz (call $has_star (local.get $pp) (local.get $pl)))
      (then (return (call $streq (local.get $pp) (local.get $pl) (local.get $sp) (local.get $sl)))))
    ;; fe = index of the first '*', ls = index after the last '*'
    (local.set $fe (i32.const 0))
    (block $d (loop $c
      (br_if $d (i32.eq (i32.load8_u (i32.add (local.get $pp) (local.get $fe))) (i32.const 42)))
      (local.set $fe (i32.add (local.get $fe) (i32.const 1)))
      (br $c)))
    (local.set $ls (local.get $pl))
    (block $d2 (loop $c2
      (br_if $d2 (i32.eq (i32.load8_u (i32.add (local.get $pp) (i32.sub (local.get $ls) (i32.const 1)))) (i32.const 42)))
      (local.set $ls (i32.sub (local.get $ls) (i32.const 1)))
      (br $c2)))
    (local.set $ll (i32.sub (local.get $pl) (local.get $ls)))
    (if (i32.lt_u (local.get $sl) (i32.add (local.get $fe) (local.get $ll))) (then (return (i32.const 0))))
    (if (i32.eqz (call $memeq (local.get $sp) (local.get $pp) (local.get $fe))) (then (return (i32.const 0))))
    (if (i32.eqz (call $memeq (i32.add (local.get $sp) (i32.sub (local.get $sl) (local.get $ll)))
                              (i32.add (local.get $pp) (local.get $ls)) (local.get $ll)))
      (then (return (i32.const 0))))
    ;; the region between the prefix and the suffix
    (local.set $rp (i32.add (local.get $sp) (local.get $fe)))
    (local.set $rl (i32.sub (i32.sub (local.get $sl) (local.get $fe)) (local.get $ll)))
    ;; middle parts: the runs between consecutive '*' in pattern[fe+1 .. ls-1)
    (local.set $i (i32.add (local.get $fe) (i32.const 1)))
    (block $md (loop $mc
      (br_if $md (i32.ge_u (local.get $i) (local.get $ls)))
      (local.set $j (local.get $i))
      (block $ed (loop $ec
        (br_if $ed (i32.eq (i32.load8_u (i32.add (local.get $pp) (local.get $j))) (i32.const 42)))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $ec)))
      (if (i32.gt_u (local.get $j) (local.get $i))
        (then
          (local.set $k (call $find (local.get $rp) (local.get $rl) (local.get $pos)
                                    (i32.add (local.get $pp) (local.get $i)) (i32.sub (local.get $j) (local.get $i))))
          (if (i32.lt_s (local.get $k) (i32.const 0)) (then (return (i32.const 0))))
          (local.set $pos (i32.add (local.get $k) (i32.sub (local.get $j) (local.get $i))))))
      (local.set $i (i32.add (local.get $j) (i32.const 1)))
      (br $mc)))
    (i32.const 1))

  (func $is_dstar (param $p i32) (param $l i32) (result i32)
    (if (i32.ne (local.get $l) (i32.const 2)) (then (return (i32.const 0))))
    (i32.and (i32.eq (i32.load8_u (local.get $p)) (i32.const 42))
             (i32.eq (i32.load8_u offset=1 (local.get $p)) (i32.const 42))))

  ;; The host's `segs_match` over the pattern segments ($gp/$gn) from `pi`
  ;; and the segment array `sa` from `si` (of `sn`).
  (func $segs_match (param $pi i32) (param $sa i32) (param $si i32) (param $sn i32) (result i32)
    (local $pp i32) (local $pl i32) (local $i i32)
    (if (i32.ge_u (local.get $pi) (global.get $gn)) (then (return (i32.eq (local.get $si) (local.get $sn)))))
    (local.set $pp (i32.load (i32.add (global.get $gp) (i32.shl (local.get $pi) (i32.const 3)))))
    (local.set $pl (i32.load offset=4 (i32.add (global.get $gp) (i32.shl (local.get $pi) (i32.const 3)))))
    (if (call $is_dstar (local.get $pp) (local.get $pl))
      (then
        (local.set $i (local.get $si))
        (block $d (loop $c
          (br_if $d (i32.gt_u (local.get $i) (local.get $sn)))
          (if (call $segs_match (i32.add (local.get $pi) (i32.const 1)) (local.get $sa) (local.get $i) (local.get $sn))
            (then (return (i32.const 1))))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br $c)))
        (return (i32.const 0))))
    (if (i32.ge_u (local.get $si) (local.get $sn)) (then (return (i32.const 0))))
    (if (i32.eqz (call $star_match (local.get $pp) (local.get $pl)
                                   (i32.load (i32.add (local.get $sa) (i32.shl (local.get $si) (i32.const 3))))
                                   (i32.load offset=4 (i32.add (local.get $sa) (i32.shl (local.get $si) (i32.const 3))))))
      (then (return (i32.const 0))))
    (call $segs_match (i32.add (local.get $pi) (i32.const 1)) (local.get $sa) (i32.add (local.get $si) (i32.const 1)) (local.get $sn)))

  ;; The host's glob `walk`: every entry's relative path, descending while
  ;; `depth` (-1 = unbounded) allows and the entry is a directory.
  (func $glob_walk (param $d i32) (param $dl i32) (param $rel i32) (param $rell i32) (param $depth i32) (result i32)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $head i32) (local $cnt i32) (local $e i32)
    (local $np i32) (local $nl i32) (local $cp i32) (local $cl i32) (local $xp i32) (local $xl i32)
    (call $resolve (local.get $d) (local.get $dl)) (local.set $rl) (local.set $rp) (local.set $fd)
    (call $names_at (local.get $fd) (local.get $rp) (local.get $rl)) (local.set $e) (local.set $cnt) (local.set $head)
    (if (local.get $e)
      (then (global.set $ep (local.get $d)) (global.set $el (local.get $dl)) (return (local.get $e))))
    (block $done (loop $n
      (br_if $done (i32.eqz (local.get $head)))
      (local.set $np (i32.add (local.get $head) (i32.const 8)))
      (local.set $nl (i32.load offset=4 (local.get $head)))
      (call $join (local.get $rel) (local.get $rell) (local.get $np) (local.get $nl)) (local.set $xl) (local.set $xp)
      (call $join (local.get $d) (local.get $dl) (local.get $np) (local.get $nl)) (local.set $cl) (local.set $cp)
      (call $push (local.get $xp) (local.get $xl))
      (if (i32.ne (local.get $depth) (i32.const 1))
        (then
          (if (call $is_dir_path (local.get $cp) (local.get $cl))
            (then
              (local.set $e (call $glob_walk (local.get $cp) (local.get $cl) (local.get $xp) (local.get $xl)
                                             (select (i32.const -1) (i32.sub (local.get $depth) (i32.const 1))
                                                     (i32.lt_s (local.get $depth) (i32.const 0)))))
              (if (local.get $e) (then (return (local.get $e))))))))
      (local.set $head (i32.load (local.get $head)))
      (br $n)))
    (i32.const 0))

  ;; fs.glob (C-228): the host's `fs_glob`, step for step.
  (func $op_glob (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $segs i32) (local $n i32) (local $k i32) (local $i i32) (local $base i32) (local $basel i32) (local $w i32)
    (local $sp i32) (local $sl i32) (local $root i32) (local $rootl i32) (local $pre i32) (local $prel i32)
    (local $depth i32) (local $e i32) (local $rels i32) (local $rsa i32) (local $rsn i32) (local $x i32) (local $xp i32) (local $xl i32)
    (call $split (local.get $a) (local.get $al)) (local.set $n) (local.set $segs)
    ;; k = the leading segments without a '*'
    (block $kd (loop $kc
      (br_if $kd (i32.ge_u (local.get $k) (local.get $n)))
      (br_if $kd (call $has_star (i32.load (i32.add (local.get $segs) (i32.shl (local.get $k) (i32.const 3))))
                                 (i32.load offset=4 (i32.add (local.get $segs) (i32.shl (local.get $k) (i32.const 3))))))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))
      (br $kc)))
    (global.set $gp (i32.add (local.get $segs) (i32.shl (local.get $k) (i32.const 3))))
    (global.set $gn (i32.sub (local.get $n) (local.get $k)))
    ;; base = ("/" if absolute) + segs[..k].join("/")
    (local.set $base (call $alloc (i32.add (local.get $al) (i32.const 1))))
    (local.set $w (local.get $base))
    (if (i32.and (i32.gt_u (local.get $al) (i32.const 0)) (i32.eq (i32.load8_u (local.get $a)) (i32.const 47)))
      (then (i32.store8 (local.get $w) (i32.const 47)) (local.set $w (i32.add (local.get $w) (i32.const 1)))))
    (block $bd (loop $bc
      (br_if $bd (i32.ge_u (local.get $i) (local.get $k)))
      (if (local.get $i) (then (i32.store8 (local.get $w) (i32.const 47)) (local.set $w (i32.add (local.get $w) (i32.const 1)))))
      (local.set $w (call $put (local.get $w)
                               (i32.load (i32.add (local.get $segs) (i32.shl (local.get $i) (i32.const 3))))
                               (i32.load offset=4 (i32.add (local.get $segs) (i32.shl (local.get $i) (i32.const 3))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $bc)))
    (local.set $basel (i32.sub (local.get $w) (local.get $base)))
    ;; no wildcard segment: the base itself, when it exists
    (if (i32.eqz (global.get $gn))
      (then
        (if (i32.and (i32.gt_u (local.get $basel) (i32.const 0))
                     (i32.eqz (call $stat (local.get $base) (local.get $basel) (i32.const 1))))
          (then (call $push (local.get $base) (local.get $basel))))
        (call $sorted_frames (global.get $rh) (global.get $rc))
        (return (call $ok))))
    (if (i32.gt_u (local.get $basel) (i32.const 0))
      (then
        (if (i32.eqz (call $is_dir_path (local.get $base) (local.get $basel)))
          (then (return (call $ok (i32.const 0) (i32.const 0)))))))
    (local.set $root (local.get $base))
    (local.set $rootl (local.get $basel))
    (if (i32.eqz (local.get $basel)) (then (call $s_dot) (local.set $rootl) (local.set $root)))
    ;; prefix = base, or base + "/" unless base is "" or "/"
    (local.set $pre (local.get $base))
    (local.set $prel (local.get $basel))
    (if (i32.gt_u (local.get $basel) (i32.const 0))
      (then
        (if (i32.eqz (i32.and (i32.eq (local.get $basel) (i32.const 1)) (i32.eq (i32.load8_u (local.get $base)) (i32.const 47))))
          (then
            (local.set $pre (call $alloc (i32.add (local.get $basel) (i32.const 1))))
            (local.set $w (call $put (local.get $pre) (local.get $base) (local.get $basel)))
            (i32.store8 (local.get $w) (i32.const 47))
            (local.set $prel (i32.add (local.get $basel) (i32.const 1)))))))
    ;; depth: unbounded under "**", else the pattern's segment count
    (local.set $depth (global.get $gn))
    (local.set $i (i32.const 0))
    (block $dd (loop $dc
      (br_if $dd (i32.ge_u (local.get $i) (global.get $gn)))
      (if (call $is_dstar (i32.load (i32.add (global.get $gp) (i32.shl (local.get $i) (i32.const 3))))
                          (i32.load offset=4 (i32.add (global.get $gp) (i32.shl (local.get $i) (i32.const 3)))))
        (then (local.set $depth (i32.const -1))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $dc)))
    ;; walk into the result list, then take it as the candidate list
    (local.set $e (call $glob_walk (local.get $root) (local.get $rootl) (i32.const 0) (i32.const 0) (local.get $depth)))
    (if (local.get $e)
      (then (return (call $fail_e (local.get $op) (global.get $ep) (global.get $el) (i32.const 0) (i32.const 0)
                                  (i32.const 0) (local.get $e)))))
    (local.set $rels (global.get $rh))
    (global.set $rh (i32.const 0))
    (global.set $rt (i32.const 0))
    (global.set $rc (i32.const 0))
    (if (i32.gt_u (local.get $basel) (i32.const 0))
      (then
        (if (call $segs_match (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0))
          (then (call $push (local.get $base) (local.get $basel))))))
    (block $fd (loop $fc
      (br_if $fd (i32.eqz (local.get $rels)))
      (local.set $xp (i32.add (local.get $rels) (i32.const 8)))
      (local.set $xl (i32.load offset=4 (local.get $rels)))
      (call $split (local.get $xp) (local.get $xl)) (local.set $rsn) (local.set $rsa)
      (if (call $segs_match (i32.const 0) (local.get $rsa) (i32.const 0) (local.get $rsn))
        (then
          (call $join2 (local.get $pre) (local.get $prel) (local.get $xp) (local.get $xl)) (local.set $sl) (local.set $sp)
          (call $push (local.get $sp) (local.get $sl))))
      (local.set $rels (i32.load (local.get $rels)))
      (br $fc)))
    (call $sorted_frames (global.get $rh) (global.get $rc))
    (call $ok))

  ;; plain concatenation
  (func $join2 (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i32 i32)
    (local $o i32)
    (local.set $o (call $alloc (i32.add (local.get $al) (local.get $bl))))
    (drop (call $put (call $put (local.get $o) (local.get $a) (local.get $al)) (local.get $b) (local.get $bl)))
    (local.get $o) (i32.add (local.get $al) (local.get $bl)))

  ;; ── process-environment answers ────────────────────────────────────────

  ;; env.os: the WASI leg reports "wasi" (C-189).
  (func $op_os (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $s_wasi)
    (call $ok))

  (func $op_temp_dir (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $tmp)
    (call $ok))

  ;; env.cwd: the cwd relative paths resolve against, else "/" (env_cwd.almd)
  (func $op_cwd (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (local $p i32) (local $l i32)
    (call $cwd) (local.set $l) (local.set $p)
    (if (i32.eqz (local.get $l)) (then (call $s_slash) (local.set $l) (local.set $p)))
    (call $ok (local.get $p) (local.get $l)))

  ;; the fan prefetch pair's start/finish: no slot state on a sequential host
  (func $op_nop (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)
    (call $unit))
  ;; the generated tail (fs_service.rs) follows and closes the module
