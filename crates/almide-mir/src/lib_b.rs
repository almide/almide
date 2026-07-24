
/// The closed set of primitive-floor operations (the trusted, wasm-spec-faithful
/// surface the self-hosted runtime is written over).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimKind {
    /// Reinterpret a heap handle (i32 pointer) as an i64 address value — the
    /// String/List→Int bridge so all address math is `Int` `IntBinOp`.
    Handle,
    /// Load `width` bytes (1/4/8) at a computed i64 address, zero-extended to i64.
    Load { width: u8 },
    /// Load a 4-byte i32 HANDLE at a computed i64 address — UNLIKE `Load { width: 4 }`, the
    /// result keeps the `Ptr` (i32) repr (no i64 zero-extend), so it IS a heap handle a caller
    /// can pass to a String/List consumer. The bridge for extracting a heap element from a slot
    /// (a `match Some(s)` payload / a `List[String]` element). A borrowed alias — no ownership.
    LoadHandle,
    /// Store the low `width` bytes (1/4/8) of an i64 value at a computed i64 address.
    Store { width: u8 },
    /// Bounds-checked element ADDRESS for a direct `xs[i]` index — `args = [list_handle,
    /// index]` (both i64-uniform: the handle reinterpreted to an address, the index a scalar
    /// i64), dst = the i64 element-slot address `list + LIST_HEADER + idx*ELEM_SIZE`. Renders
    /// `(call $elem_addr ...)` (the SAME preamble helper v0's `$list_get`/`$list_set` use), so a
    /// negative or `>= cap` index TRAPS (the controlled-halt bounds wall) instead of reading
    /// outside the block — v0's `a[i]` likewise halts on OOB (it prints `index out of bounds`
    /// and exits 1; this traps). For an in-bounds index the loaded element byte-matches v0. A
    /// scalar address computation, no ownership (a no-op in verify_ownership like every Prim).
    ElemAddr,
    /// Abort: write the String-block message to STDERR and proc_exit(1) — the
    /// self-host arm of the §13 termination convention (math.pow negative
    /// exponent, int.rotate nonpositive width). Never returns.
    Die,
    /// `process.exit(code)` — the WASI `proc_exit` host call with a USER exit
    /// code (`args = [code]`, i64 wrapped to i32; no message line, unlike
    /// [`PrimKind::Die`]'s fixed exit-1 + stderr). Never returns; carries no
    /// ownership event and no capability of its own (the frontend's E006
    /// already forces the calling fn to be `effect`). #782: the assert/T18
    /// desugar's `process.exit(1)` statement rode the retired v0 emitter —
    /// this is its v1 floor.
    ProcExit,
    /// The `fd_write` WASI host call — `args = [fd, iov, count, nwritten]`, dst = the
    /// i64 errno. A sandbox exit; carries [`Capability::Stdout`].
    FdWrite,
    /// The `random_get` WASI host call — `args = [buf, buf_len]`, dst = the i64 errno;
    /// fills `buf_len` bytes at `buf` with host entropy. The second sandbox exit; reached
    /// only by the self-hosted `random.int`. Carries [`Capability::Entropy`] (the
    /// cap_witness counts it exactly like `FdWrite` → Stdout), so a function using it is
    /// caps-verified ONLY if it declares Entropy — never accept-but-unsafe.
    RandomGet,
    /// The `clock_time_get` WASI host call — `args = [clock_id, precision, time_ptr]`, dst =
    /// the i64 errno; writes the current clock value (nanoseconds) as an i64 at `time_ptr`.
    /// A SCALAR-dst sandbox exit (like [`RandomGet`] — NO heap result, NO ownership event),
    /// reached only by the self-hosted `env.unix_timestamp` (which reads `time_ptr` and
    /// divides by 1e9 to seconds). Carries [`Capability::Clock`] — a DISTINCT capability
    /// (a clock read is neither a filesystem nor an entropy effect; the cap_witness counts
    /// it exactly like `RandomGet` → Entropy), so a function using it is caps-verified ONLY
    /// if it declares Clock — never accept-but-unsafe. NON-DETERMINISTIC (no byte-match).
    ClockTimeGet,
    /// The `args_sizes_get` + `args_get` WASI host calls, packaged as ONE high-level
    /// HEAP-RESULT prim — no args, dst = a fresh OWNED `List[String]` of the program
    /// arguments `argv[1..]` (SKIP argv[0], matching native `env.args`). Each element
    /// is a canonical Almide String copied from the NUL-terminated argv C-string. The
    /// third sandbox exit, reached only by the self-hosted `env.args`. Carries
    /// [`Capability::CliArgs`] (the cap_witness counts it exactly like `RandomGet` →
    /// Entropy), so a function using it is caps-verified ONLY if it declares CliArgs —
    /// never accept-but-unsafe. Its dst is a heap Ptr (like `LoadHandle`), so the
    /// ownership certificate emits an `i` (alloc) for it, balanced by the caller's
    /// scope-end drop (a recursive `DropListStr` over the owned element Strings).
    ArgsGetList,
    /// The SAME WASI args floor as [`ArgsGetList`] but INCLUDING argv[0] (the program
    /// path) — `process.args()` = native `std::env::args()`. Renders as
    /// `(call $args_get_list (i32.const 0))` (the one parameterized bridge, skip=0);
    /// same fresh OWNED `List[String]` dst, same [`Capability::CliArgs`] accounting.
    ArgsGetListFull,
    /// The WASI `environ_sizes_get` + `environ_get` lookup, packaged as ONE high-level
    /// HEAP-RESULT prim — `args = [name]` (a BORROWED `String` handle), dst = a fresh
    /// OWNED `Option[String]`: a 0-slot block (none) or a 1-slot block whose @12 holds
    /// the owned value String (some) — the `materialize_opt_str_some` layout, so the
    /// caller's `match`/`??`/`DropListStr` machinery handles it identically to a
    /// self-host-built Option. Scans the `KEY=VALUE\0` environ entries for `name`
    /// followed by `=` (byte-exact, first hit wins) — native `std::env::var(name).ok()`
    /// is the oracle (C-133; the runner passes the host env through
    /// `wasmtime -S inherit-env=y`). Reached only by the self-hosted `env.get`.
    /// Carries [`Capability::CliArgs`] — the Env effect-profile's canonical capability
    /// (reading the process's initial environment, the same class as argv; the profile
    /// map `"Env" => CliArgs` already binds them). Its dst is a heap Ptr (like
    /// [`ArgsGetList`]), so the ownership certificate emits an `i` (alloc) for it,
    /// balanced by the caller's scope-end drop (the flat `DropListStr` frees the owned
    /// payload String, if any, then the block) or a heap-return move-out.
    EnvGet,
    /// The WASI `fd_read`-from-stdin line-read sequence, packaged as ONE high-level HEAP-RESULT
    /// prim — no args, dst = a fresh OWNED canonical `String` of ONE line of standard input.
    /// Reads fd 0 BYTE-BY-BYTE (so it never over-reads past the newline — a later
    /// `read_n_bytes` of the body still sees the right stream) until a `\n` (excluded from the
    /// result) or EOF, then strips a trailing `\r` (matching native
    /// `read_line().trim_end_matches('\n').trim_end_matches('\r')`). EOF with no bytes yields the
    /// empty String. Reached only by the self-hosted `io.read_line`. Carries [`Capability::Stdin`]
    /// — a DISTINCT capability (reading standard input is neither a write, a filesystem, an
    /// entropy, nor a clock effect; the cap_witness counts it exactly like `RandomGet` → Entropy),
    /// so a function using it is caps-verified ONLY if it declares Stdin — never accept-but-unsafe.
    /// NON-DETERMINISTIC (reads live stdin): no byte-match across runs unless stdin is fixed. Its
    /// dst is a heap Ptr (like [`ArgsGetList`]), so the ownership certificate emits an `i` (alloc)
    /// for it, balanced by the caller's scope-end flat `Drop` (a String owns no nested handles) or
    /// a heap-return move-out.
    ReadLine,
    /// `read_n_bytes(n)` — the WASI stdin-N-bytes floor (io.read_n_bytes), the SIBLING of
    /// [`PrimKind::ReadLine`]: `args = [n]` (an `Int`, the byte count), dst = a fresh OWNED `Bytes`
    /// block (the same byte-buffer block layout a `String` uses, built by the preamble `$read_n_bytes`
    /// via `$rtf_str`). Reads UP TO `n` bytes from fd 0 (stopping early at EOF). Carries
    /// Capability::Stdin (same DISTINCT cap as ReadLine). NON-DETERMINISTIC (live stdin): no byte-match.
    /// Its dst is a heap Ptr, so the ownership certificate emits an `i` (alloc) balanced by the caller's
    /// scope-end flat `Drop` (a Bytes owns no nested handles) or a heap-return move-out.
    ReadNBytes,
    /// The WASI `path_open` + `fd_read` file-read sequence, packaged as ONE high-level
    /// HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle), dst = a fresh
    /// OWNED `Result[String, String]`. Opens the file at `path` (relative to the first
    /// preopened dir, leading `/` stripped — the same absolute-path fallback the native
    /// emit's `__resolve_path` uses) and reads its bytes: on success builds `Ok(content)`
    /// where `content` is a canonical Almide String of the file bytes; on a path_open
    /// error builds `Err(<message>)`. The result block is the EXACT `materialize_result_str`
    /// layout — a 1-slot DynListStr `[rc][len@4=1][cap@8][@12 String handle][@16 tag]`
    /// (tag 0 = Ok, 1 = Err) — so the caller's `!`/`match`/`DropListStr` machinery handles
    /// it identically to a self-host-built `Result[String, String]`. The FOURTH sandbox
    /// exit, reached only by the self-hosted `fs.read_text`. Carries [`Capability::FsRead`]
    /// (the cap_witness counts it exactly like `ArgsGetList` → CliArgs), so a function using
    /// it is caps-verified ONLY if it declares FsRead — never accept-but-unsafe. Its dst is
    /// a heap Ptr (like `ArgsGetList`), so the ownership certificate emits an `i` (alloc) for
    /// it, balanced by the caller's scope-end drop (the flat `DropListStr` over the one owned
    /// payload String).
    ReadTextFile,
    /// The WASI `path_open(O_DIRECTORY)` + `fd_readdir` directory-listing sequence, packaged
    /// as ONE high-level HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle), dst
    /// = a fresh OWNED `Result[List[String], String]`. Opens the directory at `path` (same
    /// preopen-relative resolution as [`ReadTextFile`]) and reads its entries via an
    /// `fd_readdir` re-read-on-truncation loop, parsing each variable-length dirent record
    /// (`d_next u64 / d_ino u64 / d_namlen u32 / d_type u8 / name[d_namlen]`), SKIPPING `.`
    /// and `..` (WASI yields them, native `std::fs::read_dir` does not), then SORTING the names
    /// lexicographically (to byte-match the Rust runtime's `names.sort()`), and builds
    /// `Ok([name, …])` where the payload is a fresh owned `List[String]`. On a path_open error
    /// it builds `Err(<message>)`. The result block is the cap-as-tag layout `[rc][len@4=1]
    /// [cap@8=1][@12 List[String] handle][@16 tag]` (tag 0 = Ok, 1 = Err) — the SAME shape as
    /// [`ReadTextFile`], only the @12 payload is a nested `List[String]` (so the scope-end drop
    /// is the RECURSIVE [`StmtKind::DropResultListStr`], not the flat `DropListStr` that would
    /// leak the inner element Strings). The FIFTH sandbox exit, reached only by the self-hosted
    /// `fs.list_dir`. Carries [`Capability::FsRead`] (the cap_witness counts it exactly like
    /// [`ReadTextFile`] → FsRead), so a function using it is caps-verified ONLY if it declares
    /// FsRead — never accept-but-unsafe. Its dst is a heap Ptr (like [`ReadTextFile`]), so the
    /// ownership certificate emits an `i` (alloc) for it, balanced by the caller's scope-end
    /// recursive drop (or a heap-return move-out).
    ReadDir,
    /// The WASI `path_open(O_CREAT|O_TRUNC)` + `fd_write` file-WRITE sequence, packaged as ONE
    /// high-level HEAP-RESULT prim — `args = [path, content]` (both BORROWED `String` handles,
    /// the caller still owns them), dst = a fresh OWNED `Result[Unit, String]`. Opens (creating +
    /// truncating) the file at `path` (relative to the first preopened dir, leading `/` stripped —
    /// the same resolution [`ReadTextFile`] uses) and writes `content`'s bytes via `fd_write`: on
    /// success builds `Ok(())`, on a path_open / fd_write error builds `Err(<message>)`. The result
    /// block reuses the cap-as-tag layout `[rc][len@4][cap@8][@12][@16 tag]` (tag 0 = Ok, 1 = Err),
    /// but DIVERGES from [`ReadTextFile`] in the Ok arm: a `Unit` payload owns NO String, so Ok is
    /// built with `len@4 = 0` (and `@12 = 0`, `@16 = 0`) — EXACTLY the `materialize_result_ok`
    /// convention — so the caller's scope-end flat `DropListStr` frees NOTHING at @12 (it would
    /// trap on a null `rc_dec` if Ok carried a phantom `len = 1`). The Err arm sets `len@4 = 1`,
    /// `@12 = msg String`, `@16 tag = 1` (the flat `DropListStr` frees the one owned message). The
    /// FIFTH host-write sandbox exit, reached only by the self-hosted `fs.write`. Carries
    /// [`Capability::FsWrite`] — a DISTINCT capability from FsRead (a write is strictly greater
    /// authority), counted in cap_witness — so a function using it is caps-verified ONLY if it
    /// declares FsWrite; never accept-but-unsafe. Its dst is a heap Ptr (like [`ReadTextFile`]),
    /// so the ownership certificate emits an `i` (alloc) for it, balanced by the caller's scope-end
    /// flat `DropListStr` (sound for BOTH arms given the `len@4 = 0` Ok convention above).
    WriteTextFile,
    /// The WASI `path_create_directory` recursive-mkdir sequence, packaged as ONE high-level
    /// HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle, the caller still owns
    /// it), dst = a fresh OWNED `Result[Unit, String]`. Creates the directory at `path`
    /// (relative to the first preopened dir, leading `/` stripped — the same resolution
    /// [`WriteTextFile`] uses), creating each missing parent segment (so `a/b/c` makes all
    /// three); an existing dir (errno EEXIST = 20) counts as success. On success builds
    /// `Ok(())` (the `len@4 = 0` `materialize_result_ok` convention, IDENTICAL to
    /// [`WriteTextFile`]'s Ok arm), on a `path_create_directory` error builds
    /// `Err(<message>)` (`len@4 = 1`, `@12 = msg`, `@16 tag = 1`). A mkdir IS a filesystem
    /// WRITE, so it REUSES [`Capability::FsWrite`] (NOT a new capability — that would be a
    /// false distinction); counted in cap_witness exactly like [`WriteTextFile`]. Its dst is
    /// a heap Ptr, so the ownership certificate emits an `i` (alloc), balanced by the
    /// caller's scope-end flat `DropListStr` (sound for BOTH arms given the `len@4 = 0` Ok).
    MakeDir,
    /// The WASI `path_remove_directory` / `path_unlink_file` RECURSIVE-remove sequence, packaged
    /// as ONE high-level HEAP-RESULT prim — `args = [path]` (a BORROWED `String` handle, the
    /// caller still owns it), dst = a fresh OWNED `Result[Unit, String]`. Removes the tree rooted
    /// at `path` (relative to the first preopened dir, leading `/` stripped — the same resolution
    /// [`WriteTextFile`] uses): if `path` opens as a directory it RECURSIVELY removes every entry
    /// (a child directory via `path_remove_directory` after it is emptied, a child file via
    /// `path_unlink_file`) then removes the now-empty directory; if it is a file it is unlinked
    /// directly — matching native `fs.remove_all` (`remove_dir_all` for a dir, `remove_file`
    /// otherwise). On success builds `Ok(())` (the `len@4 = 0` `materialize_result_ok` convention,
    /// IDENTICAL to [`WriteTextFile`]'s Ok arm), on a removal error builds `Err(<message>)`
    /// (`len@4 = 1`, `@12 = msg`, `@16 tag = 1`). A remove IS a filesystem WRITE, so it REUSES
    /// [`Capability::FsWrite`] (NOT a new capability — that would be a false distinction); counted
    /// in cap_witness exactly like [`WriteTextFile`]. Its dst is a heap Ptr, so the ownership
    /// certificate emits an `i` (alloc), balanced by the caller's scope-end flat `DropListStr`
    /// (sound for BOTH arms given the `len@4 = 0` Ok).
    RemoveAll,
    /// The WASI `path_filestat_get` existence query, packaged as ONE high-level SCALAR prim —
    /// `args = [path]` (a BORROWED `String` handle, the caller still owns it), dst = a SCALAR
    /// `Bool` (an i64 0/1). Stats `path` (relative to the first preopened dir, leading `/`
    /// stripped — the same resolution [`ReadTextFile`] uses) and yields `1` if a file OR
    /// directory exists there (errno 0), `0` otherwise — matching native `fs.exists`
    /// (`std::path::Path::exists`). UNLIKE every other fs prim this is NOT a heap result: a stat
    /// allocates nothing, so its dst is a plain scalar (NO `materialize_result` block, NO
    /// scope-end drop, NO ownership-cert `i` — it falls in the scalar-result `_ => {}` arm).
    /// A stat IS a filesystem READ, so it REUSES [`Capability::FsRead`] (NOT a new capability —
    /// the SAME accounting as [`ReadTextFile`] → FsRead); counted in cap_witness. Reached only by
    /// the self-hosted `fs.exists`.
    PathExists,
    /// The WASI `path_filestat_get` FULL-stat query — `args = [bufaddr, path]` (a raw scratch
    /// ADDRESS the caller owns — the self-host's 64-byte Bytes data region — plus a BORROWED
    /// `String` handle), dst = the SCALAR errno (i64; 0 = the host wrote the 64-byte WASI
    /// filestat at `bufaddr`: filetype@16, size@32, mtim@48). The self-hosted `fs.stat` reads
    /// the fields off its own scratch via `prim.load*` and builds the FileStat record in
    /// ordinary Almide — the prim stays a thin syscall wrapper (no heap result, no ownership
    /// event; the same scalar-dst discipline as [`PathExists`]). A stat IS a filesystem READ,
    /// so it REUSES [`Capability::FsRead`] (counted in cap_witness). Reached only by the
    /// self-hosted `fs.stat`.
    PathFilestat,
    /// Release one reference of a RAW heap handle (`(call $rc_dec …)`), the inverse of [`RcInc`].
    /// The MECHANISM the self-hosted recursive `value.__drop_value` frees a dynamic Value tree with
    /// (the §4.1-compliant alternative to a hand-written WAT drop): it operates on raw Int handles,
    /// so its ownership cert is EMPTY (a `Prim` is a no-op in verify_ownership) — like `string_eq`.
    /// REUSES the proven `$rc_dec` (no new WAT func). args = [addr], no dst (Unit). TRUSTED like the
    /// inline DropListStr's per-element rc_dec — its leak/double-free safety is the differential
    /// test's burden (a value.stringify round-trip), NOT the ownership cert. Use is contained to the
    /// drop routine.
    RcDec,
    /// Acquire one reference of a RAW heap handle (`(call $rc_inc …)`) — the self-host `value.array`
    /// SHALLOW-COPIES a `List[Value]` by `rc_inc`-ing each element into a new owned list (matching
    /// v0's `items.clone()` observably) so the borrowed `items` param is untouched. args = [addr],
    /// no dst. REUSES the proven `$rc_inc`. Cert no-op (raw handle), trusted like RcDec.
    RcInc,
    /// The FLOAT floor: a `Float` scalar is the i64-uniform value holding the f64 BITS, so
    /// every float op `reinterpret`s i64→f64, computes, and `reinterpret`s back (a compare /
    /// to-int yields a real i64). Scalar, no ownership — the cert is untouched (these are
    /// `Op::Prim`, no-ops in verify_ownership). This opens the whole `float.*` / `math.*`
    /// f64 category for self-host over `prim.fabs` / `prim.fadd` / `prim.f2i` / etc.
    FloatUn(FUnOp),
    FloatBin(FBinOp),
    /// `float.from_int(x)` — the sitofp floor (#806 step 2): ONE
    /// `f64.convert_i64_s` (bits-reinterpreted into the i64-uniform float
    /// slot), replacing the self-host runtime CALL that dominated inlined hot
    /// loops. `args = [int_value]`, dst = the f64 BITS. A pure scalar
    /// conversion — no ownership event, no capability.
    F64FromInt,
    FloatCmp(FCmpOp),
    /// `i64.trunc_sat_f64_s(reinterpret(x))` — Float → Int (saturating truncate, v0's `as i64`).
    FloatToInt,
    /// `reinterpret(f64.convert_i64_s(x))` — Int → Float.
    IntToFloat,
    /// IDENTITY — the raw f64↔i64 BIT reinterpret (`float.to_bits` / `int.bits_to_float`):
    /// the i64-uniform value ALREADY holds the f64 bits, so this is a no-op pass-through.
    FloatBits,
    /// `f32.demote_f64` — Float (f64) → Float32. The narrower f32 value is held as its 32-bit
    /// pattern in the LOW half of the i64 slot (`i32.reinterpret_f32` then zero-extend). Rounds to
    /// nearest, matching Rust's `n as f32`.
    F32Demote,
    /// `f64.promote_f32` — Float32 → Float (f64). Reads the low-32 f32 pattern (`i32.wrap_i64`
    /// then `f32.reinterpret_i32`) and widens exactly.
    F32Promote,
    /// `f32.convert_i64_s` — Int → Float32 directly (single rounding), matching Rust's `n as f32`.
    /// Result is the f32 pattern in the low half of the i64 slot.
    IntToF32,
    /// IDENTITY — Float32 → its 32-bit pattern as an Int. A Float32 value ALREADY holds the f32
    /// bits in the low 32 of the i64 slot (high 32 zero, from F32Demote/IntToF32's zero-extend), so
    /// this is a type-only reinterpret (no-op pass-through), like FloatBits for f64.
    F32Bits,
    /// A binary f32 op over two Float32 values (each the low-32 f32 pattern in its i64 slot):
    /// unwrap → f32 op (per-op f32 rounding, matching native Rust f32 and v0's F32Add family) →
    /// re-wrap. The f64 `FloatBin` on these bit patterns computed garbage (the low-32 f32 bits
    /// reinterpreted as f64 are a denormal).
    F32Bin(FBinOp),
    /// A binary f32 comparison (`f32.eq`/`lt`/… over the low-32 patterns) → i64-uniform Bool.
    F32Cmp(FCmpOp),
    /// A unary f32 op (neg/abs/…) over the low-32 pattern.
    F32Un(FUnOp),
    /// REGION open (region_alloc.rs): `dst` = the packed allocator state
    /// (`bump | freelist << 32`), then the free-list is emptied so every
    /// allocation inside the region is a pure frontier bump — no free-list
    /// block can be captured into (and then leak with) the region.
    RegionSave,
    /// REGION close: restore `bump` and `freelist` from the packed state in
    /// `args[0]` — every block allocated inside the region is reclaimed
    /// wholesale by the frontier reset (nothing escapes: see the
    /// region_alloc.rs qualification), and the free-list is exactly what it
    /// was at entry.
    RegionRestore,
    /// REGION-ONLY compact allocation (region_compact.rs, issue #838 stage 2):
    /// dst = a HEADERLESS `bytes`-byte block. Inside a region the free-list is
    /// empty, so `$alloc` is a pure frontier bump (grow check included) and the
    /// rc/len/cap header + tag slot are provably dead — nothing inside a
    /// qualified family reads them, and the frontier reset is the only free.
    /// `zero` = fill the block with zeroes: the per-region SINGLETON twin,
    /// whose fields must read as the original all-const block's padding (0).
    /// Exists ONLY inside region windows / `__rgn_` clones; a headerless block
    /// must never reach generic runtime code.
    RegionAllocC { bytes: u32, zero: bool },
    /// Compact-block HANDLE field load: `dst = *(i32*)(args[0] + off)` — the
    /// region twin of `LoadHandle` minus the Handle/Add address bridge and the
    /// 8-byte element slot (handles pack as raw 4-byte pointers).
    RegionLoadH { off: u32 },
    /// Compact-block SCALAR field load: `dst = *(i64*)(args[0] + off)`.
    RegionLoadS { off: u32 },
    /// Compact-block HANDLE field store: `*(i32*)(args[0] + off) = args[1]`.
    RegionStoreH { off: u32 },
    /// Compact-block SCALAR field store: `*(i64*)(args[0] + off) = args[1]`.
    RegionStoreS { off: u32 },
    /// Compact-region tag read: `dst = if args[0] == args[1] then tag else
    /// args[2]`. Inside a qualified family every variant value is either THE
    /// per-region nullary-ctor singleton or a compact dynamic block, and every
    /// block is a distinct bump address — pointer identity IS the tag. Chains
    /// (via `args[2]`) when a family carries two singletons.
    RegionTagSel { tag: i64 },
}

