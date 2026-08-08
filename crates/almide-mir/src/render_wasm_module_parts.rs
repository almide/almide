// WAT module assembly: the pieces `render_wasm` / `render_wasm_program` put
// AROUND the per-function bodies — the data section, the heap-handle locals,
// the burned-name alias remap, and the region-clone rc_inc strip.
//
// Split out of `render_wasm.rs` to keep each file under the line ceiling; this
// is `include!`d, so it shares that module's imports.

/// Heap handles (`Alloc`/`Dup` dsts) become i32 list-pointer locals, in first-use
/// order so the declaration list is deterministic.
fn heap_handle_locals(ops: &[Op]) -> Vec<ValueId> {
    let mut heap_locals: Vec<ValueId> = Vec::new();
    for op in ops {
        let (Op::Alloc { dst, .. } | Op::Dup { dst, .. }) = op else {
            continue;
        };
        if !heap_locals.contains(dst) {
            heap_locals.push(*dst);
        }
    }
    heap_locals
}

/// Assign each distinct string label a data-section offset starting at `base`,
/// emitting one `(data …)` line per label. Deduplicated, so a label used twice
/// is stored once.
fn collect_label_data(
    ops: &[Op],
    base: u32,
    label_off: &mut BTreeMap<String, (u32, u32)>,
    data: &mut String,
) {
    let mut cursor = base;
    for op in ops {
        let Op::Call { args, .. } = op else { continue };
        for a in args {
            let CallArg::Label(label) = a else { continue };
            if label_off.contains_key(label) {
                continue;
            }
            let len = label.len() as u32;
            label_off.insert(label.clone(), (cursor, len));
            data.push_str(&format!("  (data (i32.const {cursor}) {:?})\n", label));
            cursor += len;
        }
    }
}

/// Rewrite every unresolvable-as-spelled call / drop target that HAS an alias to that
/// alias, in place. Extracted verbatim from [`try_render_wasm_program`] (codopsy
/// round-3 sweep, #852).
fn remap_burned_names_to_aliases(p: &mut MirProgram, resolvable: &BTreeSet<String>) {
    for f in &mut p.functions {
        for op in &mut f.ops {
            remap_op_target(op, resolvable);
        }
    }
}

/// One op's target. A call names its fn directly; a drop names a TYPE whose
/// drop fn is derived, so both go through `drop_target_name` first.
fn remap_op_target(op: &mut Op, resolvable: &BTreeSet<String>) {
    match op {
        Op::CallFn { name, .. } => {
            if let Some(alias) = burned_alias(name, resolvable, resolve_rt_alias) {
                *name = alias;
            }
        }
        Op::DropVariant { ty: target, .. }
        | Op::DropWrapperRec {
            drop_fn: target, ..
        } => {
            let spelled = drop_target_name(target);
            if let Some(alias) = burned_alias(&spelled, resolvable, resolve_drop_alias) {
                *target = alias;
            }
        }
        _ => {}
    }
}

/// The alias for `name` when it is NOT resolvable as spelled, or `None` when it
/// resolves already or has no alias.
fn burned_alias(
    name: &str,
    resolvable: &BTreeSet<String>,
    lookup: fn(&str, &BTreeSet<String>) -> Option<String>,
) -> Option<String> {
    if resolvable.contains(name) {
        return None;
    }
    lookup(name, resolvable)
}

/// Inside a region clone the refcount is dead weight (nothing frees before the
/// frontier reset), and a `Dup` singleton alias per instance would otherwise
/// serialize on ONE rc cell's read-modify-write chain. MakeUnique is rejected by
/// the region qualifier, so every `rc_inc` line in a clone body stems from a Dup
/// — drop them all (region_alloc.rs's documented contract).
fn strip_region_clone_rc_incs(fn_name: &str, body: String) -> String {
    if !fn_name.starts_with("__rgn_") {
        return body;
    }
    body.lines()
        .filter(|l| !l.contains("call $rc_inc"))
        .map(|l| format!("{l}\n"))
        .collect()
}

