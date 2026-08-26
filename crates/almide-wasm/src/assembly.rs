//! Structural module assembly (sections, tables, globals, code, data) —
//! split from emit_program for the complexity budget.

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, ElementSection, Elements, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    MemorySection, MemoryType, Module, RefType, TableSection, TableType, TypeSection, ValType,
};

use crate::func::Pool;
use crate::work::FnWork;
use crate::*;

pub(crate) struct AssembleIn<'a> {
    /// Pooled "Error: out of memory" block for the allocator's C-197 die.
    pub(crate) oom_msg: u32,
    pub(crate) table: &'a FnTable,
    pub(crate) work: &'a FnWork,
    pub(crate) pool: &'a Pool,
    pub(crate) lowered: &'a [Result<(Function, std::collections::HashSet<usize>), String>],
    /// Program-fn indices REACHABLE from main (the emit_program BFS) —
    /// an unreached body ships as a 3-byte `unreachable` stub, which is
    /// what keeps a small program's module small while the linked
    /// registry graph stays fully loaded for resolution.
    pub(crate) reachable: &'a std::collections::HashSet<usize>,
    pub(crate) main_fn: &'a Function,
    pub(crate) entry_fn_indices: &'a [u32],
    pub(crate) extra_fns: &'a [(u32, Function)],
    pub(crate) global_decls: &'a [(almide_ir::VarId, SliceTy)],
    pub(crate) main_index: u32,
    pub(crate) true_base: u32,
    pub(crate) false_base: u32,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn assemble_module(a: AssembleIn<'_>) -> Result<Vec<u8>, EmitError> {
    let AssembleIn {
        table,
        work,
        pool,
        oom_msg,
        lowered,
        reachable,
        main_fn,
        entry_fn_indices,
        extra_fns,
        global_decls,
        main_index,
        true_base,
        false_base,
    } = a;
    // ── assemble the module structurally ────────────────────────────────
    let line_start = (pool.data.len() as u32 + 15) & !15;
    let heap_start = u64::from(line_start) + LINE_BUF_MIN;
    let pages = heap_start.div_ceil(65536);

    let mut types = TypeSection::new();
    types.ty().function([ValType::I32, ValType::I32], []); // 0: print import (ptr, len)
    types.ty().function([ValType::I32], []); // 1: block-print(base) / exit(code)
    types.ty().function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]); // 2: append_copy
    types.ty().function([ValType::I32, ValType::I64], [ValType::I32]); // 3: append_i64
    types.ty().function([], []); // 4: main
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // 5: append_bool / concat / str_eq
    types.ty().function([ValType::I64], [ValType::I32]); // 6: itoa / int_to_string
    types.ty().function([ValType::I32], [ValType::I32]); // 7: alloc
    types.ty().function([ValType::I32, ValType::I64], [ValType::I32]); // 8: list_get/push (8-byte)
    types.ty().function([ValType::I32], [ValType::I64]); // 9: str_len_chars
    types.ty().function([ValType::I32, ValType::I32, ValType::I32, ValType::I64], [ValType::I32]); // 10: scan_w64
    types.ty().function([ValType::I32, ValType::I32, ValType::I32, ValType::I32], [ValType::I32]); // 11: scan_w32/str
    types.ty().function([ValType::I32], [ValType::F64]); // 12: f16_to_f64
    types.ty().function([ValType::I32, ValType::I64], [ValType::I32]); // 13: cp_off / str_repeat
    types.ty().function([ValType::I32, ValType::I64, ValType::I64], [ValType::I32]); // 14: str_slice
    types
        .ty()
        .function([ValType::I32; 5], [ValType::I64]); // 15: fs_call
    types.ty().function([ValType::I32], []); // 16: host_read
    types.ty().function([ValType::I32; 3], []); // 17: copy
    for (i, info) in table.infos.iter().enumerate() {
        // Refused functions keep a placeholder type — their stub body is
        // `unreachable` and no call site ever targets them.
        debug_assert_eq!(T_FN_BASE as usize + i, 18 + i);
        if info.refuse.is_some() {
            types.ty().function([], []);
        } else {
            let params: Vec<ValType> = info.params.iter().map(|t| t.val_type()).collect();
            let results: Vec<ValType> = info.ret.iter().map(|t| t.val_type()).collect();
            types.ty().function(params, results);
        }
    }
    // Function-value types (call_indirect signatures + extra fns),
    // indices assigned eagerly during lowering from itype_base.
    for (ps, r) in work.itypes.borrow().iter() {
        types.ty().function(ps.clone(), r.iter().copied().collect::<Vec<_>>());
    }

    let mut imports = ImportSection::new();
    imports.import("almide", "println", EntityType::Function(0));
    imports.import("almide", "eprintln", EntityType::Function(0));
    imports.import("almide", "exit", EntityType::Function(1));
    imports.import("almide", "fs_call", EntityType::Function(15));
    imports.import("almide", "host_read", EntityType::Function(16));

    let mut functions = FunctionSection::new();
    functions.function(1); // F_PRINTLN_BLOCK
    functions.function(1); // F_EPRINTLN_BLOCK
    functions.function(2); // F_APPEND_COPY
    functions.function(6); // F_ITOA
    functions.function(3); // F_APPEND_I64
    functions.function(5); // F_APPEND_BOOL
    functions.function(7); // F_ALLOC
    functions.function(6); // F_INT_TO_STRING
    functions.function(5); // F_CONCAT
    functions.function(5); // F_STR_EQ
    functions.function(8); // F_LIST_GET_8
    functions.function(8); // F_LIST_GET_4 (idx is ALWAYS i64 — only the slot width differs)
    functions.function(8); // F_LIST_PUSH_8
    functions.function(5); // F_LIST_PUSH_4
    functions.function(5); // F_LIST_JOIN
    functions.function(7); // F_BLOCK_COPY
    functions.function(5); // F_BUF_TO_BLOCK
    functions.function(9); // F_STR_LEN_CHARS
    functions.function(10); // F_SCAN_W64
    functions.function(11); // F_SCAN_W32
    functions.function(11); // F_SCAN_STR
    functions.function(12); // F_F16_TO_F64
    functions.function(13); // F_CP_OFF
    functions.function(14); // F_STR_SLICE
    functions.function(13); // F_STR_REPEAT
    functions.function(5); // F_STR_CMP
    functions.function(11); // F_STR_REPLACE
    functions.function(17); // F_COPY
    functions.function(1); // F_FREE ((i32) -> ())
    for i in 0..table.infos.len() {
        functions.function(T_FN_BASE + i as u32);
    }
    functions.function(T_MAIN); // main, last
    for (ti, _) in extra_fns {
        functions.function(*ti);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: pages,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: false, shared: false },
        &ConstExpr::i32_const(line_start as i32),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(heap_start as i32),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(line_start as i32),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: false, shared: false },
        &ConstExpr::i32_const(heap_start as i32),
    );
    // Deterministic meter (G_DET_FUEL/ENTRY/VERDICT/SPEND/DEPTH).
    for init in [i64::MAX, 0, 0, 0] {
        globals.global(
            GlobalType { val_type: ValType::I64, mutable: true, shared: false },
            &ConstExpr::i64_const(init),
        );
    }
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(0),
    );
    // T5-1 wall deadline / hit / verdict (globals 9/10/11).
    globals.global(
        GlobalType { val_type: ValType::I64, mutable: true, shared: false },
        &ConstExpr::i64_const(i64::MAX),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(0),
    );
    globals.global(
        GlobalType { val_type: ValType::I64, mutable: true, shared: false },
        &ConstExpr::i64_const(0),
    );

    // The funcref table always exists (a call_indirect in ANY body needs
    // it, entries or not); slot 0 stays uninitialized — null funcref =
    // trap — so fn-value slots are +1-biased (W-1).
    let mut tables = TableSection::new();
    let mut elements = ElementSection::new();
    let n = entry_fn_indices.len() as u64 + 1;
    tables.table(TableType {
        element_type: RefType::FUNCREF,
        minimum: n,
        maximum: Some(n),
        table64: false,
        shared: false,
    });
    if !entry_fn_indices.is_empty() {
        elements.active(
            Some(0),
            &ConstExpr::i32_const(1),
            Elements::Functions(entry_fn_indices.to_vec().into()),
        );
    }

    for (_, sty) in global_decls {
        let vt = sty.val_type();
        let init = match vt {
            ValType::I64 => ConstExpr::i64_const(0),
            ValType::F64 => ConstExpr::f64_const(0.0.into()),
            _ => ConstExpr::i32_const(0),
        };
        globals.global(GlobalType { val_type: vt, mutable: true, shared: false }, &init);
    }

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("main", ExportKind::Func, main_index);
    // The bump-heap watermark: monotonic, so its final value IS the
    // allocation total (plus the fixed base) — the alloc-ledger
    // observable (#1586). Behavior-neutral: nothing in-module reads
    // exports.
    exports.export("__heap", ExportKind::Global, G_HEAP);

    let mut code = CodeSection::new();
    code.function(&emit_block_print(F_PRINTLN_IMPORT));
    code.function(&emit_block_print(F_EPRINTLN_IMPORT));
    code.function(&emit_append_copy());
    code.function(&emit_itoa());
    code.function(&emit_append_i64());
    code.function(&emit_append_bool(true_base, false_base));
    code.function(&emit_alloc(oom_msg));
    code.function(&emit_int_to_string());
    code.function(&emit_concat());
    code.function(&emit_str_eq());
    code.function(&emit_list_get(Scalar::Int));
    code.function(&emit_list_get(Scalar::Str));
    code.function(&emit_list_push(Scalar::Int));
    code.function(&emit_list_push(Scalar::Str));
    code.function(&emit_list_join());
    code.function(&emit_block_copy());
    code.function(&emit_buf_to_block());
    code.function(&emit_str_len_chars());
    code.function(&emit_scan_w64());
    code.function(&emit_scan_w32());
    code.function(&emit_scan_str());
    code.function(&emit_f16_to_f64());
    code.function(&emit_cp_off());
    code.function(&emit_str_slice());
    code.function(&emit_str_repeat());
    code.function(&emit_str_cmp());
    code.function(&emit_str_replace());
    code.function(&emit_copy());
    code.function(&emit_free());
    for (i, l) in lowered.iter().enumerate() {
        match l {
            // A lowered body ships only when the main BFS reaches it —
            // dead registry bodies become the same loud stub the refused
            // ones use (stack-polymorphic `unreachable` satisfies any
            // declared signature).
            Ok((f, _)) if reachable.contains(&i) => {
                code.function(f);
            }
            _ => {
                let mut stub = Function::new([]);
                stub.instructions().unreachable().end();
                code.function(&stub);
            }
        }
    }
    code.function(main_fn);
    for (_, f) in extra_fns {
        code.function(f);
    }

    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), pool.data.iter().copied());

    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&exports);
    if !entry_fn_indices.is_empty() {
        module.section(&elements);
    }
    module.section(&code).section(&data);
    Ok(module.finish())
}

