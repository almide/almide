//! #3492: the name of a monomorphized instance, built so that no two
//! different instances share a name and no user fn can spell one.
//!
//! The old spelling `<fn>__<A>_<B>` was not injective on either side. A user
//! type `List_Int` and the applied `List[Int]` both mangled to `List_Int`, so
//! `tag[List[Int]]` and `tag[List_Int]` shared one fn. A user fn named
//! `wrap__Int` had the name of the instance `wrap[Int]`.
//!
//! An instance is now named in the compiler's `__` fn-name space, which #3483
//! keeps free of user entry fns:
//!
//! ```text
//! __almd_mono<len>_<base>__<types>
//! ```
//!
//! `<len>` is the byte length of `<base>`, so the base is recovered exactly
//! ([`mono_instance_base`]) whatever it contains. `<types>` is a prefix-free
//! code over [`Ty`] ([`mangle_ty`]): a `_` that comes from a user name is
//! written `_u` and a `.` is written `_d`, and every structural mark is `_`
//! followed by a different letter. A user name therefore cannot produce a
//! structural mark, and the code decodes to exactly one type list.

use almide_lang::types::{Ty, TypeConstructorId};

/// The prefix every monomorphized instance's name starts with.
pub const MONO_INSTANCE_PREFIX: &str = "__almd_mono";

/// Separates the types of a binding list (and of a type's arguments).
const COMMA: &str = "_c";

/// The name of the instance of the generic fn `base` at the mangled
/// binding list `types` ([`mangle_ty_list`]).
pub fn mono_instance_name(base: &str, types: &str) -> String {
    format!("{MONO_INSTANCE_PREFIX}{}_{base}__{types}", base.len())
}

/// The generic fn a monomorphized instance was specialized from, or `None`
/// when `name` is not an instance name.
pub fn mono_instance_base(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(MONO_INSTANCE_PREFIX)?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let len: usize = rest[..digits].parse().ok()?;
    let rest = rest[digits..].strip_prefix('_')?;
    let base = rest.get(..len)?;
    rest[len..].starts_with("__").then_some(base)
}

/// `name` with a monomorphized instance read as the generic fn it came from.
/// Name-keyed decisions about a fn (registry routing, the `__fallible_*`
/// carrier family, …) are about the generic; the instance's types travel in
/// the call's own types.
pub fn mono_base_or_self(name: &str) -> &str {
    mono_instance_base(name).unwrap_or(name)
}

/// The mangled form of a binding list, in the caller's order.
pub fn mangle_ty_list<'a>(tys: impl IntoIterator<Item = &'a Ty>) -> String {
    tys.into_iter().map(mangle_ty).collect::<Vec<_>>().join(COMMA)
}

/// The prefix-free code of one type (see the module docs).
pub fn mangle_ty(ty: &Ty) -> String {
    let mut out = String::new();
    push_ty(&mut out, ty);
    out
}

fn push_ty(out: &mut String, ty: &Ty) {
    if let Some(word) = scalar_word(ty) {
        out.push_str(word);
        return;
    }
    match ty {
        Ty::Named(name, args) => {
            out.push_str("_n");
            push_user_name(out, name.as_str());
            if !args.is_empty() {
                push_args(out, args);
            }
        }
        Ty::Applied(TypeConstructorId::UserDefined(name), args) => {
            out.push_str("_g");
            push_user_name(out, name);
            push_args(out, args);
        }
        // A builtin constructor's word, always followed by its argument list:
        // `Matrix_l_r` is not the scalar `Matrix`.
        Ty::Applied(id, args) => {
            out.push_str(&id.to_string());
            push_args(out, args);
        }
        Ty::Tuple(elems) => {
            out.push_str("Tup");
            push_args(out, elems);
        }
        Ty::Record { fields } => push_record(out, "Rec", fields),
        Ty::OpenRecord { fields } => push_record(out, "ORec", fields),
        Ty::Union(members) => {
            out.push_str("Union");
            push_args(out, members);
        }
        Ty::Fn { params, ret, is_effect } => {
            out.push_str(if *is_effect { "EffFn_l" } else { "Fn_l" });
            push_list(out, params);
            out.push_str("_a");
            push_ty(out, ret);
            out.push_str("_r");
        }
        // Anything else (a variant type, a const value, a leftover type
        // variable) is spelled by its full debug form, escaped: distinct
        // types print distinct forms, so the code stays injective.
        other => {
            out.push_str("_z");
            push_user_name(out, &format!("{other:?}"));
        }
    }
}

/// The fixed word of a scalar type. No word contains `_`, and none is
/// followed by `_l`, so a word never runs into the code after it.
fn scalar_word(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int => "Int",
        Ty::Float => "Float",
        Ty::Int8 => "Int8",
        Ty::Int16 => "Int16",
        Ty::Int32 => "Int32",
        Ty::Int64 => "Int64",
        Ty::UInt8 => "UInt8",
        Ty::UInt16 => "UInt16",
        Ty::UInt32 => "UInt32",
        Ty::UInt64 => "UInt64",
        Ty::Float32 => "Float32",
        Ty::Float64 => "Float64",
        Ty::String => "String",
        Ty::Bool => "Bool",
        Ty::Bytes => "Bytes",
        Ty::Matrix => "Matrix",
        Ty::Unit => "Unit",
        Ty::RawPtr => "RawPtr",
        _ => return None,
    })
}

