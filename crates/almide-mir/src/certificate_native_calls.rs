
/// The NATIVE-CALLS witness of a natively rendered program (#4006 class P),
/// judged by `NativeCalls.check_native_calls_cert` (proofs/NativeCalls.v):
/// one `#`-separated block per function, in `functions` order,
///
/// ```text
/// <modes>|<sites>|<marks>
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
            .map(|p| if f.owned_params.contains(&p.value) { "1" } else { "0" }.to_string())
            .collect()
    };
    let blocks: Vec<String> = functions
        .iter()
        .map(|f| {
            let mut sites: Vec<String> = Vec::new();
            for (at, op) in f.ops.iter().enumerate() {
                match op {
                    Op::CallFn { name: callee, args, .. } => {
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
                        // with that argument — handed it over (1) or kept it (0).
                        let modes: Vec<String> = match program.get(callee.as_str()) {
                            Some(c) => args
                                .iter()
                                .zip(&c.params)
                                .filter(|(_, p)| p.repr.is_heap())
                                .map(|(a, _)| {
                                    let moved = matches!(a, CallArg::Handle(v) if handed.contains(v));
                                    if moved { "1" } else { "0" }.to_string()
                                })
                                .collect(),
                            None => borrow_modes(args),
                        };
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
            let (lines, _, _, moved) = certificate_lines(f);
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
            let body: Vec<&str> = ordered.iter().map(|(_, l)| l.as_str()).collect();
            format!("{}|{}|{}\n{}", heap_modes(f).join(" "), sites.join(";"), marks.join(" "), body.join("\n"))
        })
        .collect();
    blocks.join("#")
}
