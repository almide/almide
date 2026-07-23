//! MIR → native Rust renderer — the NATIVE leg of the trust spine (#764).
//!
//! Renders the SAME Perceus-disciplined MIR the wasm leg consumes, mapping the
//! ownership ops onto Rust's own memory management instead of literal RC:
//!
//!   Dup        → `.clone()` / `.to_string()` (a new owned handle; the clone IS the +1)
//!   Drop       → erased — Rust's scope-end (or reassignment) drop realizes the free.
//!                `verify_ownership` certifies the balance on the SAME ops pre-render.
//!   CallFn     → a user fn call, or a CLOSED runtime-boundary shim (`print_str`,
//!                `int.to_string`, `__str_concat`, …) mapped to native Rust —
//!                mirroring v0's runtime/rs floor, never re-implemented inline
//!
//! Ownership modes across calls follow the MIR call-mode signature: a heap arg is
//! BORROWED (`&str` param), a heap result is FRESH OWNED (`String` return).
//!
//! HONEST WALL: anything outside the rung subset returns `Err(LowerError::
//! Unsupported)` — the CLI falls back to v0. A rendered program is never wrong;
//! an unrenderable one declines loudly. Same discipline as the wasm ladder.
//!
//! Rung-2 subset: i64 scalars; String values (literals, `int.to_string`,
//! `__str_concat`, `string.eq`, `string.len`); String params/returns on user fns;
//! full scalar-or-String control flow (if-as-value, loops).

use crate::lower::LowerError;
use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, Repr, ValueId};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The native type a MIR value renders to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NTy {
    I64,
    /// An OWNED `String` local (fresh from a literal alloc, a heap-returning
    /// call, or a clone).
    Str,
    /// A BORROWED `&str` — a heap fn param (the MIR call mode borrows heap args).
    StrRef,
    /// An OWNED `Vec<i64>` local (rung 4 — a scalar-list literal / clone / call result).
    Vec,
    /// A BORROWED `&[i64]` — a scalar-list fn param (the same borrow call mode).
    VecRef,
    /// A real `f64` local (rung 5 — no i64-bits convention on native). MIR
    /// carries Float as i64 BITS; the boundary into a float op converts via
    /// `f64::from_bits` (bit-exact), and every float-op result stays `f64`.
    F64,
}

impl NTy {
    fn is_stringy(self) -> bool {
        matches!(self, NTy::Str | NTy::StrRef)
    }
    fn is_veccy(self) -> bool {
        matches!(self, NTy::Vec | NTy::VecRef)
    }
}

/// The DECLARED signature kind of a param/return, computed by the pipeline from the
/// Almide-level `Ty` (a `MirParam` carries only reprs — `Repr::Ptr` is String OR
/// List, and only the declaration disambiguates). Rung 4: scalar lists join strings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NativeSigKind {
    I64,
    Str,
    ListI64,
    F64,
}

/// fn name → (param kinds, return kind; None = Unit). Built by the pipeline where
/// the declared `Ty` is visible; the render trusts it (the precision wall already
/// rejected anything outside these kinds).
pub type NativeSigs = std::collections::BTreeMap<String, (Vec<NativeSigKind>, Option<NativeSigKind>)>;

fn wall(msg: impl Into<String>) -> LowerError {
    LowerError::Unsupported(msg.into())
}

fn var(v: ValueId) -> String {
    format!("v{}", v.0)
}

/// The rung-4 bounds-checked element accessors — byte-identical abort text to the
/// wasm `$elem_addr_chk` ("Error: index out of bounds" + exit 1) and to v0 native.
const IDX_GET_SHIM: &str = "fn almide_idx_get(v: &[i64], i: i64) -> i64 {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize]\n}";
const IDX_SET_SHIM: &str = "fn almide_idx_set(v: &mut Vec<i64>, i: i64, x: i64) {\n        if i < 0 || i as usize >= v.len() { eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }\n        v[i as usize] = x;\n}";

