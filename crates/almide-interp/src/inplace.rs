//! In-place container mutation, modeled by binding write-back.
//!
//! The stdlib's `mut`-receiver mutators (`list.push`, `map.insert`, …) mutate
//! the CALLER's binding and mostly return `Unit`: the program reads the effect
//! on the variable, never a returned container. The generic dispatch path
//! evaluates every argument by value before the callee sees it, so by then the
//! receiver's binding identity is gone and the effect has nowhere to land —
//! which is why this whole family used to abstain.
//!
//! Intercepting one step earlier fixes that. At the call site the receiver is
//! still an *expression*; when it is a plain `Var`, its `VarId` is the binding,
//! and the mutation becomes read → transform → `scope.assign`. That is exactly
//! the shape `IrStmtKind::IndexAssign` already uses for `xs[i] = v`, and it is
//! exactly Almide's COW value semantics: `list.push(xs, v)` observes as
//! `xs = xs + [v]`, so an alias bound before the push keeps its own elements
//! (the C-033 `alias_cow` matrix pins that promise on both backends).
//!
//! Two receiver shapes stay out of scope, and both abstain by NAME so the
//! ledger records what is missing rather than a generic capability gap:
//!
//! 1. **A `mut` parameter.** `call_function` binds parameters in the callee's
//!    own frame, so a write-back there stops at that frame and the caller never
//!    sees it. Native/wasm return the buffer to the caller's slot
//!    (`MutParamLoweringPass`, C-132); until the interp models that copy-out,
//!    writing back locally would silently drop the effect — a wrong third vote,
//!    which is worse than an honest skip (issue #1022).
//! 2. **A non-`Var` receiver** — a record field (`push9(b.items, 7)`), an
//!    index, a temporary. There is no single `VarId` to assign to.

use std::rc::Rc;

use almide_ir::VarId;

use crate::value::Value;

/// The ops this tier writes back. The rule is a WHOLE-CONTAINER mutation over
/// a container the interp models natively: append one element, remove one
/// element, or empty the container. Every one has exact semantics with no
/// bounds or endianness surface, and each mirrors a named runtime fn:
/// `almide_rt_list_push` → `Vec::push`, `almide_rt_map_insert` →
/// `AlmideMap::insert`, `almide_rt_string_push` → `String::push_str`, …
///
/// The `bytes` BYTE-LEVEL WRITER family (`set_*` / `append_*` / `write_*`, plus
/// `fill` / `copy_from` / `copy_within`) joins them through [`bytes_write_op`],
/// which derives each member's bounds rule and byte order from its NAME rather
/// than from a hand-kept list (issue #1021). The name is the whole rule, so a
/// new family member is covered on arrival — and
/// `every_stdlib_bytes_writer_is_modeled` reads `stdlib/bytes.almd` itself and
/// fails if one ever is not.
///
/// `every_written_back_op_is_an_intercepted_op` (below) is the gate that keeps
/// this table and `is_inplace_mutating_op` from drifting apart.
pub(crate) fn writes_back(module: &str, func: &str) -> bool {
    if module == "bytes" && bytes_write_op(func).is_some() {
        return true;
    }
    matches!(
        (module, func),
        ("list", "push")
            | ("list", "pop")
            | ("list", "clear")
            | ("map", "insert")
            | ("map", "delete")
            | ("map", "clear")
            | ("string", "push")
            | ("string", "clear")
            | ("bytes", "push")
            | ("bytes", "clear")
    )
}

/// A byte-level scalar encoding: `width` bytes in the requested order.
///
/// SIGNEDNESS IS ABSENT ON PURPOSE. `almide_rt_bytes_append_u16_le` casts
/// `val as u16` and `..._i16_le` casts `val as i16`, but both are truncating
/// two's-complement casts of the SAME i64, so `(n as u16).to_le_bytes()` and
/// `(n as i16).to_le_bytes()` are byte-for-byte equal. Modeling one width
/// instead of two kinds removes a cell that could only ever be got wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Enc {
    width: usize,
    big_endian: bool,
    float: bool,
}

/// One member of the byte-level writer family, keyed by what it does to the
/// buffer rather than by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BytesWrite {
    /// `append_*`, and the big-endian `write_*` serialization twins: encode the
    /// scalar and EXTEND the buffer. No bounds rule — appends always fit.
    Append(Enc),
    /// `set_*` (including `set_at`, a 1-byte store): encode and splice at `pos`.
    /// A window that does not fit is a SILENT no-op — every `almide_rt_bytes_set_*`
    /// guards with `if p + N <= b.len()` and returns, never aborts.
    Set(Enc),
    /// `write_bool` — `b.push(if val { 1 } else { 0 })`.
    WriteBool,
    /// `write_string_be` — a u32 BE length prefix, then the UTF-8 bytes.
    WriteStringBe,
    /// `fill` — overwrite every existing byte; the length never changes.
    Fill,
    /// `copy_from(dst, src, dst_off, src_off, len)`.
    CopyFrom,
    /// `copy_within(b, src_start, src_end, dst)`.
    CopyWithin,
    /// The sized-type surface (`write_uint16(b, v, .le)`), whose endianness is a
    /// trailing `Endian` ARGUMENT instead of part of the name. Each `@inline_rust`
    /// body dispatches to the same `almide_rt_bytes_append_*` pair, so the model
    /// is [`Self::Append`] with the order read at runtime.
    AppendSized { width: usize, float: bool },
    /// `set_uint16(b, offset, v, .le)` and friends — [`Self::Set`] with a runtime order.
    SetSized { width: usize, float: bool },
}

