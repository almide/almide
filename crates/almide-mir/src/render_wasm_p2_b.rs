
fn render_op_drop_b(op: &Op) -> String {
    match op {
        // RECURSIVE drop of a `value.as_array` Result `Result[List[Value], String]` — the self-hosted
        // `$__drop_result_lv` (value_core.almd) tag-dispatches at the last ref: Ok frees the
        // `List[Value]` payload recursively, Err frees the String, then the block. A flat `DropListStr`
        // would only rc_dec the @12 list handle, leaking its element Values. Single cert `d`.
        Op::DropResultListValue { v } => {
            format!("    (call $__drop_result_lv (local.get {}))\n", local(*v))
        }
        // RECURSIVE drop of a `Result[Value, String]` (the `ok(value.array(...))` shape) — the
        // self-hosted `$__drop_result_value` (value_core.almd) tag-dispatches at the last ref: Ok
        // frees the Value @12 via `$__drop_value` (recursive), Err frees the String @12, then the
        // block. value_core is always linked here (the program built the Value via a `value.*` ctor).
        Op::DropResultValue { v } => {
            format!("    (call $__drop_result_value (local.get {}))\n", local(*v))
        }
        // INLINE recursive drop of a `Result[(String, Int), String]` (toml `parse_key_part`'s
        // `ok((slice, pos))`). Wrapper `[rc][len@4=1][cap@8][@12 payload-handle][@16 tag]`: at the
        // wrapper's last ref (rc==1), tag@16==0 (Ok) frees the `(String, Int)` tuple @12 — at the
        // TUPLE's last ref `rc_dec` its String slot0 @12 (the Int slot1 @20 is scalar), then the tuple
        // block; tag==1 (Err) `rc_dec`s the String @12. THEN the wrapper block (always). Self-contained
        // (no helper ⇒ no value_core link needed); the tuple handle is re-loaded rather than spilled to
        // a scratch local. Cert = the single final wrapper `call $rc_dec` (`d`); the nested frees are
        // the trusted raw-handle routine, leak-loop verified.
        Op::DropResultStrInt { v } => {
            let p = local(*v);
            // @12 (low 32) = payload handle (Ok: the tuple; Err: the String); @16 = tag (0=Ok).
            let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
            // The tuple's String slot0 @12 (read off the tuple handle = `payload`).
            let tup_str = format!("(i32.load (i32.add {payload} (i32.const 12)))");
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                 \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const 0))\n\
                 \x20       (then\n\
                 \x20         (if (i32.eq (i32.load {payload}) (i32.const 1)) (then (call $rc_dec {tup_str})))\n\
                 \x20         (call $rc_dec {payload}))\n\
                 \x20       (else (call $rc_dec {payload})))))\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // Result[(Value, Int), String] (toml parse_val). Wrapper @12 = the (Value,Int) tuple / Err
        // String; @16 = tag. At the wrapper's last ref: Ok → value_core's $__drop_value_tuple frees the
        // tuple recursively (its Value slot via $__drop_value, then the tuple block); Err → rc_dec the
        // String @12; THEN the wrapper block. value_core is linked (the Ok built a Value via value.*).
        Op::DropResultValueInt { v } => {
            let p = local(*v);
            let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                 \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const 0))\n\
                 \x20       (then (call $__drop_value_tuple {payload}))\n\
                 \x20       (else (call $rc_dec {payload})))))\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // Result[(List[Value], Int), String] (toml collect_array_items). Ok → value_core's
        // $__drop_list_value_tuple frees the tuple recursively (its List[Value] slot's element Values,
        // the List block, the tuple block); Err → rc_dec the String @12; THEN the wrapper.
        Op::DropResultListValueInt { v } => {
            let p = local(*v);
            let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                 \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const 0))\n\
                 \x20       (then (call $__drop_list_value_tuple {payload}))\n\
                 \x20       (else (call $rc_dec {payload})))))\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // Result[(List[String], Int), String] (toml parse_key / parse_table_key). Wrapper @12 =
        // (List[String], Int) tuple / Err String; @16 = tag. Ok: at the tuple's last ref, at the
        // List's last ref rc_dec each element String (the inner loop), the List block, then the tuple
        // block; Err: rc_dec the String. THEN the wrapper. Scratch: $dlli = tuple handle, $dllinner =
        // List handle, $dlsi/$dlsn = the element loop (declared in render_wasm.rs when this op present).
        Op::DropResultListStrInt { v } => {
            let p = local(*v);
            let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                 \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const 0))\n\
                 \x20       (then\n\
                 \x20         (local.set $dlli {payload})\n\
                 \x20         (if (i32.eq (i32.load (local.get $dlli)) (i32.const 1)) (then\n\
                 \x20           (local.set $dllinner (i32.load (i32.add (local.get $dlli) (i32.const 12))))\n\
                 \x20           (if (i32.eq (i32.load (local.get $dllinner)) (i32.const 1)) (then\n\
                 \x20             (local.set $dlsi (i32.const 0))\n\
                 \x20             (local.set $dlsn (i32.load (i32.add (local.get $dllinner) (i32.const 4))))\n\
                 \x20             (block $dlsbrk (loop $dlscont\n\
                 \x20               (br_if $dlsbrk (i32.ge_s (local.get $dlsi) (local.get $dlsn)))\n\
                 \x20               (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get $dllinner) (i32.add (i32.const 12) (i32.mul (local.get $dlsi) (i32.const 8)))))))\n\
                 \x20               (local.set $dlsi (i32.add (local.get $dlsi) (i32.const 1)))\n\
                 \x20               (br $dlscont)))))\n\
                 \x20           (call $rc_dec (local.get $dllinner))))\n\
                 \x20         (call $rc_dec (local.get $dlli)))\n\
                 \x20       (else (call $rc_dec {payload})))))\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // `Result[List[String], String]` (fs.list_dir's `ok([name,…])`): the cap-as-tag
        // wrapper `[rc][len@4=1][cap@8=1][@12 payload][@16 tag]`. At the wrapper's last ref
        // (rc==1), Ok (tag@16==0): the @12 payload is a `List[String]` — at ITS last ref
        // `rc_dec` each element String (the [@12 + i*8] slots, len@4), then the List block;
        // Err (tag 1): `rc_dec` the String @12. THEN the wrapper block. A flat `DropListStr`
        // would `rc_dec` the @12 List HANDLE only, leaking the element Strings + the List block.
        // Mirrors `DropResultListStrInt` minus the tuple layer (payload IS the list, not a
        // (list, int) tuple) — reuses the $dlli / $dlsi / $dlsn scratch.
        Op::DropResultListStr { v } => {
            let p = local(*v);
            let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                 \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const 0))\n\
                 \x20       (then\n\
                 \x20         (local.set $dlli {payload})\n\
                 \x20         (if (i32.eq (i32.load (local.get $dlli)) (i32.const 1)) (then\n\
                 \x20           (local.set $dlsi (i32.const 0))\n\
                 \x20           (local.set $dlsn (i32.load (i32.add (local.get $dlli) (i32.const 4))))\n\
                 \x20           (block $dlsbrk (loop $dlscont\n\
                 \x20             (br_if $dlsbrk (i32.ge_s (local.get $dlsi) (local.get $dlsn)))\n\
                 \x20             (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get $dlli) (i32.add (i32.const 12) (i32.mul (local.get $dlsi) (i32.const 8)))))))\n\
                 \x20             (local.set $dlsi (i32.add (local.get $dlsi) (i32.const 1)))\n\
                 \x20             (br $dlscont)))))\n\
                 \x20         (call $rc_dec (local.get $dlli)))\n\
                 \x20       (else (call $rc_dec {payload})))))\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // RECURSIVE drop of a CUSTOM variant (ADT brick 5b) — the GENERATED per-type
        // `$__drop_<ty>` (the `$__drop_value` shape, auto-linked from generated Almide): at the
        // last ref it reads the tag, recursively frees each variant ctor field + rc_dec's each
        // leaf field, then the block. Single cert `d`; the recursion is the trusted prim-only
        // routine (empty cert), verified by the create+drop LEAK LOOP.
        Op::DropVariant { v, ty } => {
            // A cross-module record/variant type carries its module prefix (`types.RunResult`);
            // sanitize the dot to match the generated `__drop_…` fn name (`drop_fn_ident`). For a
            // single-file (dot-free) type this is the identity — byte-identical render.
            format!("    (call $__drop_{} (local.get {}))\n", ty.replace('.', "_"), local(*v))
        }
        // RECURSIVE drop of an Option/Result WRAPPER whose @12 payload is a heap RECORD (the
        // `some({key, val})` / `ok({val, next})` shape). At the wrapper's LAST ref (rc==1), recurse
        // into the record via the generated `$__drop_<drop_fn>` (which at the record's OWN last ref
        // frees its nested heap fields then the record block — a flat `rc_dec` of the @12 handle would
        // free only the record BLOCK, leaking those fields). Then `rc_dec` the wrapper block. The
        // Option shape recurses iff `len@4 > 0` (Some, not None); the Result shape iff `tag@16 == 0`
        // (Ok-record, not an Err String — which is freed by a flat `rc_dec`). Dot-sanitized to match
        // `drop_fn_ident`. Cert = the final wrapper `call $rc_dec` (`d`); the recursion is the trusted
        // generated routine. Mirrors `DropResultValue` (Value payload) / the masked `DropListStr`.
        Op::DropWrapperRec { v, drop_fn, is_result, err_rec } => {
            let p = local(*v);
            let dn = drop_fn.replace('.', "_");
            if *is_result {
                let payload = format!("(i32.load (i32.add (local.get {p}) (i32.const 12)))");
                // The recursive arm rides the tag: Ok-record wrappers (`resrec:`) recurse on
                // tag@16 == 0; the heap-Ok × variant-Err class (`reserr:` — err_rec) recurses
                // on tag@16 == 1 and flat-frees the Ok payload.
                let rec_tag = if *err_rec { 1 } else { 0 };
                format!(
                    "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                     \x20     (if (i32.eq (i32.load (i32.add (local.get {p}) (i32.const 16))) (i32.const {rec_tag}))\n\
                     \x20       (then (call $__drop_{dn} {payload}))\n\
                     \x20       (else (call $rc_dec {payload})))))\n\
                     \x20   (call $rc_dec (local.get {p}))\n"
                )
            } else {
                let payload =
                    format!("(i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.const 12))))");
                format!(
                    "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1)) (then\n\
                     \x20     (if (i32.gt_s (i32.load (i32.add (local.get {p}) (i32.const 4))) (i32.const 0))\n\
                     \x20       (then (call $__drop_{dn} {payload})))))\n\
                     \x20   (call $rc_dec (local.get {p}))\n"
                )
            }
        }
        _ => unreachable!("render_op_drop_b: {op:?} is not in this group"),
    }
}