/// A unary f64 op (the value is the f64 bits in an i64; render reinterprets around it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FUnOp {
    Abs,
    Sqrt,
    Floor,
    Ceil,
    Neg,
}

/// A binary f64 op.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    /// `f64.copysign(a, b)` — magnitude of `a` with the sign bit of `b` (the basis for an
    /// exact `f64::signum`: `copysign(1.0, x)`, with NaN handled by the caller).
    CopySign,
}

/// An f64 comparison — yields an i64 0/1 (the Bool / `if` condition).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FCmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// A scalar integer binary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntOp {
    Add,
    Sub,
    Mul,
    /// Signed division — traps on divide-by-zero (matching v0's checked `DivInt`).
    Div,
    /// Signed remainder — traps on divide-by-zero (matching v0's checked `ModInt`).
    Mod,
    // Comparisons: produce a Bool scalar (i64 0/1) — the `if` condition. Signed.
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    // Bitwise i64 ops (the int.band/bor/bxor/bshl/bshr floor). Scalar, no ownership.
    And,
    Or,
    Xor,
    Shl,
    /// Arithmetic (sign-extending) shift right, matching v0's `>>` on `i64`.
    Shr,
    /// LOGICAL (zero-filling) shift right (`i64.shr_u`) — for unsigned/bit-width ops like
    /// int.rotate_* which shift the value as a u64. The shift amount is wasm-masked to 0..63.
    ShrU,
}