/// One emitted-helper BODY (split from resolve_extras for the
/// complexity budget).
fn helper_body(h: &Helper, work: &FnWork, helper_snapshot: &[Helper], hpos: usize) -> Function {
    match h {
    Helper::JsonValue { float_to_string, frags } => value_helpers::emit_json_value_helper(
        work.helper_base.get(),
        helper_snapshot,
        *float_to_string,
        *frags,
    ),
    Helper::JsonQuote { frags } => value_helpers::emit_json_quote_helper(*frags),
    Helper::JsonValuePretty { float_to_string, frags, pfrags } => {
        value_helpers::emit_json_value_pretty_helper(
            work.helper_base.get(),
            helper_snapshot,
            *float_to_string,
            *frags,
            *pfrags,
        )
    }
    Helper::ValueField => value_helpers::emit_value_field_helper(),
    Helper::Utf8Lossy => utf8_helpers::emit_utf8_lossy_helper(),
    Helper::ValueEq { key_off, val_off } => value_helpers::emit_value_eq_helper(
        work.helper_base.get() + hpos as u32,
        *key_off,
        *val_off,
    ),
    Helper::ValueMerge { key_off, val_off } => {
        value_helpers::emit_value_merge_helper(*key_off, *val_off)
    }
    Helper::ValueKeys => value_helpers::emit_value_keys_helper(),
    Helper::StringSplit => value_helpers::emit_string_split_helper(),
    Helper::ScanF64 => runtime::emit_scan_f64(),
    Helper::BytesToString { inv_pre, inv_mid, inc_pre } => {
        utf8_helpers::emit_bytes_to_string_helper(*inv_pre, *inv_mid, *inc_pre)
    }
    Helper::FastExp => matrix_scalars::emit_fast_exp(),
    Helper::GeluScalar { fast_exp } => matrix_scalars::emit_gelu_scalar(*fast_exp),
    Helper::Q10Val => matrix_scalars::emit_q10_val(F_F16_TO_F64),
    Helper::DisplayNamed { ti } => {
        match work.display_bodies.borrow_mut().remove(ti) {
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
    Helper::NamedEq { ti } => match work.eq_bodies.borrow_mut().remove(ti) {
        Some(work::DisplayBuild::Built(f)) => f,
        _ => {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        }
    },
    Helper::JsonPathSet => {
        json_path_helpers::emit_json_path_set_helper(work.helper_base.get(), helper_snapshot)
    }
    Helper::JsonPathRemove => {
        json_path_helpers::emit_json_path_remove_helper(work.helper_base.get(), helper_snapshot)
    }
    Helper::ScanDeep { key } => match work.scan_bodies.borrow_mut().remove(key) {
        Some(work::DisplayBuild::Built(f)) => f,
        _ => {
            let mut f = Function::new([]);
            f.instructions().unreachable().end();
            f
        }
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
        let params = match h {
            Helper::ValueKeys | Helper::Utf8Lossy | Helper::BytesToString { .. } => {
                vec![ValType::I32]
            }
            Helper::ScanF64 => vec![ValType::I32, ValType::I32, ValType::I32, ValType::F64],
            Helper::ScanDeep { .. } => {
                vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32]
            }
            Helper::JsonValuePretty { .. } | Helper::JsonPathRemove => {
                vec![ValType::I32, ValType::I32, ValType::I32]
            }
            Helper::JsonPathSet => {
                vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32]
            }
            Helper::FastExp | Helper::GeluScalar { .. } => vec![ValType::F64],
            Helper::Q10Val => vec![ValType::I32, ValType::I64, ValType::I64],
            _ => vec![ValType::I32, ValType::I32],
        };
        let ret = match h {
            Helper::FastExp | Helper::GeluScalar { .. } | Helper::Q10Val => ValType::F64,
            _ => ValType::I32,
        };
        let ti = work.itype(params, Some(ret));
        let f = helper_body(h, work, helper_snapshot.as_slice(), hpos);
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
        }
    }
    (extra_fns, entry_fn_indices)
}