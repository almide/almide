//! Module functions a package's own `native/*.rs` calls back into (#3424).
//!
//! The native code spells the linked name of a module function
//! (`crate::almide_rt_tf_0v0_entry`). Two things follow for this render:
//! such a function is a ROOT even when no Almide code reaches it, and it needs
//! a crate-root item under exactly that name with the host's signature — the
//! render's own definitions are private and prefixed.
//!
//! An all-scalar, pure signature has one host spelling (`i64`, `f64`, `bool`,
//! the narrow integers, `()`) and is adapted here. Any other signature takes
//! the spelling the standard codegen emits for that name — the one the native
//! code was written against — from the build's host source
//! (`native_callback_host`, #4640); what that cannot adapt is refused, so
//! `run`/`build` keep their standard-codegen route for it.
use almide_ir::{IrFunction, IrProgram};
use almide_lang::types::Ty;
use crate::lower::LowerError;
use crate::render_native::{wall, NativeSigKind, NativeSigs};
use std::cell::RefCell;
use std::rc::Rc;
use std::fmt::Write as _;

thread_local! {
    static NATIVE_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
    static HOST: RefCell<Host> = const { RefCell::new(Host::Absent) };
    static HOST_TYPES: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The standard codegen's Rust for the program being rendered: made at most
/// once, and only when a callback's signature is not all-scalar.
#[derive(Default)]
enum Host {
    #[default]
    Absent,
    Pending(Box<dyn FnOnce() -> String>),
    Ready(Rc<str>),
}

/// The text of every `native/*.rs` the build compiles, for the render made
/// while this guard lives. A nested scope restores the preceding one.
pub struct Scope(String, Host);
impl Scope {
    pub fn enter(native_text: String) -> Self {
        Self(NATIVE_TEXT.with(|t| t.replace(native_text)), HOST.with(|h| h.take()))
    }

    /// `make` renders the program through the standard codegen; the adapter
    /// of a non-scalar callback reads that fn's signature from it.
    pub fn with_host_source(self, make: impl FnOnce() -> String + 'static) -> Self {
        HOST.with(|h| h.replace(Host::Pending(Box::new(make))));
        self
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        NATIVE_TEXT.with(|t| t.replace(std::mem::take(&mut self.0)));
        HOST.with(|h| h.replace(std::mem::take(&mut self.1)));
    }
}

/// The host types the non-scalar callbacks of the program being rendered
/// name, for the render made while this guard lives: a host type this
/// render's own bodies do not use (`Bytes` is a `Vec<u8>` here, a shared
/// handle on the host) still needs its definition beside the adapter.
pub(crate) struct HostTypes(String);
impl HostTypes {
    pub(crate) fn enter(ir: &IrProgram) -> Self {
        let mut types = String::new();
        for f in ir.functions.iter().filter(|f| is_root(f.name.as_str())) {
            let named = f.params.iter().map(|p| &p.ty).chain(std::iter::once(&f.ret_ty));
            for ty in named.filter(|ty| host_scalar(ty).is_none()) {
                if let Ok(host) = crate::native_foreign::host_type(ty) {
                    types.push_str(&host);
                    types.push('\n');
                }
            }
        }
        Self(HOST_TYPES.with(|t| t.replace(types)))
    }
}
impl Drop for HostTypes {
    fn drop(&mut self) {
        HOST_TYPES.with(|t| t.replace(std::mem::take(&mut self.0)));
    }
}

/// The runtime items the callbacks' host types reach.
pub(crate) fn runtime_items() -> Result<Vec<&'static str>, LowerError> {
    HOST_TYPES.with(|t| crate::native_floor::runtime_items_named_by(&t.borrow())).map_err(wall)
}

/// The host source of the scope, rendering it on first use.
fn host_source() -> Option<Rc<str>> {
    let pending = HOST.with(|h| match h.take() {
        Host::Pending(make) => Some(make),
        other => { h.replace(other); None }
    });
    if let Some(make) = pending {
        let text: Rc<str> = make().into();
        HOST.with(|h| h.replace(Host::Ready(text)));
    }
    HOST.with(|h| match &*h.borrow() { Host::Ready(text) => Some(text.clone()), _ => None })
}

pub(crate) fn is_ident(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }

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
/// `rendered` is the program's own items, whose signatures the adapters call.
pub(crate) fn render(ir: &IrProgram, sigs: &NativeSigs, rendered: &str) -> Result<String, LowerError> {
    let mut out = String::new();
    for f in ir.functions.iter().filter(|f| is_root(f.name.as_str())) {
        let item = match adapter(f, sigs) {
            Some(item) => item,
            None => host_adapter(f, sigs, rendered)?,
        };
        write!(out, "{item}").expect("writing to a String cannot fail");
    }
    Ok(out)
}

/// The adapter of a callback whose signature is not all-scalar.
fn host_adapter(f: &IrFunction, sigs: &NativeSigs, rendered: &str) -> Result<String, LowerError> {
    let (Some(host), Some((kinds, result))) = (host_source(), sigs.get(f.name.as_str())) else {
        return Err(wall(format!(
            "native: fn `{}` is called from native code with a signature that has no single host spelling",
            f.name
        )));
    };
    crate::native_callback_host::adapter(f, (kinds, *result), rendered, &host)
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
