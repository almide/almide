// ── assembly.rs, the emitted-helper bodies and the export restore state ──
//
// include!-spliced into `assembly.rs` at module level (the 800-line file
// discipline). The imports are assembly.rs's own.

/// One emitted-helper BODY (split from resolve_extras for the
/// complexity budget).
/// #4203: a module running the contained serve loop (op 75) exports its
/// table and every global, so the embedded host can call the loop's
/// closure and put the instance back to its state at the call after a
/// handler abort. A module without the op keeps its bytes.
fn export_serve_restore_state(exports: &mut ExportSection, work: &FnWork, global_count: u32) {
    if !work.host_ops.borrow().contains(&crate::fs_meta::OP_HTTP_SERVE_RUN) {
        return;
    }
    exports.export("__almide_table", ExportKind::Table, 0);
    for g in 0..global_count {
        exports.export(&format!("__almide_g{g}"), ExportKind::Global, g);
    }
}

/// A per-program helper's body and its declared RC signature (#4278 B1).
fn helper_body(h: &Helper, work: &FnWork, helper_snapshot: &[Helper], hpos: usize) -> (Function, Option<HelperSig>) {
    match crate::helper_sig::per_program(h) {
        Some((f, sig)) => (f, Some(sig)),
        None => (helper_body_unsigned(h, work, helper_snapshot, hpos), None),
    }
}

fn helper_body_unsigned(h: &Helper, work: &FnWork, helper_snapshot: &[Helper], hpos: usize) -> Function {
    match h {
    Helper::JsonValue { float_to_string, frags } => json_helpers::emit_json_value_helper(
        work.helper_base.get(),
        helper_snapshot,
        *float_to_string,
        *frags,
    ),
    Helper::JsonQuote { frags, ctrl } => json_helpers::emit_json_quote_helper(*frags, Some(*ctrl)),
    Helper::ReprQuote { frags } => json_helpers::emit_json_quote_helper(*frags, None),
    Helper::JsonValuePretty { float_to_string, frags, pfrags } => {
        json_helpers::emit_json_value_pretty_helper(
            work.helper_base.get(),
            helper_snapshot,
            *float_to_string,
            *frags,
            *pfrags,
        )
    }
    Helper::ValueField => value_helpers::emit_value_field_helper(),
    Helper::Utf8Lossy => utf8_helpers::emit_utf8_lossy_helper(),
    Helper::ErrText => utf8_helpers::emit_err_text_helper(),
    Helper::ValueEq { key_off, val_off } => value_helpers::emit_value_eq_helper(
        work.helper_base.get() + hpos as u32,
        *key_off,
        *val_off,
    ),
    Helper::ValueKeys => value_helpers::emit_value_keys_helper(),
    Helper::StringSplit => value_helpers::emit_string_split_helper(),
    Helper::ScanF64 => runtime::emit_scan_f64(),
    Helper::MapReserve => runtime_alloc::emit_map_reserve(),
    Helper::BytesToString { inv_pre, inv_mid, inc_pre } => {
        utf8_helpers::emit_bytes_to_string_helper(*inv_pre, *inv_mid, *inc_pre)
    }
    _ => match map_index::helper_body(h)
        .or_else(|| crate::map_ordered::helper_body(h, work.helper_base.get() + hpos as u32))
        .or_else(|| crate::value_set_path::helper_body(h, work.helper_base.get() + hpos as u32))
        .or_else(|| crate::fan::js_async::helper_body(h))
        .or_else(|| runtime_alloc::helper_body(h, work))
        .or_else(|| runtime_line::helper_body(h, work))
    {
        Some(f) => f,
        None => helper_body_b(h, work),
    },
    }
}