/// A runtime function the MIR can call. An enum (not a string) so the renderer
/// mapping is TOTAL and the runtime surface is a closed, auditable set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtFn {
    /// `list[index] = value` in place (after a [`Op::MakeUnique`]).
    ListSet,
    /// push a value onto a list in place (after a [`Op::MakeUnique`]); the
    /// result is rebound to `dst` (the buffer may move).
    ListPush,
    /// `println` a list as `label=e0,e1,…`.
    PrintList,
    /// `println` a scalar integer.
    PrintInt,
    /// `println` a heap string (the value-semantics subset's string print). A
    /// WITNESS-LEVEL primitive today: it carries the ownership (borrows the
    /// string handle) and capability ([`Capability::Stdout`]) facts the proven
    /// checker re-verifies, but the renderers do NOT lower it yet — strings are
    /// `Init::Opaque` skeletons in this subset (no content bytes), so a faithful
    /// `print_str` render awaits the string-content lowering brick. Until then a
    /// renderer asked to emit it refuses LOUDLY (the catch-all panic), never
    /// silently — the flight-grade totality rule.
    PrintStr,
}

impl RtFn {
    /// The host [`Capability`] this runtime function reaches, if any. Pure heap
    /// ops touch no host effect; the print ops reach [`Capability::Stdout`]. This
    /// is the SINGLE mapping the capability witness derives "used capabilities"
    /// from — exhaustive, so a new effectful runtime fn cannot silently escape
    /// the sandbox accounting.
    pub const fn capability(self) -> Option<Capability> {
        match self {
            RtFn::ListSet | RtFn::ListPush => None,
            RtFn::PrintList | RtFn::PrintInt | RtFn::PrintStr => Some(Capability::Stdout),
        }
    }
}

