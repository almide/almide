fn render_op(
    op: &Op,
    label_off: &BTreeMap<String, (u32, u32)>,
    func_slots: &BTreeMap<String, u32>,
    param_counts: &BTreeMap<String, usize>,
    masks: &BTreeMap<ValueId, Vec<usize>>,
    reprs: &BTreeMap<ValueId, Repr>,
    floats: &BTreeSet<ValueId>,
) -> String {
    match op {
        // A STRING literal — a heap block `[rc][len][cap][utf8 bytes...]` (same header
        // as a list; len/cap are BYTE counts). $alloc the block, set the header, store
        // each byte. Real DATA reproduced from the MIR (the un-defer, ③ exec slice).
        Op::Alloc { dst, init: Init::Str(string), .. } => {
            let bytes = string.as_bytes();
            let blen = bytes.len() as u32;
            // A String block is sized LIST-COMPATIBLY so the free-list reuses it: `cap` is
            // the ELEMENT count `ceil(blen / ELEM_SIZE)` (rounded up so the bytes fit), and
            // the allocation is `LIST_HEADER + cap*ELEM_SIZE` — exactly what the `$alloc`
            // reuse check recomputes from `cap`. `len` stays the BYTE length (what print
            // reads). Storing `cap = blen` (a byte count) made the reuse formula
            // `LIST_HEADER + blen*ELEM_SIZE` overshoot the real size, so freed String
            // blocks were never reclaimed and a String-allocating loop leaked → OOM.
            let cap_elems = blen.div_ceil(ELEM_SIZE);
            let total = LIST_HEADER + cap_elems * ELEM_SIZE;
            let mut s = format!(
                "    (local.set {d} (call $alloc (i32.const {total})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) (i32.const {blen}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.const {cap_elems}))\n",
                d = local(*dst),
            );
            for (i, b) in bytes.iter().enumerate() {
                let off = LIST_HEADER + i as u32;
                s.push_str(&format!(
                    "    (i32.store8 (i32.add (local.get {d}) (i32.const {off})) (i32.const {b}))\n",
                    d = local(*dst),
                ));
            }
            s
        }
        // A BYTES constant — physically the SAME `[rc][len][cap][bytes…]` block as a String
        // literal (len/cap are byte counts), but the source bytes are arbitrary (not UTF-8).
        // Materializes a const Bytes module global (the aes S-box) with no runtime call.
        Op::Alloc { dst, init: Init::Bytes(data), .. } => {
            let blen = data.len() as u32;
            let cap_elems = blen.div_ceil(ELEM_SIZE);
            let total = LIST_HEADER + cap_elems * ELEM_SIZE;
            let mut s = format!(
                "    (local.set {d} (call $alloc (i32.const {total})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) (i32.const {blen}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.const {cap_elems}))\n",
                d = local(*dst),
            );
            for (i, b) in data.iter().enumerate() {
                let off = LIST_HEADER + i as u32;
                s.push_str(&format!(
                    "    (i32.store8 (i32.add (local.get {d}) (i32.const {off})) (i32.const {b}))\n",
                    d = local(*dst),
                ));
            }
            s
        }
        // A runtime-sized OWNED String of `len` bytes: round the byte length up to
        // ELEM_SIZE (list-compatible so the free-list reuses it), $alloc, set rc=1 + the
        // byte len + the element cap. The data is left UNINITIALIZED for the caller to fill
        // via `prim.store8` (the self-host `int.to_string` builder). Cert: one `Alloc` = i,
        // init-agnostic — a fresh owned object, no checker change.
        Op::Alloc { dst, init: Init::DynStr { len }, .. } => {
            let wlen = format!("(i32.wrap_i64 (local.get {}))", local(*len));
            // round byte len up to ELEM_SIZE: (len + ELEM_SIZE-1) & ~(ELEM_SIZE-1)
            let rounded = format!(
                "(i32.and (i32.add {wlen} (i32.const {add})) (i32.const {mask}))",
                add = ELEM_SIZE - 1,
                mask = -(ELEM_SIZE as i32),
            );
            format!(
                "    (local.set {d} (call $alloc (i32.add (i32.const {LIST_HEADER}) {rounded})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) {wlen})\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.shr_u {rounded} (i32.const {shift})))\n",
                d = local(*dst),
                shift = ELEM_SIZE.trailing_zeros(),
            )
        }
        // A materialized `Some(payload)`: a 1-element list (len=1) whose `data[0]` holds
        // the scalar payload. `None` is the 0-element list (`Init::Opaque`, len=0). A
        // variant `match` reads `len` as the tag and `data[0]` as the payload. Cert: one
        // `Alloc` = i, init-agnostic (no checker change).
        Op::Alloc { dst, init: Init::OptSome { payload }, .. } => {
            let cap = 1 + PUSH_HEADROOM;
            format!(
                "    (local.set {d} (call $list_new (i32.const 1) (i32.const {cap})))\n\
                 \x20   (call $list_set (local.get {d}) (i32.const 0) (local.get {p}))\n",
                d = local(*dst),
                p = local(*payload),
            )
        }
        // A runtime-sized OWNED `List[Int]` of `len` i64 slots: $alloc `LIST_HEADER +
        // len*ELEM_SIZE` bytes, set rc=1 + len + cap (= the element count). Elements are
        // left UNINITIALIZED for the caller to fill via `prim.store64`. The list-building
        // sibling of `DynStr`. Cert: one `Alloc` = i, init-agnostic — no checker change.
        // A DynList (List[Int], scalar slots) OR a DynListStr (List[String], heap-handle
        // slots) — physically IDENTICAL: alloc `LIST_HEADER + len*ELEM_SIZE`, rc=1, len=cap.
        // (The DropListStr free is what distinguishes the nested-ownership variant.)
        // The rung-4 SCALAR-list literal — the SAME `[rc][len][cap][slots…]` block the
        // inline `Alloc{DynList}`+store sequence built (byte-behavior identical): alloc
        // len==cap==N, store each raw i64 slot value at its offset. One op, so the
        // native leg can map it to `vec![…]` without prim-idiom guessing.
        Op::ListLit { dst, elems } => {
            let n = elems.len() as u32;
            let total = LIST_HEADER + n * ELEM_SIZE;
            let mut s = format!(
                "    (local.set {d} (call $alloc (i32.const {total})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) (i32.const {n}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.const {n}))\n",
                d = local(*dst),
            );
            for (i, e) in elems.iter().enumerate() {
                let off = LIST_HEADER + i as u32 * ELEM_SIZE;
                // #806 step 3a: an f64-classified element crosses back to the
                // i64 slot with ONE boundary reinterpret (amortized outside the
                // hot arithmetic; bit-exact).
                let ev = if floats.contains(e) {
                    format!("(i64.reinterpret_f64 (local.get {}))", local(*e))
                } else {
                    format!("(local.get {})", local(*e))
                };
                s.push_str(&format!(
                    "    (i64.store (i32.add (local.get {d}) (i32.const {off})) {ev})\n",
                    d = local(*dst),
                ));
            }
            s
        }
        // The rung-4 bounds-checked SCALAR element load. The check + address are
        // INLINE-EXPANDED (#806): the old `call $elem_addr_chk` put a function call
        // in every `v[j]` of a hot loop (~17x vs native on spectralnorm — wasmtime
        // does not inline across wasm calls), where native's LLVM reduces the same
        // check to a few instructions. The expansion is byte-for-byte the SAME
        // semantics as `$elem_addr_chk`: idx<0 || idx>=LEN aborts with the
        // native-identical bounds message (never silent corruption), else
        // list + HEADER + idx*ELEM_SIZE. Operands are locals, so re-evaluating
        // them costs nothing and no scratch local is needed.
        Op::ListGetScalar { dst, list, idx } => {
            // #806 step 3a: an f64-classified dst loads the slot as a REAL f64
            // (`f64.load` moves the same 8 bytes bit-exactly — no reinterpret).
            let load = if floats.contains(dst) { "f64.load" } else { "i64.load" };
            format!(
                "    (if (i32.or (i32.lt_s (i32.wrap_i64 (local.get {i})) (i32.const 0))\n\
                 \x20                (i32.ge_s (i32.wrap_i64 (local.get {i}))\n\
                 \x20                          (i32.load (i32.add (local.get {l}) (i32.const {LIST_LEN_OFFSET})))))\n\
                 \x20     (then (call $__div_trap (i32.const {BOUNDS_MSG_ADDR}) (i32.const 27))))\n\
                 \x20   (local.set {d} ({load} (i32.add (i32.add (local.get {l}) (i32.const {LIST_HEADER}))\n\
                 \x20                                     (i32.mul (i32.wrap_i64 (local.get {i})) (i32.const {ELEM_SIZE})))))\n",
                d = local(*dst),
                l = local(*list),
                i = local(*idx),
            )
        }
        // The rung-4 bounds-checked SCALAR element store (COW is the caller's
        // MakeUnique before this op) — the inline-expanded twin of the load above.
        Op::ListSetScalar { list, idx, val } => {
            // #806 step 3a: an f64-classified val stores as a REAL f64 (bit-exact).
            let store = if floats.contains(val) { "f64.store" } else { "i64.store" };
            format!(
                "    (if (i32.or (i32.lt_s (i32.wrap_i64 (local.get {i})) (i32.const 0))\n\
                 \x20                (i32.ge_s (i32.wrap_i64 (local.get {i}))\n\
                 \x20                          (i32.load (i32.add (local.get {l}) (i32.const {LIST_LEN_OFFSET})))))\n\
                 \x20     (then (call $__div_trap (i32.const {BOUNDS_MSG_ADDR}) (i32.const 27))))\n\
                 \x20   ({store} (i32.add (i32.add (local.get {l}) (i32.const {LIST_HEADER}))\n\
                 \x20                       (i32.mul (i32.wrap_i64 (local.get {i})) (i32.const {ELEM_SIZE})))\n\
                 \x20              (local.get {v}))\n",
                l = local(*list),
                i = local(*idx),
                v = local(*val),
            )
        }
        Op::Alloc { dst, init: Init::DynList { len } | Init::DynListStr { len }, .. } => {
            let wlen = format!("(i32.wrap_i64 (local.get {}))", local(*len));
            let bytes = format!("(i32.mul {wlen} (i32.const {ELEM_SIZE}))");
            format!(
                "    (local.set {d} (call $alloc (i32.add (i32.const {LIST_HEADER}) {bytes})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) {wlen})\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) {wlen})\n",
                d = local(*dst),
            )
        }
        // `None` SIZED LIKE `OptSome` (len 0, cap 1+headroom) so the size-bucketed free-list
        // can REUSE one block between a closure's Some and None results (distinct sizes would
        // fragment the head-only `$alloc` free-list and grow memory). len 0 reads as None.
        Op::Alloc { dst, init: Init::OptNone, .. } => {
            let cap = 1 + PUSH_HEADROOM;
            format!(
                "    (local.set {d} (call $list_new (i32.const 0) (i32.const {cap})))\n",
                d = local(*dst),
            )
        }
        Op::Alloc { dst, init, .. } => {
            let elems: &[i64] = match init {
                Init::IntList(e) => e,
                Init::Opaque
                | Init::Str(_)
                | Init::Bytes(_)
                | Init::DynStr { .. }
                | Init::OptSome { .. }
                | Init::OptNone
                | Init::DynList { .. }
                | Init::DynListStr { .. } => &[],
            };
            let len = elems.len() as u32;
            let cap = len + PUSH_HEADROOM;
            let mut s = format!(
                "    (local.set {d} (call $list_new (i32.const {len}) (i32.const {cap})))\n",
                d = local(*dst)
            );
            for (i, e) in elems.iter().enumerate() {
                s.push_str(&format!(
                    "    (call $list_set (local.get {d}) (i32.const {i}) (i64.const {e}))\n",
                    d = local(*dst)
                ));
            }
            s
        }
        // An alias SHARES the object and bumps its refcount (A1.3-render): dst and
        // src become two handles to the SAME block, rc += 1 — matching the cert's
        // Alias = +1 and exercising the proven rc machine on a shared cell (whereas
        // eager-copy kept every cell at 1). In-place mutation is guarded by cow.
        Op::Dup { dst, src } => format!(
            "    (local.set {d} (local.get {s}))\n    (call $rc_inc (local.get {s}))\n",
            d = local(*dst),
            s = local(*src)
        ),
        // A runtime call → a wasm `call` of the (bootstrap) runtime function.
        Op::Call { dst, func, args, .. } => render_call(*dst, func, args, label_off),
        Op::IntBinOp { dst, op, a, b } => {
            let args = format!("(local.get {}) (local.get {})", local(*a), local(*b));
            // CHECKED division/remainder: divisor 0 / MIN÷-1 abort via $__div_trap
            // with the native-identical stderr line + exit 1 (C-001/C-035) — never a
            // bare i64.div_s hard trap (exit 134, no message). The checks + op are
            // INLINE-EXPANDED (#806): the old `call $__chk_div` put a function call
            // in every hot-loop `/`/`%` (wasmtime does not inline across wasm
            // calls); the expansion is instruction-for-instruction the SAME
            // semantics as `$__chk_div`/`$__chk_rem`. Operands are locals, so the
            // re-evaluations cost nothing and no scratch local is needed.
            if matches!(op, IntOp::Div | IntOp::Mod) {
                let instr = if matches!(op, IntOp::Div) { "i64.div_s" } else { "i64.rem_s" };
                return format!(
                    "    (if (i64.eqz (local.get {b}))\n\
                     \x20     (then (call $__div_trap (i32.const {DIVZERO_MSG_ADDR}) (i32.const 24))))\n\
                     \x20   (if (i32.and (i64.eq (local.get {a}) (i64.const -9223372036854775808))\n\
                     \x20                (i64.eq (local.get {b}) (i64.const -1)))\n\
                     \x20     (then (call $__div_trap (i32.const {OVERFLOW_MSG_ADDR}) (i32.const 24))))\n\
                     \x20   (local.set {d} ({instr} {args}))\n",
                    a = local(*a),
                    b = local(*b),
                    d = local(*dst),
                );
            }
            // A comparison yields an i32 0/1 → zero-extend to the i64 scalar model.
            let expr = match op {
                IntOp::Add => format!("(i64.add {args})"),
                IntOp::Sub => format!("(i64.sub {args})"),
                IntOp::Mul => format!("(i64.mul {args})"),
                IntOp::Div | IntOp::Mod => unreachable!("inline-expanded above"),
                IntOp::Lt => format!("(i64.extend_i32_u (i64.lt_s {args}))"),
                IntOp::Le => format!("(i64.extend_i32_u (i64.le_s {args}))"),
                IntOp::Gt => format!("(i64.extend_i32_u (i64.gt_s {args}))"),
                IntOp::Ge => format!("(i64.extend_i32_u (i64.ge_s {args}))"),
                IntOp::Eq => format!("(i64.extend_i32_u (i64.eq {args}))"),
                IntOp::Ne => format!("(i64.extend_i32_u (i64.ne {args}))"),
                IntOp::And => format!("(i64.and {args})"),
                IntOp::Or => format!("(i64.or {args})"),
                IntOp::Xor => format!("(i64.xor {args})"),
                IntOp::Shl => format!("(i64.shl {args})"),
                IntOp::Shr => format!("(i64.shr_s {args})"),
                IntOp::ShrU => format!("(i64.shr_u {args})"),
            };
            format!("    (local.set {d} {expr})\n", d = local(*dst))
        }
        // An indirect (closure) call: push the args, then the table index, and dispatch
        // through the module function table with the closure signature OF THIS ARITY
        // (`$closure_fnN`, N = arg count). The table + every `(type $closure_fnN)` are
        // emitted by render_wasm_program for each arity present; `table_idx` is the runtime
        // slot of the lifted lambda.
        Op::CallIndirect { dst, table_idx, args, result } => {
            // The closure ABI is uniform i64 (`$closure_fnN` = N i64 params). A HEAP arg (a Ptr,
            // an i32 local) is WIDENED to i64 to match; the lambda narrows it back at entry
            // (render_wasm_fn's lambda heap-param coercion).
            let argstr = args
                .iter()
                .map(|a| match a {
                    // Widen only a genuinely-i32 Ptr local; an i64 (address-repr'd)
                    // handle already matches the uniform closure ABI.
                    CallArg::Handle(v) if reprs.get(v).map_or(true, |r| r.is_heap()) => {
                        format!("(i64.extend_i32_u (local.get {}))", local(*v))
                    }
                    other => render_arg_wasm(other, reprs),
                })
                .collect::<Vec<_>>()
                .join(" ");
            let arity = args.len();
            // Pick the closure type by arity AND result class: `_v` = void (a `() -> Unit`
            // closure — the lifted lambda has NO wasm result, so the dispatch type must be
            // resultless and the call must NOT be dropped), `_h` = heap/i32, else scalar i64.
            let suffix = match result {
                None => "_v",
                Some(r) if r.is_heap() => "_h",
                Some(_) => "",
            };
            // The table index is a wasm i32; the MIR value is the uniform i64, so wrap it.
            let call = format!(
                "(call_indirect (type $closure_fn{arity}{suffix}) {argstr} (i32.wrap_i64 (local.get {})))",
                local(*table_idx)
            );
            match (dst, result) {
                (Some(d), _) => format!("    (local.set {} {call})\n", local(*d)),
                (None, None) => format!("    {call}\n"),
                (None, Some(_)) => format!("    (drop {call})\n"),
            }
        }
        Op::CallFn { dst, name, args, result } => {
            // A caps-accounting ELIDED-CALL MARKER (`record_elided_calls`) is an
            // `Op::CallFn { dst: None, args: [], result: None }` whose NAME carries
            // the elided callee's caps identity — it must keep that name for the
            // caps gate, but it must NOT render as a real `(call $name)`: when
            // `$name` declares parameters, a 0-arg call underflows the wasm stack
            // and wasmtime rejects the module. Render NOTHING for such a marker.
            //
            // A GENUINE 0-arg void call to a 0-PARAMETER function has the IDENTICAL
            // shape (`dst:None, args:[], result:None`) and IS valid wasm — it must
            // still render. The discriminator: a real call always supplies its
            // callee's params, so only a marker calls a param-taking function with
            // zero args.
            let is_elided_marker = dst.is_none()
                && args.is_empty()
                && result.is_none()
                && param_counts.get(name).copied().unwrap_or(0) > 0;
            if is_elided_marker {
                return String::new();
            }
            let argstr =
                args.iter().map(|a| render_arg_wasm(a, reprs)).collect::<Vec<_>>().join(" ");
            match dst {
                Some(d) => format!("    (local.set {} (call ${name} {argstr}))\n", local(*d)),
                None => format!("    (call ${name} {argstr})\n"),
            }
        }
        // A host wasm IMPORT call (`@extern(wasm, module, name)`). Emit a `(call
        // $__import_module_name …)`; the matching `(import …)` is declared at module
        // scope by render_wasm_program. The MIR is i64-uniform for scalars / i32 for
        // heap handles, so each arg is COERCED to its import valtype (`abi`, parallel to
        // `args`): a Float arg's i64 local holds the f64 BITS → `f64.reinterpret_i64`; a
        // Bool arg → `i32.wrap_i64`; an Int/heap arg passes through. The result is
        // coerced back to the MIR dst valtype (a heap dst i32, else a scalar i64).
        Op::CallImport { dst, module, name, args, abi, result, result_abi } => {
            let sym = crate::render_wasm::import_symbol(module, name);
            let argstr = args
                .iter()
                .zip(abi.iter())
                .map(|(a, ty)| render_import_arg_wasm(a, *ty))
                .collect::<Vec<_>>()
                .join(" ");
            let call = format!("(call ${sym} {argstr})");
            match (dst, result_abi) {
                (Some(d), Some(rt)) => {
                    // Coerce the import's result valtype back to the i64-uniform / i32-heap
                    // MIR local: an f64 result → its i64 bits; an i32 Bool result → i64;
                    // an i32 heap pointer or i64 → the dst local directly.
                    let dst_heap = result.map(|r| r.is_heap()).unwrap_or(false);
                    let coerced = match rt {
                        crate::WasmAbi::F64 => format!("(i64.reinterpret_f64 {call})"),
                        crate::WasmAbi::I32 if !dst_heap => format!("(i64.extend_i32_u {call})"),
                        _ => call,
                    };
                    format!("    (local.set {} {coerced})\n", local(*d))
                }
                // A Unit-returning import (`-> Unit`, no MIR result) is a void call.
                _ => format!("    {call}\n"),
            }
        }
        // A release: decrement the refcount cell (RuntimeModel.v's rt_dec). The
        // `$rc_dec` primitive traps if the cell is already 0 — the double-free /
        // use-after-free sentinel. This is the byte the perceus V binds each
        // witness drop to (the leak-freedom realization on the artifact).
        Op::Drop { v } => format!("    (call $rc_dec (local.get {}))\n", local(*v)),
        // RECURSIVE drop of a List[String]: IFF this is the last reference (rc==1), free each
        // element handle first (an aliased list keeps its elements alive), THEN rc_dec the
        // list block. The element handle lives in the i64 slot (`12 + i*8`), i32.wrap'd back.
        // Uses the function-wide scratch locals $dlsi/$dlsn (declared in render_wasm_fn).
        Op::DropListStr { v } => {
            let p = local(*v);
            // A MIXED record/tuple block carries a per-value HEAP-SLOT MASK: free EXACTLY
            // those slots (the scalar slots must NOT be `rc_dec`'d), then the block. The mask
            // slot indices are compile-time known, so the free is UNROLLED (no runtime loop);
            // the block's `len@4` is the field count, not iterated. The uniform `List[String]`
            // (no mask) keeps the runtime loop over every slot. Both are gated on rc==1 so a
            // shared block's aliases don't free the heap fields early — and both emit the SAME
            // single `d` to the certificate (an `Op::DropListStr`).
            if let Some(slots) = masks.get(v) {
                let frees = slots
                    .iter()
                    .map(|&i| {
                        let off = 12 + (i as u32) * 8;
                        format!(
                            "         (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.const {off})))))\n"
                        )
                    })
                    .collect::<String>();
                format!(
                    "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1))\n\
                     \x20     (then\n\
                     {frees}\
                     \x20     ))\n\
                     \x20   (call $rc_dec (local.get {p}))\n"
                )
            } else {
                format!(
                    "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1))\n\
                     \x20     (then\n\
                     \x20       (local.set $dlsi (i32.const 0))\n\
                     \x20       (local.set $dlsn (i32.load (i32.add (local.get {p}) (i32.const 4))))\n\
                     \x20       (block $dlsbrk (loop $dlscont\n\
                     \x20         (br_if $dlsbrk (i32.ge_s (local.get $dlsi) (local.get $dlsn)))\n\
                     \x20         (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.add (i32.const 12) (i32.mul (local.get $dlsi) (i32.const 8)))))))\n\
                     \x20         (local.set $dlsi (i32.add (local.get $dlsi) (i32.const 1)))\n\
                     \x20         (br $dlscont)))))\n\
                     \x20   (call $rc_dec (local.get {p}))\n"
                )
            }
        }
        // RECURSIVE drop of a `List[List[String]]` (the csv `rows` shape) — a NESTED loop, no link.
        // At the OUTER list's last ref (rc==1), for each element slot: load the inner `List[String]`
        // handle; at ITS last ref free each cell String (per-slot `rc_dec`); `rc_dec` the inner block;
        // THEN `rc_dec` the outer block. A flat `DropListStr` would only `rc_dec` each inner HANDLE,
        // never running the inner list's last-ref free → the cell Strings LEAK. Cert = the single `d`
        // (the inner frees are the trusted raw-handle routine, leak-loop verified). Uses the dedicated
        // outer-loop locals `$dlli`/`$dlln`/`$dllinner`; the inner loop reuses `$dlsi`/`$dlsn`.
        Op::DropListListStr { v } => {
            let p = local(*v);
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1))\n\
                 \x20     (then\n\
                 \x20       (local.set $dlli (i32.const 0))\n\
                 \x20       (local.set $dlln (i32.load (i32.add (local.get {p}) (i32.const 4))))\n\
                 \x20       (block $dllbrk (loop $dllcont\n\
                 \x20         (br_if $dllbrk (i32.ge_s (local.get $dlli) (local.get $dlln)))\n\
                 \x20         (local.set $dllinner (i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.add (i32.const 12) (i32.mul (local.get $dlli) (i32.const 8)))))))\n\
                 \x20         (if (i32.eq (i32.load (local.get $dllinner)) (i32.const 1))\n\
                 \x20           (then\n\
                 \x20             (local.set $dlsi (i32.const 0))\n\
                 \x20             (local.set $dlsn (i32.load (i32.add (local.get $dllinner) (i32.const 4))))\n\
                 \x20             (block $dlsbrk (loop $dlscont\n\
                 \x20               (br_if $dlsbrk (i32.ge_s (local.get $dlsi) (local.get $dlsn)))\n\
                 \x20               (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get $dllinner) (i32.add (i32.const 12) (i32.mul (local.get $dlsi) (i32.const 8)))))))\n\
                 \x20               (local.set $dlsi (i32.add (local.get $dlsi) (i32.const 1)))\n\
                 \x20               (br $dlscont)))))\n\
                 \x20         (call $rc_dec (local.get $dllinner))\n\
                 \x20         (local.set $dlli (i32.add (local.get $dlli) (i32.const 1)))\n\
                 \x20         (br $dllcont))))\n\
                 \x20     )\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // RUNTIME-TAG-DISPATCHED RECURSIVE drop of a dynamic `Value` — the self-hosted
        // `$__drop_value` (value_core.almd): at the LAST ref (rc==1) it frees the nested payload by
        // tag (Array tag 5 → each element Value recursively; Str tag 4 → the one String; scalar < 4
        // → nothing), then releases the block. A Value only exists if a `value.*` ctor built it, so
        // value_core (and `$__drop_value` with it) is ALWAYS linked wherever a `DropValue` is emitted.
        // The Op keeps its single cert `d`; the recursion is the trusted routine (raw-handle, empty
        // cert), verified by the create+drop LEAK LOOP (the freelist makes a leak observable as an
        // OOB trap). REPLACES the flat inline drop, which leaked an Array's element Values (tag 5).
        Op::DropValue { v } => {
            format!("    (call $__drop_value (local.get {}))\n", local(*v))
        }
        // RECURSIVE drop of a `List[Value]` — the self-hosted `$__drop_list_value` (value_core.almd):
        // IFF the last reference (rc==1), it calls `$__drop_value` on each element (tag-dispatched, so
        // a Str/Array element's nested payload is freed too — a flat `DropListStr` per-slot `rc_dec`
        // would LEAK it), THEN frees the list block. Linked alongside `$__drop_value` whenever any
        // value.* is used (a List[Value] only arises in value-model code). Single cert `d`; the
        // recursion is the trusted routine (empty cert), verified by the create+drop LEAK LOOP.
        Op::DropListValue { v } => {
            format!("    (call $__drop_list_value (local.get {}))\n", local(*v))
        }
        // RECURSIVE drop of a `List[(String, Value)]` — the self-hosted `$__drop_list_str_value`
        // (value_core.almd): at the list's last ref each (String, Value) tuple element is freed at its own
        // last ref (its String slot rc_dec'd flat, its Value slot freed recursively via `$__drop_value`),
        // then the tuple block, then the list block. A flat `DropListStr` would only rc_dec the @12 tuple
        // handle, leaking each tuple's String + Value. Single cert `d`; the recursion is the trusted
        // routine (empty cert), verified by the create+drop LEAK LOOP. The TUPLE-element `DropListValue`.
        Op::DropListStrValue { v } => {
            format!("    (call $__drop_list_str_value (local.get {}))\n", local(*v))
        }
        // RECURSIVE drop of a `List[(String, String)]` — the self-hosted `$__drop_list_str_str`
        // (value_core.almd): each tuple's BOTH String slots rc_dec'd flat at the tuple's last ref, then
        // the tuple, then the list block. The (String,String) counterpart of `DropListStrValue` (the
        // map.entries / svg render_attrs shape). Single cert `d`; trusted recursion, leak-loop verified.
        Op::DropListStrStr { v } => {
            format!("    (call $__drop_list_str_str (local.get {}))\n", local(*v))
        }
        // INLINE recursive drop of a `List[(Int, String)]` (`list.enumerate` / `[(1,"a"),…]`). At the
        // list's last ref (rc==1), loop each element: load the `(Int, String)` tuple handle (@12 + i*8);
        // at the tuple's last ref `rc_dec` ONLY its String slot1 @20 (the Int slot0 @12 is scalar), then
        // the tuple block; then the list block. The prior routing emitted a call to a never-generated
        // `$__drop_list_int_str` → invalid wat (#11/#28). Self-contained (no helper); single cert `d`.
        Op::DropListIntStr { v } => {
            let p = local(*v);
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1))\n\
                 \x20     (then\n\
                 \x20       (local.set $dlli (i32.const 0))\n\
                 \x20       (local.set $dlln (i32.load (i32.add (local.get {p}) (i32.const 4))))\n\
                 \x20       (block $dllbrk (loop $dllcont\n\
                 \x20         (br_if $dllbrk (i32.ge_s (local.get $dlli) (local.get $dlln)))\n\
                 \x20         (local.set $dllinner (i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.add (i32.const 12) (i32.mul (local.get $dlli) (i32.const 8)))))))\n\
                 \x20         (if (i32.eq (i32.load (local.get $dllinner)) (i32.const 1))\n\
                 \x20           (then\n\
                 \x20             (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get $dllinner) (i32.const 20)))))\n\
                 \x20             (call $rc_dec (local.get $dllinner))))\n\
                 \x20         (local.set $dlli (i32.add (local.get $dlli) (i32.const 1)))\n\
                 \x20         (br $dllcont))))\n\
                 \x20     )\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
        // RECURSIVE drop of a `value.as_array` Result `Result[List[Value], String]` — the self-hosted
        // `$__drop_result_lv` (value_core.almd) tag-dispatches at the last ref: Ok frees the
        // `List[Value]` payload recursively, Err frees the String, then the block. A flat `DropListStr`
        // would only rc_dec the @12 list handle, leaking its element Values. Single cert `d`.
        // The (String, Int) MIRROR: rc_dec the String slot0 @12 (the Int slot1 @20 is
        // scalar) — the tokenizer vocab-pairs literal `[("alpha", 1), …]`.
        Op::DropListStrInt { v } => {
            let p = local(*v);
            format!(
                "    (if (i32.eq (i32.load (local.get {p})) (i32.const 1))\n\
                 \x20     (then\n\
                 \x20       (local.set $dlli (i32.const 0))\n\
                 \x20       (local.set $dlln (i32.load (i32.add (local.get {p}) (i32.const 4))))\n\
                 \x20       (block $dllbrk (loop $dllcont\n\
                 \x20         (br_if $dllbrk (i32.ge_s (local.get $dlli) (local.get $dlln)))\n\
                 \x20         (local.set $dllinner (i32.wrap_i64 (i64.load (i32.add (local.get {p}) (i32.add (i32.const 12) (i32.mul (local.get $dlli) (i32.const 8)))))))\n\
                 \x20         (if (i32.eq (i32.load (local.get $dllinner)) (i32.const 1))\n\
                 \x20           (then\n\
                 \x20             (call $rc_dec (i32.wrap_i64 (i64.load (i32.add (local.get $dllinner) (i32.const 12)))))\n\
                 \x20             (call $rc_dec (local.get $dllinner))))\n\
                 \x20         (local.set $dlli (i32.add (local.get $dlli) (i32.const 1)))\n\
                 \x20         (br $dllcont))))\n\
                 \x20     )\n\
                 \x20   (call $rc_dec (local.get {p}))\n"
            )
        }
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
        Op::Prim { kind, dst, args } => {
            let w = |i: usize| format!("(i32.wrap_i64 (local.get {}))", local(args[i]));
            let body = match kind {
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
                // `float.from_int` — the single-instruction sitofp floor (#806).
                // An f64-classified dst (step 3a) takes the convert result directly.
                PrimKind::F64FromInt => {
                    let conv = format!("(f64.convert_i64_s (local.get {}))", local(args[0]));
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        conv
                    } else {
                        format!("(i64.reinterpret_f64 {conv})")
                    }
                }
                // FLOAT floor: the i64 value holds the f64 bits — reinterpret around the
                // op. #806 step 3a: an f64-CLASSIFIED operand is read bare (it is a real
                // f64 local), and an f64-classified dst takes the f64 result directly —
                // the hot-loop shape with ZERO reinterprets.
                PrimKind::FloatUn(op) => {
                    let f = |a: usize| {
                        if floats.contains(&args[a]) {
                            format!("(local.get {})", local(args[a]))
                        } else {
                            format!("(f64.reinterpret_i64 (local.get {}))", local(args[a]))
                        }
                    };
                    let inner = match op {
                        FUnOp::Abs => format!("(f64.abs {})", f(0)),
                        FUnOp::Sqrt => format!("(f64.sqrt {})", f(0)),
                        FUnOp::Floor => format!("(f64.floor {})", f(0)),
                        FUnOp::Ceil => format!("(f64.ceil {})", f(0)),
                        FUnOp::Neg => format!("(f64.neg {})", f(0)),
                    };
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        inner
                    } else {
                        format!("(i64.reinterpret_f64 {inner})")
                    }
                }
                PrimKind::FloatBin(op) => {
                    let f = |a: usize| {
                        if floats.contains(&args[a]) {
                            format!("(local.get {})", local(args[a]))
                        } else {
                            format!("(f64.reinterpret_i64 (local.get {}))", local(args[a]))
                        }
                    };
                    let instr = match op {
                        FBinOp::Add => "f64.add",
                        FBinOp::Sub => "f64.sub",
                        FBinOp::Mul => "f64.mul",
                        FBinOp::Div => "f64.div",
                        FBinOp::Min => "f64.min",
                        FBinOp::Max => "f64.max",
                        FBinOp::CopySign => "f64.copysign",
                    };
                    let inner = format!("({instr} {} {})", f(0), f(1));
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        inner
                    } else {
                        format!("(i64.reinterpret_f64 {inner})")
                    }
                }
                PrimKind::FloatCmp(op) => {
                    let f = |a: usize| {
                        if floats.contains(&args[a]) {
                            format!("(local.get {})", local(args[a]))
                        } else {
                            format!("(f64.reinterpret_i64 (local.get {}))", local(args[a]))
                        }
                    };
                    let instr = match op {
                        FCmpOp::Lt => "f64.lt",
                        FCmpOp::Le => "f64.le",
                        FCmpOp::Gt => "f64.gt",
                        FCmpOp::Ge => "f64.ge",
                        FCmpOp::Eq => "f64.eq",
                        FCmpOp::Ne => "f64.ne",
                    };
                    // f64 compare yields an i32 0/1 — extend to the i64-uniform Bool.
                    format!("(i64.extend_i32_u ({instr} {} {}))", f(0), f(1))
                }
                // SATURATING float→int (i64.trunc_SAT_f64_s), matching Rust's `as` cast (v0): NaN → 0,
                // > i64::MAX → i64::MAX, < i64::MIN → i64::MIN — NO trap. The plain `i64.trunc_f64_s`
                // traps on NaN/inf/out-of-range, diverging from v0 (and float_to_uint64.almd already
                // assumes the saturating form for its f >= 2^64 → u64::MAX path).
                PrimKind::FloatToInt => {
                    let x = if floats.contains(&args[0]) {
                        format!("(local.get {})", local(args[0]))
                    } else {
                        format!("(f64.reinterpret_i64 (local.get {}))", local(args[0]))
                    };
                    format!("(i64.trunc_sat_f64_s {x})")
                }
                PrimKind::IntToFloat => {
                    let conv = format!("(f64.convert_i64_s (local.get {}))", local(args[0]));
                    if dst.is_some_and(|d| floats.contains(&d)) {
                        conv
                    } else {
                        format!("(i64.reinterpret_f64 {conv})")
                    }
                }
                // to_bits / bits_to_float: the value IS the bits — identity pass-through.
                PrimKind::FloatBits => format!("(local.get {})", local(args[0])),
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
            };
            match dst {
                Some(d) => format!("    (local.set {} {body})\n", local(*d)),
                None => format!("    {body}\n"),
            }
        }
        // A scalar reassignment of a stable local — the loop-carried state. Reads `src`,
        // writes the var's own local (reusing the same wasm local is legal: read then set).
        Op::SetLocal { local: l, src } => {
            format!("    (local.set {} (local.get {}))\n", local(*l), local(*src))
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
    }
}