/// Borrow a stringy value as `&str` for a call argument.
fn as_str_arg(code: &str, t: NTy) -> String {
    match t {
        NTy::Str => format!("&{code}"),
        NTy::StrRef => code.to_string(),
        NTy::Vec | NTy::VecRef => unreachable!("as_str_arg on vec"),
        NTy::I64 | NTy::F64 => unreachable!("as_str_arg on scalar"),
    }
}

/// Read a MIR scalar as a real `f64`: an I64 local holds the f64 BITS (the MIR
/// Float convention — every float literal is a `ConstInt` of the bits), an F64
/// local IS the value. Bit-exact either way.
fn as_f64_arg(code: &str, t: NTy) -> Result<String, LowerError> {
    match t {
        NTy::F64 => Ok(code.to_string()),
        NTy::I64 => Ok(format!("f64::from_bits({code} as u64)")),
        _ => Err(wall("native: float op on a heap value")),
    }
}

/// The CLOSED runtime-boundary map: self-hosted runtime fn name → (arg NTys
/// [stringy args listed as `Str`], result, native Rust shim). Adding a name here
/// is adding to the trusted floor — keep it tiny; everything else walls. Every
/// addition needs a differential-corpus row in the same PR
/// (tests/native_v1_differential_test.rs).
fn shim(name: &str) -> Option<(&'static [NTy], Option<NTy>, &'static str)> {
    match name {
        "int.to_string" => Some((
            &[NTy::I64],
            Some(NTy::Str),
            "fn rt_int_to_string(n: i64) -> String { n.to_string() }",
        )),
        "print_str" => Some((
            &[NTy::Str],
            None,
            "fn rt_print_str(s: &str) { println!(\"{}\", s); }",
        )),
        "__str_concat" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::Str),
            "fn rt_str_concat(a: &str, b: &str) -> String { [a, b].concat() }",
        )),
        "string.eq" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_eq(a: &str, b: &str) -> i64 { (a == b) as i64 }",
        )),
        "string.len" => Some((
            // Codepoint count, NOT byte length (C-016 discipline).
            &[NTy::Str],
            Some(NTy::I64),
            "fn rt_string_len(s: &str) -> i64 { s.chars().count() as i64 }",
        )),
        // String predicates/transforms: each shim is the EXACT v0 native oracle
        // expression (runtime/rs/src/string.rs delegates to Rust std the same way),
        // so the differential gate pins byte-equality, and C-016/C-019/C-020's
        // full-Unicode discipline carries over unchanged.
        "string.contains" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_contains(s: &str, sub: &str) -> i64 { s.contains(sub) as i64 }",
        )),
        "string.starts_with" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_starts_with(s: &str, p: &str) -> i64 { s.starts_with(p) as i64 }",
        )),
        "string.ends_with" => Some((
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_ends_with(s: &str, p: &str) -> i64 { s.ends_with(p) as i64 }",
        )),
        "string.to_upper" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_upper(s: &str) -> String { s.to_uppercase() }",
        )),
        "string.to_lower" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_to_lower(s: &str) -> String { s.to_lowercase() }",
        )),
        "string.trim" => Some((
            &[NTy::Str],
            Some(NTy::Str),
            "fn rt_string_trim(s: &str) -> String { s.trim().to_string() }",
        )),
        "string.repeat" => Some((
            &[NTy::Str, NTy::I64],
            Some(NTy::Str),
            "fn rt_string_repeat(s: &str, n: i64) -> String { s.repeat(n as usize) }",
        )),
        "string.cmp" => Some((
            // Byte-wise lexicographic, -1/0/1 (C-019: rt_string_extra cmp = native oracle).
            &[NTy::Str, NTy::Str],
            Some(NTy::I64),
            "fn rt_string_cmp(a: &str, b: &str) -> i64 {\n    match a.cmp(b) { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, std::cmp::Ordering::Greater => 1 }\n}",
        )),
        "float.to_string" => Some((
            // The EXACT v0 native oracle (runtime/rs/src/float.rs::almide_rt_float_to_string):
            // shortest round-trip Display, integral values forced to a `.0` tail.
            &[NTy::F64],
            Some(NTy::Str),
            "fn rt_float_to_string(n: f64) -> String {\n    let s = format!(\"{}\", n);\n    if n.fract() == 0.0 && !s.contains('.') && !s.contains(\"inf\") && !s.contains(\"NaN\") {\n        format!(\"{}.0\", s)\n    } else {\n        s\n    }\n}",
        )),
        "__chk_div" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            "fn rt_chk_div(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a / b\n}",
        )),
        "__chk_mod" => Some((
            &[NTy::I64, NTy::I64],
            Some(NTy::I64),
            // v0's `almide_mod` macro prints "division by zero" for a zero rhs (mod and
            // div share the message — the C-002 oracle text); keep byte parity.
            "fn rt_chk_mod(a: i64, b: i64) -> i64 {\n    if b == 0 { eprintln!(\"Error: division by zero\"); std::process::exit(1); }\n    if a == i64::MIN && b == -1 { eprintln!(\"Error: integer overflow\"); std::process::exit(1); }\n    a % b\n}",
        )),
        _ => None,
    }
}

