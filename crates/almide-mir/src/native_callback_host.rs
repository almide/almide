//! The crate-root adapter of a linked callback whose signature is not
//! all-scalar (#4640).
//!
//! A package's `native/*.rs` is written against the Rust signature the
//! standard codegen emits for a module function: which parameters it borrows
//! (`&str`, `&[i64]`) and which it takes, and the shape an effect fn answers
//! in. That signature is not re-derived here. It is READ from the standard
//! codegen's own output for the same program (`host`), and the adapter is
//! declared with exactly that text. The rendered function's signature is read
//! from this render's output (`rendered`) the same way.
//!
//! Where the two spell a type identically the value passes through untouched
//! — a borrow is re-lent, an owned value is moved. Where they differ, the
//! value is copied across by the foreign value adapter
//! (`native_foreign::convert_value`), which only ever reads through a shared
//! borrow: a parameter the host lent is never consumed, and a result the
//! rendered function returned is owned by the adapter until it drops it. The
//! adapter holds no `unsafe` and sits outside the certified functions, as the
//! scalar adapters do.
//!
//! A record or a variant crosses under the definition the standard codegen
//! declares for it, carried beside the adapter (`native_callback_types`). A
//! host type outside the spellings `host_type` gives (a function value, a
//! `mut` parameter, a generic or structural record) is refused by name.
use almide_ir::{IrFunction, IrParam};
use almide_lang::types::Ty;
use crate::lower::LowerError;
use crate::native_foreign::{convert_value, host_type};
use crate::render_native::{wall, NTy, NativeSigKind};
use crate::render_native_closure::owned_nty;
use std::fmt::Write as _;

/// The parameters (`name`, `type`) and return type (empty for none) of a
/// rendered `fn` item.
struct Sig {
    params: Vec<(String, String)>,
    ret: String,
}

/// One `name: type` parameter; a `mut` binding mode is not part of the name.
fn param(text: &str) -> Option<(String, String)> {
    let (name, ty) = text.split_once(':')?;
    let name = name.trim();
    Some((name.strip_prefix("mut ").unwrap_or(name).trim().to_string(), ty.trim().to_string()))
}

/// Where the parameter list opened just before `rest` closes, and the
/// top-level commas inside it.
fn list_end(rest: &str) -> Option<(usize, Vec<usize>)> {
    let mut depth = 0usize;
    let mut commas = Vec::new();
    let mut prev = ' ';
    for (i, c) in rest.char_indices() {
        match c {
            '(' | '[' | '<' => depth += 1,
            '>' if prev == '-' => {}
            ')' if depth == 0 => return Some((i, commas)),
            ')' | ']' | '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas.push(i),
            _ => {}
        }
        prev = c;
    }
    None
}

/// The signature of the non-generic item `fn <name>(…)` in `text`.
fn sig_of(text: &str, name: &str) -> Option<Sig> {
    let head = format!("fn {name}(");
    let at = text.match_indices(&head)
        .find(|(at, _)| !text[..*at].chars().next_back().is_some_and(crate::native_callbacks::is_ident))?.0;
    let rest = &text[at + head.len()..];
    let (end, commas) = list_end(rest)?;
    let mut params = Vec::new();
    let mut from = 0;
    for stop in commas.into_iter().chain(std::iter::once(end)) {
        let piece = rest[from..stop].trim();
        if !piece.is_empty() { params.push(param(piece)?); }
        from = stop + 1;
    }
    let tail = rest[end + 1..].split('{').next()?.trim();
    let ret = tail.strip_prefix("->").unwrap_or(tail).trim();
    (ret.is_empty() || tail.starts_with("->")).then(|| Sig { params, ret: ret.to_string() })
}

/// A type spelling with its layout dropped: `(i64, AlmideString,)` and
/// `(i64, AlmideString)` are one type.
pub(crate) fn canon(ty: &str) -> String {
    ty.replace(' ', "").replace(",)", ")")
}

/// Whether `theirs` is a borrow of the owned host type `owned`: `&T`, or the
/// slice forms `&str` and `&[E]`.
fn lends(theirs: &str, owned: &str) -> bool {
    let (theirs, owned) = (canon(theirs), canon(owned));
    let Some(lent) = theirs.strip_prefix('&') else { return false };
    let elem = owned.strip_prefix("Vec<").and_then(|o| o.strip_suffix('>'));
    lent == owned
        || (owned == "AlmideString" && lent == "str")
        || elem.is_some_and(|e| lent.strip_prefix('[').and_then(|l| l.strip_suffix(']')) == Some(e))
}

/// One parameter crossing from the host signature to the rendered one.
struct Crossing<'a> {
    name: &'a str,
    theirs: &'a str,
    own: &'a str,
}

impl Crossing<'_> {
    /// The argument the rendered function takes for this parameter; a copy
    /// across representations is bound in `lets` first.
    fn argument(&self, p: &IrParam, kind: NativeSigKind, lets: &mut String) -> Result<String, LowerError> {
        let Crossing { name, theirs, own } = *self;
        if theirs == own { return Ok(name.to_string()); }
        if p.is_mut { return Err(wall(format!("native: callback parameter `{name}` is `mut`: no host adapter"))); }
        let owned = host_type(&p.ty).map_err(|_| wall(format!(
            "native: callback parameter `{name}: {theirs}` ({}) has no host adapter", p.ty.display()
        )))?;
        let source = if canon(theirs) == canon(&owned) { format!("&{name}") }
            else if lends(theirs, &owned) { name.to_string() }
            else { return Err(wall(format!("native: callback parameter `{name}: {theirs}` is not a host spelling of {}", p.ty.display()))); };
        let k = owned_nty(kind);
        let value = if theirs == "&str" { format!("AlmideString::from({name})") }
            else { convert_value(&source, &p.ty, k, false, 0, true)? };
        let local = format!("__almide_{name}");
        writeln!(lets, "    let {local}: {} = {value};", crate::render_native_rec::owned_ty(k)).expect("writing to a String cannot fail");
        Ok(if own.starts_with('&') { format!("&{local}") } else { local })
    }
}