/// The width/kind of a sized-type writer suffix (`uint16`, `float32`, …). These
/// spell the type, not the byte layout, so their endianness arrives separately.
fn sized_suffix(name: &str) -> Option<(usize, bool)> {
    Some(match name {
        "uint16" => (2, false),
        "uint32" => (4, false),
        "int32" => (4, false),
        "float32" => (4, true),
        _ => return None,
    })
}

/// Parse a name-encoded scalar suffix (`u8`, `i16_le`, `f64_be`) into its layout.
///
/// A 1-byte value must carry NO order suffix and a multi-byte value must carry
/// one — that is what the stdlib spells, and rejecting the other shapes keeps an
/// unrecognized name an honest `None` (an abstain) rather than a guess.
fn enc_of(suffix: &str) -> Option<Enc> {
    let (kind, big_endian) = match suffix.rsplit_once('_') {
        Some((k, "le")) => (k, Some(false)),
        Some((k, "be")) => (k, Some(true)),
        _ => (suffix, None),
    };
    let (tag, digits) = kind.split_at_checked(1)?;
    let float = match tag {
        "u" | "i" => false,
        "f" => true,
        _ => return None,
    };
    let width = match digits.parse::<usize>().ok()? {
        8 => 1,
        16 => 2,
        32 => 4,
        64 => 8,
        _ => return None,
    };
    // A float is only ever f32/f64, and a single byte has no order to pick.
    if float && width < 4 {
        return None;
    }
    match (width, big_endian) {
        (1, None) => Some(Enc { width: 1, big_endian: false, float }),
        (_, Some(big_endian)) => Some(Enc { width, big_endian, float }),
        _ => None,
    }
}

/// Classify a `bytes` function as a byte-level writer, from its NAME alone.
///
/// The mapping is total over `stdlib/bytes.almd`'s `set_` / `append_` / `write_`
/// prefixes plus the three named buffer ops, and
/// `every_stdlib_bytes_writer_is_modeled` proves that against the stdlib source.
pub(crate) fn bytes_write_op(func: &str) -> Option<BytesWrite> {
    // The sized-type surface FIRST: `set_uint16` also starts with `set_`, and
    // `uint16` is not a name-encoded suffix, so order matters only for clarity.
    if let Some(rest) = func.strip_prefix("write_") {
        if let Some((width, float)) = sized_suffix(rest) {
            return Some(BytesWrite::AppendSized { width, float });
        }
    }
    if let Some(rest) = func.strip_prefix("set_") {
        if let Some((width, float)) = sized_suffix(rest) {
            return Some(BytesWrite::SetSized { width, float });
        }
    }
    match func {
        // `almide_rt_bytes_set_at`: `if (i as usize) < b.len() { b[i] = val as u8 }`
        // — a 1-byte `Set`, bound-identical to `set_u8`'s `p + 1 <= b.len()`.
        "set_at" => return Some(BytesWrite::Set(Enc { width: 1, big_endian: false, float: false })),
        "write_bool" => return Some(BytesWrite::WriteBool),
        "write_string_be" => return Some(BytesWrite::WriteStringBe),
        "fill" => return Some(BytesWrite::Fill),
        "copy_from" => return Some(BytesWrite::CopyFrom),
        "copy_within" => return Some(BytesWrite::CopyWithin),
        _ => {}
    }
    if let Some(s) = func.strip_prefix("append_") {
        return enc_of(s).map(BytesWrite::Append);
    }
    // `write_u8` / `write_u32_be` / `write_i64_be` / `write_f64_be` are appends
    // too — the serialization cursor's big-endian twins of `append_*`.
    if let Some(s) = func.strip_prefix("write_") {
        return enc_of(s).map(BytesWrite::Append);
    }
    if let Some(s) = func.strip_prefix("set_") {
        return enc_of(s).map(BytesWrite::Set);
    }
    None
}

