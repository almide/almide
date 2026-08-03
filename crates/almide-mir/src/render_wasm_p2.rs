/// The read-only tables one wasm op is rendered against.
///
/// Three are `BTreeMap<String, _>` and were adjacent parameters, so the label,
/// slot and arity tables could be transposed silently — and a wrong slot table
/// emits a call to the wrong function index, which the module still validates.
#[derive(Copy, Clone)]
pub(crate) struct OpTables<'a> {
    pub label_off: &'a BTreeMap<String, (u32, u32)>,
    pub func_slots: &'a BTreeMap<String, u32>,
    pub param_counts: &'a BTreeMap<String, usize>,
    pub masks: &'a BTreeMap<ValueId, Vec<usize>>,
    pub reprs: &'a BTreeMap<ValueId, Repr>,
    pub floats: &'a BTreeSet<ValueId>,
    /// This op is a FUNCTION-TAIL `CallFn` (`tail_call_indexes`): its result
    /// flows unmodified through only Else/EndIf merges (+ its own Consume) to
    /// the fn return, with no drops or effects after — render `return_call`
    /// so a mutual-tail-recursion chain runs in constant stack (#864).
    pub tail_call: bool,
}

fn render_op(op: &Op, t: OpTables<'_>, fuser: &mut Fuser) -> String {
    let OpTables { label_off, func_slots, param_counts, masks, reprs, floats, tail_call } = t;
    // Router split out for codopsy cognitive-complexity (pure text-move, no behavior
    // change): the original single ~1100-line exhaustive match over every `Op` variant
    // is now 4 group helpers by variant family (alloc/list-literal, call/binop, the
    // recursive Drop* family, and the misc tail incl. Prim). `Op` has no OR-guards
    // duplicating a variant across groups (unlike the IrExprKind matches elsewhere in
    // this crate), so each group is a fully independent, order-irrelevant subset —
    // grouping here carries none of the "guarded arm committed elsewhere" risk that
    // rules out grouping for a guard-heavy match.
    match op {
        Op::Alloc { .. }
        | Op::ListLit { .. }
        | Op::ListGetScalar { .. }
        | Op::ListSetScalar { .. } => render_op_alloc_lit(op, floats),
        Op::Dup { .. }
        | Op::Call { .. }
        | Op::IntBinOp { .. }
        | Op::CallIndirect { .. }
        | Op::CallFn { .. }
        | Op::CallImport { .. } => {
            render_op_call(op, &WasmEnv { label_off, param_counts, reprs, floats }, tail_call, fuser)
        }
        Op::Drop { .. }
        | Op::DropListStr { .. }
        | Op::DropListListStr { .. }
        | Op::DropValue { .. }
        | Op::DropListValue { .. }
        | Op::DropListStrValue { .. }
        | Op::DropListStrStr { .. }
        | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. }
        | Op::DropResultListValue { .. }
        | Op::DropResultValue { .. }
        | Op::DropResultStrInt { .. }
        | Op::DropResultValueInt { .. }
        | Op::DropResultListValueInt { .. }
        | Op::DropResultListStrInt { .. }
        | Op::DropResultListStr { .. }
        | Op::DropVariant { .. }
        | Op::DropWrapperRec { .. } => render_op_drop(op, masks),
        _ => render_op_misc(op, func_slots, floats, fuser),
    }
}

