//! Hashable keys for user types on the standard codegen (#4787) — the twin of
//! the native render's `render_native_key_encode.rs`.
//!
//! A `Set` element or `Map` key of a record or variant type stayed on the
//! runtime's linear scan: the runtime hashes only the key types it can name.
//! Each qualifying type gets its canonical key bytes (`AlmideKeyEncode`,
//! runtime map.rs), and `main` registers them, so the runtime indexes them
//! like an Int or String key.
//!
//! A type qualifies only when its `==` is the derived field-by-field one and
//! every field is an exact-equality type with key bytes: Int and the sized
//! integers, Bool, String, and lists, options, results and tuples of
//! those or of another qualifying type. A Float (IEEE `==`: `0.0 == -0.0`),
//! a closure, a Map, Set or Value field, a generic or a recursive type keeps
//! the linear scan. Only programs that build a Set or a Map carry the impls.

use std::collections::HashMap;

use almide_ir::{IrProgram, IrTypeDecl, IrTypeDeclKind, IrVariantKind};
use almide_lang::types::constructor::TypeConstructorId as C;
use almide_lang::types::Ty;

use super::RenderContext;

/// The marker line in `main` that becomes the key-type registration call
/// (or nothing) once the program's key types are known.
pub(crate) const KEY_TYPES_MARKER: &str = "    //__ALMIDE_KEY_TYPES__\n";

fn ty_ok(t: &Ty, decls: &HashMap<String, &IrTypeDecl>, ctx: &RenderContext, seen: &mut HashMap<String, Option<bool>>) -> bool {
    match t {
        // Bytes is `AlmideRcCow<Vec<u8>>` here, with no key impl: linear.
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::Bool | Ty::String => true,
        Ty::Applied(C::List | C::Option, a) if a.len() == 1 => ty_ok(&a[0], decls, ctx, seen),
        Ty::Applied(C::Result, a) if a.len() == 2 => ty_ok(&a[0], decls, ctx, seen) && ty_ok(&a[1], decls, ctx, seen),
        Ty::Tuple(es) if (2..=4).contains(&es.len()) => es.iter().all(|e| ty_ok(e, decls, ctx, seen)),
        Ty::Named(n, args) if args.is_empty() => decls.get(n.as_str()).is_some_and(|td| decl_ok(td, decls, ctx, seen)),
        _ => false,
    }
}

fn decl_ok(td: &IrTypeDecl, decls: &HashMap<String, &IrTypeDecl>, ctx: &RenderContext, seen: &mut HashMap<String, Option<bool>>) -> bool {
    let name = td.name.as_str();
    match seen.get(name) {
        // A cycle (recursive types are excluded below anyway).
        Some(None) => return false,
        Some(Some(known)) => return *known,
        None => {}
    }
    seen.insert(name.to_string(), None);
    let ok = decl_fields_ok(td, decls, ctx, seen);
    seen.insert(name.to_string(), Some(ok));
    ok
}

fn decl_fields_ok(td: &IrTypeDecl, decls: &HashMap<String, &IrTypeDecl>, ctx: &RenderContext, seen: &mut HashMap<String, Option<bool>>) -> bool {
    let name = td.name.as_str();
    let ann = &ctx.ann;
    if td.generics.as_ref().is_some_and(|g| !g.is_empty())
        || ann.eq_blocked_types.contains(name)
        || ann.fn_blocked_types.contains(name)
        || ann.recursive_enums.contains(name)
        || ann.region_enums.contains(name)
    {
        return false;
    }
    match &td.kind {
        IrTypeDeclKind::Record { fields } => fields.iter().all(|f| ty_ok(&f.ty, decls, ctx, seen)),
        IrTypeDeclKind::Variant { cases, .. } => cases.iter().all(|v| match &v.kind {
            IrVariantKind::Unit => true,
            IrVariantKind::Tuple { fields } => fields.iter().all(|t| ty_ok(t, decls, ctx, seen)),
            IrVariantKind::Record { fields } => fields.iter().all(|f| ty_ok(&f.ty, decls, ctx, seen)),
        }),
        _ => false,
    }
}