/// Group 4 of [`render_op`]: COW (`MakeUnique`), `FuncRef`, `ConstInt`, the scalar
/// `Prim` dispatch (IntOp/FloatOp/comparisons/conversions), `SetLocal`, and the
/// no-op tail (`Consume`/`Borrow`/`Const`/`Pure`/the if- and loop-markers rendered
/// stateful by `render_wasm_fn`). Verbatim subset of the original single match.
fn render_op_misc(
    op: &Op,
    func_slots: &BTreeMap<String, u32>,
    floats: &BTreeSet<ValueId>,
    fuser: &mut Fuser,
) -> String {
    match op {
        // COPY-ON-WRITE before an in-place mutation (A1.3-render, refining
        // CowSafety.v): if the block is SHARED (rc > 1), clone it so the mutation
        // touches no alias. The `rc_dec` runs FIRST (rc 2→1 — the alias keeps the
        // original alive, so no temp is needed), then `list_copy` reads the
        // still-live original into a fresh uniquely-owned block. rc == 1 → no-op.
        Op::MakeUnique { v } => format!(
            "    (if (i32.gt_s (i32.load (i32.add (local.get {v}) (i32.const {rc}))) (i32.const 1))\n      (then\n        (call $rc_dec (local.get {v}))\n        (local.set {v} (call $list_copy (local.get {v})))))\n",
            v = local(*v),
            rc = LIST_RC_OFFSET
        ),
        // Still no-ops: Consume MOVES the reference out (the receiver releases it
        // later — no dec at THIS site); Const/Borrow/Pure touch no refcount.
        // A materialized integer constant: set the local to the immediate. (A
        // deferred `Const` renders to nothing — the local keeps the zero default.)
        // A function reference: resolve the lifted function's name to its module
        // function-table slot (its position) and materialize the slot as the scalar value
        // a later CallIndirect dispatches through. Unknown name → slot 0 (defensive).
        Op::FuncRef { dst, name } => {
            let slot = func_slots.get(name).copied().unwrap_or(0);
            format!("    (local.set {} (i64.const {slot}))\n", local(*dst))
        }
        Op::ConstInt { dst, value } => {
            // #806 step 3a: an f64-classified dst materializes the SAME bit
            // pattern as a real f64 hexfloat const (bit-exact).
            if floats.contains(dst) {
                format!(
                    "    (local.set {} (f64.const {}))\n",
                    local(*dst),
                    wat_f64_const(*value as u64)
                )
            } else {
                format!("    (local.set {} (i64.const {value}))\n", local(*dst))
            }
        }
        // A primitive-floor op, hand-mapped INLINE (no preamble func). The MIR is
        // i64-uniform; wrap to i32 at the wasm memory boundary, zero-extend a loaded /
        // returned i32 back to i64. This is the whole trusted floor for raw memory +
        // the fd_write host call — everything else (print_str) is Almide over it.
        Op::Prim { kind, dst, args } => render_op_prim(kind, dst, args, floats, fuser),
        // A scalar reassignment of a stable local — the loop-carried state. Reads `src`,
        // writes the var's own local (reusing the same wasm local is legal: read then set).
        Op::SetLocal { local: l, src } => {
            format!("    (local.set {} {})\n", local(*l), fuser.operand(*src))
        }
        Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::Const { .. }
        | Op::Pure { .. }
        // The if- and loop-markers are rendered STATEFULLY by render_wasm_fn (the
        // flat→nested wasm `if`/`else` and `block`/`loop`); render_op never sees them.
        | Op::IfThen { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd => String::new(),
        _ => unreachable!("render_op_misc: {op:?} is not in this group"),
    }
}

/// The `Op::Prim` arm of [`render_op_misc`], split out separately (it alone was
/// ~290 lines / the dominant share of that group's complexity): a primitive-floor
/// op, hand-mapped INLINE (no preamble func) — memory load/store, WASI syscalls,
/// refcount raw ops, and the full int/float/f32 scalar-op dispatch. Verbatim body,
/// moved wholesale.
fn render_op_prim(
    kind: &PrimKind,
    dst: &Option<ValueId>,
    args: &[ValueId],
    floats: &BTreeSet<ValueId>,
    fuser: &mut Fuser,
) -> String {
    let body = render_op_prim_mem_io(kind, args)
        .unwrap_or_else(|| render_op_prim_float(kind, dst, args, floats, fuser));
    match dst {
        Some(d) => format!("    (local.set {} {body})\n", local(*d)),
        None => format!("    {body}\n"),
    }
}


/// The memory/syscall/refcount half of [`render_op_prim`]: raw `Handle`/`Load`/
/// `Store`/`ElemAddr`, the WASI syscall floor (`Die`/`ProcExit`/`FdWrite`/
/// `RandomGet`/`ClockTimeGet`/`ArgsGet*`/`EnvGet`/fs ops/`ReadLine`/`ReadNBytes`),
/// and raw `RcDec`/`RcInc`. Split further into `_a` (mem/random/clock) and `_b`
/// (WASI CLI-args/env/fs/refcount) — `PrimKind` has no repeated variant across any
/// of these groups, so the split carries none of the guard-order risk a
/// duplicated-discriminant match would. `None` defers to [`render_op_prim_float`]
/// for the remaining (float/f32 arithmetic) `PrimKind`s.
fn render_op_prim_mem_io(kind: &PrimKind, args: &[ValueId]) -> Option<String> {
    render_op_prim_mem_io_a(kind, args).or_else(|| render_op_prim_mem_io_b(kind, args))
}

fn render_op_prim_mem_io_a(kind: &PrimKind, args: &[ValueId]) -> Option<String> {
    let w = |i: usize| format!("(i32.wrap_i64 (local.get {}))", local(args[i]));
    Some(match kind {
                PrimKind::Handle => format!("(i64.extend_i32_u (local.get {}))", local(args[0])),
                PrimKind::Load { width: 1 } => format!("(i64.extend_i32_u (i32.load8_u {}))", w(0)),
                PrimKind::Load { width: 4 } => format!("(i64.extend_i32_u (i32.load {}))", w(0)),
                PrimKind::Load { .. } => format!("(i64.load {})", w(0)),
                // An i32 HANDLE load — NO i64 extend; the dst local is `Ptr` (i32), so the loaded
                // i32 handle is a real String/List pointer (see value_reprs_wasm).
                PrimKind::LoadHandle => format!("(i32.load {})", w(0)),
                PrimKind::Store { width: 1 } => format!("(i32.store8 {} {})", w(0), w(1)),
                PrimKind::Store { width: 4 } => format!("(i32.store {} {})", w(0), w(1)),
                PrimKind::Store { .. } => format!("(i64.store {} (local.get {}))", w(0), local(args[1])),
                // Bounds-checked element ADDRESS via the preamble `$elem_addr` (idx<0 || idx>=cap
                // TRAPs — v0's `a[i]` likewise halts on OOB). Both args wrap to i32 (list ptr,
                // index); the returned i32 address zero-extends back to the i64-uniform dst.
                PrimKind::ElemAddr => {
                    format!("(i64.extend_i32_u (call $elem_addr_chk {} {}))", w(0), w(1))
                }
                PrimKind::Die => format!("(call $__die {})", w(0)),
                // proc_exit(code): the i64 user code wraps to the WASI i32. Never
                // returns — nothing follows on this path at runtime.
                PrimKind::ProcExit => format!("(call $proc_exit {})", w(0)),
                PrimKind::FdWrite => {
                    format!("(i64.extend_i32_u (call $fd_write {} {} {} {}))", w(0), w(1), w(2), w(3))
                }
                // random_get(buf, buf_len) — the WASI entropy floor; fills buf with random bytes,
                // returns an i32 errno that zero-extends back to the i64-uniform dst.
                PrimKind::RandomGet => {
                    format!("(i64.extend_i32_u (call $random_get {} {}))", w(0), w(1))
                }
                // clock_time_get(clock_id, precision, time_ptr) — the WASI wall-clock floor; writes
                // the current clock value (ns) as an i64 at time_ptr, returns an i32 errno that
                // zero-extends back to the i64-uniform dst. NOTE the WASI `precision` param is i64
                // (the requested resolution), so it is passed RAW (the i64-uniform local), NOT
                // i32-wrapped like clock_id / time_ptr — the generic scalar path's blanket
                // `i32.wrap_i64` on every arg would corrupt it, hence this custom arm.
                PrimKind::ClockTimeGet => {
                    format!(
                        "(i64.extend_i32_u (call $clock_time_get {} (local.get {}) {}))",
                        w(0),
                        local(args[1]),
                        w(2)
                    )
                }
        _ => return None,
    })
}

fn render_op_prim_mem_io_b(kind: &PrimKind, args: &[ValueId]) -> Option<String> {
    let w = |i: usize| format!("(i32.wrap_i64 (local.get {}))", local(args[i]));
    Some(match kind {
                // args_get_list() — the WASI CLI-args floor; builds a fresh owned
                // `List[String]` of argv[1..] in the preamble helper. dst is a heap Ptr
                // (i32 handle, value_reprs_wasm), so the call result sets the local DIRECTLY
                // (no i64 extend) — exactly like a LoadHandle.
                PrimKind::ArgsGetList => "(call $args_get_list (i32.const 1))".to_string(),
                PrimKind::ArgsGetListFull => "(call $args_get_list (i32.const 0))".to_string(),
                // env_get(name) — the WASI environ lookup floor; scans KEY=VALUE entries
                // for `name` + '=' and builds a fresh owned `Option[String]` in the
                // preamble helper. Same heap-Ptr name arg + heap-Ptr dst conventions as
                // ReadTextFile (the i32 handle passes DIRECTLY, no wrap).
                PrimKind::EnvGet => {
                    format!("(call $env_get (local.get {}))", local(args[0]))
                }
                // read_text_file(path) — the WASI file-read floor; opens + reads the file at
                // `path` and builds a fresh owned `Result[String, String]` in the preamble helper.
                // The path arg is a heap Ptr local (i32 handle, like a $list ptr), passed DIRECTLY
                // (no i32.wrap — it is already i32, like `Handle`'s arg). dst is a heap Ptr
                // (value_reprs_wasm), so the call result sets the local directly (no i64 extend).
                PrimKind::ReadTextFile => {
                    format!("(call $read_text_file (local.get {}))", local(args[0]))
                }
                // read_dir(path) — the WASI directory-listing floor; path_open(O_DIRECTORY) +
                // fd_readdir, parses the dirent buffer (skipping `.`/`..`), sorts the names, and
                // builds a fresh owned `Result[List[String], String]` in the preamble helper.
                // Same heap-Ptr path arg + heap-Ptr dst conventions as ReadTextFile.
                PrimKind::ReadDir => {
                    format!("(call $read_dir (local.get {}))", local(args[0]))
                }
                // write_text_file(path, content) — the WASI file-WRITE floor; path_open(O_CREAT|
                // O_TRUNC) + fd_write of `content`'s bytes, then builds a fresh owned
                // `Result[Unit, String]` (Ok(()) / Err) in the preamble helper. Both args are heap
                // Ptr locals (i32 handles), passed DIRECTLY (no i32.wrap). dst is a heap Ptr
                // (value_reprs_wasm), so the call result sets the local directly (no i64 extend).
                PrimKind::WriteTextFile => {
                    format!(
                        "(call $write_text_file (local.get {}) (local.get {}))",
                        local(args[0]),
                        local(args[1])
                    )
                }
                // make_dir(path) — the WASI directory-CREATE floor; recursive
                // path_create_directory, then builds a fresh owned `Result[Unit, String]`
                // (Ok(()) / Err) in the preamble helper. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). dst is a heap Ptr (value_reprs_wasm), so
                // the call result sets the local directly (no i64 extend) — mirror WriteTextFile.
                PrimKind::MakeDir => {
                    format!("(call $make_dir (local.get {}))", local(args[0]))
                }
                // remove_all(path) — the WASI recursive-remove floor; recursively unlinks files +
                // removes directories under `path`, then builds a fresh owned `Result[Unit, String]`
                // (Ok(()) / Err) in the preamble helper. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). dst is a heap Ptr (value_reprs_wasm), so
                // the call result sets the local directly (no i64 extend) — mirror MakeDir.
                PrimKind::RemoveAll => {
                    format!("(call $remove_all (local.get {}))", local(args[0]))
                }
                // path_exists(path) — the WASI path-stat floor; path_filestat_get on `path`,
                // yielding 1 if it exists (errno 0) else 0. The path arg is a heap Ptr local (i32
                // handle), passed DIRECTLY (no i32.wrap). UNLIKE the heap-result fs prims, dst is a
                // SCALAR Bool (an i64 local), so the i32 0/1 the $path_exists func returns is
                // i64.extend'd into it — exactly the FloatCmp scalar-Bool widening discipline.
                PrimKind::PathExists => {
                    format!("(i64.extend_i32_u (call $path_exists (local.get {})))", local(args[0]))
                }
                // path_filestat(bufaddr, path) — the WASI FULL-stat floor; path_filestat_get on
                // `path` writing the 64-byte filestat at `bufaddr`. The bufaddr arg is an i64
                // scalar local (wrapped to the i32 address); the path arg is a heap Ptr local
                // (i32 handle, passed directly). dst is the SCALAR errno (i64.extend'd) — the
                // PathExists scalar-result discipline.
                PrimKind::PathFilestat => {
                    format!(
                        "(i64.extend_i32_u (call $path_filestat_q (i32.wrap_i64 (local.get {})) (local.get {})))",
                        local(args[0]),
                        local(args[1])
                    )
                }
                // read_line() — the WASI stdin-line floor; reads fd 0 byte-by-byte until '\n'/EOF
                // and builds a fresh owned canonical `String` (newline excluded, trailing '\r'
                // stripped) in the preamble helper. NO args. dst is a heap Ptr (value_reprs_wasm),
                // so the call result sets the local directly (no i64 extend) — like ArgsGetList.
                PrimKind::ReadLine => "(call $read_line)".to_string(),
                // read_n_bytes(n) — the WASI stdin-N-bytes floor; reads up to n bytes from fd 0 into a
                // fresh owned Bytes block (the byte-buffer layout, built in the preamble helper). The
                // n arg is an Int (i64 local), wrapped to i32 for the byte count; dst is a heap Ptr.
                PrimKind::ReadNBytes => {
                    format!("(call $read_n_bytes (i32.wrap_i64 (local.get {})))", local(args[0]))
                }
                // RAW refcount ops (the self-host drop/copy mechanism) — reuse the proven $rc_dec/
                // $rc_inc on the i32-wrapped handle. dst is None (Unit), so the `match dst` below
                // emits the call as a STATEMENT (no local.set).
                PrimKind::RcDec => format!("(call $rc_dec {})", w(0)),
                PrimKind::RcInc => format!("(call $rc_inc {})", w(0)),
        _ => return None,
    })
}


/// The float/f32 arithmetic half of [`render_op_prim`]: `F64FromInt`/`FloatUn`/
/// `FloatBin`/`FloatCmp`/`FloatToInt`/`IntToFloat`/`FloatBits`, and the f32-narrow
/// mirror (`F32Demote`/`F32Promote`/`IntToF32`/`F32Bits`/`F32Bin`/`F32Cmp`/`F32Un`).

/// The float/f32 arithmetic half of [`render_op_prim`], split further into `_f64`
/// (`F64FromInt`/`FloatUn`/`FloatBin`/`FloatCmp`/`FloatToInt`/`IntToFloat`/
/// `FloatBits`) and `_f32` (the f32-narrow mirror: `F32Demote`/`F32Promote`/
/// `IntToF32`/`F32Bits`/`F32Bin`/`F32Cmp`/`F32Un`) — `PrimKind` has no repeated
/// variant across the two, so the split carries none of the guard-order risk a
/// duplicated-discriminant match would.
fn render_op_prim_float(
    kind: &PrimKind,
    dst: &Option<ValueId>,
    args: &[ValueId],
    floats: &BTreeSet<ValueId>,
    fuser: &mut Fuser,
) -> String {
    render_op_prim_f64(kind, dst, args, floats, fuser)
        .unwrap_or_else(|| render_op_prim_f32(kind, args))
}

fn render_op_prim_f64(
    kind: &PrimKind,
    dst: &Option<ValueId>,
    args: &[ValueId],
    floats: &BTreeSet<ValueId>,
    fuser: &mut Fuser,
) -> Option<String> {
    Some(match kind {
                // `float.from_int` — the single-instruction sitofp floor (#806).
                // An f64-classified dst (step 3a) takes the convert result directly.
                PrimKind::F64FromInt => {
                    let conv = format!("(f64.convert_i64_s {})", fuser.operand(args[0]));
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        conv
                    } else {
                        format!("(i64.reinterpret_f64 {conv})")
                    }
                }
                // FLOAT floor: the i64 value holds the f64 bits — reinterpret around the
                // op. #806 step 3a: an f64-CLASSIFIED operand is read bare (it is a real
                // f64 local), and an f64-classified dst takes the f64 result directly —
                // the hot-loop shape with ZERO reinterprets. Operands splice pending
                // single-use defs (step 3c).
                PrimKind::FloatUn(op) => {
                    let x = float_operand(fuser, floats, args[0]);
                    let inner = match op {
                        FUnOp::Abs => format!("(f64.abs {x})"),
                        FUnOp::Sqrt => format!("(f64.sqrt {x})"),
                        FUnOp::Floor => format!("(f64.floor {x})"),
                        FUnOp::Ceil => format!("(f64.ceil {x})"),
                        FUnOp::Neg => format!("(f64.neg {x})"),
                    };
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        inner
                    } else {
                        format!("(i64.reinterpret_f64 {inner})")
                    }
                }
                PrimKind::FloatBin(op) => {
                    let a = float_operand(fuser, floats, args[0]);
                    let b = float_operand(fuser, floats, args[1]);
                    let instr = match op {
                        FBinOp::Add => "f64.add",
                        FBinOp::Sub => "f64.sub",
                        FBinOp::Mul => "f64.mul",
                        FBinOp::Div => "f64.div",
                        FBinOp::Min => "f64.min",
                        FBinOp::Max => "f64.max",
                        FBinOp::CopySign => "f64.copysign",
                    };
                    let inner = format!("({instr} {a} {b})");
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        inner
                    } else {
                        format!("(i64.reinterpret_f64 {inner})")
                    }
                }
                PrimKind::FloatCmp(op) => {
                    let a = float_operand(fuser, floats, args[0]);
                    let b = float_operand(fuser, floats, args[1]);
                    let instr = match op {
                        FCmpOp::Lt => "f64.lt",
                        FCmpOp::Le => "f64.le",
                        FCmpOp::Gt => "f64.gt",
                        FCmpOp::Ge => "f64.ge",
                        FCmpOp::Eq => "f64.eq",
                        FCmpOp::Ne => "f64.ne",
                    };
                    // f64 compare yields an i32 0/1 — extend to the i64-uniform Bool.
                    format!("(i64.extend_i32_u ({instr} {a} {b}))")
                }
                // SATURATING float→int (i64.trunc_SAT_f64_s), matching Rust's `as` cast (v0): NaN → 0,
                // > i64::MAX → i64::MAX, < i64::MIN → i64::MIN — NO trap. The plain `i64.trunc_f64_s`
                // traps on NaN/inf/out-of-range, diverging from v0 (and float_to_uint64.almd already
                // assumes the saturating form for its f >= 2^64 → u64::MAX path).
                PrimKind::FloatToInt => {
                    let x = float_operand(fuser, floats, args[0]);
                    format!("(i64.trunc_sat_f64_s {x})")
                }
                PrimKind::IntToFloat => {
                    let conv = format!("(f64.convert_i64_s {})", fuser.operand(args[0]));
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        conv
                    } else {
                        format!("(i64.reinterpret_f64 {conv})")
                    }
                }
                // to_bits / bits_to_float: the value IS the bits — identity pass-through.
                PrimKind::FloatBits => format!("(local.get {})", local(args[0])),
        _ => return None,
    })
}