/// `_l` + the types separated by `_c` + `_r`.
fn push_args(out: &mut String, tys: &[Ty]) {
    out.push_str("_l");
    push_list(out, tys);
    out.push_str("_r");
}

fn push_list(out: &mut String, tys: &[Ty]) {
    for (i, t) in tys.iter().enumerate() {
        if i > 0 {
            out.push_str(COMMA);
        }
        push_ty(out, t);
    }
}

/// A structural record is its field names and types, sorted by name:
/// `Rec_l` + `<field>_k<type>` separated by `_c` + `_r`.
fn push_record(out: &mut String, word: &str, fields: &[(almide_base::intern::Sym, Ty)]) {
    let mut sorted: Vec<&(almide_base::intern::Sym, Ty)> = fields.iter().collect();
    sorted.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    out.push_str(word);
    out.push_str("_l");
    for (i, (name, t)) in sorted.iter().enumerate() {
        if i > 0 {
            out.push_str(COMMA);
        }
        push_user_name(out, name.as_str());
        out.push_str("_k");
        push_ty(out, t);
    }
    out.push_str("_r");
}

/// A name the user spelled: ASCII alphanumerics stay, `_` is `_u`, `.` is
/// `_d`, and any other char is `_x<hex>_`. No escape is `_c`, `_k`, `_l`,
/// `_r`, `_a`, so the name ends where the code says it does.
fn push_user_name(out: &mut String, name: &str) {
    for ch in name.chars() {
        match ch {
            c if c.is_ascii_alphanumeric() => out.push(c),
            '_' => out.push_str("_u"),
            '.' => out.push_str("_d"),
            c => out.push_str(&format!("_x{:x}_", c as u32)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn named(n: &str, args: Vec<Ty>) -> Ty {
        Ty::Named(sym(n), args)
    }
    fn list(t: Ty) -> Ty {
        Ty::Applied(TypeConstructorId::List, vec![t])
    }
    fn rec(fields: &[(&str, Ty)]) -> Ty {
        Ty::Record { fields: fields.iter().map(|(n, t)| (sym(n), t.clone())).collect() }
    }

    /// The pairs #3492 found sharing one name, and their neighbours.
    #[test]
    fn user_spellings_never_meet_a_structural_type() {
        let groups: Vec<Vec<Vec<Ty>>> = vec![
            vec![vec![list(Ty::Int)], vec![named("List_Int", vec![])]],
            vec![vec![named("Pair", vec![Ty::Int, Ty::String])], vec![named("Pair_Int_String", vec![])]],
            vec![vec![Ty::Int, named("String_X", vec![])], vec![named("Int_String", vec![]), named("X", vec![])]],
            vec![vec![named("Box", vec![list(Ty::Int)])], vec![named("Box_List_Int", vec![])], vec![list(named("Box", vec![Ty::Int]))], vec![named("List_Box_Int", vec![])]],
            vec![vec![rec(&[("a_b", Ty::Int), ("c", Ty::Int)])], vec![rec(&[("a", Ty::Int), ("b_c", Ty::Int)])], vec![rec(&[("a", Ty::Int), ("b", Ty::Int), ("c", Ty::Int)])]],
            vec![vec![named("varlib.Pigment", vec![])], vec![named("varlib_Pigment", vec![])]],
            vec![vec![Ty::Matrix], vec![Ty::Applied(TypeConstructorId::Matrix, vec![])]],
            vec![vec![Ty::Int], vec![named("Int", vec![])], vec![Ty::Int, Ty::Int], vec![]],
        ];
        for g in groups {
            let names: Vec<String> = g.iter().map(|tys| mono_instance_name("tag", &mangle_ty_list(tys))).collect();
            let unique: std::collections::HashSet<&String> = names.iter().collect();
            assert_eq!(unique.len(), names.len(), "two type lists share one instance: {names:?}");
        }
    }

    /// A user fn whose name holds `__` or a type word is a different base.
    #[test]
    fn the_base_is_recovered_exactly() {
        for base in ["wrap", "wrap__Int", "__fallible_map", "a_b", "almide_fn___g", ""] {
            let n = mono_instance_name(base, &mangle_ty_list(&[Ty::Int]));
            assert_eq!(mono_instance_base(&n), Some(base), "{n}");
        }
        assert_ne!(mono_instance_name("wrap", "Int"), mono_instance_name("wrap__Int", ""));
        assert_eq!(mono_instance_base("wrap__Int"), None);
        assert_eq!(mono_instance_base("__fallible_map"), None);
        assert_eq!(mono_base_or_self("__fallible_map"), "__fallible_map");
    }

    #[test]
    fn scalars_and_builtins_read_plainly() {
        assert_eq!(mono_instance_name("wrap", &mangle_ty_list(&[Ty::Int])), "__almd_mono4_wrap__Int");
        assert_eq!(mangle_ty_list(&[list(Ty::Int), Ty::String]), "List_lInt_r_cString");
        assert_eq!(mangle_ty(&named("List_Int", vec![])), "_nList_uInt");
    }
}