/// Emitted helpers + table-entry extras (shims, adapters, lifted
/// lambdas) — split from emit_program for the complexity budget.
pub(crate) fn resolve_extras(
    table: &FnTable,
    work: &FnWork,
    lifted_fns: &[LoweredLifted],
) -> (Vec<(u32, Function)>, Vec<u32>) {
    let extra_base = F_FN_BASE + table.infos.len() as u32 + 1;
    let mut extra_fns: Vec<(u32, Function)> = Vec::new();
    // Emitted helpers assemble FIRST — their indices were promised during
    // lowering; the table-entry extras follow.
    let helper_snapshot: Vec<Helper> = work.helpers.borrow().clone();
    for (hpos, h) in helper_snapshot.iter().enumerate() {
        let params = match map_index::helper_params(h)
            .or_else(|| crate::map_ordered::helper_params(h))
            .or_else(|| crate::value_set_path::helper_params(h))
            .or_else(|| crate::fan::js_async::helper_params(h))
            .or_else(|| runtime_alloc::helper_params(h))
            .or_else(|| runtime_line::helper_params(h))
        {
            Some(p) => p,
            None => match h {
            Helper::ValueKeys
            | Helper::Utf8Lossy
            | Helper::ErrText
            | Helper::BytesToString { .. } => vec![ValType::I32],
            Helper::ScanF64 => vec![ValType::I32, ValType::I32, ValType::I32, ValType::F64],
            Helper::ScanDeep { .. } => {
                vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32]
            }
            Helper::JsonValuePretty { .. } => {
                vec![ValType::I32, ValType::I32, ValType::I32]
            }
            _ => vec![ValType::I32, ValType::I32],
            },
        };
        let ret = match h {
            _ if runtime_line::helper_is_void(h) => None,
            _ => crate::fan::js_async::helper_result(h).unwrap_or_else(|| runtime_alloc::helper_result(h)),
        };
        let ti = work.itype(params, ret);
        let (f, sig) = helper_body(h, work, helper_snapshot.as_slice(), hpos);
        sig.iter().for_each(|s| crate::witness::rc::helper(extra_base + extra_fns.len() as u32, s));
        extra_fns.push((ti, f));
    }
    let mut entry_fn_indices: Vec<u32> = Vec::new();
    for e in work.entries.borrow().clone() {
        match e {
            TableEntry::Fn(i) => {
                // Uniform closure convention: env leads — a plain fn's
                // table slot holds a forwarding shim.
                let info = &table.infos[i];
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(info.params.iter().map(|t| t.val_type()));
                let ti = work.itype(ps, info.ret.map(SliceTy::val_type));
                let idx = extra_base + extra_fns.len() as u32;
                extra_fns.push((
                    ti,
                    emit_env_shim(F_FN_BASE + i as u32, &info.params, info.ret, false),
                ));
                entry_fn_indices.push(idx);
            }
            TableEntry::Adapter { target, raw } => {
                let info = &table.infos[target];
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(info.params.iter().map(|t| t.val_type()));
                let ti = work.itype(ps, Some(ValType::I32));
                let idx = extra_base + extra_fns.len() as u32;
                let _ = raw;
                extra_fns.push((
                    ti,
                    emit_env_shim(F_FN_BASE + target as u32, &info.params, info.ret, true),
                ));
                entry_fn_indices.push(idx);
            }
            TableEntry::Lambda(j) => {
                let (ps, r, f, _) = &lifted_fns[j as usize];
                let ti = work.itype(ps.clone(), *r);
                let idx = extra_base + extra_fns.len() as u32;
                extra_fns.push((ti, f.clone()));
                entry_fn_indices.push(idx);
            }
            TableEntry::Direct(idx) => entry_fn_indices.push(idx),
        }
    }
    (extra_fns, entry_fn_indices)
}
/// The named-type / json-path half of the helper table —
/// split from `helper_body` for the complexity budget.
fn helper_body_b(h: &Helper, work: &FnWork) -> Function {
    match h {
    Helper::DisplayNamed { ti, irk } => {
        match work.display_bodies.borrow_mut().remove(&(*ti, *irk)) {
            Some(work::DisplayBuild::Built(f)) => f,
            // Failed (all callers refused) — keep the promised
            // index aligned with a loud stub.
            _ => {
                let mut f = Function::new([]);
                f.instructions().unreachable().end();
                f
            }
        }
    }
    Helper::DisplayTy { ety, irk } => match work.display_ty_bodies.borrow_mut().remove(&(*ety, *irk)) {
        Some(work::DisplayBuild::Built(f)) => f,
        _ => {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        }
    },
    Helper::NamedOp { op, ti } => match work.named_bodies.borrow_mut().remove(&(*op, *ti)) {
        Some(work::DisplayBuild::Built(f)) => f,
        _ => {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        }
    },
    Helper::ScanDeep { key } => match work.scan_bodies.borrow_mut().remove(key) {
        Some(work::DisplayBuild::Built(f)) => f,
        _ => {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        }
    },
    _ => unreachable!("helper routed to the wrong half"),
    }
}

