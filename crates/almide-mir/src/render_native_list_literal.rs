//! A list literal built whole by `store_list_slot` (render_native_addr_op.rs):
//! each element waits in its own local until the last one is stored, then the
//! list is one `vec![…]`. A map literal's entry pairs and entry list are not
//! built at all (crate::native_map_entries).
use super::*;

fn put(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    out.push_str(line);
    out.push('\n');
}

/// The local of slot `k` of the literal `b`, holding `rhs` and typed `ty` —
/// a map entry's pair (`pair`) is left to inference.
pub(super) fn elem_local(out: &mut String, indent: usize, (b, k): (&str, usize), ty: &str, rhs: &str, pair: bool) {
    let ann = if pair { String::new() } else { format!(": {ty}") };
    put(out, indent, &format!("let {b}_e{k}{ann} = {rhs};"));
}

/// After slot `k` of the `n` of the string list `base`: the whole list once
/// its last element is stored, unless it is a map entry's pair left unbuilt.
fn built_whole(out: &mut String, indent: usize, base: ValueId, (n, k): (usize, usize), ty: &str) {
    if k + 1 == n && !crate::native_map_entries::is_deferred(base) {
        let b = var(base);
        let es: Vec<String> = (0..n).map(|j| format!("{b}_e{j}")).collect();
        put(out, indent, &format!("let mut {b}: {ty} = vec![{}];", es.join(", ")));
    }
}

/// A string `rhs` stored into the string list `base` built whole; `false`
/// when `base` is not built whole.
pub(super) fn str_elem(st: &mut AddrState, out: &mut String, indent: usize, base: ValueId, rhs: &str) -> bool {
    let Some((n, k)) = st.literal.get(&base).copied() else { return false };
    elem_local(out, indent, (&var(base), k), "AlmideString", rhs, false);
    st.literal.insert(base, (n, k + 1));
    built_whole(out, indent, base, (n, k), "Vec<AlmideString>");
    true
}

/// Whether the borrowed slot store of `v` (from `src`) into `base` copies:
/// a borrow-view slot whose value is still read after the store.
pub(super) fn copies(st: &AddrState, base: ValueId, (src, v): (ValueId, ValueId), t: NTy) -> bool {
    st.borrowed_list_slots.contains(&(base, v)) && !crate::render_native_keyed::dies_at_store(src, v, t)
}
