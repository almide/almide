// Fixed-preamble dead-import/dead-function/dead-data elimination.
//
// The v1 wasm renderer's preamble (`render_wasm_p3::preamble`) is a single
// hand-written WAT blob: 17 WASI imports + ~54 runtime helper functions +
// a dozen fixed error-message `data` segments, unconditionally spliced into
// EVERY module regardless of what the program actually calls (`hello.almd`'s
// `println("Hello, World!")` used to link `path_open`/`fd_readdir`/
// `clock_time_get`/... and carry "file not found"/"mkdir failed"/etc. bytes —
// none of it reachable from a `println`-only program). `wasm-opt` could trim
// this externally, but a v1-VERIFIED module is shipped byte-verbatim (the
// trust-spine proves exactly those bytes, and `wasm-opt` is an untrusted
// transform outside that proof — see `cli/build.rs`'s `produced_by_v1`
// comment) — so the elimination has to happen INSIDE the renderer, over
// units the trust-spine already emitted, not as an external unverified pass.
//
// This is a pure REMOVAL over already-rendered text: reachability walks the
// `$name` / `(i32.const N)` references the program's own emitted code and
// the preamble's own kept units make, and anything unreached is dropped.
// Soundness follows from how this codebase renders calls and addresses:
// every call site is a symbolic `$name` (never a raw numeric function/import
// index), so `wat::parse_str` re-resolves indices from whatever text
// survives — a name still referenced is always still defined. Every
// preamble data segment's address is a source-level named constant
// (`BOUNDS_MSG_ADDR` etc.) baked to a literal decimal `i32.const N` by the
// SAME Rust `format!` call on both the segment's declaration and every call
// site that passes it — there is no other representation (no runtime
// arithmetic ever computes one of these addresses), so a bare-decimal token
// scan finds every real reference; a coincidental match (a user program
// that happens to contain the literal integer 208) only over-approximates
// (keeps something unnecessarily), which is always safe, never unsound.

/// A single top-level unit of the fixed preamble.
enum PreambleSeg {
    /// Verbatim, never-removed text: the `(module` header, `(memory ...)`,
    /// and `(global ...)` declarations.
    Fixed(String),
    /// An `(import "mod" "name" (func $sym ...))`, `(func $sym ...)`, or
    /// `(data (i32.const N) "...")` block, keyed by the symbol/address it
    /// declares (a func/import's `$name`, or a data segment's decimal
    /// address prefixed `#` so the two key spaces can never collide).
    Removable { key: String, text: String },
}

