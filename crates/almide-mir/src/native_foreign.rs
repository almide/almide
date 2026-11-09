//! Declared native foreign calls. The MIR import retains the selected target
//! and effect class; ownership is the existing borrowed-import contract.
//! Host code and the author's purity declaration remain trusted.
use almide_ir::{IrFunction, IrProgram};
use almide_lang::{ast::ExternAttr, types::{Ty, TypeConstructorId as C, extern_abi::{NativeExtern, native_extern_kind, c_param_abi, c_return_abi, CConv}}};
use crate::{Capability, ImportTarget, MirFunction, Op, ValueId};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NativeSigs, NTy, owned_nty};
use std::{cell::RefCell, collections::BTreeMap, fmt::Write as _};

thread_local! {
    static DECLS: RefCell<BTreeMap<String, Declaration>> = RefCell::new(BTreeMap::new());
    static ORIGINALS: RefCell<BTreeMap<String, IrFunction>> = RefCell::new(BTreeMap::new());
}

#[derive(Clone)]
struct Declaration { host: IrFunction, lowered_return: Ty }

/// Original foreign signatures must survive the ordinary value write-back
/// transformation. Scope them around preparation as well as MIR rendering.
pub(crate) struct OriginalScope(BTreeMap<String, IrFunction>);
impl OriginalScope {
    pub(crate) fn enter() -> Self {
        Self(ORIGINALS.with(|d| std::mem::take(&mut *d.borrow_mut())))
    }
}
impl Drop for OriginalScope {
    fn drop(&mut self) { ORIGINALS.with(|d| *d.borrow_mut() = std::mem::take(&mut self.0)); }
}

pub(crate) fn prepare_mutable(ir: &mut IrProgram) {
    for f in &mut ir.functions {
        if selected(f).is_none() || !f.params.iter().any(|p| p.is_mut) { continue; }
        ORIGINALS.with(|d| { d.borrow_mut().entry(f.name.to_string()).or_insert_with(|| f.clone()); });
        // A hole or portable fallback cannot describe what the selected host
        // writes. Every declared mut parameter participates in write-back.
        f.mutated_params = f.params.iter().enumerate().filter_map(|(i, p)| p.is_mut.then_some(i)).collect();
    }
}

pub(crate) struct Scope(BTreeMap<String, Declaration>);
impl Scope {
    pub(crate) fn enter(ir: &IrProgram) -> Self {
        let next = ir.functions.iter().filter(|f| selected(f).is_some())
            .map(|f| {
                let host = ORIGINALS.with(|d| d.borrow().get(f.name.as_str()).cloned()).unwrap_or_else(|| f.clone());
                (f.name.to_string(), Declaration { host, lowered_return: f.ret_ty.clone() })
            }).collect();
        Self(DECLS.with(|d| std::mem::replace(&mut *d.borrow_mut(), next)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) { DECLS.with(|d| *d.borrow_mut() = std::mem::take(&mut self.0)); }
}

pub(crate) fn selected(f: &IrFunction) -> Option<(NativeExtern, &ExternAttr)> {
    f.extern_attrs.iter().find_map(|a| native_extern_kind(a.target.as_str()).map(|k| (k, a)))
}

pub(crate) fn import_caps(target: ImportTarget) -> Vec<Capability> {
    match target {
        ImportTarget::Wasm => Vec::new(), // The structural byte projector owns this leg.
        ImportTarget::Native { is_effect, .. } => {
            let mut caps = vec![Capability::Foreign];
            if is_effect { caps.push(Capability::ForeignEffect); }
            caps
        }
    }
}

pub(crate) fn extend_bound(caps: &mut Vec<Capability>, effect: bool) {
    if DECLS.with(|d| !d.borrow().is_empty()) {
        caps.push(Capability::Foreign);
        if effect { caps.push(Capability::ForeignEffect); }
    }
}

pub(crate) fn check_decl(f: &IrFunction, kind: NativeExtern) -> Result<(), LowerError> {
    let original = ORIGINALS.with(|d| d.borrow().get(f.name.as_str()).cloned());
    let f = original.as_ref().unwrap_or(f);
    if f.params.iter().any(|p| p.is_mut) {
        if original.is_none() || f.is_effect || f.ret_ty.result_ok_ty().is_some() {
            return Err(wall("native: fallible foreign write-back needs both return channels"));
        }
    }
    for (ty, param) in f.params.iter().map(|p| (&p.ty, true)).chain(std::iter::once((&f.ret_ty, false))) {
        if kind == NativeExtern::C {
            let abi = if param { c_param_abi(ty) } else { c_return_abi(ty) }
                .ok_or_else(|| wall(format!("native: foreign C type {ty:?} has no declared ABI")))?;
            if abi.conv == CConv::Region {
                return Err(wall("native: foreign Region needs its loan/release adapter"));
            }
        }
        if param && kind == NativeExtern::Rust {
            if let Ty::Fn { params, ret, .. } = ty {
                for p in params { check_value_type(p)?; }
                check_value_type(ret)?;
                continue;
            }
        }
        check_value_type(ty)?;
    }
    Ok(())
}

fn check_value_type(ty: &Ty) -> Result<(), LowerError> {
    if scalar_host_type(ty).is_some() || matches!(ty, Ty::Unit | Ty::String | Ty::Bytes) || crate::render_native_rec::is_matrix_ty(ty) { return Ok(()); }
    match ty {
        Ty::Applied(C::List | C::Set | C::Map | C::Option | C::Result, args) => args.iter().try_for_each(check_value_type),
        Ty::Tuple(args) => args.iter().try_for_each(check_value_type),
        _ => Err(wall(format!("native: foreign type {ty:?} needs its declared host representation"))),
    }
}

fn scalar_host_type(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int | Ty::Int64 => "i64", Ty::Int8 => "i8", Ty::Int16 => "i16", Ty::Int32 => "i32",
        Ty::UInt8 => "u8", Ty::UInt16 => "u16", Ty::UInt32 => "u32", Ty::UInt64 => "u64",
        Ty::Float | Ty::Float64 => "f64", Ty::Float32 => "f32", Ty::Bool => "bool",
        _ => return None,
    })
}