fn impl_for(td: &IrTypeDecl, ctx: &RenderContext) -> String {
    let n = td.name.as_str();
    let body = match &td.kind {
        IrTypeDeclKind::Record { fields } => fields.iter()
            .map(|f| format!("        self.{}.almide_key_encode(out);\n", ctx.field_ident(f.name.as_str())))
            .collect::<String>(),
        IrTypeDeclKind::Variant { cases, .. } => {
            let mut arms = String::from("        match self {\n");
            for (k, v) in cases.iter().enumerate() {
                let tag = format!("out.extend_from_slice(&{k}u32.to_le_bytes());");
                match &v.kind {
                    IrVariantKind::Unit => arms.push_str(&format!("            {n}::{} => {{ {tag} }}\n", v.name)),
                    IrVariantKind::Tuple { fields } => {
                        let binds: Vec<String> = (0..fields.len()).map(|i| format!("v{i}")).collect();
                        let enc: String = binds.iter().map(|b| format!(" {b}.almide_key_encode(out);")).collect();
                        arms.push_str(&format!("            {n}::{}({}) => {{ {tag}{enc} }}\n", v.name, binds.join(", ")));
                    }
                    IrVariantKind::Record { fields } => {
                        let binds: Vec<String> = fields.iter().map(|f| ctx.field_ident(f.name.as_str())).collect();
                        let enc: String = binds.iter().map(|b| format!(" {b}.almide_key_encode(out);")).collect();
                        arms.push_str(&format!("            {n}::{} {{ {} }} => {{ {tag}{enc} }}\n", v.name, binds.join(", ")));
                    }
                }
            }
            arms.push_str("        }\n");
            arms
        }
        _ => String::new(),
    };
    format!("impl AlmideKeyEncode for {n} {{\n    fn almide_key_encode(&self, out: &mut Vec<u8>) {{\n{body}    }}\n}}\n")
}

/// Whether the rendered user code builds a Set or a Map (names the runtime's
/// collection types): only then does it carry the key impls.
fn uses_keyed_collections(user_code: &str) -> bool {
    crate::rust_idents::identifiers(user_code).any(|id| id == "AlmideSet" || id == "AlmideMap")
}

/// The user code with `main`'s marker replaced by the key-type registration
/// (or removed), and the impls plus the registration fn appended.
pub(crate) fn place(user_code: String, program: &IrProgram, ctx: &RenderContext) -> String {
    if !user_code.contains(KEY_TYPES_MARKER) {
        return user_code;
    }
    // Only the decls this render emits under their own name: the program's
    // type decls minus the bundled twins the runtime defines
    // (`render_program_type_decls` skips the same ones). A key type whose
    // field reaches any other decl finds none here and keeps the linear scan.
    let all: Vec<&IrTypeDecl> = program.type_decls.iter().filter(|td| super::runtime_owned::twin_spelling(td).is_none()).collect();
    let decls: HashMap<String, &IrTypeDecl> = all.iter().map(|td| (td.name.to_string(), *td)).collect();
    let ok: Vec<&IrTypeDecl> = if uses_keyed_collections(&user_code) {
        all.iter().copied().filter(|td| decl_ok(td, &decls, ctx, &mut HashMap::new())).collect()
    } else {
        Vec::new()
    };
    if ok.is_empty() {
        return user_code.replacen(KEY_TYPES_MARKER, "", 1);
    }
    let mut out = user_code.replacen(KEY_TYPES_MARKER, "    almide_register_key_types();\n", 1);
    out.push('\n');
    for td in &ok {
        out.push_str(&impl_for(td, ctx));
    }
    out.push_str("fn almide_register_key_types() {\n");
    for td in &ok {
        out.push_str(&format!("    almide_map_register_key::<{}>();\n", td.name));
    }
    out.push_str("}\n");
    out
}
