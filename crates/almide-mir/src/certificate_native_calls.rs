
/// The NATIVE-CALLS witness of a natively rendered program (#4006 class P),
/// judged by `NativeCalls.check_native_calls_cert` (proofs/NativeCalls.v):
/// one `#`-separated block per function, in `functions` order,
///
/// ```text
/// <modes>|<sites>|<marks>|<hosts>
/// <line of heap param 0>
/// …
/// <every other line>
/// ```
///
/// - `modes`: the function's heap-param modes (1 = a MOVE param of
///   `native_move_params`, 0 = borrowed), in param order;
/// - `sites`: one row per `CallFn` to a program function (its block index,
///   then one mode per heap param of the callee: 1 where the caller handed
///   the argument over — a call-site move follows the call — else 0), and
///   per `CallIndirect` one row per possible callee (closure targets never
///   move: a function taken as a value keeps borrowing). The checker holds
///   each row to the callee's declared modes, so a site that keeps an
///   argument its callee takes over is refused;
/// - `marks`: for every call-site move (`MirFunction::call_moves`), the line
///   of the object it moves — where the certificate wrote its `M`;
/// - `hosts`: one id per call to a native fs host op (an `fs.*` host op the
///   render serves from the runtime's fs code, #4006 class E): the index of
///   its runtime symbol in `host_ops::NATIVE_FS_HOST_CALLS`, or that list's
///   length when the symbol is not in it, which the checker rejects
///   (`NativeCalls.hosts_ok`: the named set is the trusted base's row);
/// - `wargs` (#4491): per WRITABLE site (a call that lends an argument to a
///   writable-borrow param, `MirFunction::call_mut_borrows`; its row's mode
///   there is 2), in site order, the line of each heap argument's object —
///   the lines whose call-site probes the certificate wrote `B`; an argument
///   with no line of the block gets the out-of-range index, which the
///   checker rejects;
/// - the lines: [`certificate_lines`], the heap params' lines first (empty
///   when a param has no event), a MOVE param's line without the leading `i`
///   (the checker seeds it from the mode), then the rest in emission order.
///
/// A callee outside the program that the render resolves to the native
/// runtime (a shim, a floor body, the list concat, a dotted self-hosted
/// stdlib name or a `__` helper — borrowing by the renderer contract) has no
/// row; any other unknown callee gets the out-of-range block index, which the
/// checker rejects.
pub fn native_calls_witness(functions: &[MirFunction]) -> String {
    let program: BTreeMap<String, MirFunction> = functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
    let names: Vec<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    let index_of: BTreeMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let unknown = names.len();
    // A callee outside the program is one the render resolves to the native
    // runtime (a shim, a floor body, the list concat) — all of which borrow
    // their heap args by the renderer contract — or a dotted / `__` name.
    let known = |n: &str| {
        n.contains('.')
            || n.starts_with("__")
            || crate::render_native_shims::shim(n).is_some()
            || crate::native_floor::declared(n).is_some()
            || crate::native_list_ops::is_list_concat(n)
    };
    let (table_targets, table_unseeable) = call_modes_witness_func_table(&program, &names);
    let heap_modes = |f: &MirFunction| -> Vec<String> {
        f.params
            .iter()
            .filter(|p| p.repr.is_heap())
            .map(|p| {
                if f.owned_params.contains(&p.value) {
                    "1"
                } else if f.mut_borrow_params.contains(&p.value) {
                    "2"
                } else {
                    "0"
                }
                .to_string()
            })
            .collect()
    };
    let blocks: Vec<String> = functions
        .iter()
        .map(|f| {
            let mut sites: Vec<String> = Vec::new();
            let mut hosts: Vec<String> = Vec::new();
            // Per writable site: its heap arguments' arg indices.
            let mut writable: Vec<Vec<usize>> = Vec::new();
            for (at, op) in f.ops.iter().enumerate() {
                match op {
                    Op::CallFn { name: callee, args, .. } => {
                        if let Some(id) = native_fs_host_site(callee) {
                            hosts.push(id.to_string());
                        }
                        let Some(idx) = direct_callee_index(callee, &index_of, unknown, &known) else { continue };
                        // The arguments the caller handed over right after this call.
                        let handed: Vec<ValueId> = (at + 1..f.ops.len())
                            .take_while(|j| f.call_moves.contains(j))
                            .filter_map(|j| match f.ops[j] {
                                Op::Consume { v } => Some(v),
                                _ => None,
                            })
                            .collect();
                        // One mode per heap param of the callee: what the caller did
                        // with that argument — handed it over (1), lent it writable
                        // (2, #4491) or kept it (0).
                        let lent = |k: usize| f.call_mut_borrows.contains(&(at, k));
                        let modes: Vec<String> = match program.get(callee.as_str()) {
                            Some(c) => args
                                .iter()
                                .zip(&c.params)
                                .enumerate()
                                .filter(|(_, (_, p))| p.repr.is_heap())
                                .map(|(k, (a, _))| {
                                    let moved = matches!(a, CallArg::Handle(v) if handed.contains(v));
                                    if moved { "1" } else if lent(k) { "2" } else { "0" }.to_string()
                                })
                                .collect(),
                            None => borrow_modes(args),
                        };
                        if let (Some(c), true) = (program.get(callee.as_str()), modes.iter().any(|m| m == "2")) {
                            writable.push(args.iter().zip(&c.params).enumerate().filter(|(_, (_, p))| p.repr.is_heap()).map(|(k, _)| k).collect());
                        }
                        sites.push(site_row(idx, &modes));
                    }
                    Op::CallIndirect { args, .. } => {
                        let actual = borrow_modes(args);
                        let possible =
                            indirect_callee_indices(&program, &index_of, args, (table_targets.as_slice(), table_unseeable));
                        if possible.is_empty() {
                            sites.push(site_row(unknown, &actual));
                        } else {
                            sites.extend(possible.into_iter().map(|idx| site_row(idx, &actual)));
                        }
                    }
                    _ => {}
                }
            }
            let (lines, _, _, moved, lent_objs) = certificate_lines(f);
            let heap: Vec<ValueId> = f.params.iter().filter(|p| p.repr.is_heap()).map(|p| p.value).collect();
            let mut ordered: Vec<(ValueId, String)> = heap
                .iter()
                .map(|p| {
                    let line = lines.iter().find(|(o, _)| o == p).map(|(_, l)| l.clone()).unwrap_or_default();
                    let line = if f.owned_params.contains(p) { line.strip_prefix('i').unwrap_or(&line).to_string() } else { line };
                    (*p, line)
                })
                .collect();
            ordered.extend(lines.into_iter().filter(|(o, _)| !heap.contains(o)));
            let marks: Vec<String> = moved
                .iter()
                .map(|o| ordered.iter().position(|(x, _)| x == o).unwrap_or(ordered.len()).to_string())
                .collect();
            let line_of = |o: &Option<ValueId>| o.and_then(|o| ordered.iter().position(|(x, _)| *x == o)).unwrap_or(ordered.len());
            let wargs: Vec<String> = writable
                .iter()
                .enumerate()
                .map(|(n, ks)| {
                    let objs = lent_objs.get(n);
                    ks.iter()
                        .map(|&k| line_of(&objs.and_then(|o| o.get(k).copied()).flatten()).to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            let body: Vec<&str> = ordered.iter().map(|(_, l)| l.as_str()).collect();
            let header = format!("{}|{}|{}|{}", heap_modes(f).join(" "), sites.join(";"), marks.join(" "), hosts.join(" "));
            let header = if wargs.is_empty() { header } else { format!("{header}|{}", wargs.join(";")) };
            format!("{header}\n{}", body.join("\n"))
        })
        .collect();
    blocks.join("#")
}

/// The witness host id of a `CallFn` to `callee` when it is a native fs host
/// call: an `fs` host op the render serves from the runtime floor. Its id is
/// the index of the runtime symbol it calls in the named set, or the set's
/// length (out of range: refused) when that symbol is not named there.
fn native_fs_host_site(callee: &str) -> Option<usize> {
    let row = crate::host_ops::host_op_for_call_name(callee)?;
    if row.module != "fs" {
        return None;
    }
    let symbol = crate::native_floor::declared(callee)?.symbol.as_str();
    Some(crate::host_ops::native_fs_host_id(symbol).unwrap_or(crate::host_ops::NATIVE_FS_HOST_CALLS.len()))
}