fn is_carrier(k: NTy) -> bool {
    matches!(k, NTy::Res | NTy::ResStr)
        || matches!(k, NTy::Rec(i) if crate::render_native_rec::result_payloads(i).is_some())
}

/// The adapter's tail: `call`'s result as the host return type `theirs`.
fn returned(f: &IrFunction, result: Option<NativeSigKind>, theirs: &str, own: &str, call: &str) -> Result<String, LowerError> {
    if theirs == own { return Ok(call.to_string()); }
    // An effect fn answers `Result<T, String>` on the host; the rendered one
    // carries a Result only when it can fail.
    let lifted = f.is_effect && f.ret_ty.result_ok_ty().is_none();
    let host_ty = if lifted { Ty::result(f.ret_ty.clone(), Ty::String) } else { f.ret_ty.clone() };
    let host = host_type(&host_ty).map_err(|_| wall(format!(
        "native: callback return `{theirs}` ({}) has no host adapter", host_ty.display()
    )))?;
    if canon(&host) != canon(theirs) {
        return Err(wall(format!("native: callback return `{theirs}` is not a host spelling of {}", host_ty.display())));
    }
    let Some(kind) = result else {
        return if lifted && f.ret_ty == Ty::Unit { Ok(format!("{call};\n    Ok(())")) }
            else { Err(wall(format!("native: callback `{}` renders no value for its `{theirs}`", f.name))) };
    };
    let k = owned_nty(kind);
    let value = if lifted && !is_carrier(k) {
        // The same owned type on both sides moves into the `Ok`.
        if host_type(&f.ret_ty)? == own { return Ok(format!("Ok({call})")); }
        format!("Ok({})", convert_value("&__almide_ret", &f.ret_ty, k, true, 0, true)?)
    } else {
        convert_value("&__almide_ret", &host_ty, k, true, 0, true)?
    };
    Ok(format!("let __almide_ret = {call};\n    {value}"))
}

/// The crate-root item the native code links for `f`, declared with the
/// signature `host` holds for that name, over the function `rendered` holds.
pub(crate) fn adapter(
    f: &IrFunction, (kinds, result): (&[NativeSigKind], Option<NativeSigKind>), rendered: &str, host: &str,
) -> Result<String, LowerError> {
    let refuse = |what: &str| wall(format!("native: fn `{}` is called from native code, and {what}", f.name));
    let mangled = crate::render_native::mangle(f.name.as_str());
    let own = sig_of(rendered, &mangled).ok_or_else(|| refuse("this render holds no signature for it"))?;
    let theirs = sig_of(host, f.name.as_str()).ok_or_else(|| refuse("the standard codegen emits no plain signature for it"))?;
    let arity = f.params.len();
    if [own.params.len(), theirs.params.len(), kinds.len()] != [arity; 3] {
        return Err(refuse("its signatures disagree on the parameter count"));
    }
    let mut lets = String::new();
    let mut params = Vec::new();
    let mut args = Vec::new();
    for (j, (p, kind)) in f.params.iter().zip(kinds).enumerate() {
        let (name, ty) = &theirs.params[j];
        params.push(format!("{name}: {ty}"));
        args.push(Crossing { name, theirs: ty, own: &own.params[j].1 }.argument(p, *kind, &mut lets)?);
    }
    let call = format!("{mangled}({})", args.join(", "));
    let body = returned(f, result, &theirs.ret, &own.ret, &call)?;
    let ret = if theirs.ret.is_empty() { String::new() } else { format!(" -> {}", theirs.ret) };
    Ok(format!("pub fn {}({}){ret} {{\n{lets}    {body}\n}}\n", f.name, params.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::{lends, sig_of};

    #[test]
    fn a_signature_is_read_from_the_item_with_that_whole_name() {
        let text = "pub fn f_more(a: i64) {\n}\npub fn f(mut xs: &[AlmideString], n: Result<i64, AlmideString>, g: &dyn Fn(i64) -> (i64, i64)) -> Option<(i64, AlmideString)> {\n";
        let sig = sig_of(text, "f").expect("the item is there");
        let params: Vec<_> = sig.params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        assert_eq!(params, ["xs: &[AlmideString]", "n: Result<i64, AlmideString>", "g: &dyn Fn(i64) -> (i64, i64)"]);
        assert_eq!(sig.ret, "Option<(i64, AlmideString)>");
        assert_eq!(sig_of(text, "f_more").expect("the item is there").ret, "");
        assert!(sig_of(text, "f_mor").is_none());
        assert!(sig_of("pub fn g<T: Clone>(a: T) -> T {", "g").is_none(), "a generic item has no single signature");
    }

    #[test]
    fn a_borrow_is_of_the_owned_host_type_or_its_slice() {
        assert!(lends("&str", "AlmideString"));
        assert!(lends("&AlmideString", "AlmideString"));
        assert!(lends("&[i64]", "Vec<i64>"));
        assert!(lends("&Option<AlmideString>", "Option<AlmideString>"));
        assert!(!lends("&[AlmideString]", "Vec<i64>"));
        assert!(!lends("Vec<i64>", "Vec<i64>"));
        assert!(!lends("&Point", "AlmideString"));
        assert!(lends("&(i64, AlmideString)", "(i64, AlmideString,)"));
    }
}