/// #1699 — which of the 34 fixed-slot helpers this module actually reaches.
/// Roots are every body that ships after the helper block (the reachable
/// lowered fns, `main`, the program-driven extras); edges INSIDE the helper
/// set come from scanning each helper's own encoded body, folded to a
/// fixpoint.
///
/// #1962: the proven runtime core (`$alloc` / `$free` / `$inc` / `$dec_flat`
/// / `$cow`) is NOT seeded any more — it is reached like every other
/// helper, through a call from a shipped body. A program that never
/// allocates (hello, world: one data-segment string through `$println`)
/// ships the five as 2-byte `unreachable` stubs, index-stable, and drops
/// ~2 KB from every such artifact. The byte-grounding gate
/// (`proofs/check-structural-bytes.sh`) dumps the core bodies through
/// `dump_runtime_bytes` at unit level, independent of any module, so the
/// `StructuralDecode.v` theorems keep their subject either way. Nothing
/// outside a body reaches the core: the exports are `memory` / `main` /
/// `__heap` / the `@export` fns, and the WASI transforms park their
/// buffers on their own page rather than calling `$alloc`.
fn used_static_helpers<'a>(
    helpers: &[(u32, Function, Option<HelperSig>)],
    roots: impl Iterator<Item = &'a Function>,
    seeds: &[u32],
) -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    let mut used: HashSet<u32> = HashSet::new();
    let mut pending: Vec<u32> = Vec::new();
    fn note(set: &mut HashSet<u32>, pending: &mut Vec<u32>, idx: u32) {
        if (F_PRINTLN_BLOCK..F_FN_BASE).contains(&idx) && set.insert(idx) {
            pending.push(idx);
        }
    }
    // Helpers the HOST calls (#2265: the JS host's allocator and release)
    // are roots no body reaches; they and their callees ship.
    for &idx in seeds {
        note(&mut used, &mut pending, idx);
    }
    for f in roots {
        for idx in called_function_indices(f) {
            note(&mut used, &mut pending, idx);
        }
    }
    let mut scanned: HashSet<u32> = HashSet::new();
    while let Some(idx) = pending.pop() {
        if !scanned.insert(idx) {
            continue;
        }
        if let Some((_, f, _)) = helpers.iter().find(|(i, _, _)| *i == idx) {
            for callee in called_function_indices(f) {
                note(&mut used, &mut pending, callee);
            }
        }
    }
    used
}

/// Every `call` / `return_call` target in an encoded function body — parsed
/// with wasmparser over the same bytes `wasm_encoder` wrote, never
/// pattern-matched (a raw 0x10 inside an immediate is not a call). A body
/// that fails to parse claims every helper: over-keeping is bytes,
/// under-keeping is a trap.
fn called_function_indices(f: &Function) -> Vec<u32> {
    use wasm_encoder::Encode;
    let mut buf = Vec::new();
    f.encode(&mut buf);
    let mut out = Vec::new();
    let mut r = wasmparser::BinaryReader::new(&buf, 0);
    let Ok(size) = r.read_var_u32() else {
        return (F_PRINTLN_BLOCK..F_FN_BASE).collect();
    };
    let start = r.current_position();
    let end = (start + size as usize).min(buf.len());
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&buf[start..end], 0));
    let Ok(ops) = body.get_operators_reader() else {
        return (F_PRINTLN_BLOCK..F_FN_BASE).collect();
    };
    for op in ops {
        match op {
            Ok(wasmparser::Operator::Call { function_index })
            | Ok(wasmparser::Operator::ReturnCall { function_index }) => out.push(function_index),
            Ok(_) => {}
            Err(_) => return (F_PRINTLN_BLOCK..F_FN_BASE).collect(),
        }
    }
    out
}