pub(crate) fn render(
    f: &MirFunction, op: &Op, sigs: &NativeSigs, tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String, indent: usize,
) -> Result<bool, LowerError> {
    let Op::CallImport { target: ImportTarget::Native { kind, is_effect }, module, name, args, dst, .. } = op else {
        return Ok(false);
    };
    let declaration = DECLS.with(|d| d.borrow().get(&f.name).cloned())
        .ok_or_else(|| wall("native: foreign call has no checked declaration"))?;
    let decl = &declaration.host;
    let (selected_kind, attr) = selected(decl).ok_or_else(|| wall("native: missing foreign target"))?;
    if selected_kind != *kind || attr.module.as_str() != module || attr.function.as_str() != name
        || decl.is_effect != *is_effect || args.len() != decl.params.len() {
        return Err(wall("native: foreign call disagrees with its checked declaration"));
    }
    check_decl(&decl, *kind)?;
    let mut prelude = String::new();
    let mut actuals = Vec::new();
    let mut c_params = Vec::new();
    for (j, (a, p)) in args.iter().zip(&decl.params).enumerate() {
        let v = match a {
            crate::CallArg::Scalar(v) | crate::CallArg::Handle(v) => *v,
            _ => return Err(wall("native: foreign argument must be a checked parameter value")),
        };
        let k = tys.get(&v).copied().map(owned_nty).ok_or_else(|| wall("native: untyped foreign argument"))?;
        if *kind == NativeExtern::Rust {
            if let Ty::Fn { params, ret, is_effect } = &p.ty {
                if p.is_mut { return Err(wall("native: mutable foreign callback slot needs an owned callback adapter")); }
                let callback = callback(&var(v), params, ret, *is_effect, k)?;
                actuals.push(format!("&({callback})"));
                continue;
            }
        }
        let value = convert(&format!("&{}", var(v)), &p.ty, k, true, 0)?;
        if *kind == NativeExtern::Rust {
            let borrowed = matches!(&p.ty, Ty::String | Ty::Bytes | Ty::Applied(C::List | C::Set | C::Map, _)) || crate::render_native_rec::is_matrix_ty(&p.ty);
            if p.is_mut {
                writeln!(prelude, "let mut __foreign_arg{j} = {value};").unwrap();
                actuals.push(format!("&mut __foreign_arg{j}"));
            } else {
                actuals.push(if borrowed { format!("&({value})") } else { value });
            }
        } else {
            let abi = c_param_abi(&p.ty).ok_or_else(|| wall("native: missing C parameter ABI"))?;
            c_params.push(format!("a{j}: {}", abi.c_ty));
            actuals.push(match abi.conv {
                CConv::CString => {
                    writeln!(prelude, "let __foreign_arg{j} = std::ffi::CString::new(({}).split('\\0').next().unwrap_or_default()).unwrap_or_default();", var(v)).unwrap();
                    format!("__foreign_arg{j}.as_ptr() as *const u8")
                }
                CConv::BoolAsI32 => format!("({value}) as i32"),
                CConv::Same => value,
                CConv::Region => return Err(wall("native: missing foreign Region adapter")),
            });
        }
    }
    let call = if *kind == NativeExtern::Rust {
        format!("{module}::{name}({})", actuals.join(", "))
    } else {
        let abi = c_return_abi(&decl.ret_ty).ok_or_else(|| wall("native: missing C return ABI"))?;
        let link = if matches!(module.as_str(), "c" | "m") {
            format!("#[cfg_attr(not(windows), link(name = {module:?}))]")
        } else { format!("#[link(name = {module:?})]") };
        writeln!(prelude, "{link} unsafe extern \"C\" {{ #[link_name = {name:?}] fn __foreign_call({}) -> {}; }}", c_params.join(", "), abi.c_ty).unwrap();
        let call = format!("unsafe {{ __foreign_call({}) }}", actuals.join(", "));
        if abi.conv == CConv::BoolAsI32 { format!("({call}) != 0") } else { call }
    };
    let pad = "    ".repeat(indent);
    for line in prelude.lines() { writeln!(out, "{pad}{line}").unwrap(); }
    if let Some(dst) = dst {
        let k = sigs.get(&f.name).and_then(|(_, r)| *r).map(crate::render_native_closure::owned_nty)
            .ok_or_else(|| wall("native: missing foreign return kind"))?;
        writeln!(out, "{pad}let __foreign_result = {call};").unwrap();
        let mut returned: Vec<String> = decl.params.iter().enumerate()
            .filter_map(|(i, p)| p.is_mut.then(|| format!("__foreign_arg{i}"))).collect();
        let (host_result, result_ty) = if returned.is_empty() {
            ("&__foreign_result".to_string(), &decl.ret_ty)
        } else {
            if decl.ret_ty != Ty::Unit { returned.insert(0, "__foreign_result".into()); }
            let value = if returned.len() == 1 { returned.remove(0) }
                else { format!("({},)", returned.join(", ")) };
            writeln!(out, "{pad}let __foreign_writeback = {value};").unwrap();
            ("&__foreign_writeback".to_string(), &declaration.lowered_return)
        };
        let result = convert(&host_result, result_ty, k, false, 0)?;
        writeln!(out, "{pad}let mut {}: {} = {result};", var(*dst), crate::render_native_rec::owned_ty(k)).unwrap();
        tys.insert(*dst, k);
    } else {
        if decl.ret_ty != Ty::Unit { return Err(wall("native: foreign return value was discarded")); }
        writeln!(out, "{pad}{call};").unwrap();
    }
    Ok(true)
}

include!("native_foreign_values.rs");
include!("native_foreign_callback.rs");

/// A foreign callback can mention a runtime value handle that the native
/// body's own representation does not use (Bytes is a Vec there). Carry
/// the actual host signature's type dependencies through the shared index.
pub(crate) fn runtime_items() -> Result<Vec<&'static str>, LowerError> {
    let types = DECLS.with(|d| {
        let mut types = String::new();
        for decl in d.borrow().values() {
            for p in &decl.host.params {
                if let Ty::Fn { params, ret, .. } = &p.ty {
                    for t in params.iter().chain(std::iter::once(ret.as_ref())) {
                        types.push_str(&host_type(t)?);
                        types.push('\n');
                    }
                }
            }
        }
        Ok::<_, LowerError>(types)
    })?;
    crate::native_floor::runtime_items_named_by(&types).map_err(wall)
}

#[cfg(test)]
#[path = "native_foreign_tests.rs"]
mod tests;