/// A host CAPABILITY a function may reach — the unit of the sandbox promise
/// (the 4th flight-grade property, proofs/CapabilityBound.v: a program reaches
/// ONLY the capabilities it declares). A VALUE OBJECT, not a raw id: you write
/// `Capability::Stdout`, never `0`. The stable registry id the proven checker
/// compares is recovered via [`Capability::id`], so the "Stdout = 0" mapping
/// lives in exactly ONE place and MUST match the Coq capability registry. The
/// set is closed and grows only as the runtime gains host effects (fs, net, …).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Writing to standard output (the only host effect the current MIR subset
    /// reaches, via [`RtFn::PrintInt`] / [`RtFn::PrintList`]).
    Stdout,
    /// Reading host ENTROPY — the WASI `random_get` floor ([`PrimKind::RandomGet`]),
    /// reached by the self-hosted `random.int`. The second sandbox exit. A pure `fn`
    /// declares ∅, so it can NEVER reach entropy un-witnessed (the checker REJECTS
    /// `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    Entropy,
    /// Reading the program's COMMAND-LINE ARGUMENTS — the WASI `args_sizes_get` /
    /// `args_get` floor ([`PrimKind::ArgsGetList`]), reached by the self-hosted
    /// `env.args`. The third sandbox exit. Accounted exactly like Entropy/Stdout: a
    /// pure `fn` declares ∅ and so can NEVER read argv un-witnessed (the checker
    /// REJECTS `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    CliArgs,
    /// Reading a FILE from the host filesystem — the WASI `path_open` / `fd_read` floor
    /// ([`PrimKind::ReadTextFile`]), reached by the self-hosted `fs.read_text`. The fourth
    /// sandbox exit. Accounted exactly like CliArgs/Entropy/Stdout: a pure `fn` declares ∅
    /// and so can NEVER read a file un-witnessed (the checker REJECTS `used ⊄ allowed`);
    /// only an `effect fn` (which declares the host caps) may.
    FsRead,
    /// Writing a FILE to the host filesystem — the WASI `path_open(O_CREAT|O_TRUNC)` /
    /// `fd_write` floor ([`PrimKind::WriteTextFile`]), reached by the self-hosted `fs.write`.
    /// The fifth sandbox exit. A STRICTLY GREATER authority than [`Self::FsRead`] (a write
    /// creates/truncates host state), so it is a DISTINCT capability with its own id — never
    /// aliased to FsRead (conflating read and write would be a capability lie: a fn declaring
    /// only read could mutate the filesystem). Accounted exactly like FsRead: a pure `fn`
    /// declares ∅ and so can NEVER write a file un-witnessed (the checker REJECTS
    /// `used ⊄ allowed`); only an `effect fn` (which declares the host caps) may.
    FsWrite,
    /// Reading the host WALL CLOCK — the WASI `clock_time_get` floor
    /// ([`PrimKind::ClockTimeGet`]), reached by the self-hosted `env.unix_timestamp`. The
    /// sixth sandbox exit. A clock read is neither a filesystem effect nor an entropy draw,
    /// so it is a DISTINCT capability with its own id — never aliased to FsRead/FsWrite or
    /// Entropy. Accounted exactly like Entropy/FsRead: a pure `fn` declares ∅ and so can
    /// NEVER read the clock un-witnessed (the checker REJECTS `used ⊄ allowed`); only an
    /// `effect fn` (which declares the host caps) may.
    Clock,
    /// Reading STANDARD INPUT — the WASI `fd_read`-from-fd-0 floor ([`PrimKind::ReadLine`]),
    /// reached by the self-hosted `io.read_line`. The seventh sandbox exit. Reading stdin is
    /// neither a write, a filesystem read, an entropy draw, nor a clock read, so it is a DISTINCT
    /// capability with its own id — never aliased to FsRead/FsWrite/Entropy/Clock (a fn that
    /// consumes the operator's input stream is a real, separately-grantable authority). Accounted
    /// exactly like Entropy/FsRead: a pure `fn` declares ∅ and so can NEVER read stdin
    /// un-witnessed (the checker REJECTS `used ⊄ allowed`); only an `effect fn` (which declares
    /// the host caps) may.
    Stdin,
}