/// The scalar's bytes, in `enc`'s order. Mirrors the native `as`-cast +
/// `to_{le,be}_bytes` of every `almide_rt_bytes_{append,set,write}_*`.
fn encode(enc: Enc, v: &Value) -> Option<Vec<u8>> {
    let mut out = if enc.float {
        let Value::Float(f) = v else { return None };
        match enc.width {
            // `val as f32` is the native cast — it ROUNDS, and the rounded
            // value is what lands in the buffer.
            4 => (*f as f32).to_le_bytes().to_vec(),
            8 => f.to_le_bytes().to_vec(),
            _ => return None,
        }
    } else {
        let Value::Int(n) = v else { return None };
        // The low `width` bytes of the two's-complement i64 — exactly what a
        // truncating `as u16`/`as i16`/… cast keeps.
        n.to_le_bytes()[..enc.width].to_vec()
    };
    if enc.big_endian {
        out.reverse();
    }
    Some(out)
}

/// Read an `Endian` argument (`.le` / `.be`) as "is big-endian".
fn endian_is_big(v: &Value) -> Option<bool> {
    let Value::Variant { ctor, .. } = v else { return None };
    match ctor.as_str() {
        "LittleEndian" => Some(false),
        "BigEndian" => Some(true),
        _ => None,
    }
}

/// The receiver's `VarId`, when the receiver is a plain variable.
pub(crate) fn receiver_var(recv: &almide_ir::IrExpr) -> Option<VarId> {
    match &recv.kind {
        almide_ir::IrExprKind::Var { id } => Some(*id),
        _ => None,
    }
}

/// Apply one in-place op to the receiver's storage slot. Returns the value the
/// call itself evaluates to (`list.pop -> Option[A]`, everything else `Unit`),
/// or `None` for a shape the typed IR should never produce.
///
/// Every arm reaches its container through `Rc::make_mut`, which is the COW
/// rule itself: sole owner → mutate in place (so a push loop stays linear);
/// an alias exists → clone once, and the alias keeps the elements it had at
/// its bind point (C-033 `alias_cow`).
///
/// Each arm mirrors its native runtime fn exactly, including the truncating
/// `val as u8` of `almide_rt_bytes_push` — the interp models `Bytes` as a
/// `List` of `Int`, so without the mask a `bytes.push(b, 256)` would read back
/// as 256 here and 0 on both backends.
pub(crate) fn apply(module: &str, func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    match module {
        "list" => apply_list(func, slot, rest),
        "map" => apply_map(func, slot, rest),
        "string" => apply_string(func, slot, rest),
        "bytes" => apply_bytes(func, slot, rest),
        _ => None,
    }
}