fn shim_rust_name(name: &str) -> String {
    format!("rt_{}", name.trim_start_matches("__").replace('.', "_"))
}

/// Render a whole MIR program to a self-contained Rust source, or WALL.
pub fn try_render_native_program(prog: &MirProgram, sigs: &NativeSigs) -> Result<String, LowerError> {
    let user_fns: BTreeMap<&str, &MirFunction> =
        prog.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    if !user_fns.contains_key("main") {
        return Err(wall("native: no main in the MIR program"));
    }

    let mut used_shims: Vec<&'static str> = Vec::new();
    let mut bodies = String::new();
    let mut fn_rets: BTreeMap<String, Option<NTy>> = BTreeMap::new();
    for func in &prog.functions {
        // The Perceus balance is machine-checked on the SAME ops this render
        // erases Drops from — the certificate that scope-end drop realizes it.
        if let Err(violations) = crate::verify_ownership(func) {
            return Err(wall(format!(
                "native: ownership verification failed for `{}`: {violations:?}",
                func.name
            )));
        }
        let (rendered, ret_nty) = render_fn(func, &user_fns, sigs, &mut used_shims)?;
        fn_rets.insert(func.name.clone(), ret_nty);
        bodies.push_str(&rendered);
        bodies.push('\n');
    }

    let mut out = String::from(
        "// Generated by the Almide v1 trust spine (native leg).\n\
         #![allow(unused_variables, unused_mut, unreachable_code, dead_code, non_snake_case)]\n\n",
    );
    used_shims.sort();
    used_shims.dedup();
    for s in used_shims {
        out.push_str(s);
        out.push_str("\n\n");
    }
    out.push_str(&bodies);
    // Rung-5 closures slab: the CallIndirect dispatch tables. One dispatcher per
    // ARITY (user args beyond the env block); the index space is the SAME
    // name-sorted lambda order `Op::FuncRef` renders (both derive from the
    // `user_fns` BTreeMap, so def and call site agree by construction). Only an
    // i64-returning lambda gets an arm — a heap-returning one is reachable only
    // through a CallIndirect with a heap result, which walls above, so its
    // missing arm can never be hit; the `_` arm is the §13 controlled halt.
    let lambda_names: Vec<&str> = user_fns
        .keys()
        .copied()
        .filter(|n| n.starts_with("__lambda_"))
        .collect();
    if !lambda_names.is_empty() {
        let mut arities: BTreeMap<usize, Vec<(usize, &str)>> = BTreeMap::new();
        for (idx, name) in lambda_names.iter().enumerate() {
            if fn_rets.get(*name) != Some(&Some(NTy::I64)) {
                continue;
            }
            let arity = user_fns[name].params.len().saturating_sub(1);
            arities.entry(arity).or_default().push((idx, name));
        }
        for (arity, fns) in arities {
            let params: String = (0..arity).map(|i| format!(", a{i}: i64")).collect::<String>();
            let args: String = (0..arity).map(|i| format!(", a{i}")).collect::<String>();
            out.push_str(&format!(
                "fn __almd_ci_{arity}(idx: i64, env: &[i64]{params}) -> i64 {{\n    match idx {{\n"
            ));
            for (idx, name) in fns {
                out.push_str(&format!("        {idx} => {}(env{args}),\n", mangle(name)));
            }
            out.push_str(
                "        _ => { eprintln!(\"Error: closure index out of range\"); \
                 std::process::exit(1) }\n    }\n}\n\n",
            );
        }
    }
    Ok(out)
}