fn render_op_prim_f32(kind: &PrimKind, args: &[ValueId]) -> String {
    match kind {
                // f64 → f32 (demote, round-to-nearest), held as the low-32 f32 bit pattern.
                PrimKind::F32Demote => format!(
                    "(i64.extend_i32_u (i32.reinterpret_f32 (f32.demote_f64 (f64.reinterpret_i64 (local.get {})))))",
                    local(args[0])
                ),
                // low-32 f32 pattern → f64 (promote, exact). Serves both float.from_float32 and
                // int.bits_to_f32 (`f32::from_bits(bits as u32) as f64`).
                PrimKind::F32Promote => format!(
                    "(i64.reinterpret_f64 (f64.promote_f32 (f32.reinterpret_i32 (i32.wrap_i64 (local.get {})))))",
                    local(args[0])
                ),
                // i64 → f32 directly (single rounding, v0's `n as f32`), held as the low-32 f32 pattern.
                PrimKind::IntToF32 => format!(
                    "(i64.extend_i32_u (i32.reinterpret_f32 (f32.convert_i64_s (local.get {}))))",
                    local(args[0])
                ),
                // Float32 → its 32-bit pattern as Int: identity (the value IS the low-32 bits).
                PrimKind::F32Bits => format!("(local.get {})", local(args[0])),
                // f32 arithmetic over two low-32 patterns: unwrap → f32 op → re-wrap. Per-op
                // f32 rounding matches native Rust f32 and v0's F32Add family.
                PrimKind::F32Bin(op) => {
                    let f = |a: usize| {
                        format!("(f32.reinterpret_i32 (i32.wrap_i64 (local.get {})))", local(args[a]))
                    };
                    let instr = match op {
                        FBinOp::Add => "f32.add",
                        FBinOp::Sub => "f32.sub",
                        FBinOp::Mul => "f32.mul",
                        FBinOp::Div => "f32.div",
                        FBinOp::Min => "f32.min",
                        FBinOp::Max => "f32.max",
                        FBinOp::CopySign => "f32.copysign",
                    };
                    format!(
                        "(i64.extend_i32_u (i32.reinterpret_f32 ({instr} {} {})))",
                        f(0),
                        f(1)
                    )
                }
                PrimKind::F32Cmp(op) => {
                    let f = |a: usize| {
                        format!("(f32.reinterpret_i32 (i32.wrap_i64 (local.get {})))", local(args[a]))
                    };
                    let instr = match op {
                        FCmpOp::Lt => "f32.lt",
                        FCmpOp::Le => "f32.le",
                        FCmpOp::Gt => "f32.gt",
                        FCmpOp::Ge => "f32.ge",
                        FCmpOp::Eq => "f32.eq",
                        FCmpOp::Ne => "f32.ne",
                    };
                    format!("(i64.extend_i32_u ({instr} {} {}))", f(0), f(1))
                }
                PrimKind::F32Un(op) => {
                    let x =
                        format!("(f32.reinterpret_i32 (i32.wrap_i64 (local.get {})))", local(args[0]));
                    let inner = match op {
                        FUnOp::Abs => format!("(f32.abs {x})"),
                        FUnOp::Sqrt => format!("(f32.sqrt {x})"),
                        FUnOp::Floor => format!("(f32.floor {x})"),
                        FUnOp::Ceil => format!("(f32.ceil {x})"),
                        FUnOp::Neg => format!("(f32.neg {x})"),
                    };
                    format!("(i64.extend_i32_u (i32.reinterpret_f32 {inner}))")
                }
        _ => unreachable!("render_op_prim_f32: {kind:?} is not in this group"),
    }
}