/// Group 1 of [`render_op`]: heap-block allocation (`Alloc` — String/Bytes/DynStr/
/// OptSome/DynList-DynListStr/OptNone/the generic init fallback) and the flat-list
/// scalar ops (`ListLit`/`ListGetScalar`/`ListSetScalar`). Verbatim subset of the
/// original single match.
/// A constant byte block `[rc][len][cap][bytes…]` — the shared emitter of
/// the String-literal and Bytes-constant Alloc arms (physically the same
/// block; only the source of the bytes differs). Sized LIST-COMPATIBLY so
/// the free-list reuses it: `cap` is the ELEMENT count `ceil(blen /
/// ELEM_SIZE)` and the allocation is `LIST_HEADER + cap*ELEM_SIZE` — storing
/// a byte count as `cap` made the reuse formula overshoot and String-
/// allocating loops leaked → OOM. `len` stays the BYTE length (what print
/// reads).
fn render_byte_block_alloc(dst: ValueId, bytes: &[u8]) -> String {
    let blen = bytes.len() as u32;
    let cap_elems = blen.div_ceil(ELEM_SIZE);
    let total = LIST_HEADER + cap_elems * ELEM_SIZE;
    let mut s = format!(
        "    (local.set {d} (call $alloc (i32.const {total})))\n\
         \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
         \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) (i32.const {blen}))\n\
         \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.const {cap_elems}))\n",
        d = local(dst),
    );
    for (i, b) in bytes.iter().enumerate() {
        let off = LIST_HEADER + i as u32;
        s.push_str(&format!(
            "    (i32.store8 (i32.add (local.get {d}) (i32.const {off})) (i32.const {b}))\n",
            d = local(dst),
        ));
    }
    s
}

fn render_op_alloc_lit(op: &Op, floats: &BTreeSet<ValueId>) -> String {
    match op {
        // A STRING literal — a heap block `[rc][len][cap][utf8 bytes...]` (same header
        // as a list; len/cap are BYTE counts). $alloc the block, set the header, store
        // each byte. Real DATA reproduced from the MIR (the un-defer, ③ exec slice).
        Op::Alloc { dst, init: Init::Str(string), .. } => {
            render_byte_block_alloc(*dst, string.as_bytes())
        }
        // A BYTES constant — physically the SAME `[rc][len][cap][bytes…]` block as a String
        // literal (len/cap are byte counts), but the source bytes are arbitrary (not UTF-8).
        // Materializes a const Bytes module global (the aes S-box) with no runtime call.
        Op::Alloc { dst, init: Init::Bytes(data), .. } => {
            render_byte_block_alloc(*dst, data)
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
            // The bounds compare runs on the FULL i64 index (C-067): wrapping
            // first let an index >= 2^32 truncate into range and silently read
            // the wrong slot (`xs[2^32+1]` returned `xs[1]` where native
            // aborts). len < 2^31, so the zero-extended len compare is exact;
            // the address math after the check keeps the (now proven-safe) wrap.
            let load = if floats.contains(dst) { "f64.load" } else { "i64.load" };
            format!(
                "    (if (i32.or (i64.lt_s (local.get {i}) (i64.const 0))\n\
                 \x20                (i64.ge_s (local.get {i})\n\
                 \x20                          (i64.extend_i32_u (i32.load (i32.add (local.get {l}) (i32.const {LIST_LEN_OFFSET}))))))\n\
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
            // Full-i64 bounds compare — same C-067 clause as the load above
            // (a wrapped index >= 2^32 silently stored into the wrong slot).
            let store = if floats.contains(val) { "f64.store" } else { "i64.store" };
            format!(
                "    (if (i32.or (i64.lt_s (local.get {i}) (i64.const 0))\n\
                 \x20                (i64.ge_s (local.get {i})\n\
                 \x20                          (i64.extend_i32_u (i32.load (i32.add (local.get {l}) (i32.const {LIST_LEN_OFFSET}))))))\n\
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
                | Init::Empty
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
        _ => unreachable!("render_op_alloc_lit: {op:?} is not in this group"),
    }
}




/// Group 3 of [`render_op`]: the plain `Drop` release plus every RECURSIVE drop
/// shape. Split further into `_a` (the flat-element list family) and `_b` (the
/// Result/Variant/Wrapper family) — `Op` has no repeated variant across the two,
/// so the split carries none of the guard-order risk a duplicated-discriminant
/// match would.
fn render_op_drop(op: &Op, masks: &BTreeMap<ValueId, Vec<usize>>) -> String {
    render_op_drop_a(op, masks).unwrap_or_else(|| render_op_drop_b(op))
}

fn render_op_drop_a(op: &Op, masks: &BTreeMap<ValueId, Vec<usize>>) -> Option<String> {
    Some(match op {
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
        _ => return None,
    })
}

include!("render_wasm_calls.rs");