/// The `Op::FuncRef` table index of a lifted lambda: its position in the
/// NAME-SORTED lambda list (the `user_fns` BTreeMap order — the same order the
/// dispatch tables above are generated from).
fn lambda_index(user_fns: &BTreeMap<&str, &MirFunction>, name: &str) -> Option<usize> {
    user_fns.keys().filter(|n| n.starts_with("__lambda_")).position(|n| *n == name)
}

/// Native param/result NTy for a repr: scalars are i64; a heap repr is a STRING
/// (the pipeline's precision wall on declared `Ty` guarantees this).
fn repr_nty(repr: &Repr, borrowed: bool) -> Result<NTy, LowerError> {
    match repr {
        Repr::Scalar { .. } => Ok(NTy::I64),
        Repr::Ptr { .. } | Repr::Boxed { .. } => Ok(if borrowed { NTy::StrRef } else { NTy::Str }),
    }
}

fn render_fn(
    func: &MirFunction,
    user_fns: &BTreeMap<&str, &MirFunction>,
    sigs: &NativeSigs,
    used_shims: &mut Vec<&'static str>,
) -> Result<(String, Option<NTy>), LowerError> {
    let own_sig = sigs.get(func.name.as_str());
    let mut tys: BTreeMap<ValueId, NTy> = BTreeMap::new();
    for (i, p) in func.params.iter().enumerate() {
        // The MIR call mode BORROWS heap args. The DECLARED kind (from the sig
        // table) disambiguates a heap `Repr::Ptr` param: `&str` vs `&[i64]`.
        let nty = match own_sig.and_then(|(ps, _)| ps.get(i)) {
            Some(NativeSigKind::ListI64) => NTy::VecRef,
            Some(NativeSigKind::Str) => NTy::StrRef,
            Some(NativeSigKind::I64) => NTy::I64,
            Some(NativeSigKind::F64) => NTy::F64,
            None => repr_nty(&p.repr, true)?,
        };
        tys.insert(p.value, nty);
    }

    let is_main = func.name == "main";
    let mut out = String::new();
    let mut indent = 1usize;
    // Each open if-as-value join: (marker, dst) — the decl is patched in once the
    // first arm yield reveals the join type.
    let mut if_stack: Vec<Option<(String, ValueId)>> = Vec::new();

    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    // Dead pure-Handle elision (rung-5 variants slab): the variant-match lower
    // still threads a `Prim{Handle}` for the heap-payload arms, but a
    // scalar-only match reads every slot through ListGetScalar and leaves the
    // handle dead. Handle is PURE (address materialization, no ownership, no
    // side effect), so skipping an unused one is sound — and it keeps the
    // subset honest: a USED Handle still walls below.
    let used: std::collections::BTreeSet<ValueId> = {
        let mut u = std::collections::BTreeSet::new();
        for op in &func.ops {
            match op {
                Op::IntBinOp { a, b, .. } => {
                    u.insert(*a);
                    u.insert(*b);
                }
                Op::Prim { args, .. } => {
                    u.extend(args.iter().copied());
                }
                Op::Call { args, .. } | Op::CallFn { args, .. } => {
                    for a in args {
                        if let CallArg::Handle(v) | CallArg::Scalar(v) = a {
                            u.insert(*v);
                        }
                    }
                }
                Op::ListGetScalar { list, idx, .. } => {
                    u.insert(*list);
                    u.insert(*idx);
                }
                Op::ListSetScalar { list, idx, val } => {
                    u.insert(*list);
                    u.insert(*idx);
                    u.insert(*val);
                }
                Op::SetLocal { src, .. } => {
                    u.insert(*src);
                }
                Op::Dup { src, .. } => {
                    u.insert(*src);
                }
                Op::IfThen { cond, .. } => {
                    u.insert(*cond);
                }
                Op::Else { val } | Op::EndIf { val } => {
                    if let Some(v) = val {
                        u.insert(*v);
                    }
                }
                _ => {}
            }
        }
        if let Some(r) = func.ret {
            u.insert(r);
        }
        u
    };
    for op in &func.ops {
        match op {
            Op::Prim { kind: crate::PrimKind::Handle, dst: Some(d), .. } if !used.contains(d) => {
                line!("// dead handle elided");
            }
            Op::ConstInt { dst, value } => {
                tys.insert(*dst, NTy::I64);
                line!("let mut {}: i64 = {}i64;", var(*dst), value);
            }
            Op::Alloc { dst, init, .. } => match init {
                Init::Str(s) => {
                    tys.insert(*dst, NTy::Str);
                    line!("let mut {}: String = String::from({s:?});", var(*dst));
                }
                other => return Err(wall(format!("native: Alloc {other:?} — outside the rung subset"))),
            },
            Op::Dup { dst, src } => render_dup(dst, src, &mut tys, &mut out, indent)?,
            // Rung-4 scalar-list literal: the natural Vec spelling. Elements are raw
            // i64 slot values (the wasm leg stores the same bits).
            Op::ListLit { dst, elems } => {
                for e in elems {
                    if tys.get(e) != Some(&NTy::I64) {
                        return Err(wall("native: ListLit with a non-scalar element"));
                    }
                }
                tys.insert(*dst, NTy::Vec);
                let items = elems.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ");
                line!("let mut {}: Vec<i64> = vec![{items}];", var(*dst));
            }
            // Rung-5 closures slab: a FuncRef is the lambda's DISPATCH-TABLE index
            // (the name-sorted position shared with the `__almd_ci_*` tables).
            Op::FuncRef { dst, name } => {
                let idx = lambda_index(user_fns, name).ok_or_else(|| {
                    wall(format!("native: FuncRef to unknown lambda `{name}`"))
                })?;
                tys.insert(*dst, NTy::I64);
                line!("let mut {}: i64 = {idx}; // fn table: {name}", var(*dst));
            }
            // A CallIndirect dispatches through the arity's `__almd_ci_*` table:
            // the leading Handle arg is the closure block (BORROWED env — `&[i64]`),
            // the rest are scalar user args, the result an i64. Heap args/results
            // are outside this slab (the wasm leg keeps them; native walls).
            Op::CallIndirect { dst, table_idx, args, result } => {
                render_call_indirect(dst, table_idx, args, result, &mut tys, &mut out, indent)?
            }
            // A scalar-capture closure block is a plain `Vec<i64>` — its recursive
            // `$__drop_closure` erases to scope-end (the drop header is 0: no heap,
            // no nested, no closure slots to free). A non-Vec value here would be a
            // heap-capturing block (prim-built) — its OWNING fn walls on the prims
            // long before this drop renders.
            Op::DropVariant { v, ty } if ty.as_str() == "closure" => {
                match tys.get(v) {
                    Some(NTy::Vec) => line!("// drop(closure block): scope-end"),
                    other => {
                        return Err(wall(format!(
                            "native: DropVariant(closure) of a non-Vec value ({other:?})"
                        )))
                    }
                }
            }
            // Rung-4 bounds-checked element load/store — the shims abort with the
            // byte-identical "Error: index out of bounds" + exit 1 the wasm
            // `$elem_addr_chk` and v0 native emit.
            Op::ListGetScalar { dst, list, idx } => {
                render_list_get_scalar(dst, list, idx, &mut tys, &mut out, indent, used_shims)?
            }
            Op::ListSetScalar { list, idx, val } => {
                render_list_set_scalar(list, idx, val, &tys, &mut out, indent, used_shims)?
            }
            // Drop is ERASED: Rust frees at scope end (or at reassignment for a
            // loop-carried handle). `verify_ownership` above certified balance.
            Op::Drop { v } => {
                if matches!(tys.get(v), Some(NTy::StrRef | NTy::VecRef)) {
                    return Err(wall("native: Drop of a borrowed param — MIR call-mode violation"));
                }
                line!("// drop: scope-end");
            }
            // A RECORD result's drop routes as the mask-driven `DropListStr` (the
            // record block IS a list block; the mask lists its heap slots). The
            // native rung-5 subset admits ALL-SCALAR records only — the mask is
            // empty, the free is the block itself → scope-end, same as `Drop`.
            // Anything non-Vec here would carry heap slots → wall.
            Op::DropListStr { v } => {
                match tys.get(v) {
                    Some(NTy::Vec) => line!("// drop(record/list block): scope-end"),
                    Some(NTy::VecRef) => {
                        return Err(wall("native: DropListStr of a borrowed param — MIR call-mode violation"))
                    }
                    other => {
                        return Err(wall(format!(
                            "native: DropListStr of a non-list value ({other:?}) — outside the rung subset"
                        )))
                    }
                }
            }
            // Pure ownership bookkeeping — no native code.
            Op::Consume { .. } | Op::Borrow { .. } | Op::MakeUnique { .. } => {}
            Op::SetLocal { local, src } => render_set_local(local, src, &mut tys, &mut out, indent)?,
            Op::IntBinOp { dst, op, a, b } => {
                tys.insert(*dst, NTy::I64);
                let rendered = render_int_binop(op, *a, *b, used_shims)?;
                line!("let mut {}: i64 = {};", var(*dst), rendered);
            }
            Op::CallFn { dst, name, args, result } => {
                render_call_fn(dst, name, args, result, user_fns, sigs, &mut tys, &mut out, indent, used_shims)?
            }
            // Rung-5 float floor: MIR floats are i64 BITS; native computes in real
            // f64. Every op below is IEEE-754-exact on both targets (hardware ops,
            // identical bit results), so byte-identity holds through
            // `float.to_string`. Min/Max/CopySign are excluded: Rust's `f64::min`
            // NaN semantics differ from wasm `f64.min` (they only occur inside
            // self-host bodies, which never render natively).
            Op::Prim { kind: crate::PrimKind::FloatBin(op), dst: Some(d), args } if args.len() == 2 => {
                render_float_bin(op, d, args, &mut tys, &mut out, indent)?
            }
            // `float.from_int` — int (i64) to f64, carried per the float floor.
            Op::Prim { kind: crate::PrimKind::F64FromInt, dst: Some(d), args } if args.len() == 1 => {
                tys.insert(*d, NTy::F64);
                line!("let mut {}: f64 = ({} as f64);", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::FloatUn(op), dst: Some(d), args } if args.len() == 1 => {
                render_float_un(op, d, args, &mut tys, &mut out, indent)?
            }
            Op::Prim { kind: crate::PrimKind::FloatCmp(op), dst: Some(d), args } if args.len() == 2 => {
                render_float_cmp(op, d, args, &mut tys, &mut out, indent)?
            }
            // Witness-level runtime calls (`println` lowers through these).
            Op::Call { dst, func, args, .. } => {
                render_call_witness(dst, func, args, &tys, &mut out, indent, used_shims)?
            }
            Op::IfThen { cond, dst } => render_if_then(cond, dst, &mut out, &mut indent, &mut if_stack),
            Op::Else { val } => render_else(val, &mut tys, &mut out, &mut indent, &if_stack)?,
            Op::EndIf { val } => render_end_if(val, &tys, &mut out, &mut indent, &mut if_stack)?,
            Op::LoopStart => {
                line!("loop {{");
                indent += 1;
            }
            Op::LoopBreakUnless { cond } => {
                line!("if {} == 0 {{ break; }}", var(*cond));
            }
            Op::LoopEnd => {
                indent -= 1;
                line!("}}");
            }
            other => {
                return Err(wall(format!(
                    "native: op {:?} — outside the rung subset",
                    op_name(other)
                )))
            }
        }
    }

    if !if_stack.is_empty() {
        return Err(wall("native: unbalanced IfThen/EndIf markers"));
    }

    // Signature: the return type is known only after the body typed `func.ret`.
    let mut sig = if is_main {
        if func.ret.is_some() {
            return Err(wall("native: main with a return value"));
        }
        String::from("fn main()")
    } else {
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                // The param's NTy was seeded from the SIG table above — read it back
                // so a list param renders `&[i64]` (repr alone cannot tell).
                let t = tys.get(&p.value).copied().unwrap_or(NTy::I64);
                let spelled = match t {
                    NTy::StrRef | NTy::Str => "&str",
                    NTy::VecRef | NTy::Vec => "&[i64]",
                    NTy::I64 => "i64",
                    NTy::F64 => "f64",
                };
                format!("{}: {}", var(p.value), spelled)
            })
            .collect();
        let ret = match func.ret {
            None => String::new(),
            Some(v) => match tys.get(&v) {
                Some(NTy::I64) => " -> i64".to_string(),
                Some(NTy::Str) => " -> String".to_string(),
                Some(NTy::StrRef) => " -> String".to_string(),
                Some(NTy::Vec) => " -> Vec<i64>".to_string(),
                Some(NTy::VecRef) => " -> Vec<i64>".to_string(),
                Some(NTy::F64) => " -> f64".to_string(),
                None => return Err(wall("native: return value untyped")),
            },
        };
        format!("fn {}({}){}", mangle(&func.name), params.join(", "), ret)
    };
    sig.push_str(" {\n");

    // The trailing return expression (moved out — fresh owned for heap).
    if let Some(v) = func.ret {
        let t = tys[&v];
        let expr = match t {
            NTy::I64 | NTy::F64 | NTy::Str | NTy::Vec => var(v),
            NTy::StrRef => format!("{}.to_string()", var(v)),
            NTy::VecRef => format!("{}.to_vec()", var(v)),
        };
        out.push_str("    ");
        out.push_str(&expr);
        out.push('\n');
    }
    out.push_str("}\n");
    let ret_nty = func.ret.map(|v| tys[&v]);
    Ok((format!("{sig}{out}"), ret_nty))
}

