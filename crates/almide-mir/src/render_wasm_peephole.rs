// Constant-fold-through-extend/wrap: a NARROW, provably-safe wasm-text
// peephole over each function's OWN rendered body (not the preamble — that's
// `render_wasm_dce.rs`).
//
// The i64-uniform scalar convention (every `Int`-repr value is an i64 local;
// `prim.*` addressing/memory ops need i32) means a self-hosted fn doing raw
// pointer arithmetic — `stdlib/print_str.almd`'s `prim.store32(8, data)` — has
// every literal offset (8, 12, 100, ...) rendered as `(local.set $vN
// (i64.const V))` then, at each actual memory op, unwound back via
// `(i32.wrap_i64 (local.get $vN))`. When `$vN` is defined EXACTLY ONCE and
// that definition is a bare `i64.const`, the wrap's result is itself a
// COMPILE-TIME CONSTANT (`V as u32 as i32`, exactly wasm's own wrap
// semantics) — substituting `(i32.const <wrapped V>)` directly can never
// observe a different value, no matter how many times `$vN` is read this way
// or what runs between its definition and any given use: unlike folding a
// NON-constant value (a local read, a memory load, a call result) through the
// same round-trip — which risks reordering a side effect, or reading a value
// that changed in between — a constant has neither concern. This pass is
// deliberately scoped to exactly that one always-safe case.

/// `(local.set $vN (i64.const V))` count and last-seen value per local, plus
/// the TOTAL `local.set $vN` count (any RHS) per local — the latter is what
/// proves "exactly one definition, and it's this constant" rather than
/// merely "the last constant assignment we happened to see" (a local
/// reassigned through a non-constant path elsewhere must never be folded).
fn single_const_defs(body: &str) -> std::collections::BTreeMap<String, i64> {
    let bytes = body.as_bytes();
    let mut set_count: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut const_val: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i..].starts_with(b"(local.set $v") {
            i += 1;
            continue;
        }
        let end = match_paren(body, i);
        let stmt = &body[i..end];
        if let Some(name) = local_set_target(stmt) {
            *set_count.entry(name.clone()).or_insert(0) += 1;
            if let Some(v) = const_set_value(stmt) {
                const_val.insert(name, v);
            }
        }
        i = end;
    }
    const_val
        .into_iter()
        .filter(|(name, _)| set_count.get(name).copied().unwrap_or(0) == 1)
        .collect()
}

/// The `$vN` a `(local.set $vN ...)` statement (already paren-matched) assigns.
fn local_set_target(stmt: &str) -> Option<String> {
    let rest = stmt.strip_prefix("(local.set ")?;
    let end = rest.find(|c: char| c.is_whitespace() || c == ')')?;
    Some(rest[..end].to_string())
}

/// If `(local.set $vN (i64.const V))` is EXACTLY that shape (the whole RHS is
/// one bare `i64.const`, nothing more), `V`; else `None` (a non-constant or
/// compound RHS is never a fold candidate).
fn const_set_value(stmt: &str) -> Option<i64> {
    let target = local_set_target(stmt)?;
    let prefix = format!("(local.set {target} (i64.const ");
    let rest = stmt.strip_prefix(&prefix)?;
    let rest = rest.strip_suffix("))")?;
    rest.parse::<i64>().ok()
}

/// Every `(i32.wrap_i64 (local.get $vN))` in `body`, replaced with
/// `(i32.const <V as u32 as i32>)` for each `$vN` in `folds` — the `as`
/// chain is exactly wasm's own `i32.wrap_i64` truncation, so the replacement
/// value is bit-for-bit what the original round-trip would have computed.
fn apply_const_wrap_folds(body: &str, folds: &std::collections::BTreeMap<String, i64>) -> String {
    // Byte-slice matching (not `&str`), same reason as `render_wasm_dce.rs`'s
    // `split_preamble_segments`: a plain `i += 1` byte walk over `&str`
    // slicing can land mid-UTF-8-character and panic. `out` is built from
    // whole matched byte ranges (either a passthrough single byte — always
    // ASCII in this codebase's own generated WAT — or a complete `expr`
    // slice), so it stays valid UTF-8 throughout.
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i..].starts_with(b"(i32.wrap_i64 (local.get $v") {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let end = match_paren(body, i);
        let expr = &body[i..end];
        let name = expr
            .strip_prefix("(i32.wrap_i64 (local.get ")
            .and_then(|r| r.strip_suffix("))"));
        match name.and_then(|n| folds.get(n)) {
            Some(&v) => {
                let wrapped = v as u32 as i32;
                out.extend_from_slice(format!("(i32.const {wrapped})").as_bytes());
            }
            None => out.extend_from_slice(expr.as_bytes()),
        }
        i = end;
    }
    String::from_utf8(out).expect("byte-for-byte copy of valid UTF-8 input stays valid UTF-8")
}

/// Apply [`apply_const_wrap_folds`] using this SAME function body's own
/// single-constant-definition locals (see [`single_const_defs`]) — the
/// self-contained entry point `render_wasm_program`/`render_wasm` call per
/// rendered function.
pub(crate) fn fold_const_wrap_roundtrips(body: &str) -> String {
    let folds = single_const_defs(body);
    let folded = if folds.is_empty() {
        body.to_string()
    } else {
        apply_const_wrap_folds(body, &folds)
    };
    strip_dead_const_sets(&folded)
}

/// Remove `(local.set $vN (i64.const V))` / `(local.set $vN (f64.const V))`
/// statements whose local is NEVER read (`(local.get $vN)` absent from the
/// whole rendered function, tail included). These are constants whose every
/// consumer was folded away statically — e.g. the divisor local of a
/// `÷ 2^k` after the Div render strength-reduced it — which otherwise cost
/// one dead store per hot-loop iteration. Removing a never-read write cannot
/// change any observable: the local has no other reader by construction, and
/// a bare-const RHS has no side effect.
fn strip_dead_const_sets(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i..].starts_with(b"(local.set $v") {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let end = match_paren(body, i);
        let stmt = &body[i..end];
        let is_dead_const = local_set_target(stmt).is_some_and(|name| {
            let bare_const = stmt
                .strip_prefix(&format!("(local.set {name} "))
                .and_then(|r| r.strip_suffix("))"))
                .is_some_and(|rhs| {
                    (rhs.starts_with("(i64.const ") || rhs.starts_with("(f64.const "))
                        && !rhs[1..].contains('(')
                });
            // `(local.get $vN)` with its closing paren — exact-name match, so
            // `$v1` never shadows `$v10`.
            bare_const && !body.contains(&format!("(local.get {name})"))
        });
        if !is_dead_const {
            out.extend_from_slice(stmt.as_bytes());
        }
        i = end;
    }
    String::from_utf8(out).expect("byte-for-byte copy of valid UTF-8 input stays valid UTF-8")
}
