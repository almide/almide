/// The WASI FILESYSTEM slice of the fixed WAT preamble — the byte-for-byte
/// continuation of [`preamble_with_bump_base`]'s template: `$path_norm` (host-cwd
/// path normalization), `fs.read_text` / `fs.write` / `fs.list_dir` / `fs.mkdir_p`
/// / `fs.remove_all` and their dirent helpers. Split out of `render_wasm_p3.rs` at
/// the fs-section boundary (max-lines, #852): part 1 ends with the `$env_get`
/// runtime, this slice supplies everything after it, and the concatenation
/// reproduces the original template byte-identically.
pub(crate) fn preamble_wasi_fs_wat() -> String {
    // Byte-length → ELEMENT-count rounding for a canonical String's `cap` field:
    // `cap = ((len + ELEM_SIZE-1) & -ELEM_SIZE) >> log2(ELEM_SIZE)`. Derived from
    // ELEM_SIZE here (not spelled 7/-8/3) so the allocator's size invariant
    // `block_bytes == LIST_HEADER + cap*ELEM_SIZE` cannot drift if ELEM_SIZE changes —
    // the same three constants the renderer's `Init::DynStr` path computes (#892).
    let elem_round_add = ELEM_SIZE - 1;
    let elem_round_mask = -(ELEM_SIZE as i32);
    let elem_shift = ELEM_SIZE.trailing_zeros();
    // The errno → native-Display mapping, once per fs floor, each keeping its OWN
    // pre-#1385 string as the unmapped-errno fallback (see `fs_errno_msg_wat`).
    let rtf_errno_map =
        fs_errno_msg_wat("        ", RTF_NOTFOUND_ADDR, RTF_NOTFOUND_LEN, "file not found");
    let write_errno_map =
        fs_errno_msg_wat("        ", WRITE_ERR_ADDR, WRITE_ERR_LEN, "write failed");
    let write_fd_errno_map =
        fs_errno_msg_wat("            ", WRITE_ERR_ADDR, WRITE_ERR_LEN, "write failed");
    let mkdir_errno_map =
        fs_errno_msg_wat("        ", MKDIR_ERR_ADDR, MKDIR_ERR_LEN, "mkdir failed");
    let remove_errno_map =
        fs_errno_msg_wat("        ", REMOVE_ERR_ADDR, REMOVE_ERR_LEN, "remove failed");
    let rdir_errno_map =
        fs_errno_msg_wat("        ", RDIR_ERR_ADDR, RDIR_ERR_LEN, "directory not found");
    let rdir_rd_errno_map =
        fs_errno_msg_wat("            ", RDIR_ERR_ADDR, RDIR_ERR_LEN, "directory not found");
    let rename_errno_map =
        fs_errno_msg_wat("        ", WRITE_ERR_ADDR, WRITE_ERR_LEN, "write failed");
    // The ONE preopen → dirfd resolution step, spliced into `$path_norm`'s tail
    // (#1394). All 13 WASI path-call sites take their dirfd from its result, so
    // the rule has exactly one source.
    let preopen_resolve = preopen_resolve_wat();
    let utf8_validate = utf8_validate_wat("        ");
    format!(
        r#"  ;; fs.read_text(path) — open the file at $path and read its bytes, returning a fresh
  ;; OWNED `Result[String, String]` in the EXACT `materialize_result_str` cap-as-tag
  ;; layout: a 1-slot DynListStr `[rc][len@4=1][cap@8=1][@12 String handle][@16 tag]`
  ;; (tag 0 = Ok, 1 = Err), so the caller's `!`/`match`/`DropListStr` machinery handles
  ;; it identically to a self-host-built Result. $path is a borrowed canonical String
  ;; `[rc][len@4][cap@8][bytes@12…]`. WASI floor: `path_open` (relative to the preopen
  ;; dirfd `$path_norm` resolved the path to, with the matched prefix removed — #1394)
  ;; gives a file fd; `fd_filestat_get` its byte size;
  ;; `fd_read` the bytes; we copy them into a canonical String and wrap it Ok. On a
  ;; path_open error we wrap the message "file not found" Err. The FOURTH sandbox exit
  ;; (Capability::FsRead) — the result is an owned heap handle the caller's scope-end
  ;; DropListStr balances (frees the @12 payload String + the block).
  ;; Normalize a fs path for the WASI floor, then RESOLVE it to the preopen it
  ;; actually belongs to. Two steps, one function:
  ;;   1. NORMALIZE — an ABSOLUTE path is already the guest path. A RELATIVE one
  ;;      is joined onto the HOST CWD ("$PWD/…"; PWD arrives via the harness's
  ;;      `-S inherit-env=y`, ALMIDE_CWD wins over it — #874). WASI itself has no
  ;;      cwd, so an unjoined relative path would resolve against the preopen ROOT
  ;;      and every relative fs op would silently diverge from native (the
  ;;      fs_stat_test vein). No usable PWD (absent, empty, or not absolute) → the
  ;;      bytes pass through unchanged (the pre-fix behavior).
  ;;   2. RESOLVE — walk the WASI preopen table and pick the LONGEST-PREFIX match
  ;;      (see `preopen_resolve_wat`, #1394). The dirfd used to be a hard-coded 3
  ;;      at all 13 path-call sites, which is right only under a host that
  ;;      preopens exactly one directory and that directory is `/`.
  ;; Returns (dirfd, pdata, plen) — the fd the path is relative TO, and its
  ;; remainder byte range (multi-value). EVERY fs floor fn resolves through here,
  ;; so there is exactly ONE copy of the rule to keep right.
  (func $path_norm (param $path i32) (result i32 i32 i32)
    (local $pdata i32) (local $plen i32)
    (local $cnt_ptr i32) (local $sz_ptr i32) (local $cnt i32) (local $bufsz i32)
    (local $envp i32) (local $envbuf i32) (local $i i32) (local $entry i32)
    (local $pwd i32) (local $pwdlen i32) (local $buf i32) (local $j i32) (local $w i32)
    ;; preopen-resolution scratch (#1394)
    (local $tab i32) (local $pre i32) (local $rec i32) (local $n i32) (local $fd i32)
    (local $nameptr i32) (local $namelen i32) (local $k i32) (local $ok i32) (local $pi i32)
    (local $rem i32) (local $remlen i32)
    (local $best_fd i32) (local $best_len i32) (local $brem i32) (local $bremlen i32)
    (local.set $pdata (i32.add (local.get $path) (i32.const {LIST_HEADER})))
    (local.set $plen (i32.load (i32.add (local.get $path) (i32.const {LIST_LEN_OFFSET}))))
    (block $normed
    ;; ABSOLUTE — this IS the guest path; fall straight through to resolution.
    (br_if $normed (i32.and (i32.gt_u (local.get $plen) (i32.const 0))
                            (i32.eq (i32.load8_u (local.get $pdata)) (i32.const {ASCII_SLASH}))))
    ;; relative — snapshot the environ (the SAME lazy init as $env_get)
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
    ;; scan for "ALMIDE_CWD=" first — the launcher's REAL cwd (#874): the
    ;; inherited PWD is STALE when a parent set the child cwd without updating
    ;; it (Node execFileSync cwd option, IDE run configs). `almide run/test` pins it
    ;; via `wasmtime --env`; without the pin the fallback PWD scan below keeps
    ;; the old behavior. The 11-byte key match is two overlapping loads:
    ;; bytes 0..8 "ALMIDE_C" (LE i64) and bytes 7..11 "CWD=" (LE i32).
    (local.set $pwd (i32.const 0))
    (local.set $i (i32.const 0))
    (block $adone (loop $aloop
      (br_if $adone (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $entry (i32.load (i32.add (local.get $envp) (i32.mul (local.get $i) (i32.const 4)))))
      (if (i32.and
            (i64.eq (i64.load (local.get $entry)) (i64.const 0x435F4544494D4C41))
            (i32.eq (i32.load (i32.add (local.get $entry) (i32.const 7))) (i32.const 0x3D445743)))
        (then
          (local.set $pwd (i32.add (local.get $entry) (i32.const 11)))
          (local.set $pwdlen (i32.const 0))
          (block $a_sdone (loop $a_sloop
            (br_if $a_sdone (i32.eqz (i32.load8_u (i32.add (local.get $pwd) (local.get $pwdlen)))))
            (local.set $pwdlen (i32.add (local.get $pwdlen) (i32.const 1)))
            (br $a_sloop)))))
      (br_if $adone (i32.ne (local.get $pwd) (i32.const 0)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $aloop)))
    ;; fall back to the "PWD=" entry
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ne (local.get $pwd) (i32.const 0)))
      (br_if $done (i32.ge_u (local.get $i) (local.get $cnt)))
      (local.set $entry (i32.load (i32.add (local.get $envp) (i32.mul (local.get $i) (i32.const 4)))))
      (if (i32.and
            (i32.and (i32.eq (i32.load8_u (local.get $entry)) (i32.const 80))
                     (i32.eq (i32.load8_u (i32.add (local.get $entry) (i32.const 1))) (i32.const 87)))
            (i32.and (i32.eq (i32.load8_u (i32.add (local.get $entry) (i32.const 2))) (i32.const 68))
                     (i32.eq (i32.load8_u (i32.add (local.get $entry) (i32.const 3))) (i32.const 61))))
        (then
          (local.set $pwd (i32.add (local.get $entry) (i32.const 4)))
          (local.set $pwdlen (i32.const 0))
          (block $sdone (loop $sloop
            (br_if $sdone (i32.eqz (i32.load8_u (i32.add (local.get $pwd) (local.get $pwdlen)))))
            (local.set $pwdlen (i32.add (local.get $pwdlen) (i32.const 1)))
            (br $sloop)))))
      (br_if $done (i32.ne (local.get $pwd) (i32.const 0)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $loop)))
    ;; no usable PWD → the path stays relative; a "." preopen (if the host has
    ;; one) claims it below, otherwise the fd-3 fallback reproduces today exactly.
    (br_if $normed (i32.eqz (local.get $pwd)))
    (br_if $normed (i32.eqz (local.get $pwdlen)))
    (br_if $normed (i32.ne (i32.load8_u (local.get $pwd)) (i32.const {ASCII_SLASH})))
    ;; buf = PWD + "/" + path — the GUEST-ABSOLUTE join. The leading '/' is KEPT
    ;; (stripping it is the RESOLVER's job now, not the joiner's), so at most
    ;; pwdlen + 1 + plen bytes.
    (local.set $buf (call $alloc8 (i32.add (i32.add (local.get $pwdlen) (i32.const 1))
                                           (local.get $plen))))
    (local.set $w (i32.const 0))
    (local.set $j (i32.const 0))
    (block $c1 (loop $l1
      (br_if $c1 (i32.ge_u (local.get $j) (local.get $pwdlen)))
      (i32.store8 (i32.add (local.get $buf) (local.get $w))
                  (i32.load8_u (i32.add (local.get $pwd) (local.get $j))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $l1)))
    ;; PWD "/" already ends in the separator — never emit "//".
    (if (i32.ne (i32.load8_u (i32.add (local.get $pwd)
                                      (i32.sub (local.get $pwdlen) (i32.const 1))))
                (i32.const {ASCII_SLASH}))
      (then
        (i32.store8 (i32.add (local.get $buf) (local.get $w)) (i32.const {ASCII_SLASH}))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))))
    (local.set $j (i32.const 0))
    (block $c2 (loop $l2
      (br_if $c2 (i32.ge_u (local.get $j) (local.get $plen)))
      (i32.store8 (i32.add (local.get $buf) (local.get $w))
                  (i32.load8_u (i32.add (local.get $pdata) (local.get $j))))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $l2)))
    (local.set $pdata (local.get $buf))
    (local.set $plen (local.get $w)))
{preopen_resolve}

  (func $read_text_file (param $path i32) (param $validate i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32) (local $fd_out i32) (local $errno i32)
    (local $valid i32) (local $vi i32) (local $vb0 i32) (local $vb1 i32) (local $vw i32) (local $vlo i32) (local $vhi i32) (local $vk i32)
    (local $fd i32) (local $stat i32) (local $fsize i32) (local $iov i32)
    (local $nread i32) (local $data i32) (local $datb i32) (local $str i32) (local $result i32)
    (local $j i32) (local $msg i32) (local $maddr i32) (local $mlen i32)
    ;; dirfd + path bytes + length via $path_norm (the preopen the path belongs to,
    ;; and its remainder relative to that preopen — #1394).
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    ;; The FIXED out-param scratch (stat/fd_out/iov/nread) comes from the ONE-TIME
    ;; cached block ($rtf_scratch — the environ-snapshot principle): $alloc8 scratch
    ;; is immortal by design, so taking it per call leaked ~80 bytes on EVERY fs
    ;; read (the fold_lines churn bisect's linear ceiling; the trace attributed the
    ;; 4/64/8/4-byte quartet block-by-block).
    (if (i32.eqz (global.get $rtf_scratch))
      (then (global.set $rtf_scratch (call $alloc8 (i32.const 88)))))
    ;; path_open(dirfd, dirflags=0, path_ptr, path_len, oflags=0,
    ;;   rights_base = fd_read(2) | fd_seek(4) = 6, rights_inheriting=0, fdflags=0, fd_out)
    (local.set $fd_out (i32.add (global.get $rtf_scratch) (i32.const 64)))
    (local.set $errno
      (call $path_open (local.get $dirfd) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 0) (i64.const 6) (i64.const 0) (i32.const 0) (local.get $fd_out)))
    ;; The READ half runs only when path_open succeeded; it may set $errno itself.
    ;; #1233/#1368 — fd_read's errno used to be DROPPED, and `path_open` SUCCEEDS on a
    ;; DIRECTORY (the failure surfaces at fd_read as ISDIR). Dropping it fell through to
    ;; the Ok arm and built `ok("")` from a zero-length read where native
    ;; (std::fs::read_to_string) returns Err("Is a directory (os error 21)") — a SILENT
    ;; wrong Result branch across the whole read family (read_text / read_lines /
    ;; read_bytes[_raw] and their _if_exists twins, which share this one floor).
    ;; Carrying it into the SHARED mapping below is the errno-carrying read prim C-215
    ;; named. Note the ORDER: fd_close must not clobber the read errno.
    (if (i32.eqz (local.get $errno))
      (then
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; fd_filestat_get → file size (i64 @ stat+32; take the low 32 bits). The stat buffer
        ;; MUST be 8-aligned (the host writes an i64 there) — the scratch base is `$alloc8`-
        ;; aligned and stat sits at offset 0.
        (local.set $stat (global.get $rtf_scratch))
        (drop (call $fd_filestat_get (local.get $fd) (local.get $stat)))
        ;; filetype@16 == 3 is a DIRECTORY. Classify it from the STAT, not from
        ;; fd_read's errno: the errno a host reports for reading a directory fd is
        ;; host-specific (wasmtime says BADF, not ISDIR), while the filetype byte is
        ;; the SAME one fs.is_dir already reads. errno 31 (ISDIR) then renders
        ;; native's exact "Is a directory (os error 21)" through the mapping below.
        (if (i32.eq (i32.load8_u (i32.add (local.get $stat) (i32.const 16))) (i32.const 3))
          (then (local.set $errno (i32.const 31)))
          (else
            (local.set $fsize (i32.load (i32.add (local.get $stat) (i32.const 32))))
            ;; fd_read into a fresh buffer; iov = [buf_ptr, buf_len]. The buffer is a
            ;; CANONICAL free-list block (`$list_new` with the byte capacity rounded to
            ;; whole slots, exactly $rtf_str's rounding — the allocator invariant
            ;; `block_bytes == LIST_HEADER + cap*ELEM_SIZE` holds by construction), so
            ;; the `$rc_dec` at the exit frees it back to the free list: the content
            ;; buffer was the UNBOUNDED half of the per-read leak (filesize bytes per
            ;; call, immortal under $alloc8).
            (local.set $datb (call $list_new (i32.const 0)
              (i32.shr_u (i32.and (i32.add (local.get $fsize) (i32.const 15)) (i32.const -8))
                         (i32.const 3))))
            (local.set $data (i32.add (local.get $datb) (i32.const {LIST_HEADER})))
            (local.set $iov (i32.add (global.get $rtf_scratch) (i32.const 72)))
            (i32.store (local.get $iov) (local.get $data))
            (i32.store (i32.add (local.get $iov) (i32.const 4)) (local.get $fsize))
            (local.set $nread (i32.add (global.get $rtf_scratch) (i32.const 80)))
            (local.set $errno
              (call $fd_read (local.get $fd) (local.get $iov) (i32.const 1) (local.get $nread)))))
        (drop (call $fd_close (local.get $fd)))))
    ;; On a path_open OR fd_read error build Err(<native std::io Display>) — the WASI errno
    ;; maps to the EXACT text native std::fs emits ($fs_errno_msg), so `err(e)` byte-matches.
    (if (i32.ne (local.get $errno) (i32.const 0))
      (then
{rtf_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (local.set $result (call $rtf_result (local.get $msg) (i32.const 1))))
      (else
        ;; the actual byte count read (may be < the stat size) is the String length.
        (local.set $fsize (i32.load (local.get $nread)))
{utf8_validate}        (if (i32.eqz (local.get $valid))
          (then
            ;; #1506 — the text floor refuses invalid UTF-8 exactly like native
            ;; std::fs::read_to_string: Err with its InvalidData message. The bytes floor
            ;; ($validate = 0) never takes this arm.
            (local.set $msg (call $rtf_str (i32.const {FS_ERR_UTF8_ADDR}) (i32.const {FS_ERR_UTF8_LEN})))
            (local.set $result (call $rtf_result (local.get $msg) (i32.const 1))))
          (else
            ;; build the canonical String + copy the bytes, then wrap it Ok.
            (local.set $str (call $rtf_str (local.get $data) (local.get $fsize)))
            (local.set $result (call $rtf_result (local.get $str) (i32.const 0)))))))
    ;; Release the content buffer on EVERY exit that allocated it (the ok arm copied
    ;; into the canonical String; the err arms never read it again). A directory /
    ;; path_open failure never allocated one ($datb stays 0).
    (if (i32.ne (local.get $datb) (i32.const 0))
      (then (call $rc_dec (local.get $datb))))
    (local.get $result))

  ;; helper: copy $len bytes at $src into a fresh canonical String `[rc][len][cap][bytes…]`.
  ;; THE single host-floor String constructor — every WASI exit that turns raw host bytes
  ;; into an Almide String (`$args_get_list`, `$env_get`, `$read_text_file`, `$read_line`,
  ;; `$read_n_bytes`, `$read_dir`, every error-message wrap) builds through here.
  ;;
  ;; #892/#903 — the block MUST satisfy the ALLOCATOR'S SIZE INVARIANT
  ;; `block_bytes == LIST_HEADER + cap*ELEM_SIZE`, because `$alloc`'s free-list reuse test
  ;; RECOMPUTES a freed block's size from its `cap` field (it has no other record of it).
  ;; This helper and the two inline copies in `$args_get_list`/`$env_get` used to allocate
  ;; `LIST_HEADER + len` bytes and store `cap := len` — a BYTE count where the invariant
  ;; wants ELEMENTS. A 1-byte argv String then occupied 13 bytes while advertising
  ;; `12 + 1*8 = 20`, so once it was freed the next `alloc(20)` matched it and handed back
  ;; the 13-byte hole: the new block's header and payload wrote 7 bytes straight INTO the
  ;; live neighbour. No trap, no `$rc_dec` sentinel — silent corruption of whatever String
  ;; sat next to it (`args.option_or("a","-") + args.option_or("b","=")` in one expression
  ;; churns exactly the free/reuse pattern that lands the overlap on a live value).
  ;;
  ;; Building through `$list_new` — the ONE place that establishes the invariant — is what
  ;; keeps it true by construction, and makes a host-floor String byte-identical in shape
  ;; to a renderer-built one (`Init::Str`/`Init::DynStr` round the same way), so the two
  ;; interchange freely on the free list instead of poisoning it.
  (func $rtf_str (param $src i32) (param $len i32) (result i32)
    (local $str i32) (local $j i32)
    (local.set $str (call $list_new (local.get $len)
      (i32.shr_u (i32.and (i32.add (local.get $len) (i32.const {elem_round_add}))
                          (i32.const {elem_round_mask}))
                 (i32.const {elem_shift}))))
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
  ;; (0x400000)=0x400044, against the $path_norm-resolved preopen dirfd — same resolution as
  ;; $read_text_file), writes $content's bytes via fd_write, and closes the fd. Builds a fresh
  ;; OWNED `Result[Unit, String]`: Ok(()) as a 1-slot block with len@4=0 + @12=0 + tag@16=0 (the
  ;; `materialize_result_ok` convention — the scope-end flat $drop_list_str frees nothing at @12),
  ;; or Err(<native std::io Display>) via $rtf_result (len@4=1, @12=msg, tag@16=1) on a path_open
  ;; OR fd_write error — the SAME errno → text mapping every fs floor uses (#1385; before it the
  ;; whole failure space collapsed to "write failed", and fd_write's errno was DROPPED outright,
  ;; so ENOSPC/EIO/a short write read as Ok). The write is a write_all LOOP, matching native
  ;; fs::write: a partial fd_write resumes at the unwritten bytes, and an accepted-0-bytes call
  ;; is native's ErrorKind::WriteZero ("failed to write whole buffer"). The FIFTH host-write
  ;; sandbox exit (Capability::FsWrite — DISTINCT from FsRead). The result is an owned heap
  ;; handle the caller's scope-end DropListStr balances.
  (func $write_text_file (param $path i32) (param $content i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32) (local $fd_out i32) (local $errno i32)
    (local $fd i32) (local $iov i32) (local $nwritten i32) (local $obj i32) (local $msg i32)
    (local $maddr i32) (local $mlen i32) (local $wbase i32) (local $wrem i32) (local $wgot i32)
    ;; dirfd + path bytes + length via $path_norm (#1394).
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    ;; path_open(dirfd, dirflags=0, path_ptr, path_len, oflags=O_CREAT|O_TRUNC=9,
    ;;   rights_base = fd_seek|fd_write|fd_filestat_set_size = 0x400044, rights_inheriting=0,
    ;;   fdflags=0, fd_out)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    (local.set $errno
      (call $path_open (local.get $dirfd) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 9) (i64.const 4194372) (i64.const 0) (i32.const 0) (local.get $fd_out)))
    ;; On a path_open error build Err(<native std::io Display>).
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
{write_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (call $rtf_result (local.get $msg) (i32.const 1)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; write_all LOOP — iov = [unwritten_ptr, unwritten_len] each pass, because WASI may
        ;; accept FEWER bytes than offered (native fs::write's write_all resumes identically).
        ;; $errno is 0 here (path_open succeeded) and is REUSED as the write verdict: a WASI
        ;; errno, or -1 for "accepted 0 bytes with no errno" = native's ErrorKind::WriteZero.
        (local.set $iov (call $alloc8 (i32.const 8)))
        (local.set $nwritten (call $alloc8 (i32.const 4)))
        (local.set $wbase (i32.add (local.get $content) (i32.const {LIST_HEADER})))
        (local.set $wrem (i32.load (i32.add (local.get $content) (i32.const {LIST_LEN_OFFSET}))))
        (block $wdone (loop $wl
          (br_if $wdone (i32.eqz (local.get $wrem)))
          (i32.store (local.get $iov) (local.get $wbase))
          (i32.store (i32.add (local.get $iov) (i32.const 4)) (local.get $wrem))
          (local.set $errno
            (call $fd_write (local.get $fd) (local.get $iov) (i32.const 1) (local.get $nwritten)))
          (br_if $wdone (i32.ne (local.get $errno) (i32.const 0)))
          (local.set $wgot (i32.load (local.get $nwritten)))
          (if (i32.eqz (local.get $wgot))
            (then (local.set $errno (i32.const -1)) (br $wdone)))
          (local.set $wbase (i32.add (local.get $wbase) (local.get $wgot)))
          (local.set $wrem (i32.sub (local.get $wrem) (local.get $wgot)))
          (br $wl)))
        (drop (call $fd_close (local.get $fd)))
        (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
          (then
{write_fd_errno_map}            ;; the accepted-0-bytes sentinel is Rust's OWN const message, not an OS string.
            (if (i32.eq (local.get $errno) (i32.const -1)) (then
              (local.set $maddr (i32.const {FS_ERR_WRITEZERO_ADDR}))
              (local.set $mlen (i32.const {FS_ERR_WRITEZERO_LEN}))))
            (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
            (call $rtf_result (local.get $msg) (i32.const 1)))
          (else
            ;; Build Ok(()) — a 1-slot block with len@4=0 (no owned payload — the
            ;; `materialize_result_ok` convention). @12 (and its high half @16=tag) zeroed by the
            ;; i64.store so the flat DropListStr frees nothing and a `match` reads tag 0 = Ok.
            (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
            (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
            (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
            (local.get $obj))))))

  ;; fs.mkdir_p(path) — the WASI directory-CREATE floor. $path is a BORROWED canonical String.
  ;; Creates the directory at $path RECURSIVELY (each '/'-delimited prefix in turn, so `a/b/c`
  ;; makes all three), relative to the $path_norm-resolved preopen dirfd (same resolution as
  ;; $write_text_file). An already-existing dir (errno 20 = EEXIST) counts as success. Builds a
  ;; fresh OWNED `Result[Unit, String]`: Ok(()) as a 1-slot block with len@4=0 + @12=0 + tag@16=0
  ;; (the `materialize_result_ok` convention, IDENTICAL to $write_text_file — the scope-end flat
  ;; $drop_list_str frees nothing at @12), or Err(<native std::io Display>) via $rtf_result on a
  ;; path_create_directory error (len@4=1, @12=msg, tag@16=1). A mkdir IS a filesystem write
  ;; (Capability::FsWrite — the SAME cap as fs.write). The result is an owned heap handle the
  ;; caller's scope-end DropListStr balances.
  (func $make_dir (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32) (local $seg i32) (local $errno i32)
    (local $obj i32) (local $msg i32) (local $maddr i32) (local $mlen i32)
    ;; dirfd + path bytes + length via $path_norm (#1394).
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
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
      (drop (call $path_create_directory (local.get $dirfd) (local.get $pdata) (local.get $seg)))
      (br $louter)))
    ;; Final attempt: create the full path, capture errno (EEXIST = 20 here once the loop made it).
    (local.set $errno
      (call $path_create_directory (local.get $dirfd) (local.get $pdata) (local.get $plen)))
    ;; errno 0 OR 20 (EEXIST) -> Ok(()), else Err(<native std::io Display>) — the shared errno
    ;; mapping, "mkdir failed" only for an errno outside it (#1385).
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
{mkdir_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (call $rtf_result (local.get $msg) (i32.const 1)))))

  ;; fs.exists(path) — the WASI path-stat floor. $path is a BORROWED canonical String, resolved
  ;; through $path_norm to a (dirfd, path) pair (same resolution as $read_text_file), then queries
  ;; path_filestat_get(dirfd, flags=symlink_follow(1), path, path_len, stat_buf): errno 0 means a
  ;; file OR directory exists there → return 1, else 0 — matching native Path::exists(). The stat
  ;; buffer is 8-aligned $alloc8 scratch (the host writes i64 fields there). Returns a SCALAR i32
  ;; Bool (the caller i64.extend's it) — NO heap result, so no Capability beyond FsRead.
  ;; fs.stat(path) — the WASI FULL-stat floor. $buf is a CALLER-OWNED 64-byte scratch (the
  ;; self-host's Bytes data region — the host writes the WASI filestat there: filetype@16,
  ;; size@32, mtim@48); $path a BORROWED canonical String. Same resolution as $path_exists
  ;; ($path_norm-resolved preopen dirfd, symlink_follow). Returns the RAW errno (0 = ok).
  (func $path_filestat_q (param $buf i32) (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32)
    (local $scratch i32) (local $errno i32) (local $j i32)
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    ;; WASI demands an 8-ALIGNED 64-byte filestat out-buffer, but $buf is the
    ;; self-host's own Bytes data (`handle+12` — 4-aligned at best; it happened to
    ;; be 8-aligned until other rt allocs shifted the bump heap). Stat into an
    ;; aligned scratch, then copy the 64 bytes into $buf.
    (local.set $scratch (i32.and (i32.add (call $alloc8 (i32.const 72)) (i32.const 7)) (i32.const -8)))
    (local.set $errno
      (call $path_filestat_get (local.get $dirfd) (i32.const 1) (local.get $pdata) (local.get $plen)
                               (local.get $scratch)))
    (local.set $j (i32.const 0))
    (block $cdone (loop $cloop
      (br_if $cdone (i32.ge_u (local.get $j) (i32.const 64)))
      (i32.store8 (i32.add (local.get $buf) (local.get $j))
                  (i32.load8_u (i32.add (local.get $scratch) (local.get $j))))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $cloop)))
    (local.get $errno))

  ;; fs.is_symlink's floor — the NO-FOLLOW stat twin of $path_filestat_q: the identical
  ;; buffered path_filestat_get with lookupflags = 0 (the final symlink is NOT followed),
  ;; so a symlink's own filetype (7) lands at @16. Same aligned-scratch copy discipline.
  (func $path_filestat_nf (param $buf i32) (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32)
    (local $scratch i32) (local $errno i32) (local $j i32)
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    (local.set $scratch (i32.and (i32.add (call $alloc8 (i32.const 72)) (i32.const 7)) (i32.const -8)))
    (local.set $errno
      (call $path_filestat_get (local.get $dirfd) (i32.const 0) (local.get $pdata) (local.get $plen)
                               (local.get $scratch)))
    (local.set $j (i32.const 0))
    (block $cdone (loop $cloop
      (br_if $cdone (i32.ge_u (local.get $j) (i32.const 64)))
      (i32.store8 (i32.add (local.get $buf) (local.get $j))
                  (i32.load8_u (i32.add (local.get $scratch) (local.get $j))))
      (local.set $j (i32.add (local.get $j) (i32.const 1)))
      (br $cloop)))
    (local.get $errno))

  ;; fs.rename's floor — the WASI path_rename call. Both paths are BORROWED canonical
  ;; Strings, each normalized through $path_norm (which allocates a FRESH buffer per call,
  ;; so the two normalizations never clobber each other). Builds a fresh OWNED
  ;; `Result[Unit, String]`: Ok(()) with len@4=0 + tag@16=0 (the `materialize_result_ok`
  ;; convention, identical to $make_dir's Ok arm) on errno 0, else Err(<native std::io
  ;; Display>) via the SHARED errno→text mapping every fs floor uses ("write failed" only for
  ;; an errno outside it; the hand-rolled NOENT/ACCES-only half went away with #1385). A
  ;; rename IS a filesystem write (Capability::FsWrite).
  (func $rename (param $src i32) (param $dst i32) (result i32)
    (local $sdata i32) (local $slen i32) (local $sfd i32)
    (local $ddata i32) (local $dlen i32) (local $dfd i32)
    (local $errno i32) (local $maddr i32) (local $mlen i32) (local $msg i32) (local $obj i32)
    ;; The two paths resolve INDEPENDENTLY (#1394): under more than one preopen
    ;; they can legitimately land on different dirfds, which path_rename takes.
    (call $path_norm (local.get $src))
    (local.set $slen)
    (local.set $sdata)
    (local.set $sfd)
    (call $path_norm (local.get $dst))
    (local.set $dlen)
    (local.set $ddata)
    (local.set $dfd)
    (local.set $errno
      (call $path_rename (local.get $sfd) (local.get $sdata) (local.get $slen)
                         (local.get $dfd) (local.get $ddata) (local.get $dlen)))
    (if (result i32) (i32.eqz (local.get $errno))
      (then
        (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
        (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
        (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
        (local.get $obj))
      (else
{rename_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (call $rtf_result (local.get $msg) (i32.const 1)))))

  (func $path_exists (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32) (local $stat i32) (local $errno i32)
    ;; dirfd + path bytes + length via $path_norm (#1394).
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    (local.set $stat (call $alloc8 (i32.const 64)))
    (local.set $errno
      (call $path_filestat_get (local.get $dirfd) (i32.const 1) (local.get $pdata) (local.get $plen)
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

  ;; helper: RECURSIVELY remove the tree at byte-path [$pdata, $pdata+$plen) relative to the
  ;; preopen $dirfd (the one $path_norm resolved the caller's path to — #1394; it used to be a
  ;; hard-coded 3). Returns 0 on success or the FIRST non-zero errno. If the path opens as a
  ;; directory it removes every entry — recursing via a re-readdir-from-cookie-0 scan that removes
  ;; ONE entry per pass (so a removal never invalidates a live readdir cookie) — then
  ;; path_remove_directory's the emptied directory; otherwise it path_unlink_file's it as a file
  ;; (matching native remove_dir_all vs remove_file). All removals are issued against $dirfd with
  ;; full child paths (the recursion carries the SAME dirfd, since a child of a resolved path
  ;; lives under the same preopen), so the opened dir fd needs only fd_readdir rights. Used by
  ;; $remove_all.
  (func $remove_path (param $dirfd i32) (param $pdata i32) (param $plen i32) (result i32)
    (local $fd_out i32) (local $errno i32) (local $fd i32) (local $buf i32) (local $bufused_p i32)
    (local $bufused i32) (local $off i32) (local $namlen i32) (local $nameptr i32)
    (local $child i32) (local $clen i32) (local $i i32) (local $rc i32) (local $found i32)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    ;; path_open(dirfd, dirflags=0, path, plen, oflags=O_DIRECTORY=2, rights=fd_readdir(16384),
    ;;   rights_inheriting=16384, fdflags=0, fd_out)
    (local.set $errno
      (call $path_open (local.get $dirfd) (i32.const 0) (local.get $pdata) (local.get $plen)
                       (i32.const 2) (i64.const 16384) (i64.const 16384) (i32.const 0) (local.get $fd_out)))
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
        ;; not a directory (or missing) — unlink as a file; its errno is the result.
        (call $path_unlink_file (local.get $dirfd) (local.get $pdata) (local.get $plen)))
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
                (local.set $errno
                  (call $remove_path (local.get $dirfd) (local.get $child) (local.get $clen)))
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
        (local.set $errno
          (call $path_remove_directory (local.get $dirfd) (local.get $pdata) (local.get $plen)))
        (if (i32.and (i32.eqz (local.get $rc)) (i32.ne (local.get $errno) (i32.const 0)))
          (then (local.set $rc (local.get $errno))))
        (local.get $rc))))

  ;; fs.remove_all(path) — the WASI recursive-remove floor. $path is a BORROWED canonical String.
  ;; Resolves it to a (dirfd, path) pair (same resolution as $write_text_file), recursively
  ;; removes the tree at $path via $remove_path, and builds a fresh OWNED `Result[Unit, String]`:
  ;; Ok(()) (a 1-slot block, len@4=0 + @12=0 + tag@16=0 — the materialize_result_ok convention,
  ;; IDENTICAL to $make_dir's Ok arm, so the scope-end flat $drop_list_str frees nothing) when
  ;; $remove_path returns 0, or Err(<native std::io Display>) via $rtf_result on any non-zero
  ;; errno — the shared mapping, "remove failed" only outside it (#1385). A
  ;; recursive remove IS a filesystem write (Capability::FsWrite — the SAME cap as fs.write). The
  ;; result is an owned heap handle the caller's scope-end DropListStr balances.
  (func $remove_all (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32)
    (local $errno i32) (local $obj i32) (local $msg i32)
    (local $maddr i32) (local $mlen i32)
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    (local.set $errno
      (call $remove_path (local.get $dirfd) (local.get $pdata) (local.get $plen)))
    (if (result i32) (i32.eqz (local.get $errno))
      (then
        ;; Build Ok(()) — len@4=0, @12/@16 zeroed by the i64.store.
        (local.set $obj (call $list_new (i32.const 1) (i32.const 1)))
        (i64.store (i32.add (local.get $obj) (i32.const {LIST_HEADER})) (i64.const 0))
        (i32.store (i32.add (local.get $obj) (i32.const {LIST_LEN_OFFSET})) (i32.const 0))
        (local.get $obj))
      (else
{remove_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
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
  ;; against the $path_norm-resolved preopen dirfd — same resolution as $read_text_file), reads ALL of its
  ;; entries via the RESUMABLE fd_readdir sweep below, parses each dirent (`d_next 8 / d_ino 8 /
  ;; d_namlen 4 / d_type 4` = 24-byte header, then name[d_namlen]) SKIPPING "." and "..", builds
  ;; an owned List[String] of the names, SORTS it lexicographically (insertion sort via $str_lt)
  ;; to match native `names.sort()`, and wraps it Ok via $rtf_result. A path_open OR fd_readdir
  ;; error becomes Err(<native std::io Display>) through the SHARED errno mapping every fs floor
  ;; uses — "directory not found" survives only for an errno outside the mapped four (#1385).
  ;; The FIFTH sandbox exit (Capability::FsRead) — the result is an owned
  ;; Result[List[String], String] the caller's scope-end DropResultListStr balances.
  ;;
  ;; #1384 — `fd_readdir` is a RESUMABLE api and ONE pass is not a listing. A single `cookie=0`
  ;; pass into a 4 KiB buffer silently TRUNCATED every directory whose dirents did not fit (200
  ;; short-named entries came back as 130 = (4096-51)/31, 60 long-named ones as 41) and still
  ;; wrapped the short list `ok(...)`: a silent WRONG VALUE, not a wall and not an err, and
  ;; indistinguishable from a genuinely smaller directory. The sweep continues from the `d_next`
  ;; cookie of the last COMPLETE record until a pass returns FEWER bytes than the buffer (WASI's
  ;; documented end-of-directory signal), concatenating each pass's complete records into a
  ;; doubling accumulation buffer the two parse passes below then read as one contiguous dirent
  ;; run. Two traps the loop is written around:
  ;;   - a pass that EXACTLY fills the buffer is NOT necessarily the end, so `bufused == buflen`
  ;;     always re-reads (a genuinely final exact fill costs one extra empty pass, never a lost
  ;;     entry);
  ;;   - the host truncates the trailing record when a name STRADDLES the buffer end, so only
  ;;     records whose header AND name both fit are accumulated and the resume cookie is the last
  ;;     COMPLETE record's `d_next` — a half-read name can never reach $rtf_str.
  ;; A name too long to ever fit would make a pass yield no complete record at all; that doubles
  ;; the pass buffer and retries the SAME cookie instead of spinning.
  (func $read_dir (param $path i32) (result i32)
    (local $pdata i32) (local $plen i32) (local $dirfd i32) (local $fd_out i32) (local $errno i32)
    (local $fd i32) (local $buf i32) (local $bufbase i32) (local $bufused_p i32) (local $bufused i32)
    (local $off i32) (local $namlen i32) (local $skip i32) (local $count i32)
    (local $list i32) (local $ci i32) (local $name i32) (local $msg i32)
    (local $maddr i32) (local $mlen i32)
    (local $namebase i32) (local $si i32) (local $sj i32) (local $hi i64) (local $hj i64)
    (local $buflen i32) (local $cookie i64) (local $good i32) (local $rderr i32)
    (local $acc i32) (local $accbase i32) (local $acccap i32) (local $accused i32)
    (local $newacc i32) (local $cp i32)
    ;; dirfd + path bytes + length via $path_norm (#1394).
    (call $path_norm (local.get $path))
    (local.set $plen)
    (local.set $pdata)
    (local.set $dirfd)
    ;; path_open(dirfd, dirflags=1, path, plen, oflags=2 [O_DIRECTORY],
    ;;   rights_base = fd_readdir(0x4000), rights_inheriting=0, fdflags=0, fd_out)
    (local.set $fd_out (call $alloc8 (i32.const 4)))
    (local.set $errno
      (call $path_open (local.get $dirfd) (i32.const 1) (local.get $pdata) (local.get $plen)
                       (i32.const 2) (i64.const 16384) (i64.const 16384) (i32.const 0) (local.get $fd_out)))
    (if (result i32) (i32.ne (local.get $errno) (i32.const 0))
      (then
{rdir_errno_map}        (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
        (call $rtf_result (local.get $msg) (i32.const 1)))
      (else
        (local.set $fd (i32.load (local.get $fd_out)))
        ;; The 4 KiB PASS buffer and the ACCUMULATION buffer are both RECLAIMABLE $list_new
        ;; blocks (512 i64 slots = 4096 data bytes after the header) so a list_dir LOOP frees
        ;; them each call (rc_dec below) instead of leaking immortal $alloc8 scratch (which OOMs
        ;; a tight loop). The WASI write target is `$bufbase = buf + HEADER`, keeping the rc cell
        ;; @0 intact for the final $rc_dec. fd_out/bufused_p stay $alloc8 (4-byte immortal
        ;; scratch, like read_text_file's out-params — negligible).
        (local.set $buflen (i32.const 4096))
        (local.set $buf (call $list_new (i32.const 0) (i32.const 512)))
        (local.set $bufbase (i32.add (local.get $buf) (i32.const {LIST_HEADER})))
        (local.set $bufused_p (call $alloc8 (i32.const 4)))
        (local.set $acccap (i32.const 4096))
        (local.set $acc (call $list_new (i32.const 0) (i32.const 512)))
        (local.set $accbase (i32.add (local.get $acc) (i32.const {LIST_HEADER})))
        (local.set $accused (i32.const 0))
        (local.set $cookie (i64.const 0))
        (local.set $rderr (i32.const 0))
        ;; THE SWEEP — fd_readdir(fd, buf, buf_len, cookie, bufused_p), resumed until the host
        ;; reports end-of-directory by returning fewer bytes than the buffer holds.
        (block $sweepdone (loop $sweep
          ;; the errno is KEPT, not dropped: a real readdir failure must become Err. Dropping it
          ;; left $bufused reading fresh (zeroed) scratch, so a failed listing returned `ok([])` —
          ;; the same fall-through-to-Ok shape as the truncation (#1384).
          (local.set $errno (call $fd_readdir (local.get $fd) (local.get $bufbase)
                                              (local.get $buflen) (local.get $cookie)
                                              (local.get $bufused_p)))
          (if (i32.ne (local.get $errno) (i32.const 0))
            (then (local.set $rderr (i32.const 1)) (br $sweepdone)))
          (local.set $bufused (i32.load (local.get $bufused_p)))
          ;; Scan this pass for COMPLETE records only — header AND name inside $bufused.
          ;; $good = the bytes they cover; $cookie = the last one's d_next (the resume point).
          (local.set $off (i32.const 0))
          (local.set $good (i32.const 0))
          (block $scandone (loop $scan
            (br_if $scandone (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $bufused)))
            (local.set $namlen (i32.load (i32.add (i32.add (local.get $bufbase) (local.get $off)) (i32.const 16))))
            ;; a d_namlen past the whole buffer can never complete — and would WRAP the add below.
            (br_if $scandone (i32.gt_u (local.get $namlen) (local.get $buflen)))
            (br_if $scandone (i32.gt_u (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen))
                                       (local.get $bufused)))
            (local.set $cookie (i64.load (i32.add (local.get $bufbase) (local.get $off))))
            (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen)))
            (local.set $good (local.get $off))
            (br $scan)))
          ;; GROW the accumulation buffer (doubling) until this pass's complete records fit.
          (if (i32.gt_u (i32.add (local.get $accused) (local.get $good)) (local.get $acccap))
            (then
              (block $capdone (loop $caploop
                (br_if $capdone (i32.ge_u (local.get $acccap)
                                          (i32.add (local.get $accused) (local.get $good))))
                (local.set $acccap (i32.shl (local.get $acccap) (i32.const 1)))
                (br $caploop)))
              (local.set $newacc (call $list_new (i32.const 0) (i32.shr_u (local.get $acccap) (i32.const 3))))
              (local.set $cp (i32.const 0))
              (block $movedone (loop $moveloop
                (br_if $movedone (i32.ge_u (local.get $cp) (local.get $accused)))
                (i32.store8 (i32.add (i32.add (local.get $newacc) (i32.const {LIST_HEADER})) (local.get $cp))
                            (i32.load8_u (i32.add (local.get $accbase) (local.get $cp))))
                (local.set $cp (i32.add (local.get $cp) (i32.const 1)))
                (br $moveloop)))
              (call $rc_dec (local.get $acc))
              (local.set $acc (local.get $newacc))
              (local.set $accbase (i32.add (local.get $acc) (i32.const {LIST_HEADER})))))
          ;; APPEND this pass's complete records.
          (local.set $cp (i32.const 0))
          (block $appdone (loop $apploop
            (br_if $appdone (i32.ge_u (local.get $cp) (local.get $good)))
            (i32.store8 (i32.add (i32.add (local.get $accbase) (local.get $accused)) (local.get $cp))
                        (i32.load8_u (i32.add (local.get $bufbase) (local.get $cp))))
            (local.set $cp (i32.add (local.get $cp) (i32.const 1)))
            (br $apploop)))
          (local.set $accused (i32.add (local.get $accused) (local.get $good)))
          ;; END OF DIRECTORY: a pass that did NOT fill the buffer. An EXACT fill re-reads.
          (br_if $sweepdone (i32.lt_u (local.get $bufused) (local.get $buflen)))
          ;; full buffer, not one complete record: a single dirent exceeds the whole buffer.
          ;; Double it and retry the SAME cookie — without this the sweep would spin forever.
          (if (i32.eqz (local.get $good))
            (then
              (call $rc_dec (local.get $buf))
              (local.set $buflen (i32.shl (local.get $buflen) (i32.const 1)))
              (local.set $buf (call $list_new (i32.const 0) (i32.shr_u (local.get $buflen) (i32.const 3))))
              (local.set $bufbase (i32.add (local.get $buf) (i32.const {LIST_HEADER})))))
          (br $sweep)))
        (drop (call $fd_close (local.get $fd)))
        ;; free the pass buffer — every complete record is in $acc now.
        (call $rc_dec (local.get $buf))
        (if (local.get $rderr)
          (then
            (call $rc_dec (local.get $acc))
{rdir_rd_errno_map}            (local.set $msg (call $rtf_str (local.get $maddr) (local.get $mlen)))
            (return (call $rtf_result (local.get $msg) (i32.const 1)))))
        ;; PASS 1 — count entries (skip "." and ".."). 24-byte dirent header; d_namlen @16, name @24.
        (local.set $off (i32.const 0))
        (local.set $count (i32.const 0))
        (block $c1done (loop $c1
          ;; stop when the next header would exceed accused (never mid-record: the sweep only
          ;; accumulates COMPLETE dirents).
          (br_if $c1done (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $accused)))
          (local.set $namlen (i32.load (i32.add (i32.add (local.get $accbase) (local.get $off)) (i32.const 16))))
          (local.set $namebase (i32.add (i32.add (local.get $accbase) (local.get $off)) (i32.const 24)))
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
          (br_if $c2done (i32.gt_u (i32.add (local.get $off) (i32.const 24)) (local.get $accused)))
          (local.set $namlen (i32.load (i32.add (i32.add (local.get $accbase) (local.get $off)) (i32.const 16))))
          (local.set $namebase (i32.add (i32.add (local.get $accbase) (local.get $off)) (i32.const 24)))
          (if (i32.eqz (call $is_dot_entry (local.get $namebase) (local.get $namlen)))
            (then
              (local.set $name (call $rtf_str (local.get $namebase) (local.get $namlen)))
              (call $list_set (local.get $list) (local.get $ci) (i64.extend_i32_u (local.get $name)))
              (local.set $ci (i32.add (local.get $ci) (i32.const 1)))))
          (local.set $off (i32.add (i32.add (local.get $off) (i32.const 24)) (local.get $namlen)))
          (br $c2)))
        ;; free the accumulation buffer (all names are now copied into the list) — reclaimable,
        ;; so a list_dir loop reuses it instead of leaking.
        (call $rc_dec (local.get $acc))
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

/// The ONE errno → native `std::io` Display mapping, rendered INLINE at every WASI
/// fs error site (§4.1 forbids a new hand-written WAT function, so the branches are
/// duplicated in the OUTPUT — but they have exactly one SOURCE, here, which is what
/// stops the floors drifting apart the way the write side did: `$read_text_file`
/// carried the mapping while `$write_text_file`/`$make_dir`/`$remove_all`/`$read_dir`
/// answered a single fixed string for every failure, #1385).
///
/// Reads `$errno`, writes `$maddr`/`$mlen`. `def_addr`/`def_len` is the SITE's own
/// fallback text, kept for an errno outside the mapped set — a BOUNDED divergence,
/// not a silent one (C-273).
///
/// The mapped set is exactly the errnos whose native `std::io` Display is the SAME
/// on every Unix host: ENOENT 2, EACCES 13, ENOTDIR 20, EISDIR 21 carry those numbers
/// on both macOS and Linux, so one baked data segment is right on both. ENOTEMPTY
/// (39 Linux / 66 macOS), ENAMETOOLONG (36 / 63) and EXDEV (whose text differs:
/// "Invalid cross-device link" vs "Cross-device link") are deliberately NOT mapped —
/// a host-specific string in a portable `.wasm` would trade one divergence for a
/// worse one.
/// The UTF-8 acceptance check `$read_text_file` runs over its read buffer when its
/// `$validate` operand is 1 (#1506, C-290) — INLINE, like [`fs_errno_msg_wat`]:
/// §4.1 forbids a new hand-written WAT function, and the loop has exactly one
/// site. Sets `$valid` to 1/0 over the `$fsize` bytes at `$data`, using Rust's
/// exact `str::from_utf8` table (the one stdlib/bytes_core.almd's
/// `bytes.is_valid_utf8` replicates): a lead byte fixes the sequence width AND a
/// tighter range on the SECOND byte (rejecting overlongs, UTF-16 surrogates
/// U+D800..U+DFFF and codepoints > U+10FFFF); later bytes are plain
/// continuations 0x80..0xBF; a truncated tail is invalid. `$validate = 0` (the
/// bytes floor, `PrimKind::ReadBytesFile`) skips the loop and leaves `$valid = 1`.
fn utf8_validate_wat(indent: &str) -> String {
    let i = indent;
    format!(
        "{i};; #1506 — UTF-8 acceptance over the read buffer (text floor only). Inline: §4.1.\n\
         {i}(local.set $valid (i32.const 1))\n\
         {i}(if (local.get $validate) (then\n\
         {i}  (local.set $vi (i32.const 0))\n\
         {i}  (block $vdone (loop $vnext\n\
         {i}    (br_if $vdone (i32.ge_u (local.get $vi) (local.get $fsize)))\n\
         {i}    (local.set $vb0 (i32.load8_u (i32.add (local.get $data) (local.get $vi))))\n\
         {i}    ;; ASCII: one byte, next.\n\
         {i}    (if (i32.lt_u (local.get $vb0) (i32.const 128))\n\
         {i}      (then (local.set $vi (i32.add (local.get $vi) (i32.const 1))) (br $vnext)))\n\
         {i}    ;; a multibyte lead is 0xC2..=0xF4; anything else (a stray continuation, C0/C1, F5+) is invalid.\n\
         {i}    (if (i32.or (i32.lt_u (local.get $vb0) (i32.const 194)) (i32.gt_u (local.get $vb0) (i32.const 244)))\n\
         {i}      (then (local.set $valid (i32.const 0)) (br $vdone)))\n\
         {i}    ;; width: C2..DF = 2, E0..EF = 3, F0..F4 = 4\n\
         {i}    (local.set $vw (select (i32.const 2)\n\
         {i}                            (select (i32.const 3) (i32.const 4) (i32.lt_u (local.get $vb0) (i32.const 240)))\n\
         {i}                            (i32.lt_u (local.get $vb0) (i32.const 224))))\n\
         {i}    ;; a truncated tail is invalid\n\
         {i}    (if (i32.gt_u (i32.add (local.get $vi) (local.get $vw)) (local.get $fsize))\n\
         {i}      (then (local.set $valid (i32.const 0)) (br $vdone)))\n\
         {i}    ;; second byte: 80..BF, tightened per lead — E0: A0..BF (no overlong 3-byte),\n\
         {i}    ;; ED: 80..9F (no surrogate), F0: 90..BF (no overlong 4-byte), F4: 80..8F (<= U+10FFFF)\n\
         {i}    (local.set $vlo (i32.const 128))\n\
         {i}    (local.set $vhi (i32.const 191))\n\
         {i}    (if (i32.eq (local.get $vb0) (i32.const 224)) (then (local.set $vlo (i32.const 160))))\n\
         {i}    (if (i32.eq (local.get $vb0) (i32.const 237)) (then (local.set $vhi (i32.const 159))))\n\
         {i}    (if (i32.eq (local.get $vb0) (i32.const 240)) (then (local.set $vlo (i32.const 144))))\n\
         {i}    (if (i32.eq (local.get $vb0) (i32.const 244)) (then (local.set $vhi (i32.const 143))))\n\
         {i}    (local.set $vb1 (i32.load8_u (i32.add (local.get $data) (i32.add (local.get $vi) (i32.const 1)))))\n\
         {i}    (if (i32.or (i32.lt_u (local.get $vb1) (local.get $vlo)) (i32.gt_u (local.get $vb1) (local.get $vhi)))\n\
         {i}      (then (local.set $valid (i32.const 0)) (br $vdone)))\n\
         {i}    ;; bytes 3..w are plain continuations (10xxxxxx)\n\
         {i}    (local.set $vk (i32.const 2))\n\
         {i}    (block $vcdone (loop $vcnext\n\
         {i}      (br_if $vcdone (i32.ge_u (local.get $vk) (local.get $vw)))\n\
         {i}      (local.set $vb1 (i32.load8_u (i32.add (local.get $data) (i32.add (local.get $vi) (local.get $vk)))))\n\
         {i}      (if (i32.ne (i32.and (local.get $vb1) (i32.const 192)) (i32.const 128))\n\
         {i}        (then (local.set $valid (i32.const 0)) (br $vdone)))\n\
         {i}      (local.set $vk (i32.add (local.get $vk) (i32.const 1)))\n\
         {i}      (br $vcnext)))\n\
         {i}    (local.set $vi (i32.add (local.get $vi) (local.get $vw)))\n\
         {i}    (br $vnext)))))\n"
    )
}

fn fs_errno_msg_wat(indent: &str, def_addr: u32, def_len: u32, def_text: &str) -> String {
    let mut out = format!(
        "{indent};; errno → the EXACT native std::io Display text, INLINE (§4.1: no new wat func).\n\
         {indent};; NOENT(44)/ACCES(2)/NOTDIR(54)/ISDIR(31); anything else keeps \"{def_text}\".\n\
         {indent}(local.set $maddr (i32.const {def_addr}))\n\
         {indent}(local.set $mlen (i32.const {def_len}))\n"
    );
    for (errno, addr, len) in [
        (44, FS_ERR_NOENT_ADDR, FS_ERR_NOENT_LEN),
        (2, FS_ERR_ACCES_ADDR, FS_ERR_ACCES_LEN),
        (54, FS_ERR_NOTDIR_ADDR, FS_ERR_NOTDIR_LEN),
        (31, FS_ERR_ISDIR_ADDR, FS_ERR_ISDIR_LEN),
    ] {
        out.push_str(&format!(
            "{indent}(if (i32.eq (local.get $errno) (i32.const {errno})) (then\n\
             {indent}  (local.set $maddr (i32.const {addr})) (local.set $mlen (i32.const {len}))))\n"
        ));
    }
    out
}
