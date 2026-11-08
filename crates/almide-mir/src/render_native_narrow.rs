//! #4006: a narrow integer `var` lives in its own Rust type.
//!
//! The render carries every integer in the i64 lane, and a narrow value
//! crosses a call boundary in its real type (`narrow_abi_wrapper`). A mutable
//! local that holds one — an `Int32` loop accumulator — stayed an `i64`, so
//! its loop-carried value was sign-extended on every step and the optimizer
//! reduced the loop in the wide form: `int32_recurrence` ran 1.12x the
//! standard codegen, which declares the local `i32` (same loop, measured with
//! the one local hand-narrowed: 1.00x).
//!
//! A local reassigned by `SetLocal` whose every value is a narrow integer of
//! one kind — the result of a narrow ring op, a call returning that kind,
//! another such local, the identity re-wrap of one, or a constant in its
//! range — is declared in that type. Every read widens it back with `as i64`,
//! every write narrows with `as`: exact both ways, since each value it is ever
//! given already fits the type. The kind is read off the MIR's narrow ops and
//! the signature table, never off a type or a name. Integer locals carry no
//! ownership, so the certificate is the one the MIR already has.
//!
//! A local is kept on the lane wherever its rendered text is not a plain
//! declaration, whole-line assignment and reads (a binding, a borrow taken
//! mutably, a string on the line): the respelling never guesses.

use crate::render_native::{var, NativeSigKind, NativeSigs};
use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// `rendered` with every narrow mutable local of `func` respelled.
pub(crate) fn narrow_locals(func: &MirFunction, sigs: &NativeSigs, rendered: String) -> String {
    narrow_local_kinds(func, sigs)
        .into_iter()
        .fold(rendered, |text, (v, ty)| respell(&text, &var(v), ty).unwrap_or(text))
}

/// One value a local can be given.
#[derive(Clone, Copy)]
enum Src {
    /// Exactly this narrow kind.
    Narrow(&'static str),
    /// A constant (fits a kind or not).
    Const(i64),
    /// Whatever the local or identity copy `v` holds.
    Same(ValueId),
    /// Something no narrow kind describes.
    Wide,
}

/// The narrow kind of each mutable local all of whose values are of it.
fn narrow_local_kinds(func: &MirFunction, sigs: &NativeSigs) -> BTreeMap<ValueId, &'static str> {
    let params: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut def: BTreeMap<ValueId, Src> = BTreeMap::new();
    let mut sets: BTreeMap<ValueId, Vec<ValueId>> = BTreeMap::new();
    for op in &func.ops {
        let src = match op {
            Op::SetLocal { local, src } => {
                sets.entry(*local).or_default().push(*src);
                continue;
            }
            Op::ConstInt { value, .. } => Src::Const(*value),
            Op::Prim { kind: PrimKind::NativeNarrowBin { bits, signed, .. }, .. } => {
                NativeSigKind::Narrow { bits: *bits, signed: *signed }.narrow_rust().map_or(Src::Wide, Src::Narrow)
            }
            Op::CallFn { name, .. } => {
                sigs.get(name.as_str()).and_then(|(_, r)| (*r).and_then(|k| k.narrow_rust())).map_or(Src::Wide, Src::Narrow)
            }
            Op::IntBinOp { op: IntOp::Or, a, b, .. } if a == b => Src::Same(*a),
            _ => Src::Wide,
        };
        if let Some(d) = crate::mir_ops::defined_value(op) {
            def.insert(d, src);
        }
    }
    // Optimistic fixpoint: a local is narrow until one of its values is not;
    // a local given only constants and locals still open waits for a kind.
    let mut kind: BTreeMap<ValueId, Option<&'static str>> =
        sets.keys().filter(|v| !params.contains(v)).map(|v| (*v, None)).collect();
    let mut wide: BTreeSet<ValueId> = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        let open: Vec<ValueId> = kind.keys().filter(|v| !wide.contains(v)).copied().collect();
        for v in &open {
            let srcs = &sets[v];
            let first = def.get(v).copied().unwrap_or(Src::Wide);
            // A source that is itself a local gives whatever that local holds.
            let of = |s: &ValueId| if sets.contains_key(s) { Src::Same(*s) } else { def.get(s).copied().unwrap_or(Src::Wide) };
            let given = std::iter::once(first).chain(srcs.iter().map(of));
            let resolved: Vec<Resolved> = given.map(|s| resolve(s, &def, &kind, &wide)).collect();
            let kinds: BTreeSet<&'static str> =
                resolved.iter().filter_map(|r| if let Resolved::Kind(t) = r { Some(*t) } else { None }).collect();
            let got = kinds.iter().next().copied();
            let narrow = kinds.len() <= 1
                && resolved.iter().all(|r| match r {
                    Resolved::Wide => false,
                    Resolved::Const(c) => got.is_none_or(|t| fits(t, *c)),
                    Resolved::Kind(_) | Resolved::Pending => true,
                });
            if !narrow {
                wide.insert(*v);
                changed = true;
            } else if kind[v] != got {
                kind.insert(*v, got);
                changed = true;
            }
        }
    }
    kind.into_iter().filter(|(v, _)| !wide.contains(v)).filter_map(|(v, t)| Some((v, t?))).collect()
}