/// `Op::Dup` — mint a fresh handle from `src` per its NTy (verbatim arm body
/// extracted from [`render_fn`]; see that function for the op-loop context).
fn render_dup(
    dst: &ValueId,
    src: &ValueId,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
) -> Result<(), LowerError> {
    macro_rules! line {
        ($($arg:tt)*) => {{
            for _ in 0..indent { out.push_str("    "); }
            writeln!(out, $($arg)*).unwrap();
        }};
    }
    let t = *tys.get(src).ok_or_else(|| wall("native: Dup of untyped value"))?;
    match t {
        NTy::I64 => {
            tys.insert(*dst, NTy::I64);
            line!("let mut {} = {};", var(*dst), var(*src));
        }
        NTy::Str => {
            tys.insert(*dst, NTy::Str);
            line!("let mut {} = {}.clone();", var(*dst), var(*src));
        }
        NTy::StrRef => {
            // Dup of a borrowed param mints a fresh owned handle.
            tys.insert(*dst, NTy::Str);
            line!("let mut {} = {}.to_string();", var(*dst), var(*src));
        }
        NTy::Vec => {
            tys.insert(*dst, NTy::Vec);
            line!("let mut {} = {}.clone();", var(*dst), var(*src));
        }
        NTy::VecRef => {
            // Dup of a borrowed list param mints a fresh owned Vec.
            tys.insert(*dst, NTy::Vec);
            line!("let mut {} = {}.to_vec();", var(*dst), var(*src));
        }
        NTy::F64 => {
            tys.insert(*dst, NTy::F64);
            line!("let mut {} = {};", var(*dst), var(*src));
        }
    }
    Ok(())
}

include!("render_native_b.rs");
