;; The p3 fs service's ADAPTER (#3140): the `wasi_snapshot_preview1`
;; functions the p1 fs service (crates/almide-wasi/src/fs_service.wat) calls,
;; served over wasi:filesystem@0.3 / wasi:cli@0.3 / wasi:clocks@0.3 — so the
;; p3 component answers every fs op with the SAME service code the stock-p1
;; artifact runs (resolution, errors, walk/glob/sort), and the two cannot
;; drift. `wasi_p3_fs_service.rs` splices this text into the service in place
;; of its preview-1 import lines (the part above the FUNCS marker) and before
;; its dispatcher (the part below), and fills the at-sign NAME holes with the
;; canonical-ABI offsets it derives from the vendored WIT.
;;
;; Descriptors: preview-1 fds 3.. are the preopens (`get-directories`, in its
;; order, fetched once); an opened descriptor takes fd 64 + slot, its slot
;; one of 24 at fsp+640 ([handle u32][state u32: 0 free, 1 open, 2 open for
;; append][offset u64]) — the service closes every descriptor it opens before
;; it recurses, so it never holds more than two.
;;
;; Results the canonical ABI lands through `cabi_realloc` (the preopen and
;; environment lists, directory entry names) take the guest's heap frontier
;; (`$heap`), which the service's own arena (`$ap`) also starts from: `$hs_pre`
;; lifts the frontier over the arena before such a call and `$hs_post` lifts
;; the arena over what landed, so neither overwrites the other. Every such
;; call is reserved first (`$p3_reserve`, the #2119 discipline).
  (import "wasi:filesystem/preopens@0.3.0" "get-directories" (func $p3_get_directories (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.open-at" (func $p3_open_at (param i32 i32 i32 i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.stat-at" (func $p3_stat_at (param i32 i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.stat" (func $p3_stat (param i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.read-via-stream" (func $p3_rvs (param i32 i64 i32)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][stream-read-0][method]descriptor.read-via-stream" (func $p3_rvs_read (param i32 i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[stream-drop-readable-0][method]descriptor.read-via-stream" (func $p3_rvs_drop (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][future-read-1][method]descriptor.read-via-stream" (func $p3_rvs_fut_read (param i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[future-drop-readable-1][method]descriptor.read-via-stream" (func $p3_rvs_fut_drop (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[resource-drop]descriptor" (func $p3_desc_drop (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.write-via-stream" (func $p3_wvs (param i32 i32 i64) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.append-via-stream" (func $p3_avs (param i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[stream-new-0][method]descriptor.write-via-stream" (func $p3_wvs_new (result i64)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][stream-write-0][method]descriptor.write-via-stream" (func $p3_wvs_write (param i32 i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[stream-drop-writable-0][method]descriptor.write-via-stream" (func $p3_wvs_drop (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][future-read-1][method]descriptor.write-via-stream" (func $p3_wvs_fut_read (param i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.create-directory-at" (func $p3_mkdir (param i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.unlink-file-at" (func $p3_unlink (param i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.remove-directory-at" (func $p3_rmdir (param i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.rename-at" (func $p3_rename (param i32 i32 i32 i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[method]descriptor.read-directory" (func $p3_readdir (param i32 i32)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][stream-read-0][method]descriptor.read-directory" (func $p3_readdir_read (param i32 i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[stream-drop-readable-0][method]descriptor.read-directory" (func $p3_readdir_drop (param i32)))
  (import "wasi:filesystem/types@0.3.0" "[async-lower][future-read-1][method]descriptor.read-directory" (func $p3_readdir_fut_read (param i32 i32) (result i32)))
  (import "wasi:filesystem/types@0.3.0" "[future-drop-readable-1][method]descriptor.read-directory" (func $p3_readdir_fut_drop (param i32)))
  (import "wasi:cli/environment@0.3.0" "get-environment" (func $p3_get_environment (param i32)))
  (import "wasi:clocks/system-clock@0.3.0" "now" (func $p3_clock_now (param i32)))
  (import "wasi:cli/exit@0.3.0" "exit" (func $p3_exit (param i32)))
  (import "shim" "fs_call" (func $p3_fs_call (param i32 i32 i32 i32 i32) (result i64)))
  (import "shim" "await" (func $p3_await (param i32 i32) (result i32)))
  (import "shim" "alloc" (func $p3_alloc (param i32 i32 i32 i32) (result i32)))
  (import "shim" "reserve" (func $p3_reserve (param i32)))
  (import "shim" "env" (global $g_env (mut i32)))
  (import "shim" "envn" (global $g_envn (mut i32)))
;; @@FUNCS@@
  ;; ── the p3 adapter (#3140): preview-1 over wasi:filesystem 0.3 ─────────

  (global $ad_pre (mut i32) (i32.const 0))
  (global $ad_pren (mut i32) (i32.const -1))

  ;; Lift the heap frontier over the arena (before a call that lands
  ;; through cabi_realloc), then the arena over what landed.
  (func $hs_pre
    (local $a i32)
    (local.set $a (i32.and (i32.add (global.get $ap) (i32.const 7)) (i32.const -8)))
    (if (i32.gt_u (local.get $a) (global.get $heap)) (then (global.set $heap (local.get $a)))))

  (func $hs_post
    (local $h i32)
    (local.set $h (i32.and (i32.add (global.get $heap) (i32.const 7)) (i32.const -8)))
    (if (i32.gt_u (local.get $h) (global.get $ap)) (then (global.set $ap (local.get $h)))))

  ;; the adapter's retptr scratch (112 bytes: the stat result's footprint)
  (func $ad_ret (result i32)
    (i32.add (global.get $fsp) (i32.const 128)))

  (func $ad_slot (param $i i32) (result i32)
    (i32.add (i32.add (global.get $fsp) (i32.const 640)) (i32.shl (local.get $i) (i32.const 4))))

  (func $ad_preopens
    (local $r i32)
    (if (i32.ge_s (global.get $ad_pren) (i32.const 0)) (then (return)))
    (local.set $r (call $ad_ret))
    (call $hs_pre)
    (call $p3_reserve (i32.const @PREOPEN_RESERVE@))
    (call $p3_get_directories (local.get $r))
    (call $hs_post)
    (global.set $ad_pre (i32.load (local.get $r)))
    (global.set $ad_pren (i32.load offset=4 (local.get $r))))

  ;; the preopen row of fd, or 0
  (func $ad_pre_row (param $fd i32) (result i32)
    (local $i i32)
    (call $ad_preopens)
    (local.set $i (i32.sub (local.get $fd) (i32.const 3)))
    (if (i32.or (i32.lt_s (local.get $i) (i32.const 0)) (i32.ge_s (local.get $i) (global.get $ad_pren)))
      (then (return (i32.const 0))))
    (i32.add (global.get $ad_pre) (i32.mul (local.get $i) (i32.const 12))))

  ;; the open slot of fd, or 0
  (func $ad_open_slot (param $fd i32) (result i32)
    (local $e i32)
    (if (i32.lt_u (local.get $fd) (i32.const 64)) (then (return (i32.const 0))))
    (if (i32.ge_u (i32.sub (local.get $fd) (i32.const 64)) (i32.const 24)) (then (return (i32.const 0))))
    (local.set $e (call $ad_slot (i32.sub (local.get $fd) (i32.const 64))))
    (if (i32.eqz (i32.load offset=4 (local.get $e))) (then (return (i32.const 0))))
    (local.get $e))

  ;; the p3 descriptor behind fd, or -1
  (func $ad_handle (param $fd i32) (result i32)
    (local $e i32)
    (local.set $e (call $ad_open_slot (local.get $fd)))
    (if (local.get $e) (then (return (i32.load (local.get $e)))))
    (local.set $e (call $ad_pre_row (local.get $fd)))
    (if (local.get $e) (then (return (i32.load (local.get $e)))))
    (i32.const -1))

  ;; a result<_, error-code> at r (payload at `pay`): 0, or its errno
  (func $ad_status (param $r i32) (param $pay i32) (result i32)
    (if (i32.eqz (i32.load8_u (local.get $r))) (then (return (i32.const 0))))
    (call $p3_errno (i32.load8_u (i32.add (local.get $r) (local.get $pay)))))

  ;; option<instant> at o as nanoseconds (0 for none)
  (func $ad_ts (param $o i32) (result i64)
    (if (i32.eqz (i32.load8_u (local.get $o))) (then (return (i64.const 0))))
    (i64.add (i64.mul (i64.load offset=@OPT_SEC@ (local.get $o)) (i64.const 1000000000))
             (i64.extend_i32_u (i32.load offset=@OPT_NS@ (local.get $o)))))

  ;; a descriptor-stat at s as a preview-1 filestat (64 bytes) at d
  (func $ad_filestat (param $s i32) (param $d i32)
    (memory.fill (local.get $d) (i32.const 0) (i32.const 64))
    (i32.store8 offset=16 (local.get $d) (call $p3_ftype (i32.load8_u offset=@ST_TYPE@ (local.get $s))))
    (i64.store offset=24 (local.get $d) (i64.load offset=@ST_NLINK@ (local.get $s)))
    (i64.store offset=32 (local.get $d) (i64.load offset=@ST_SIZE@ (local.get $s)))
    (i64.store offset=40 (local.get $d) (call $ad_ts (i32.add (local.get $s) (i32.const @ST_ATIM@))))
    (i64.store offset=48 (local.get $d) (call $ad_ts (i32.add (local.get $s) (i32.const @ST_MTIM@))))
    (i64.store offset=56 (local.get $d) (call $ad_ts (i32.add (local.get $s) (i32.const @ST_CTIM@)))))

  (func $fd_prestat_get (param $fd i32) (param $buf i32) (result i32)
    (local $row i32)
    (local.set $row (call $ad_pre_row (local.get $fd)))
    (if (i32.eqz (local.get $row)) (then (return (i32.const 8))))
    (i32.store (local.get $buf) (i32.const 0))
    (i32.store offset=4 (local.get $buf) (i32.load offset=8 (local.get $row)))
    (i32.const 0))

  (func $fd_prestat_dir_name (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $row i32) (local $n i32)
    (local.set $row (call $ad_pre_row (local.get $fd)))
    (if (i32.eqz (local.get $row)) (then (return (i32.const 8))))
    (local.set $n (i32.load offset=8 (local.get $row)))
    (if (i32.gt_u (local.get $n) (local.get $l)) (then (local.set $n (local.get $l))))
    (memory.copy (local.get $p) (i32.load offset=4 (local.get $row)) (local.get $n))
    (i32.const 0))

  ;; oflags (creat 1 / directory 2 / excl 4 / trunc 8) are open-flags bit for
  ;; bit; the read / write rights pick the descriptor-flags; fdflags' APPEND
  ;; marks the slot so writes go through append-via-stream.
  (func $path_open (param $dirfd i32) (param $dirflags i32) (param $p i32) (param $l i32) (param $oflags i32)
                   (param $rb i64) (param $ri i64) (param $fdflags i32) (param $out i32) (result i32)
    (local $h i32) (local $i i32) (local $df i32) (local $r i32) (local $e i32)
    (local.set $h (call $ad_handle (local.get $dirfd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    ;; a free slot first, so a full table never strands an opened descriptor
    (block $f (loop $s
      (br_if $f (i32.ge_u (local.get $i) (i32.const 24)))
      (br_if $f (i32.eqz (i32.load offset=4 (call $ad_slot (local.get $i)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $s)))
    (if (i32.ge_u (local.get $i) (i32.const 24)) (then (return (i32.const 33))))
    (if (i32.or (i64.ne (i64.and (local.get $rb) (i64.const 2)) (i64.const 0))
                (i32.and (local.get $oflags) (i32.const 2)))
      (then (local.set $df (i32.const 1))))
    (if (i64.ne (i64.and (local.get $rb) (i64.const 64)) (i64.const 0))
      (then (local.set $df (i32.or (local.get $df) (i32.const 2)))))
    (local.set $r (call $ad_ret))
    (call $p3_open_at (local.get $h) (i32.and (local.get $dirflags) (i32.const 1)) (local.get $p) (local.get $l)
                      (i32.and (local.get $oflags) (i32.const 15)) (local.get $df) (local.get $r))
    (if (i32.load8_u (local.get $r))
      (then (return (call $p3_errno (i32.load8_u offset=@OPEN_PAY@ (local.get $r))))))
    (local.set $e (call $ad_slot (local.get $i)))
    (i32.store (local.get $e) (i32.load offset=@OPEN_PAY@ (local.get $r)))
    (i32.store offset=4 (local.get $e) (select (i32.const 2) (i32.const 1) (i32.and (local.get $fdflags) (i32.const 1))))
    (i64.store offset=8 (local.get $e) (i64.const 0))
    (i32.store (local.get $out) (i32.add (local.get $i) (i32.const 64)))
    (i32.const 0))

  (func $fd_close (param $fd i32) (result i32)
    (local $e i32)
    (local.set $e (call $ad_open_slot (local.get $fd)))
    (if (i32.eqz (local.get $e)) (then (return (select (i32.const 0) (i32.const 8) (i32.lt_u (local.get $fd) (i32.const 64))))))
    (call $p3_desc_drop (i32.load (local.get $e)))
    (i32.store offset=4 (local.get $e) (i32.const 0))
    (i32.const 0))

  ;; One read-via-stream per iovec at the slot's offset: the first non-empty
  ;; copy answers (a COMPLETED copy of 0 items is not the end, #2955); an
  ;; ended stream with nothing read asks the completion future whether it
  ;; ended in an error. A short read ends the call.
  (func $fd_read (param $fd i32) (param $iovs i32) (param $n i32) (param $out i32) (result i32)
    (local $e i32) (local $r i32) (local $k i32) (local $p i32) (local $l i32) (local $rx i32) (local $fut i32)
    (local $w i32) (local $c i32) (local $got i32) (local $err i32)
    (local.set $e (call $ad_open_slot (local.get $fd)))
    (if (i32.eqz (local.get $e)) (then (return (i32.const 8))))
    (local.set $r (call $ad_ret))
    (block $done (loop $iov
      (br_if $done (i32.ge_u (local.get $k) (local.get $n)))
      (local.set $p (i32.load (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
      (local.set $l (i32.load offset=4 (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
      (call $p3_rvs (i32.load (local.get $e)) (i64.load offset=8 (local.get $e)) (local.get $r))
      (local.set $rx (i32.load (local.get $r)))
      (local.set $fut (i32.load offset=4 (local.get $r)))
      (local.set $c (i32.const 0))
      (block $rd (loop $again
        (br_if $rd (i32.eqz (local.get $l)))
        (local.set $w (call $p3_await (local.get $rx) (call $p3_rvs_read (local.get $rx) (local.get $p) (local.get $l))))
        (local.set $c (i32.shr_u (local.get $w) (i32.const 4)))
        (br_if $rd (local.get $c))
        (br_if $rd (i32.and (local.get $w) (i32.const 15)))
        (br $again)))
      (call $p3_rvs_drop (local.get $rx))
      (if (i32.and (i32.eqz (local.get $c)) (i32.ne (local.get $l) (i32.const 0)))
        (then
          (drop (call $p3_await (local.get $fut) (call $p3_rvs_fut_read (local.get $fut) (local.get $r))))
          (local.set $err (call $ad_status (local.get $r) (i32.const @UNIT_PAY@)))))
      (call $p3_rvs_fut_drop (local.get $fut))
      (br_if $done (local.get $err))
      (i64.store offset=8 (local.get $e) (i64.add (i64.load offset=8 (local.get $e)) (i64.extend_i32_u (local.get $c))))
      (local.set $got (i32.add (local.get $got) (local.get $c)))
      (br_if $done (i32.lt_u (local.get $c) (local.get $l)))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))
      (br $iov)))
    (if (local.get $err) (then (return (local.get $err))))
    (i32.store (local.get $out) (local.get $got))
    (i32.const 0))

  ;; stdout / stderr (the service's C-197 line) go through the shim's raw
  ;; writers, ops 30 / 73. A file: one write-via-stream (append-via-stream
  ;; for an APPEND slot) per iovec at the slot's offset, the writable end
  ;; dropped and the completion future read — the durability handshake the
  ;; shim's own writes use — before the count is answered.
  (func $fd_write (param $fd i32) (param $iovs i32) (param $n i32) (param $out i32) (result i32)
    (local $e i32) (local $r i32) (local $k i32) (local $p i32) (local $l i32) (local $s i64) (local $tx i32)
    (local $fut i32) (local $w i32) (local $c i32) (local $put i32) (local $got i32) (local $err i32)
    (if (i32.or (i32.eq (local.get $fd) (i32.const 1)) (i32.eq (local.get $fd) (i32.const 2)))
      (then
        (block $sd (loop $sk
          (br_if $sd (i32.ge_u (local.get $k) (local.get $n)))
          (local.set $p (i32.load (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
          (local.set $l (i32.load offset=4 (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
          (drop (call $p3_fs_call (select (i32.const 30) (i32.const 73) (i32.eq (local.get $fd) (i32.const 1)))
                                  (i32.const 0) (i32.const 0) (local.get $p) (local.get $l)))
          (local.set $got (i32.add (local.get $got) (local.get $l)))
          (local.set $k (i32.add (local.get $k) (i32.const 1)))
          (br $sk)))
        (i32.store (local.get $out) (local.get $got))
        (return (i32.const 0))))
    (local.set $e (call $ad_open_slot (local.get $fd)))
    (if (i32.eqz (local.get $e)) (then (return (i32.const 8))))
    (local.set $r (call $ad_ret))
    (block $done (loop $iov
      (br_if $done (i32.ge_u (local.get $k) (local.get $n)))
      (local.set $p (i32.load (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
      (local.set $l (i32.load offset=4 (i32.add (local.get $iovs) (i32.shl (local.get $k) (i32.const 3)))))
      ;; tx = high half, rx = low half (the shim's packing)
      (local.set $s (call $p3_wvs_new))
      (local.set $tx (i32.wrap_i64 (i64.shr_u (local.get $s) (i64.const 32))))
      (if (i32.eq (i32.load offset=4 (local.get $e)) (i32.const 2))
        (then (local.set $fut (call $p3_avs (i32.load (local.get $e)) (i32.wrap_i64 (local.get $s)))))
        (else (local.set $fut (call $p3_wvs (i32.load (local.get $e)) (i32.wrap_i64 (local.get $s))
                                            (i64.load offset=8 (local.get $e))))))
      (local.set $put (i32.const 0))
      (block $wd (loop $wr
        (br_if $wd (i32.ge_u (local.get $put) (local.get $l)))
        (local.set $w (call $p3_await (local.get $tx)
                            (call $p3_wvs_write (local.get $tx) (i32.add (local.get $p) (local.get $put))
                                                (i32.sub (local.get $l) (local.get $put)))))
        (local.set $put (i32.add (local.get $put) (i32.shr_u (local.get $w) (i32.const 4))))
        (br_if $wd (i32.and (local.get $w) (i32.const 15)))
        (br $wr)))
      (call $p3_wvs_drop (local.get $tx))
      (drop (call $p3_await (local.get $fut) (call $p3_wvs_fut_read (local.get $fut) (local.get $r))))
      (local.set $err (call $ad_status (local.get $r) (i32.const @UNIT_PAY@)))
      (i64.store offset=8 (local.get $e) (i64.add (i64.load offset=8 (local.get $e)) (i64.extend_i32_u (local.get $put))))
      (local.set $got (i32.add (local.get $got) (local.get $put)))
      (br_if $done (local.get $err))
      (br_if $done (i32.lt_u (local.get $put) (local.get $l)))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))
      (br $iov)))
    (if (local.get $err) (then (return (local.get $err))))
    (i32.store (local.get $out) (local.get $got))
    (i32.const 0))

  (func $fd_filestat_get (param $fd i32) (param $buf i32) (result i32)
    (local $h i32) (local $r i32) (local $e i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (local.set $r (call $ad_ret))
    (call $p3_stat (local.get $h) (local.get $r))
    (local.set $e (call $ad_status (local.get $r) (i32.const @STAT_PAY@)))
    (if (local.get $e) (then (return (local.get $e))))
    (call $ad_filestat (i32.add (local.get $r) (i32.const @STAT_PAY@)) (local.get $buf))
    (i32.const 0))

  (func $path_filestat_get (param $fd i32) (param $flags i32) (param $p i32) (param $l i32) (param $buf i32) (result i32)
    (local $h i32) (local $r i32) (local $e i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (local.set $r (call $ad_ret))
    (call $p3_stat_at (local.get $h) (i32.and (local.get $flags) (i32.const 1)) (local.get $p) (local.get $l) (local.get $r))
    (local.set $e (call $ad_status (local.get $r) (i32.const @STAT_PAY@)))
    (if (local.get $e) (then (return (local.get $e))))
    (call $ad_filestat (i32.add (local.get $r) (i32.const @STAT_PAY@)) (local.get $buf))
    (i32.const 0))

  ;; The directory's entries from `cookie` on as preview-1 dirents (d_next u64
  ;; = the next entry's index, d_ino 0, d_namlen u32, d_type u8, then the
  ;; name), the last one truncated when the buffer ends inside it — a fresh
  ;; read-directory stream per call, the first `cookie` entries skipped.
  (func $fd_readdir (param $fd i32) (param $buf i32) (param $len i32) (param $cookie i64) (param $out i32) (result i32)
    (local $h i32) (local $r i32) (local $rx i32) (local $fut i32) (local $ents i32) (local $hdr i32)
    (local $w i32) (local $c i32) (local $j i32) (local $ent i32) (local $nl i32) (local $idx i64)
    (local $used i32) (local $take i32) (local $err i32) (local $full i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (local.set $r (call $ad_ret))
    (call $p3_readdir (local.get $h) (local.get $r))
    (local.set $rx (i32.load (local.get $r)))
    (local.set $fut (i32.load offset=4 (local.get $r)))
    (local.set $ents (call $alloc (i32.mul (i32.const 64) (i32.const @DE_SIZE@))))
    (local.set $hdr (call $alloc (i32.const 24)))
    (block $end (loop $batch
      ;; the names land through cabi_realloc: a declared ceiling per batch
      (call $hs_pre)
      (call $p3_reserve (i32.const 65536))
      (local.set $w (call $p3_await (local.get $rx) (call $p3_readdir_read (local.get $rx) (local.get $ents) (i32.const 64))))
      (call $hs_post)
      (local.set $c (i32.shr_u (local.get $w) (i32.const 4)))
      (local.set $j (i32.const 0))
      (block $bd (loop $each
        (br_if $bd (i32.ge_u (local.get $j) (local.get $c)))
        (local.set $ent (i32.add (local.get $ents) (i32.mul (local.get $j) (i32.const @DE_SIZE@))))
        (if (i64.ge_u (local.get $idx) (local.get $cookie))
          (then
            (local.set $nl (i32.load offset=@DE_NAME_LEN@ (local.get $ent)))
            (i64.store (local.get $hdr) (i64.add (local.get $idx) (i64.const 1)))
            (i64.store offset=8 (local.get $hdr) (i64.const 0))
            (i32.store offset=16 (local.get $hdr) (local.get $nl))
            (i32.store offset=20 (local.get $hdr) (call $p3_ftype (i32.load8_u offset=@DE_TYPE@ (local.get $ent))))
            (local.set $take (select (i32.const 24) (i32.sub (local.get $len) (local.get $used))
                                     (i32.le_u (i32.const 24) (i32.sub (local.get $len) (local.get $used)))))
            (memory.copy (i32.add (local.get $buf) (local.get $used)) (local.get $hdr) (local.get $take))
            (local.set $used (i32.add (local.get $used) (local.get $take)))
            (local.set $take (select (local.get $nl) (i32.sub (local.get $len) (local.get $used))
                                     (i32.le_u (local.get $nl) (i32.sub (local.get $len) (local.get $used)))))
            (memory.copy (i32.add (local.get $buf) (local.get $used)) (i32.load offset=@DE_NAME@ (local.get $ent)) (local.get $take))
            (local.set $used (i32.add (local.get $used) (local.get $take)))
            (if (i32.ge_u (local.get $used) (local.get $len))
              (then (local.set $full (i32.const 1)) (br $end)))))
        (local.set $idx (i64.add (local.get $idx) (i64.const 1)))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $each)))
      (br_if $end (i32.and (local.get $w) (i32.const 15)))
      (br $batch)))
    (call $p3_readdir_drop (local.get $rx))
    (if (i32.eqz (local.get $full))
      (then
        (drop (call $p3_await (local.get $fut) (call $p3_readdir_fut_read (local.get $fut) (local.get $r))))
        (local.set $err (call $ad_status (local.get $r) (i32.const @UNIT_PAY@)))))
    (call $p3_readdir_fut_drop (local.get $fut))
    (if (local.get $err) (then (return (local.get $err))))
    (i32.store (local.get $out) (local.get $used))
    (i32.const 0))

  (func $path_create_directory (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $h i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (call $p3_mkdir (local.get $h) (local.get $p) (local.get $l) (call $ad_ret))
    (call $ad_status (call $ad_ret) (i32.const @UNIT_PAY@)))

  (func $path_remove_directory (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $h i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (call $p3_rmdir (local.get $h) (local.get $p) (local.get $l) (call $ad_ret))
    (call $ad_status (call $ad_ret) (i32.const @UNIT_PAY@)))

  (func $path_unlink_file (param $fd i32) (param $p i32) (param $l i32) (result i32)
    (local $h i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i32.const 8))))
    (call $p3_unlink (local.get $h) (local.get $p) (local.get $l) (call $ad_ret))
    (call $ad_status (call $ad_ret) (i32.const @UNIT_PAY@)))

  (func $path_rename (param $fd i32) (param $p i32) (param $l i32) (param $fd2 i32) (param $p2 i32) (param $l2 i32) (result i32)
    (local $h i32) (local $h2 i32)
    (local.set $h (call $ad_handle (local.get $fd)))
    (local.set $h2 (call $ad_handle (local.get $fd2)))
    (if (i32.or (i32.lt_s (local.get $h) (i32.const 0)) (i32.lt_s (local.get $h2) (i32.const 0)))
      (then (return (i32.const 8))))
    (call $p3_rename (local.get $h) (local.get $p) (local.get $l) (local.get $h2) (local.get $p2) (local.get $l2) (call $ad_ret))
    (call $ad_status (call $ad_ret) (i32.const @UNIT_PAY@)))

  ;; The environment list, fetched once — the SAME cache the env service
  ;; (op 26) keeps, so a program that reads both lowers it once.
  (func $ad_env
    (local $r i32)
    (if (i32.ge_s (global.get $g_env) (i32.const 0)) (then (return)))
    (local.set $r (call $ad_ret))
    (call $hs_pre)
    (call $p3_reserve (i32.const @ENV_RESERVE@))
    (call $p3_get_environment (local.get $r))
    (call $hs_post)
    (global.set $g_env (i32.load (local.get $r)))
    (global.set $g_envn (i32.load offset=4 (local.get $r))))

  ;; entries are tuple<string, string>: name ptr/len at 0/4, value at 8/12
  (func $environ_sizes_get (param $cnt i32) (param $size i32) (result i32)
    (local $i i32) (local $e i32) (local $t i32)
    (call $ad_env)
    (block $d (loop $c
      (br_if $d (i32.ge_u (local.get $i) (global.get $g_envn)))
      (local.set $e (i32.add (global.get $g_env) (i32.shl (local.get $i) (i32.const 4))))
      (local.set $t (i32.add (local.get $t) (i32.add (i32.add (i32.load offset=4 (local.get $e)) (i32.load offset=12 (local.get $e)))
                                                     (i32.const 2))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (i32.store (local.get $cnt) (global.get $g_envn))
    (i32.store (local.get $size) (local.get $t))
    (i32.const 0))

  ;; "name=value\0" per entry into buf, its address into arr
  (func $environ_get (param $arr i32) (param $buf i32) (result i32)
    (local $i i32) (local $e i32) (local $w i32)
    (call $ad_env)
    (local.set $w (local.get $buf))
    (block $d (loop $c
      (br_if $d (i32.ge_u (local.get $i) (global.get $g_envn)))
      (local.set $e (i32.add (global.get $g_env) (i32.shl (local.get $i) (i32.const 4))))
      (i32.store (i32.add (local.get $arr) (i32.shl (local.get $i) (i32.const 2))) (local.get $w))
      (local.set $w (call $put (local.get $w) (i32.load (local.get $e)) (i32.load offset=4 (local.get $e))))
      (i32.store8 (local.get $w) (i32.const 61))
      (local.set $w (call $put (i32.add (local.get $w) (i32.const 1)) (i32.load offset=8 (local.get $e)) (i32.load offset=12 (local.get $e))))
      (i32.store8 (local.get $w) (i32.const 0))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $c)))
    (i32.const 0))

  ;; the wall clock in nanoseconds (instant: seconds s64 at 0, nanoseconds u32 at 8)
  (func $clock_time_get (param $id i32) (param $prec i64) (param $out i32) (result i32)
    (local $r i32)
    (local.set $r (call $ad_ret))
    (call $p3_clock_now (local.get $r))
    (i64.store (local.get $out) (i64.add (i64.mul (i64.load (local.get $r)) (i64.const 1000000000))
                                         (i64.extend_i32_u (i32.load offset=8 (local.get $r)))))
    (i32.const 0))

  (func $proc_exit (param $code i32)
    (call $p3_exit (i32.ne (local.get $code) (i32.const 0)))
    (unreachable))

  ;; ── the fan prefetch's entries into the service (shim ops 40 / 41) ─────

  ;; pseudo-op -1: the p3 descriptor a guest path resolves to (or -1), with
  ;; the relative remainder published in ppos / plen. A remainder outside the
  ;; guest's own string is copied to the heap: the async open the shim issues
  ;; with it outlives this call's arena.
  (func $p3_resolve_op (param $a i32) (param $al i32) (result i64)
    (local $fd i32) (local $rp i32) (local $rl i32) (local $h i32) (local $q i32)
    (call $resolve (local.get $a) (local.get $al)) (local.set $rl) (local.set $rp) (local.set $fd)
    (local.set $h (call $ad_handle (local.get $fd)))
    (if (i32.lt_s (local.get $h) (i32.const 0)) (then (return (i64.const -1))))
    (if (i32.or (i32.lt_u (local.get $rp) (local.get $a))
                (i32.gt_u (i32.add (local.get $rp) (local.get $rl)) (i32.add (local.get $a) (local.get $al))))
      (then
        (call $hs_pre)
        (local.set $q (call $p3_alloc (i32.const 0) (i32.const 0) (i32.const 8) (local.get $rl)))
        (call $hs_post)
        (memory.copy (local.get $q) (local.get $rp) (local.get $rl))
        (local.set $rp (local.get $q))))
    (global.set $ppos (local.get $rp))
    (global.set $plen (local.get $rl))
    (i64.extend_i32_u (local.get $h)))

  ;; pseudo-op -2: whether (a, al) is UTF-8 (the read_text acceptance).
  (func $p3_utf8_op (param $a i32) (param $al i32) (result i64)
    (i64.extend_i32_u (call $utf8_ok (local.get $a) (local.get $al))))

  ;; pseudo-op -3: whether descriptor `a` can be streamed — 1 only for a
  ;; regular file. read-via-stream on a directory is no error RESULT on a
  ;; stock wasmtime but a trap (`ErrorCode::BadDescriptor`), so the fan
  ;; await asks first and leaves anything else to the sync read.
  (func $p3_streamable_op (param $a i32) (result i64)
    (local $r i32)
    (local.set $r (call $ad_ret))
    (call $p3_stat (local.get $a) (local.get $r))
    (if (i32.load8_u (local.get $r)) (then (return (i64.const 0))))
    (i64.extend_i32_u (i32.eq (call $p3_ftype (i32.load8_u offset=@STAT_TYPE_AT@ (local.get $r))) (i32.const 4))))