enum Resolved {
    Kind(&'static str),
    Const(i64),
    /// A local not yet given a kind (it takes the others').
    Pending,
    Wide,
}

fn resolve(
    s: Src,
    def: &BTreeMap<ValueId, Src>,
    kind: &BTreeMap<ValueId, Option<&'static str>>,
    wide: &BTreeSet<ValueId>,
) -> Resolved {
    match s {
        Src::Narrow(t) => Resolved::Kind(t),
        Src::Const(c) => Resolved::Const(c),
        Src::Wide => Resolved::Wide,
        Src::Same(v) if wide.contains(&v) => Resolved::Wide,
        Src::Same(v) => match kind.get(&v) {
            Some(Some(t)) => Resolved::Kind(*t),
            Some(None) => Resolved::Pending,
            // An identity copy: what its operand is.
            None => match def.get(&v) {
                Some(Src::Same(u)) if *u != v => resolve(Src::Same(*u), def, kind, wide),
                Some(&inner) if matches!(inner, Src::Narrow(_) | Src::Const(_)) => resolve(inner, def, kind, wide),
                _ => Resolved::Wide,
            },
        },
    }
}

/// Whether the constant `c` is a value of the narrow Rust type `t`.
fn fits(t: &str, c: i64) -> bool {
    match t {
        "i8" => i8::try_from(c).is_ok(),
        "i16" => i16::try_from(c).is_ok(),
        "i32" => i32::try_from(c).is_ok(),
        "u8" => u8::try_from(c).is_ok(),
        "u16" => u16::try_from(c).is_ok(),
        _ => u32::try_from(c).is_ok(),
    }
}

/// `text` with the local `name` declared as `ty`, or `None` where its text is
/// not one declaration, whole-line assignments and plain reads.
fn respell(text: &str, name: &str, ty: &str) -> Option<String> {
    let decl = format!("let mut {name}: i64 = ");
    let assign = format!("{name} = ");
    let mut decls = 0;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_start();
        let indent = &line[..line.len() - body.len()];
        let (head, rhs) = match (body.strip_prefix(decl.as_str()), body.strip_prefix(assign.as_str())) {
            (Some(rhs), _) => {
                decls += 1;
                (Some(format!("let mut {name}: {ty} = ")), rhs)
            }
            (None, Some(rhs)) => (Some(assign.clone()), rhs),
            (None, None) => (None, body),
        };
        let rhs = widen_reads(rhs, name)?;
        out.push_str(indent);
        match head {
            None => out.push_str(&rhs),
            Some(head) => {
                let (code, nl) = rhs.strip_suffix('\n').map_or((rhs.as_str(), ""), |c| (c, "\n"));
                let expr = code.strip_suffix(';')?;
                out.push_str(&format!("{head}({expr}) as {ty};{nl}"));
            }
        }
    }
    (decls == 1).then_some(out)
}

/// `code` with each read of `name` widened to the lane; `None` at an
/// occurrence that is not a plain read.
fn widen_reads(code: &str, name: &str) -> Option<String> {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(at) = rest.find(name) {
        let (before, after) = (&rest[..at], &rest[at + name.len()..]);
        out.push_str(before);
        rest = after;
        if out.chars().next_back().is_some_and(is_ident) || after.chars().next().is_some_and(is_ident) {
            out.push_str(name);
            continue;
        }
        let trail = after.trim_start();
        let writes = trail.starts_with(':')
            || (trail.starts_with('=') && !trail.starts_with("=="))
            || ["+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>="].iter().any(|w| trail.starts_with(w));
        let binds = ["&mut ", "let ", "for ", "|", "|(", "."].iter().any(|p| out.ends_with(p));
        if writes || binds || code.contains('"') {
            return None;
        }
        out.push_str(&format!("({name} as i64)"));
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{respell, widen_reads};

    #[test]
    fn a_narrow_local_is_declared_narrow_and_widened_at_each_read() {
        let text = "    let mut v0: i64 = 7i64;\n    loop {\n        let mut v5: i64 = (almd_step((v0 as i32), (v4 as i32)) as i64);\n        v0 = v5;\n    }\n    let mut v8: AlmideString = almd_int32_to_string(v0);\n";
        let got = respell(text, "v0", "i32").unwrap();
        assert!(got.contains("    let mut v0: i32 = (7i64) as i32;\n"), "{got}");
        assert!(got.contains("almd_step(((v0 as i64) as i32), (v4 as i32))"), "{got}");
        assert!(got.contains("        v0 = (v5) as i32;\n"), "{got}");
        assert!(got.contains("almd_int32_to_string((v0 as i64))"), "{got}");
        assert!(!got.contains("v00") && got.contains("(v4 as i32)"));
    }

    #[test]
    fn a_local_that_is_not_only_read_stays_on_the_lane() {
        assert!(widen_reads("f(&mut v0);", "v0").is_none());
        assert!(widen_reads("for v0 in 0..n {", "v0").is_none());
        assert!(widen_reads("print!(\"{}\", v0);", "v0").is_none());
        assert!(widen_reads("if c { v0 = 1; }", "v0").is_none());
        assert_eq!(widen_reads("v10 + v01 + v0", "v0").unwrap(), "v10 + v01 + (v0 as i64)");
        // two declarations: not one local's text
        assert!(respell("let mut v0: i64 = 1;\nlet mut v0: i64 = 2;\n", "v0", "i8").is_none());
    }
}
