/// The WASI PREOPEN RESOLUTION tail of [`preamble_wasi_fs_wat`]'s `$path_norm`
/// (#1394) — the single point at which a guest path acquires the dirfd it will
/// actually be opened against.
///
/// ## The bug this replaces
///
/// Every WASI path call in the v1 floor used to pass `(i32.const 3)` as the
/// dirfd — 13 literals across `path_open` / `path_filestat_get` /
/// `path_create_directory` / `path_rename` / `path_unlink_file` /
/// `path_remove_directory` — and `$path_norm` made a guest-absolute path
/// "fd-3-relative" by stripping its leading `/`. That is correct only when the
/// host preopens EXACTLY ONE directory and that directory is `/`, which is
/// precisely what `wasmtime --dir=/` gives us, so every test passed. Under a
/// host that preopens more, fd 3 is whichever directory came FIRST, and the
/// stripped path resolves against it: loudly wrong when the stripped path does
/// not exist there, SILENTLY wrong (an `ok(())` write into a different host
/// file) when it does. `src/cli/run.rs::wasmtime_fs_args` already passes two
/// preopens on Windows (`--dir=.` plus the host temp dir mapped at `/tmp`), so
/// the second cell is reachable, not hypothetical.
///
/// ## The resolution rule
///
/// 1. The caller has already made the path guest-ABSOLUTE where it could (an
///    absolute path is used as-is; a relative one is joined onto `ALMIDE_CWD` /
///    `PWD`). This code never rewrites the path — it only splits it.
/// 2. Scan fds from 3 upward with `fd_prestat_get`; preopens are contiguous, so
///    the first non-preopen fd ends the table. Each directory preopen's guest
///    name comes from `fd_prestat_dir_name`; trailing `/` and NUL padding are
///    trimmed so `/tmp` and `/tmp/` name the same thing.
/// 3. Pick the preopen whose name is the LONGEST prefix of the path at a `/`
///    COMPONENT boundary; the remainder after that boundary is the relative
///    path handed to the syscall. Three shapes:
///      - `/`  prefixes every guest-absolute path (remainder = `path[1..]`);
///      - `.`  claims a path we could NOT make absolute (remainder = the path,
///             unchanged) — this is wasmtime's `--dir=.`, i.e. the Windows
///             launcher-cwd preopen;
///      - `N`  matches when `path` starts with `N` AND `path[len(N)] == '/'`,
///             so `/tmpfoo` never resolves against the `/tmp` preopen.
///    Ties keep the first match; longest wins otherwise.
/// 4. NO preopen claimed it → fd 3 with one leading `/` stripped: the exact
///    pre-#1394 transform, so an unmatched shape can never behave WORSE than it
///    did before.
///
/// This is the core of wasi-libc's `__wasilibc_find_relpath` — a cached preopen
/// table plus longest-prefix selection — deliberately WITHOUT its lexical
/// `.`/`..` folding and its symlink-expansion retry. Those change which host
/// file a path names; the rule above only decides which preopen a path already
/// names belongs to, which is the whole of the bug. `/tmp/./x` is still handed
/// to the host verbatim (the host normalizes it, exactly as native `std::fs`
/// does — C-042's fixture pins that equivalence).
///
/// ## Why it is spliced instead of being its own `(func $…)`
///
/// §4.1 (`handwritten_wasm_runtime_does_not_grow`) closes the hand-written WAT
/// surface: the WASI floor is a fixed set of functions, and this is a new step
/// in an existing one, not a new one. Splicing it into `$path_norm` keeps the
/// function count flat AND keeps the property the fix is for — all 13 call
/// sites resolve through ONE piece of code, which is what stops them drifting
/// apart the way the write-side errno tables did (#1385).
///
/// Reads `$pdata`/`$plen` (the guest path); RETURNS the `(dirfd, ptr, len)`
/// triple that is `$path_norm`'s whole result. Clobbers the scratch locals
/// `$tab $pre $rec $n $fd $nameptr $namelen $k $ok $pi $rem $remlen $best_fd
/// $best_len $brem $bremlen`, all declared by `$path_norm`.
pub(crate) fn preopen_resolve_wat() -> String {
    format!(
        r#"    ;; ── PREOPEN RESOLUTION (#1394) ───────────────────────────────────────
    ;; Build the preopen table ONCE (see the $preopen_tab global for why it is
    ;; cached and why the pointer is published last). 32 records max — a bound,
    ;; not a limit anyone reaches: wasmtime passes 1 preopen on Unix and 2 on
    ;; Windows.
    (if (i32.eqz (global.get $preopen_tab))
      (then
        (local.set $tab (call $alloc8 (i32.const 384)))
        (local.set $pre (call $alloc8 (i32.const 8)))
        (local.set $n (i32.const 0))
        (local.set $fd (i32.const 3))
        (block $ptdone (loop $ptloop
          (br_if $ptdone (i32.ge_u (local.get $n) (i32.const 32)))
          ;; preopens are CONTIGUOUS from fd 3 — the first EBADF ends the table.
          (br_if $ptdone (i32.ne (call $fd_prestat_get (local.get $fd) (local.get $pre))
                                 (i32.const 0)))
          ;; prestat: tag@0 (0 = __WASI_PREOPENTYPE_DIR), name byte length @4.
          (if (i32.eqz (i32.load8_u (local.get $pre)))
            (then
              (local.set $namelen (i32.load (i32.add (local.get $pre) (i32.const 4))))
              (local.set $nameptr (call $alloc8 (i32.add (local.get $namelen) (i32.const 1))))
              (if (i32.eqz (call $fd_prestat_dir_name (local.get $fd) (local.get $nameptr)
                                                      (local.get $namelen)))
                (then
                  ;; Trim trailing '/' and NUL padding: "/tmp" and "/tmp/" are the
                  ;; same preopen, and a host whose pr_name_len COUNTS the
                  ;; terminator must not push the boundary test off by one. "/"
                  ;; keeps its single byte.
                  (block $trdone (loop $trloop
                    (br_if $trdone (i32.le_u (local.get $namelen) (i32.const 1)))
                    (local.set $k (i32.load8_u (i32.add (local.get $nameptr)
                                                        (i32.sub (local.get $namelen) (i32.const 1)))))
                    (br_if $trdone (i32.and (i32.ne (local.get $k) (i32.const {ASCII_SLASH}))
                                            (i32.ne (local.get $k) (i32.const 0))))
                    (local.set $namelen (i32.sub (local.get $namelen) (i32.const 1)))
                    (br $trloop)))
                  (local.set $rec (i32.add (local.get $tab) (i32.mul (local.get $n) (i32.const 12))))
                  (i32.store (local.get $rec) (local.get $fd))
                  (i32.store (i32.add (local.get $rec) (i32.const 4)) (local.get $nameptr))
                  (i32.store (i32.add (local.get $rec) (i32.const 8)) (local.get $namelen))
                  (local.set $n (i32.add (local.get $n) (i32.const 1)))))))
          (local.set $fd (i32.add (local.get $fd) (i32.const 1)))
          (br $ptloop)))
        (global.set $preopen_cnt (local.get $n))
        (global.set $preopen_tab (local.get $tab))))
    ;; LONGEST-PREFIX MATCH over the table.
    (local.set $tab (global.get $preopen_tab))
    (local.set $best_fd (i32.const -1))
    (local.set $best_len (i32.const 0))
    (local.set $pi (i32.const 0))
    (block $mdone (loop $mloop
      (br_if $mdone (i32.ge_u (local.get $pi) (global.get $preopen_cnt)))
      (local.set $rec (i32.add (local.get $tab) (i32.mul (local.get $pi) (i32.const 12))))
      (local.set $fd (i32.load (local.get $rec)))
      (local.set $nameptr (i32.load (i32.add (local.get $rec) (i32.const 4))))
      (local.set $namelen (i32.load (i32.add (local.get $rec) (i32.const 8))))
      (local.set $ok (i32.const 0))
      (if (i32.and (i32.eq (local.get $namelen) (i32.const 1))
                   (i32.eq (i32.load8_u (local.get $nameptr)) (i32.const {ASCII_SLASH})))
        (then
          ;; the ROOT preopen prefixes every guest-absolute path.
          (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                       (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
            (then
              (local.set $ok (i32.const 1))
              (local.set $rem (i32.add (local.get $pdata) (i32.const 1)))
              (local.set $remlen (i32.sub (local.get $plen) (i32.const 1))))))
        (else
          (if (i32.and (i32.eq (local.get $namelen) (i32.const 1))
                       (i32.eq (i32.load8_u (local.get $nameptr)) (i32.const {ASCII_DOT})))
            (then
              ;; the "." (launcher-cwd) preopen owns paths we could NOT make absolute.
              (if (i32.or (i32.eqz (local.get $plen))
                          (i32.ne (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
                (then
                  (local.set $ok (i32.const 1))
                  (local.set $rem (local.get $pdata))
                  (local.set $remlen (local.get $plen)))))
            (else
              ;; a NAMED preopen matches only at a '/' component boundary.
              (if (i32.gt_u (local.get $plen) (local.get $namelen))
                (then
                  (if (i32.eq (i32.load8_u (i32.add (local.get $pdata) (local.get $namelen)))
                              (i32.const {ASCII_SLASH}))
                    (then
                      (local.set $ok (i32.const 1))
                      (local.set $k (i32.const 0))
                      (block $mcdone (loop $mcloop
                        (br_if $mcdone (i32.ge_u (local.get $k) (local.get $namelen)))
                        (if (i32.ne (i32.load8_u (i32.add (local.get $pdata) (local.get $k)))
                                    (i32.load8_u (i32.add (local.get $nameptr) (local.get $k))))
                          (then (local.set $ok (i32.const 0)) (br $mcdone)))
                        (local.set $k (i32.add (local.get $k) (i32.const 1)))
                        (br $mcloop)))
                      (if (local.get $ok)
                        (then
                          (local.set $rem (i32.add (i32.add (local.get $pdata) (local.get $namelen))
                                                   (i32.const 1)))
                          (local.set $remlen (i32.sub (i32.sub (local.get $plen) (local.get $namelen))
                                                      (i32.const 1)))))))))))))
      (if (i32.and (local.get $ok)
                   (i32.or (i32.lt_s (local.get $best_fd) (i32.const 0))
                           (i32.gt_u (local.get $namelen) (local.get $best_len))))
        (then
          (local.set $best_fd (local.get $fd))
          (local.set $best_len (local.get $namelen))
          (local.set $brem (local.get $rem))
          (local.set $bremlen (local.get $remlen))))
      (local.set $pi (i32.add (local.get $pi) (i32.const 1)))
      (br $mloop)))
    (if (i32.ge_s (local.get $best_fd) (i32.const 0))
      (then (return (local.get $best_fd) (local.get $brem) (local.get $bremlen))))
    ;; NO preopen claimed the path (a host that preopened nothing, or a name
    ;; shape this rule does not match): the pre-#1394 transform VERBATIM — fd 3,
    ;; one leading '/' stripped.
    (if (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                 (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH})))
      (then (return (i32.const 3) (i32.add (local.get $pdata) (i32.const 1))
                    (i32.sub (local.get $plen) (i32.const 1)))))
    (i32.const 3) (local.get $pdata) (local.get $plen))
"#
    )
}