fn apply_list(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::List(rc) = slot else { return None };
    let xs = Rc::make_mut(rc);
    match func {
        // `almide_rt_list_push`: Vec::push.
        "push" => {
            xs.push(rest.into_iter().next()?);
            Some(Value::Unit)
        }
        // `almide_rt_list_pop`: Vec::pop — returns the removed element as an
        // Option, and `None` on an empty list (no abort).
        "pop" => Some(Value::Option(xs.pop().map(Box::new))),
        "clear" => {
            xs.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_map(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::Map(rc) = slot else { return None };
    let entries = Rc::make_mut(rc);
    match func {
        // `almide_rt_map_insert` -> `AlmideMap::insert`: replace the value in
        // place when the key is present (keeping its slot), else append. The
        // shared helper is the same one `map.set` and `IrStmtKind::MapInsert`
        // use, so the mutating and functional forms cannot diverge.
        "insert" => {
            let mut it = rest.into_iter();
            crate::eval::map_insert(entries, it.next()?, it.next()?);
            Some(Value::Unit)
        }
        // `AlmideMap::remove`: "Remove, keeping the order of the remaining
        // entries" — so a positional remove, never a swap-remove.
        "delete" => {
            let key = rest.into_iter().next()?;
            if let Some(i) = entries.iter().position(|(k, _)| k == &key) {
                entries.remove(i);
            }
            Some(Value::Unit)
        }
        "clear" => {
            entries.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_string(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    let Value::Str(rc) = slot else { return None };
    let s = Rc::make_mut(rc);
    match func {
        // `almide_rt_string_push`: push_str — the argument is a String suffix,
        // not a char.
        "push" => {
            let Value::Str(suffix) = rest.into_iter().next()? else {
                return None;
            };
            s.push_str(&suffix);
            Some(Value::Unit)
        }
        "clear" => {
            s.clear();
            Some(Value::Unit)
        }
        _ => None,
    }
}

fn apply_bytes(func: &str, slot: &mut Value, rest: Vec<Value>) -> Option<Value> {
    // `Bytes` is a `List` of `Int` here (see `bridge.rs::bytes_fn`).
    let Value::List(rc) = slot else { return None };
    let xs = Rc::make_mut(rc);
    match func {
        // `almide_rt_bytes_push`: `b.push(val as u8)` — the cast truncates, so
        // the stored element is always 0..=255.
        "push" => {
            let Value::Int(v) = rest.into_iter().next()? else {
                return None;
            };
            xs.push(Value::Int((v as u8) as i64));
            Some(Value::Unit)
        }
        "clear" => {
            xs.clear();
            Some(Value::Unit)
        }
        // The byte-level writer family (#1021), modeled over the same
        // `List` of `Int` — every arm mirrors its `almide_rt_bytes_*` oracle,
        // bounds guard included.
        _ => apply_bytes_write(bytes_write_op(func)?, xs, rest),
    }
}

/// Read the buffer as raw bytes. Every element got here through a writer that
/// masked to a byte (or through `bytes.push`/`from_*`), so a non-`Int` element
/// is a malformed receiver, not a value to coerce.
fn raw(xs: &[Value]) -> Option<Vec<u8>> {
    xs.iter()
        .map(|e| match e {
            Value::Int(n) => Some(*n as u8),
            _ => None,
        })
        .collect()
}

/// Splice `src` into the buffer at `pos`, or leave it untouched when the window
/// does not fit — the shared bounds rule of every `almide_rt_bytes_set_*`
/// (`if p + N <= b.len()`), including the negative-`pos` case: `pos as usize`
/// makes a negative offset enormous, so the guard rejects it silently.
fn splice(xs: &mut [Value], pos: i64, src: &[u8]) {
    let p = pos as usize;
    if p.checked_add(src.len()).is_none_or(|end| end > xs.len()) {
        return;
    }
    for (i, b) in src.iter().enumerate() {
        xs[p + i] = Value::Int(*b as i64);
    }
}

fn apply_bytes_write(op: BytesWrite, xs: &mut Vec<Value>, rest: Vec<Value>) -> Option<Value> {
    let mut it = rest.into_iter();
    match op {
        BytesWrite::Append(enc) => {
            let src = encode(enc, &it.next()?)?;
            xs.extend(src.into_iter().map(|b| Value::Int(b as i64)));
        }
        BytesWrite::Set(enc) => {
            let Value::Int(pos) = it.next()? else { return None };
            let src = encode(enc, &it.next()?)?;
            splice(xs, pos, &src);
        }
        // `write_uint16(b, value, endian)` — the order is the LAST argument.
        BytesWrite::AppendSized { width, float } => {
            let value = it.next()?;
            let big_endian = endian_is_big(&it.next()?)?;
            let src = encode(Enc { width, big_endian, float }, &value)?;
            xs.extend(src.into_iter().map(|b| Value::Int(b as i64)));
        }
        // `set_uint16(b, offset, value, endian)`.
        BytesWrite::SetSized { width, float } => {
            let Value::Int(pos) = it.next()? else { return None };
            let value = it.next()?;
            let big_endian = endian_is_big(&it.next()?)?;
            let src = encode(Enc { width, big_endian, float }, &value)?;
            splice(xs, pos, &src);
        }
        // `almide_rt_bytes_write_bool`: `b.push(if val { 1 } else { 0 })`.
        BytesWrite::WriteBool => {
            let Value::Bool(v) = it.next()? else { return None };
            xs.push(Value::Int(i64::from(v)));
        }
        // `almide_rt_bytes_write_string_be`: a u32 BE byte-LENGTH prefix, then
        // the UTF-8 bytes (`s.as_bytes()`, so a multibyte char counts its bytes).
        BytesWrite::WriteStringBe => {
            let Value::Str(s) = it.next()? else { return None };
            let sb = s.as_bytes();
            for b in (sb.len() as u32).to_be_bytes() {
                xs.push(Value::Int(b as i64));
            }
            xs.extend(sb.iter().map(|b| Value::Int(*b as i64)));
        }
        // `almide_rt_bytes_fill`: overwrite every existing byte in place. An
        // EMPTY buffer stays empty — fill never grows it.
        BytesWrite::Fill => {
            let Value::Int(v) = it.next()? else { return None };
            let v = Value::Int((v as u8) as i64);
            for slot in xs.iter_mut() {
                *slot = v.clone();
            }
        }
        // `almide_rt_bytes_copy_from`: both offsets must be strictly inside
        // their buffers, then `len` is clamped to whichever tail is shorter.
        BytesWrite::CopyFrom => {
            let Value::List(src) = it.next()? else { return None };
            let Value::Int(dst_off) = it.next()? else { return None };
            let Value::Int(src_off) = it.next()? else { return None };
            let Value::Int(len) = it.next()? else { return None };
            let src = raw(&src)?;
            let (d, s) = (dst_off as usize, src_off as usize);
            if d >= xs.len() || s >= src.len() {
                return Some(Value::Unit);
            }
            let n = (len as usize).min(xs.len() - d).min(src.len() - s);
            for i in 0..n {
                xs[d + i] = Value::Int(src[s + i] as i64);
            }
        }
        // `almide_rt_bytes_copy_within`: `src_end` clamps to the length, and the
        // whole move is a no-op unless the source range is non-empty AND the
        // destination window fits.
        BytesWrite::CopyWithin => {
            let Value::Int(src_start) = it.next()? else { return None };
            let Value::Int(src_end) = it.next()? else { return None };
            let Value::Int(dst) = it.next()? else { return None };
            let cur = raw(xs)?;
            let (s, d) = (src_start as usize, dst as usize);
            let e = (src_end as usize).min(cur.len());
            if s < e && d.checked_add(e - s).is_none_or(|end| end > cur.len()) {
                return Some(Value::Unit);
            }
            if s < e {
                for i in 0..(e - s) {
                    xs[d + i] = Value::Int(cur[s + i] as i64);
                }
            }
        }
    }
    Some(Value::Unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(xs: &[i64]) -> Value {
        Value::list(xs.iter().map(|n| Value::Int(*n)).collect())
    }

    fn as_ints(v: &Value) -> Vec<i64> {
        match v {
            Value::List(xs) => xs
                .iter()
                .map(|e| match e {
                    Value::Int(n) => *n,
                    _ => panic!("non-int element"),
                })
                .collect(),
            _ => panic!("not a list"),
        }
    }

    fn map_of(pairs: &[(i64, i64)]) -> Value {
        Value::Map(Rc::new(
            pairs.iter().map(|(k, v)| (Value::Int(*k), Value::Int(*v))).collect(),
        ))
    }

    fn pairs(v: &Value) -> Vec<(i64, i64)> {
        match v {
            Value::Map(e) => e
                .iter()
                .map(|(k, v)| match (k, v) {
                    (Value::Int(k), Value::Int(v)) => (*k, *v),
                    _ => panic!("non-int entry"),
                })
                .collect(),
            _ => panic!("not a map"),
        }
    }

    #[test]
    fn list_push_appends_and_returns_unit() {
        let mut slot = ints(&[1, 2]);
        let out = apply("list", "push", &mut slot, vec![Value::Int(3)]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2, 3]);
        assert_eq!(out, Value::Unit);
    }

    #[test]
    fn list_pop_returns_the_removed_element() {
        let mut slot = ints(&[1, 2, 3]);
        let out = apply("list", "pop", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2]);
        assert_eq!(out, Value::Option(Some(Box::new(Value::Int(3)))));
    }

    #[test]
    fn list_pop_on_empty_is_none_not_an_error() {
        let mut slot = ints(&[]);
        let out = apply("list", "pop", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), Vec::<i64>::new());
        assert_eq!(out, Value::Option(None));
    }

    /// The COW promise of C-033, at the level this tier implements it: a value
    /// cloned out BEFORE the mutation keeps the elements it had at that point,
    /// and the mutated slot moves on alone.
    #[test]
    fn a_clone_taken_before_the_push_keeps_its_own_elements() {
        let mut slot = ints(&[1, 2, 3]);
        let alias = slot.clone();
        apply("list", "push", &mut slot, vec![Value::Int(99)]).unwrap();
        assert_eq!(as_ints(&slot), vec![1, 2, 3, 99]);
        assert_eq!(as_ints(&alias), vec![1, 2, 3]);
    }

    /// The same promise for the shrink direction, which is the shape
    /// `alias_cow` block J exercises (`list.clear` with a live alias).
    #[test]
    fn a_clone_taken_before_the_clear_keeps_its_own_elements() {
        let mut slot = ints(&[1, 2, 3]);
        let alias = slot.clone();
        apply("list", "clear", &mut slot, vec![]).unwrap();
        assert_eq!(as_ints(&slot), Vec::<i64>::new());
        assert_eq!(as_ints(&alias), vec![1, 2, 3]);
    }

    /// Sole ownership must mutate the SAME allocation — that is what keeps a
    /// push loop linear instead of copying the whole container per call.
    #[test]
    fn a_sole_owner_push_does_not_reallocate_the_container() {
        let mut slot = Value::list(Vec::with_capacity(8));
        let Value::List(rc) = &slot else { panic!() };
        let before = Rc::as_ptr(rc);
        for i in 0..4 {
            apply("list", "push", &mut slot, vec![Value::Int(i)]).unwrap();
        }
        let Value::List(rc) = &slot else { panic!() };
        assert_eq!(before, Rc::as_ptr(rc), "push cloned a uniquely-owned list");
        assert_eq!(as_ints(&slot), vec![0, 1, 2, 3]);
    }

    #[test]
    fn map_insert_replaces_in_place_and_append_keeps_order() {
        let mut slot = map_of(&[(1, 10), (2, 20)]);
        apply("map", "insert", &mut slot, vec![Value::Int(1), Value::Int(99)]).unwrap();
        apply("map", "insert", &mut slot, vec![Value::Int(3), Value::Int(30)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 99), (2, 20), (3, 30)]);
    }

    #[test]
    fn map_delete_keeps_the_order_of_the_rest() {
        let mut slot = map_of(&[(1, 10), (2, 20), (3, 30)]);
        apply("map", "delete", &mut slot, vec![Value::Int(2)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 10), (3, 30)]);
    }

    #[test]
    fn map_delete_of_a_missing_key_is_a_no_op() {
        let mut slot = map_of(&[(1, 10)]);
        apply("map", "delete", &mut slot, vec![Value::Int(9)]).unwrap();
        assert_eq!(pairs(&slot), vec![(1, 10)]);
    }

    #[test]
    fn string_push_appends_a_suffix_not_a_char() {
        let mut slot = Value::str("ab");
        apply("string", "push", &mut slot, vec![Value::str("cd")]).unwrap();
        assert_eq!(slot, Value::str("abcd"));
    }

    #[test]
    fn bytes_push_truncates_to_a_byte_like_the_native_cast() {
        // `almide_rt_bytes_push` does `b.push(val as u8)`.
        let mut slot = ints(&[]);
        apply("bytes", "push", &mut slot, vec![Value::Int(256)]).unwrap();
        apply("bytes", "push", &mut slot, vec![Value::Int(-1)]).unwrap();
        assert_eq!(as_ints(&slot), vec![0, 255]);
    }

    #[test]
    fn clear_empties_every_container_kind() {
        let mut l = ints(&[1, 2]);
        apply("list", "clear", &mut l, vec![]).unwrap();
        assert_eq!(as_ints(&l), Vec::<i64>::new());
        let mut b = ints(&[1]);
        apply("bytes", "clear", &mut b, vec![]).unwrap();
        assert_eq!(as_ints(&b), Vec::<i64>::new());
        let mut s = Value::str("x");
        apply("string", "clear", &mut s, vec![]).unwrap();
        assert_eq!(s, Value::str(String::new()));
        let mut m = map_of(&[(1, 1)]);
        apply("map", "clear", &mut m, vec![]).unwrap();
        assert!(pairs(&m).is_empty());
    }

    /// A shape the typed IR should never produce must be reported, not guessed
    /// at — `None` becomes an ICE-style abort rather than a silent wrong vote.
    #[test]
    fn a_receiver_of_the_wrong_shape_is_rejected() {
        let mut slot = Value::Int(1);
        assert!(apply("list", "push", &mut slot, vec![Value::Int(2)]).is_none());
        let mut slot = ints(&[1]);
        assert!(apply("list", "sort", &mut slot, vec![]).is_none());
    }

    /// The tier table and the interception predicate are two hand-written
    /// lists; this is the seam where they drift. Every op that `writes_back`
    /// claims MUST be one the guard actually routes here, or the write-back is
    /// dead code that never runs.
    #[test]
    fn every_written_back_op_is_an_intercepted_op() {
        for (m, f) in [
            ("list", "push"),
            ("list", "pop"),
            ("list", "clear"),
            ("map", "insert"),
            ("map", "delete"),
            ("map", "clear"),
            ("string", "push"),
            ("string", "clear"),
            ("bytes", "push"),
            ("bytes", "clear"),
        ] {
            assert!(writes_back(m, f), "{m}.{f} missing from the tier table");
            assert!(
                crate::hofs::is_inplace_mutating_op(m, f),
                "{m}.{f} writes back but is never intercepted — dead code"
            );
        }
    }

    // ── the byte-level writer family (#1021) ────────────────────────────────

    fn write(func: &str, buf: &[i64], rest: Vec<Value>) -> Vec<i64> {
        let mut slot = ints(buf);
        assert_eq!(
            apply("bytes", func, &mut slot, rest),
            Some(Value::Unit),
            "bytes.{func} did not model"
        );
        as_ints(&slot)
    }

    /// THE COMPLETENESS GATE the family's shape demands: the writer set is
    /// defined by a PREFIX over `stdlib/bytes.almd`, so the model must be total
    /// over that file — not over a list someone remembered to extend. A new
    /// `append_u128_le` is covered on arrival by `enc_of`, or this fails.
    #[test]
    fn every_stdlib_bytes_writer_is_modeled() {
        let mut seen = 0usize;
        let mut missing: Vec<String> = Vec::new();
        for line in almide_lang::embedded::SRC_BYTES.lines() {
            let t = line.trim_start();
            let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("pub fn ")) else {
                continue;
            };
            let name = rest.split('(').next().unwrap_or("").trim();
            // The FUNCTIONAL siblings (`bytes.set(b, i, v) -> Bytes`) return a new
            // buffer and are excluded by the underscore, exactly as in
            // `is_inplace_mutating_op` — the two rules must agree.
            if !(name.starts_with("set_")
                || name.starts_with("append_")
                || name.starts_with("write_"))
            {
                continue;
            }
            seen += 1;
            if bytes_write_op(name).is_none() {
                missing.push(name.to_string());
            }
        }
        assert!(seen >= 40, "only {seen} prefix writers found — did the scrape break?");
        assert!(missing.is_empty(), "unmodeled bytes writers: {missing:?}");
        // The three named buffer ops complete the family (they carry no prefix).
        for f in ["fill", "copy_from", "copy_within", "set_at"] {
            assert!(bytes_write_op(f).is_some(), "bytes.{f} unmodeled");
            assert!(writes_back("bytes", f), "bytes.{f} not in the tier table");
            assert!(crate::hofs::is_inplace_mutating_op("bytes", f), "bytes.{f} not intercepted");
        }
        // …and every modeled writer must actually be routed here, or it is dead code.
        for f in ["append_u32_le", "set_f64_be", "write_string_be", "write_uint16", "set_int32"] {
            assert!(writes_back("bytes", f), "bytes.{f} not in the tier table");
            assert!(crate::hofs::is_inplace_mutating_op("bytes", f), "bytes.{f} not intercepted");
        }
    }

    /// `almide_rt_bytes_append_*`: `(val as T).to_{le,be}_bytes()`. The cast
    /// TRUNCATES, so the high bits of a wide value never reach the buffer.
    #[test]
    fn append_encodes_width_and_order() {
        assert_eq!(write("append_u32_le", &[], vec![Value::Int(258)]), [2, 1, 0, 0]);
        assert_eq!(write("append_u32_be", &[], vec![Value::Int(258)]), [0, 0, 1, 2]);
        assert_eq!(write("append_u8", &[9], vec![Value::Int(0x1FF)]), [9, 255]);
        // -2 as i16 = 0xFFFE; the same bytes an unsigned cast of the same i64 gives.
        assert_eq!(write("append_i16_le", &[], vec![Value::Int(-2)]), [254, 255]);
        assert_eq!(write("append_i16_be", &[], vec![Value::Int(-2)]), [255, 254]);
        assert_eq!(write("append_u16_le", &[], vec![Value::Int(-2)]), [254, 255]);
        assert_eq!(write("append_i64_be", &[], vec![Value::Int(1)]), [0, 0, 0, 0, 0, 0, 0, 1]);
        // `val as f32` ROUNDS: 1.5 is exact, so this pins the layout, not the rounding.
        assert_eq!(write("append_f32_le", &[], vec![Value::Float(1.5)]), [0, 0, 0xC0, 0x3F]);
        assert_eq!(write("append_f64_be", &[], vec![Value::Float(1.5)]), [0x3F, 0xF8, 0, 0, 0, 0, 0, 0]);
    }

    /// `almide_rt_bytes_set_*`: splice at `pos`, and a window that does not fit
    /// is a SILENT no-op — never an abort, never a partial write, never a grow.
    #[test]
    fn set_splices_in_place_and_out_of_range_is_a_silent_no_op() {
        assert_eq!(write("set_u32_le", &[0, 0, 0, 0, 9], vec![Value::Int(0), Value::Int(258)]), [
            2, 1, 0, 0, 9
        ]);
        assert_eq!(write("set_u32_be", &[0, 0, 0, 0, 9], vec![Value::Int(1), Value::Int(258)]), [
            0, 0, 0, 1, 2
        ]);
        // pos + 4 > len: untouched, and the buffer does NOT grow.
        assert_eq!(write("set_u32_le", &[1, 2, 3], vec![Value::Int(0), Value::Int(258)]), [1, 2, 3]);
        assert_eq!(write("set_u32_le", &[1, 2, 3, 4], vec![Value::Int(1), Value::Int(258)]), [
            1, 2, 3, 4
        ]);
        // A NEGATIVE pos is enormous as `usize` (native's `pos as usize`), so the
        // guard rejects it — it must never wrap back into range.
        assert_eq!(write("set_u32_le", &[1, 2, 3, 4], vec![Value::Int(-1), Value::Int(258)]), [
            1, 2, 3, 4
        ]);
        // set_at / set_u8: one byte, masked, index-bound.
        assert_eq!(write("set_at", &[1, 2, 3], vec![Value::Int(1), Value::Int(0x1FF)]), [1, 255, 3]);
        assert_eq!(write("set_at", &[1, 2, 3], vec![Value::Int(3), Value::Int(7)]), [1, 2, 3]);
        assert_eq!(write("set_u8", &[1, 2, 3], vec![Value::Int(2), Value::Int(7)]), [1, 2, 7]);
    }

    /// The sized-type surface reads its order from a trailing `Endian` argument
    /// instead of from the name, and lands on the same encoding.
    #[test]
    fn sized_writers_take_their_order_from_the_endian_argument() {
        let le = || Value::Variant {
            ty: None,
            ctor: almide_lang::intern::sym("LittleEndian"),
            payload: crate::value::VariantPayload::Unit,
        };
        let be = || Value::Variant {
            ty: None,
            ctor: almide_lang::intern::sym("BigEndian"),
            payload: crate::value::VariantPayload::Unit,
        };
        assert_eq!(write("write_uint16", &[], vec![Value::Int(258), le()]), [2, 1]);
        assert_eq!(write("write_uint16", &[], vec![Value::Int(258), be()]), [1, 2]);
        assert_eq!(write("write_uint32", &[], vec![Value::Int(258), be()]), [0, 0, 1, 2]);
        assert_eq!(write("write_float32", &[], vec![Value::Float(1.5), le()]), [0, 0, 0xC0, 0x3F]);
        assert_eq!(
            write("set_uint32", &[0, 0, 0, 0], vec![Value::Int(0), Value::Int(258), le()]),
            [2, 1, 0, 0]
        );
        assert_eq!(
            write("set_int32", &[0, 0, 0], vec![Value::Int(0), Value::Int(258), le()]),
            [0, 0, 0]
        );
    }

    /// `write_bool` / `write_string_be` — the two non-scalar cursor writers.
    #[test]
    fn cursor_writers_match_their_native_shape() {
        assert_eq!(write("write_bool", &[], vec![Value::Bool(true)]), [1]);
        assert_eq!(write("write_bool", &[9], vec![Value::Bool(false)]), [9, 0]);
        // A u32 BE BYTE-length prefix, then the UTF-8 bytes — "hi" is 2 bytes.
        assert_eq!(write("write_string_be", &[], vec![Value::str("hi")]), [
            0, 0, 0, 2, 104, 105
        ]);
        // Multibyte counts BYTES, not chars: "é" is 2 bytes (C3 A9).
        assert_eq!(write("write_string_be", &[], vec![Value::str("é")]), [0, 0, 0, 2, 0xC3, 0xA9]);
    }

    /// `fill` overwrites every existing byte and NEVER changes the length —
    /// so an empty buffer stays empty.
    #[test]
    fn fill_overwrites_without_growing() {
        assert_eq!(write("fill", &[1, 2, 3], vec![Value::Int(0x1FF)]), [255, 255, 255]);
        assert!(write("fill", &[], vec![Value::Int(7)]).is_empty());
    }

    /// `almide_rt_bytes_copy_from`: an offset at-or-past its buffer's end is a
    /// no-op, otherwise `len` clamps to the shorter remaining tail.
    #[test]
    fn copy_from_clamps_to_the_shorter_tail() {
        let src = ints(&[7, 8, 9]);
        assert_eq!(
            write("copy_from", &[0, 0, 0, 0], vec![
                src.clone(),
                Value::Int(1),
                Value::Int(0),
                Value::Int(99)
            ]),
            [0, 7, 8, 9]
        );
        // len clamps to dst's remaining 2 bytes.
        assert_eq!(
            write("copy_from", &[0, 0, 0], vec![
                src.clone(),
                Value::Int(1),
                Value::Int(0),
                Value::Int(3)
            ]),
            [0, 7, 8]
        );
        // dst_off == dst.len(): nothing copied.
        assert_eq!(
            write("copy_from", &[0, 0], vec![src, Value::Int(2), Value::Int(0), Value::Int(1)]),
            [0, 0]
        );
    }

    /// `almide_rt_bytes_copy_within`: `src_end` clamps to the length, and the
    /// move happens only when the source range is non-empty AND the destination
    /// window fits.
    #[test]
    fn copy_within_moves_only_when_the_window_fits() {
        assert_eq!(
            write("copy_within", &[1, 2, 3, 0, 0], vec![
                Value::Int(0),
                Value::Int(3),
                Value::Int(2)
            ]),
            [1, 2, 1, 2, 3]
        );
        // src_end past the end clamps to len.
        assert_eq!(
            write("copy_within", &[1, 2, 3], vec![Value::Int(0), Value::Int(99), Value::Int(0)]),
            [1, 2, 3]
        );
        // The destination window does not fit — untouched.
        assert_eq!(
            write("copy_within", &[1, 2, 3], vec![Value::Int(0), Value::Int(3), Value::Int(1)]),
            [1, 2, 3]
        );
        // An empty source range is a no-op.
        assert_eq!(
            write("copy_within", &[1, 2, 3], vec![Value::Int(2), Value::Int(2), Value::Int(0)]),
            [1, 2, 3]
        );
    }

    /// A name the family does not define must stay `None` — an abstain, never a
    /// guessed layout. This is what keeps `writes_back` honest for `bytes`.
    #[test]
    fn an_unrecognized_writer_name_is_not_modeled() {
        for f in ["set_", "append_", "append_u7_le", "append_u32", "set_u16", "append_f16_le", "set_q32_le", "write_"] {
            assert!(bytes_write_op(f).is_none(), "bytes.{f} should not be modeled");
            assert!(!writes_back("bytes", f), "bytes.{f} should not claim a write-back");
        }
    }
}
