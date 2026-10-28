//! E090: an `@extern(c)` signature with a type that has no C representation
//! (#3054).
//!
//! The C ABI is a closed table (`almide_lang::types::extern_abi`): scalars at
//! their width, `Bool` as an `int`, `String` parameters as a NUL-terminated
//! copy, `Handle`, and `Region` (ADR-0031): a parameter crosses as (pointer,
//! `size_t` length), and a return needs `@region(len = …, release = "…")`
//! naming its length — a parameter of an integer type or an integer literal —
//! and the C function that frees it. A list, bytes, map, record, variant,
//! option, tuple or fn
//! value has no C form, and a `mut` parameter has no C way to write back. Such
//! a declaration used to pass `check` and die at rustc on a wrapper that
//! spelled the type as its `Debug` text — the acceptance-parity class: check
//! refuses what the build cannot honor. The renderer reads the same table.
use super::{Checker, err};
use crate::ast::{AttrValue, Attribute, ExternAttr, Param, Span, TypeExpr};
use almide_lang::stdlib_info::REGION_ATTR;
use almide_lang::types::extern_abi::{
    c_param_abi, c_return_abi, is_region, native_extern_kind, NativeExtern, C_PARAM_TYPES, C_RETURN_TYPES,
};
use almide_lang::types::Ty;

const HINT_SHIM: &str = "bind it through an `@extern(rust, \"crate::host\", \"f\")` fn in `native/*.rs` instead, which takes Almide values (`&[u8]`, `&[T]`, `&str`, records, …) and can call the C function itself";

/// The parts of an `@extern` fn declaration its C ABI check reads.
pub(super) struct ExternSig<'a> {
    pub name: &'a str,
    pub params: &'a [Param],
    pub return_type: &'a TypeExpr,
    pub span: Option<Span>,
}

impl Checker {
    pub(super) fn reject_extern_c_types(&mut self, sig: ExternSig<'_>, extern_attrs: &[ExternAttr], attrs: &[Attribute]) {
        let ExternSig { name: fn_name, params, return_type, span: decl_span } = sig;
        let region = attrs.iter().find(|a| a.name.as_str() == REGION_ATTR);
        let declared_ret = self.resolve_type_expr(return_type);
        if let Some(attr) = region {
            self.check_region_attr(fn_name, params, &declared_ret, attr, decl_span);
        }
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
            let declared = self.resolve_type_expr(&p.ty);
            if is_region(&declared) {
                continue;
            }
            let ty = self.env.resolve_named(&declared);
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
        if is_region(&declared_ret) {
            if region.is_none() {
                self.emit_extern_c(
                    format!("@extern(c) fn '{fn_name}': a Region return needs the length the foreign function returns"),
                    String::from(
                        "state it on the declaration: `@region(len = n, release = \"free_fn\")`, with `n` a parameter or an integer, \
                         and `free_fn` the C function that frees the buffer — or return `Handle` and read it through accessor functions (ADR-0031 D2)"
                    ),
                    fn_name,
                    decl_span,
                );
            }
            return;
        }
        let ret = self.env.resolve_named(&declared_ret);
        if !ret.is_unresolved() && c_return_abi(&ret).is_none() {
            self.emit_extern_c(
                format!("@extern(c) fn '{fn_name}': return type {} has no C representation", ret.display()),
                format!("an @extern(c) return is one of {C_RETURN_TYPES}; {HINT_SHIM}"),
                fn_name,
                decl_span,
            );
        }
    }

    /// `@region(len = …, release = "…")` (ADR-0031 D2, D6): only on an
    /// `@extern` fn returning `Region`; `len` is a parameter of an integer
    /// type or a non-negative integer literal; `release` is a C function name.
    fn check_region_attr(&mut self, fn_name: &str, params: &[Param], ret: &Ty, attr: &Attribute, span: Option<Span>) {
        let mut problems: Vec<String> = Vec::new();
        if !is_region(ret) {
            problems.push(format!("it is on a fn returning {}, not Region", ret.display()));
        }
        let arg = |key: &str| attr.args.iter().find(|a| a.name.is_some_and(|n| n.as_str() == key)).map(|a| &a.value);
        match arg("len") {
            Some(AttrValue::Int { value }) if *value >= 0 => {}
            Some(AttrValue::Ident { name }) => match params.iter().find(|p| p.name == *name) {
                Some(p) => {
                    let t = self.env.resolve_named(&self.resolve_type_expr(&p.ty));
                    let integer = matches!(
                        t,
                        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
                    );
                    if !(integer || t.is_unresolved()) {
                        problems.push(format!("`len = {name}` names a parameter of type {}, not an integer", t.display()));
                    }
                }
                None => problems.push(format!("`len = {name}` names no parameter of '{fn_name}'")),
            },
            _ => problems.push("`len` must be a parameter name or a non-negative integer".to_string()),
        }
        match arg("release") {
            Some(AttrValue::String { value }) if !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {}
            _ => problems.push("`release` must name the C function that frees the buffer, as a string".to_string()),
        }
        if let Some(extra) = attr.args.iter().find(|a| !matches!(a.name.map(|n| n.as_str()), Some("len" | "release"))) {
            problems.push(format!("unknown argument `{}`", extra.name.map(|n| n.as_str().to_string()).unwrap_or_else(|| "(positional)".into())));
        }
        if problems.is_empty() {
            return;
        }
        self.emit_extern_c(
            format!("@extern fn '{fn_name}': `@region` {}", problems.join("; ")),
            "write `@region(len = n, release = \"free_fn\")` on an @extern fn returning Region: `n` a parameter or an integer, `free_fn` the C function that frees the buffer (ADR-0031 D2)".to_string(),
            fn_name,
            span,
        );
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