/// Find the index right after the `)` that matches the `(` at `text[start]`.
/// Skips the contents of `"..."` string literals (WASI import names / data
/// bytes can be arbitrary) so a literal paren inside one never desyncs the
/// depth count; this codebase's WAT output has no backslash-escaped quotes.
fn match_paren(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Every distinct reference token in `text`: a `$name` (call/global target,
/// stored as-is) or the address of an `(i32.const N)` (data-segment target,
/// stored as `#N` so the two key spaces never collide). Also picks up
/// incidental non-reference `$name`s (local/param names) and `i32.const`s
/// (ordinary integer literals) — those simply never match a preamble unit's
/// key, so including them costs nothing but a few no-op set entries.
///
/// A `$name` runs until the next WAT delimiter (whitespace / `(` / `)`), NOT
/// just `[A-Za-z0-9_]` — self-hosted stdlib call names carry a literal `.`
/// (`$int.to_string`, `$value.as_array`, rendered verbatim by `Op::CallFn`'s
/// arm, un-sanitized unlike `DropVariant`/`DropWrapperRec`'s generated
/// `__drop_*` names). Stopping at the dot silently truncated these to `int`/
/// `value`, so a genuinely-called stdlib fn read as unreferenced — the
/// broadest possible way for a "which characters count as part of a name"
/// mismatch to under-approximate reachability, catch it once here, not
/// per-caller.
fn reference_tokens(text: &str) -> std::collections::BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut refs = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && !bytes[end].is_ascii_whitespace()
                && bytes[end] != b'(' && bytes[end] != b')'
            {
                end += 1;
            }
            if end > start {
                refs.insert(text[start..end].to_string());
            }
            i = end.max(i + 1);
        } else if bytes[i..].starts_with(b"i32.const ") {
            let start = i + "i32.const ".len();
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start {
                refs.insert(format!("#{}", &text[start..end]));
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    refs
}

/// Split a fully-assembled preamble string (module header, imports,
/// memory/data/globals, funcs — in textual order, extern imports already
/// spliced in) into segments, isolating each top-level `(import ...)` /
/// `(func $name ...)` / `(data (i32.const N) ...)` as its own
/// [`PreambleSeg::Removable`]. Scanning always jumps past a matched block's
/// full extent, so nested content (a func body's own `(call $x ...)`) is
/// never mistaken for a new top-level unit.
fn split_preamble_segments(pre: &str) -> Vec<PreambleSeg> {
    // Byte-slice (not `&str`) pattern matching: the preamble's WAT comments
    // contain multi-byte UTF-8 (em dashes etc.), so a plain `i += 1` byte
    // walk can land mid-character — `&pre[i..]` would then panic on the next
    // iteration's `starts_with`. `bytes.starts_with` never has that problem;
    // only the final `&pre[i..end]` extraction needs `i`/`end` to be char
    // boundaries, which they always are (both sit right on an ASCII `(`/`)`).
    let bytes = pre.as_bytes();
    let mut segs = Vec::new();
    let mut fixed_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let is_func_or_import =
            bytes[i..].starts_with(b"(import ") || bytes[i..].starts_with(b"(func $");
        let is_data = bytes[i..].starts_with(b"(data (i32.const ");
        if !is_func_or_import && !is_data {
            i += 1;
            continue;
        }
        let end = match_paren(pre, i);
        let block = &pre[i..end];
        let key = if is_data {
            data_addr(block).map(|n| format!("#{n}"))
        } else {
            first_dollar_name(block)
        };
        match key {
            // A block with no recognizable key can't be referenced by
            // anything downstream — keep it as Fixed so it's never a
            // candidate for (accidental) removal.
            None => {
                i = end;
            }
            Some(key) => {
                if fixed_start < i {
                    segs.push(PreambleSeg::Fixed(pre[fixed_start..i].to_string()));
                }
                segs.push(PreambleSeg::Removable { key, text: block.to_string() });
                fixed_start = end;
                i = end;
            }
        }
    }
    if fixed_start < pre.len() {
        segs.push(PreambleSeg::Fixed(pre[fixed_start..].to_string()));
    }
    segs
}

/// The first `$name` token in `block`, in appearance (not sorted) order — the
/// declared symbol of an `(import ...)` / `(func $name ...)` block is always
/// its first `$`-prefixed token.
fn first_dollar_name(block: &str) -> Option<String> {
    let bytes = block.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                return Some(block[start..end].to_string());
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// The address `N` of a `(data (i32.const N) "...")` block (`block` starts
/// with exactly that literal prefix — checked by the caller).
fn data_addr(block: &str) -> Option<&str> {
    let rest = block.strip_prefix("(data (i32.const ")?;
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    (end > 0).then(|| &rest[..end])
}

/// Drop every `(import ...)` / `(func $name ...)` / `(data (i32.const N)
/// ...)` in the fixed preamble that nothing in the rendered module can
/// reach. `used_text` is everything the preamble precedes in the final
/// module (data + closure table + program functions + mg helpers + `_start`
/// + public exports) — the program's own emitted code, whose references
/// seed the reachability walk. A preamble unit that itself references other
/// preamble units (`$__div_trap` calling `$fd_write`/`$proc_exit`;
/// `$__main_err` reading `DIVZERO_MSG_ADDR`'s bytes) pulls its own
/// dependencies in transitively, so e.g. any program with a bounds check
/// keeps exactly the WASI imports AND the "index out of bounds" message
/// that check's trap path needs — nothing more.
pub(crate) fn filter_unreachable_preamble(pre: &str, used_text: &str) -> String {
    let segs = split_preamble_segments(pre);
    let mut by_key: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for seg in &segs {
        if let PreambleSeg::Removable { key, text } = seg {
            by_key.insert(key.as_str(), text.as_str());
        }
    }

    let mut reachable: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut frontier: Vec<String> = reference_tokens(used_text)
        .into_iter()
        .filter(|k| by_key.contains_key(k.as_str()))
        .collect();
    reachable.extend(frontier.iter().cloned());
    while let Some(key) = frontier.pop() {
        let Some(text) = by_key.get(key.as_str()) else { continue };
        for dep in reference_tokens(text) {
            if by_key.contains_key(dep.as_str()) && reachable.insert(dep.clone()) {
                frontier.push(dep);
            }
        }
    }

    segs.into_iter()
        .map(|seg| match seg {
            PreambleSeg::Fixed(text) => text,
            PreambleSeg::Removable { key, text } => {
                if reachable.contains(&key) {
                    text
                } else {
                    String::new()
                }
            }
        })
        .collect()
}

// prune_unreachable_functions moved here from render_wasm.rs (max-lines, #852):
// the MIR-function twin of the preamble reachability pass below — same family,
// moved verbatim.
/// Drop every `MirFunction` unreachable from `main` / a declared export, so the
/// wasm renderer never has to emit it (nor put it in the closure-dispatch
/// function table, nor carry its `@extern` import).
///
/// Reachability is computed the SAME way [`filter_unreachable_preamble`] computes
/// it for the fixed runtime: render every function ONCE with the full,
/// UNPRUNED `func_slots`/`label_off` (reachability doesn't depend on final
/// slot numbers, only on which names get referenced — those are recomputed
/// fresh, self-consistently, when [`render_wasm_program`] renders the pruned
/// result), then text-scan each rendered body for `$name` references via
/// [`reference_tokens`]. This is deliberately NOT a hand-enumerated match over
/// `Op` variants: an early version of this pass matched only `CallFn`/
/// `FuncRef`/`DropVariant`/`DropWrapperRec` and missed BOTH `Op::Call{RtFn::
/// PrintStr}` (a runtime-boundary call to the auto-linked `print_str` fn,
/// outside `CallFn`) and the `DropListStrValue`-family ops that ALSO hardcode
/// a `(call $__drop_..)` outside those four — each one only surfaced as a
/// `wat::parse_str` "undefined function" error on a DIFFERENT spec file.
/// Scanning the ACTUAL RENDERED TEXT can't have a fourth such gap: whatever
/// `Op` produced a call, by the time this runs it's already literal `$name`
/// text, the same text `wat::parse_str` itself resolves names from.
///
/// ONE edge kind can't be recovered from text: `Op::FuncRef` (a lifted
/// lambda's VALUE, used as a `CallIndirect`'s dispatch index) renders to a
/// bare `(i64.const {slot})`, not a `$name` — the closure's identity IS the
/// numeric slot. A missed `FuncRef` edge doesn't fail to parse (no dangling
/// name — the SLOT still exists, just pointing at whatever ELSE the pruned
/// list's reindexing put there), so it can silently mis-dispatch at runtime
/// instead of erroring at build time (caught via `codegen_functional_test`'s
/// "multi-fn HOF" trapping after this pass, not via a build failure) — this
/// is read from the STRUCTURED `Op::FuncRef` list below, never from text.
///
/// SOUNDNESS: this can only ever REMOVE a function nothing calls — dropping
/// something still truly needed can only manifest as `wat::parse_str`
/// rejecting a dangling `(call $name)` at build time, a loud failure, never a
/// silent miscompile (see `cli/build.rs`'s `render_wasm_module`). Iterated to
/// a fixpoint (a dead function referenced only by another dead function
/// prunes in a later round).
pub(crate) fn prune_unreachable_functions(prog: &MirProgram) -> MirProgram {
    let func_slots: BTreeMap<String, u32> =
        prog.functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i as u32)).collect();
    let param_counts: BTreeMap<String, usize> =
        prog.functions.iter().map(|f| (f.name.clone(), f.params.len())).collect();
    let bodies: BTreeMap<String, String> = prog
        .functions
        .iter()
        .map(|f| (f.name.clone(), render_wasm_fn(f, &func_slots, &param_counts)))
        .collect();
    // `Op::FuncRef { name, .. }` — "the function-table slot of the lifted
    // function `name`" — renders to a bare `(i64.const {slot})`, NOT a `$name`
    // token (see its arm in `render_wasm_p2_b.rs`): the closure value IS the
    // numeric slot, materialized once the table's final layout is known. A
    // text scan of the rendered body therefore can't see this edge at all —
    // read it from the STRUCTURED `Op` instead, the one edge kind that can't
    // be recovered from rendered text.
    let func_ref_edges: BTreeMap<String, Vec<String>> = prog
        .functions
        .iter()
        .map(|f| {
            let targets = f
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::FuncRef { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            (f.name.clone(), targets)
        })
        .collect();

    // `__mg_init` (mutable-global slot init) and `__global_init` (abortable
    // top-let init) are called from the SYNTHESIZED `_start` wrapper
    // (`render_wasm_program`'s `ginit`/`mg_init` locals), never from any
    // `MirFunction`'s own ops — so neither text-scanning nor `FuncRef`
    // tracking can ever find a reference to them. `render_wasm_program`
    // itself decides whether to emit the call by NAME EXISTENCE in
    // `prog.functions` (`prog.functions.iter().any(|f| f.name == "...")`);
    // mirror that exact condition here as an extra root, or the ONLY thing
    // that decides whether these run becomes "did this pass happen to keep
    // them" instead of "does the program need them" — which is how
    // `module_var_test.almd`'s mutable globals silently stayed at their
    // zero-init default (a runtime wrong-answer, not a build failure).
    let roots: BTreeSet<String> = std::iter::once("main".to_string())
        .chain(prog.exports.iter().map(|(_, internal, _, _)| internal.clone()))
        .chain(
            prog.functions
                .iter()
                .map(|f| f.name.as_str())
                .filter(|n| *n == "__mg_init" || *n == "__global_init")
                .map(String::from),
        )
        .collect();
    let mut reachable: BTreeSet<String> = roots.clone();
    let mut frontier: Vec<String> = roots.into_iter().collect();
    while let Some(name) = frontier.pop() {
        let mut targets: Vec<String> = Vec::new();
        if let Some(body) = bodies.get(&name) {
            targets.extend(reference_tokens(body).into_iter().filter(|t| bodies.contains_key(t)));
        }
        if let Some(refs) = func_ref_edges.get(&name) {
            targets.extend(refs.iter().cloned());
        }
        for token in targets {
            if bodies.contains_key(&token) && reachable.insert(token.clone()) {
                frontier.push(token);
            }
        }
    }

    let kept: Vec<MirFunction> =
        prog.functions.iter().filter(|f| reachable.contains(&f.name)).cloned().collect();
    MirProgram {
        functions: kept,
        exports: prog.exports.clone(),
        mutable_global_count: prog.mutable_global_count,
    }
}
