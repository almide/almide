//! Module functions a package's own `native/*.rs` calls back into (#3424).
//!
//! The native code spells the linked name of a module function
//! (`crate::almide_rt_tf_0v0_entry`). Two things follow for this render:
//! such a function is a ROOT even when no Almide code reaches it, and it needs
//! a crate-root item under exactly that name with the host's signature — the
//! render's own definitions are private and prefixed.
//!
//! Only an all-scalar, pure signature has one host spelling (`i64`, `f64`,
//! `bool`, the narrow integers, `()`); any other callback signature is
//! refused here, so `run`/`build` keep their standard-codegen route for it.
use almide_ir::{IrFunction, IrProgram};
use almide_lang::types::Ty;
use crate::lower::LowerError;
use crate::render_native::{wall, NativeSigKind, NativeSigs};
use std::cell::RefCell;
use std::fmt::Write as _;

thread_local! {
    static NATIVE_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The text of every `native/*.rs` the build compiles, for the render made
/// while this guard lives. A nested scope restores the preceding one.
pub struct Scope(String);
impl Scope {
    pub fn enter(native_text: String) -> Self {
        Self(NATIVE_TEXT.with(|t| t.replace(native_text)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        NATIVE_TEXT.with(|t| t.replace(std::mem::take(&mut self.0)));
    }
}

fn is_ident(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }

/// Whether `text` spells `name` as a whole identifier.
fn names(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(is_ident)
            && !text[at + name.len()..].chars().next().is_some_and(is_ident)
    })
}

/// Whether the native code names this linked module function.
pub(crate) fn is_root(name: &str) -> bool {
    name.starts_with("almide_rt_") && NATIVE_TEXT.with(|t| {
        let t = t.borrow();
        !t.is_empty() && names(&t, name)
    })
}

fn host_scalar(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int | Ty::Int64 => "i64", Ty::Int8 => "i8", Ty::Int16 => "i16", Ty::Int32 => "i32",
        Ty::UInt8 => "u8", Ty::UInt16 => "u16", Ty::UInt32 => "u32", Ty::UInt64 => "u64",
        Ty::Float | Ty::Float64 => "f64", Ty::Bool => "bool",
        _ => return None,
    })
}

/// `e`, a host value of `ty`, as the rendered function's parameter of `kind`.
fn to_native(e: &str, ty: &Ty, kind: NativeSigKind) -> Option<String> {
    Some(match (ty, kind) {
        (Ty::Bool, NativeSigKind::I64) => format!("({e} as i64)"),
        (Ty::Float | Ty::Float64, NativeSigKind::F64) => e.to_string(),
        (Ty::Float | Ty::Float64, NativeSigKind::I64) => format!("({e}.to_bits() as i64)"),
        (Ty::Int | Ty::Int64, NativeSigKind::I64) => e.to_string(),
        (Ty::UInt64, NativeSigKind::I64) => format!("({e} as i64)"),
        (t, k) if k.narrow_rust().is_some() && host_scalar(t) == k.narrow_rust() => e.to_string(),
        _ => return None,
    })
}

/// `e`, the rendered function's result of `kind`, as the host value of `ty`.
fn to_host(e: &str, ty: &Ty, kind: NativeSigKind) -> Option<String> {
    Some(match (ty, kind) {
        (Ty::Bool, NativeSigKind::I64) => format!("({e} != 0)"),
        (Ty::Float | Ty::Float64, NativeSigKind::F64) => e.to_string(),
        (Ty::Float | Ty::Float64, NativeSigKind::I64) => format!("f64::from_bits({e} as u64)"),
        (Ty::Int | Ty::Int64, NativeSigKind::I64) => e.to_string(),
        (Ty::UInt64, NativeSigKind::I64) => format!("({e} as u64)"),
        (t, k) if k.narrow_rust().is_some() && host_scalar(t) == k.narrow_rust() => e.to_string(),
        _ => return None,
    })
}

fn adapter(f: &IrFunction, sigs: &NativeSigs) -> Option<String> {
    if f.is_effect || f.params.iter().any(|p| p.is_mut) { return None; }
    let (kinds, result) = sigs.get(f.name.as_str())?;
    if kinds.len() != f.params.len() { return None; }
    let mut params = Vec::new();
    let mut args = Vec::new();
    for (j, (p, kind)) in f.params.iter().zip(kinds).enumerate() {
        params.push(format!("a{j}: {}", host_scalar(&p.ty)?));
        args.push(to_native(&format!("a{j}"), &p.ty, *kind)?);
    }
    let call = format!("{}({})", crate::render_native::mangle(f.name.as_str()), args.join(", "));
    let (ret, body) = match (&f.ret_ty, result) {
        (Ty::Unit, None) => (String::new(), format!("{call};")),
        (ty, Some(kind)) => (format!(" -> {}", host_scalar(ty)?), to_host(&call, ty, *kind)?),
        _ => return None,
    };
    Some(format!("pub fn {}({}){ret} {{\n    {body}\n}}\n", f.name, params.join(", ")))
}

/// The crate-root items the native code links, one per callback root.
pub(crate) fn render(ir: &IrProgram, sigs: &NativeSigs) -> Result<String, LowerError> {
    let mut out = String::new();
    for f in ir.functions.iter().filter(|f| is_root(f.name.as_str())) {
        let item = adapter(f, sigs).ok_or_else(|| wall(format!(
            "native: fn `{}` is called from native code with a signature that has no single host spelling",
            f.name
        )))?;
        write!(out, "{item}").unwrap();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{is_root, names, Scope};

    #[test]
    fn a_callback_is_named_as_a_whole_identifier() {
        let text = "pub fn f(x: i64) -> i64 { crate::almide_rt_tf_0v0_entry(x) }";
        assert!(names(text, "almide_rt_tf_0v0_entry"));
        assert!(!names(text, "almide_rt_tf_0v0_ent"));
        assert!(!names(text, "rt_tf_0v0_entry"));
    }

    #[test]
    fn the_scope_bounds_the_roots_and_restores_the_outer_one() {
        assert!(!is_root("almide_rt_tf_0v0_entry"));
        let _outer = Scope::enter("crate::almide_rt_tf_0v0_entry(1)".into());
        {
            let _inner = Scope::enter(String::new());
            assert!(!is_root("almide_rt_tf_0v0_entry"));
        }
        assert!(is_root("almide_rt_tf_0v0_entry"));
        assert!(!is_root("main"));
    }
}
