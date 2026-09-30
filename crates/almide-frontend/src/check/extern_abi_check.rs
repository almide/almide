//! E090: an `@extern(c)` signature with a type that has no C representation
//! (#3054).
//!
//! The C ABI is a closed table (`almide_lang::types::extern_abi`): scalars at
//! their width, `Bool` as an `int`, `String` parameters as a NUL-terminated
//! copy, `RawPtr`. A list, bytes, map, record, variant, option, tuple or fn
//! value has no C form, and a `mut` parameter has no C way to write back. Such
//! a declaration used to pass `check` and die at rustc on a wrapper that
//! spelled the type as its `Debug` text — the acceptance-parity class: check
//! refuses what the build cannot honor. The renderer reads the same table.
use super::{Checker, err};
use crate::ast::{ExternAttr, Param, Span, TypeExpr};
use almide_lang::types::extern_abi::{
    c_param_abi, c_return_abi, native_extern_kind, NativeExtern, C_PARAM_TYPES, C_RETURN_TYPES,
};

const HINT_SHIM: &str = "bind it through an `@extern(rust, \"crate::host\", \"f\")` fn in `native/*.rs` instead, which takes Almide values (`&[u8]`, `&[T]`, `&str`, records, …) and can call the C function itself";

impl Checker {
    pub(super) fn reject_extern_c_types(
        &mut self,
        fn_name: &str,
        params: &[Param],
        return_type: &TypeExpr,
        extern_attrs: &[ExternAttr],
        decl_span: Option<Span>,
    ) {
        // The native build binds the FIRST attr naming a native kind.
        let first = extern_attrs.iter().find_map(|a| native_extern_kind(a.target.as_str()));
        if first != Some(NativeExtern::C) {
            return;
        }
        for p in params {
            if p.is_mut {
                self.emit_extern_c(
                    format!("@extern(c) fn '{fn_name}': `mut` parameter '{}' cannot be written back through C", p.name),
                    format!("drop `mut` and return the new value, or {HINT_SHIM}"),
                    fn_name,
                    decl_span,
                );
                continue;
            }
            let ty = self.env.resolve_named(&self.resolve_type_expr(&p.ty));
            if ty.is_unresolved() {
                continue;
            }
            if c_param_abi(&ty).is_none() {
                self.emit_extern_c(
                    format!("@extern(c) fn '{fn_name}': parameter '{}' has type {}, which has no C representation", p.name, ty.display()),
                    format!("an @extern(c) parameter is one of {C_PARAM_TYPES}; {HINT_SHIM}"),
                    fn_name,
                    decl_span,
                );
            }
        }
        let ret = self.env.resolve_named(&self.resolve_type_expr(return_type));
        if !ret.is_unresolved() && c_return_abi(&ret).is_none() {
            self.emit_extern_c(
                format!("@extern(c) fn '{fn_name}': return type {} has no C representation", ret.display()),
                format!("an @extern(c) return is one of {C_RETURN_TYPES}; {HINT_SHIM}"),
                fn_name,
                decl_span,
            );
        }
    }

    fn emit_extern_c(&mut self, msg: String, hint: String, fn_name: &str, span: Option<Span>) {
        let mut d = err(msg, hint, format!("fn {fn_name}")).with_code("E090");
        if let Some(sp) = span {
            d.file = self.source_file.clone();
            d.line = Some(sp.line);
            d.col = Some(sp.col);
        }
        self.emit(d);
    }
}
