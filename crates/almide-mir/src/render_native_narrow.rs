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
//! the identity re-wrap of one, or a constant in its range — is declared in
//! that type. Every read widens it back with `as i64`,
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

/// A narrow integer kind: (bits, signed).
type Kind = (u8, bool);

/// Not a narrow kind: a value no narrow kind describes.
const WIDE: Kind = (64, true);

/// One value a local can be given.
#[derive(Clone, Copy)]
enum Src {
    Narrow(Kind),
    Const(i64),
    Wide,
}

/// The narrow kind of each mutable local all of whose values are of it.
///
/// A local given another local's value is kept wide: its values are known
/// only through that local's, and no program has needed the chase.
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
            Op::Prim { kind: PrimKind::NativeNarrowBin { bits, signed, .. }, .. } => Src::Narrow((*bits, *signed)),
            Op::CallFn { name, .. } => {
                sigs.get(name.as_str()).and_then(|(_, r)| (*r).and_then(narrow_kind)).map_or(Src::Wide, Src::Narrow)
            }
            // The identity a fused re-wrap left (`native_narrow_rewrite`): its
            // operand is the ring op's own result, never a local.
            Op::IntBinOp { op: IntOp::Or, a, b, .. } if a == b => def.get(a).copied().unwrap_or(Src::Wide),
            _ => Src::Wide,
        };
        if let Some(d) = crate::mir_ops::defined_value(op) {
            def.insert(d, src);
        }
    }
    let given = |s: &ValueId| [def.get(s).copied().unwrap_or(Src::Wide), Src::Wide][sets.contains_key(s) as usize];
    sets.iter()
        .filter(|(v, _)| !params.contains(v))
        .filter_map(|(v, srcs)| {
            let views: Vec<(Option<Kind>, Option<i64>)> =
                std::iter::once(def.get(v).copied().unwrap_or(Src::Wide)).chain(srcs.iter().map(&given)).map(view).collect();
            let kinds: BTreeSet<Kind> = views.iter().filter_map(|w| w.0).collect();
            let k = kinds.first().copied().unwrap_or(WIDE);
            let narrow = (kinds.len() == 1) & (k != WIDE) & views.iter().filter_map(|w| w.1).all(|c| fits(k, c));
            let ty = NativeSigKind::Narrow { bits: k.0, signed: k.1 }.narrow_rust();
            narrow.then_some((*v, ty?))
        })
        .collect()
}

/// The narrow kind of a signature kind, if it is one.
fn narrow_kind(k: NativeSigKind) -> Option<Kind> {
    if let NativeSigKind::Narrow { bits, signed } = k { Some((bits, signed)) } else { None }
}

/// A value's kind (a constant has none of its own) and constant.
fn view(s: Src) -> (Option<Kind>, Option<i64>) {
    match s {
        Src::Narrow(k) => (Some(k), None),
        Src::Const(c) => (None, Some(c)),
        Src::Wide => (Some(WIDE), None),
    }
}

/// Whether the constant `c` is a value of kind `k`.
fn fits(k: Kind, c: i64) -> bool {
    let (half, s) = (1i128 << (k.0 - 1), i128::from(k.1));
    (-half * s..=half * (2 - s) - 1).contains(&i128::from(c))
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
        // Part of a longer identifier: not this local.
        let joined = out.chars().next_back().is_some_and(is_ident) | after.chars().next().is_some_and(is_ident);
        let trail = after.trim_start();
        let writes = trail.starts_with(':')
            | (trail.starts_with('=') & !trail.starts_with("=="))
            | ["+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>="].iter().any(|w| trail.starts_with(w));
        let binds = ["&mut ", "let ", "for ", "|", "|(", "."].iter().any(|p| out.ends_with(p));
        (joined | !(writes | binds | code.contains('"'))).then_some(())?;
        out.push_str(&[format!("({name} as i64)"), name.to_string()][usize::from(joined)]);
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{fits, respell, widen_reads};

    #[test]
    fn a_constant_fits_a_kind_exactly_within_its_range() {
        assert!(fits((8, true), -128) && fits((8, true), 127) && !fits((8, true), 128) && !fits((8, true), -129));
        assert!(fits((8, false), 0) && fits((8, false), 255) && !fits((8, false), 256) && !fits((8, false), -1));
        assert!(fits((32, true), i32::MIN.into()) && !fits((32, true), i64::from(i32::MAX) + 1));
        assert!(fits((32, false), u32::MAX.into()) && !fits((32, false), -1));
    }

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