impl Capability {
    /// The stable registry id — the ONLY place a `Capability` becomes a number.
    /// proofs/CapabilityBound.v's checker is GENERIC over `list nat` (a `subset_check`,
    /// no per-capability enumeration), so it needs no edit to admit a new id — only
    /// this mapping must stay injective + stable (Stdout = 0, Entropy = 1, CliArgs = 2,
    /// FsRead = 3, FsWrite = 4, Clock = 5, Stdin = 6).
    pub const fn id(self) -> u32 {
        match self {
            Capability::Stdout => 0,
            Capability::Entropy => 1,
            Capability::CliArgs => 2,
            Capability::FsRead => 3,
            Capability::FsWrite => 4,
            Capability::Clock => 5,
            Capability::Stdin => 6,
        }
    }
}

/// A wasm IMPORT-signature value type — the host-facing valtype an
/// [`Op::CallImport`] argument/result is mapped to from its declared Almide type
/// (Int→`I64`, Float→`F64`, Bool→`I32`, String/heap pointer→`I32`). The MIR is
/// i64-uniform for scalars (a Float local holds the f64 BITS) and i32 for heap
/// handles, so the render coerces each local to/from this valtype at the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmAbi {
    /// A 64-bit integer — the MIR scalar local passes through directly.
    I64,
    /// A 64-bit float — the MIR i64 local holds its bits; reinterpret around the call.
    F64,
    /// A 32-bit integer — a heap pointer (MIR i32, direct) or a Bool (MIR i64, wrapped).
    I32,
}

impl WasmAbi {
    /// The WAT valtype keyword for an import signature.
    pub fn wat(self) -> &'static str {
        match self {
            WasmAbi::I64 => "i64",
            WasmAbi::F64 => "f64",
            WasmAbi::I32 => "i32",
        }
    }
}

/// An argument to a runtime [`Op::Call`] / user [`Op::CallFn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallArg {
    /// A heap handle (borrowed by the call — live-checked, refcount unchanged).
    Handle(ValueId),
    /// A scalar value (a `ValueId` of scalar Repr — no ownership).
    Scalar(ValueId),
    /// An immediate integer (index / value).
    Imm(i64),
    /// An immediate string (a print label).
    Label(String),
}
