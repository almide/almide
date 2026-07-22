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

/// The per-argument render for a `render_fn` call to a LOWERED USER callee — coerces `code`
/// (already rendered) from its actual repr `got` to the callee param's declared `want`, or
/// walls with a precise mismatch message. Verbatim extraction (guard-clause flattening) of
/// the former inline `match want { .. }` in `render_fn`'s callee-call arm, no behavior
/// change — see docs/roadmap/active/code-health-codopsy.md. NOT shared with
/// [`render_native_shim_call_arg`] despite the similar shape — the shim path has no
/// `VecRef`/`Vec` arm and uses different error text, so a merge would change behavior.
fn render_native_callee_call_arg(
    code: &str,
    got: NTy,
    want: NTy,
    name: &str,
) -> Result<String, LowerError> {
    match want {
        NTy::F64 => as_f64_arg(code, got),
        NTy::I64 => {
            if got != NTy::I64 {
                return Err(wall(format!("native: heap arg to scalar param of `{name}`")));
            }
            Ok(code.to_string())
        }
        NTy::VecRef | NTy::Vec => {
            if !got.is_veccy() {
                return Err(wall(format!("native: non-list arg to list param of `{name}`")));
            }
            Ok(match got {
                NTy::Vec => format!("&{code}"),
                _ => code.to_string(),
            })
        }
        _ => {
            if !got.is_stringy() {
                return Err(wall(format!("native: scalar arg to heap param of `{name}`")));
            }
            Ok(as_str_arg(code, got))
        }
    }
}

/// The per-argument render for a `render_fn` call to a runtime SHIM — see
/// [`render_native_callee_call_arg`]'s doc for why this is a separate function.
fn render_native_shim_call_arg(
    code: &str,
    got: NTy,
    want: NTy,
    name: &str,
) -> Result<String, LowerError> {
    match want {
        NTy::F64 => as_f64_arg(code, got),
        NTy::I64 => {
            if got != NTy::I64 {
                return Err(wall(format!("native: shim `{name}` arg type mismatch")));
            }
            Ok(code.to_string())
        }
        _ => {
            if !got.is_stringy() {
                return Err(wall(format!("native: shim `{name}` arg type mismatch")));
            }
            // Heap args are BORROWED at the MIR level — by reference.
            Ok(as_str_arg(code, got))
        }
    }
}

/// The Rust type name for a shim call's SCALAR/String result — extracted (not the depth
/// culprit itself, but the enclosing `render_fn` match/if nesting was) so the call site is a
/// flat expression instead of an inline `if`/`else` at an already-deep nesting level.
/// Verbatim logic, no behavior change — see docs/roadmap/active/code-health-codopsy.md.
fn shim_result_ty_name(t: NTy) -> &'static str {
    if t == NTy::Str {
        "String"
    } else {
        "i64"
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
    // Patch an if-join marker with its typed declaration.
    let patch = |out: &mut String, marker: &str, decl: &str| {
        *out = out.replacen(marker, decl, 1);
    };

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
            Op::Dup { dst, src } => {
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
            }
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
                let Some((CallArg::Handle(env), rest)) = args.split_first() else {
                    return Err(wall("native: CallIndirect without a leading env arg"));
                };
                let et = *tys
                    .get(env)
                    .ok_or_else(|| wall("native: CallIndirect env untyped"))?;
                if !et.is_veccy() {
                    return Err(wall("native: CallIndirect env is not a closure block"));
                }
                let env_code =
                    if et == NTy::Vec { format!("&{}", var(*env)) } else { var(*env) };
                let mut rendered = vec![var(*table_idx), env_code];
                for a in rest {
                    let (code, got) = call_arg(a, &tys)?;
                    if got != NTy::I64 {
                        return Err(wall(
                            "native: CallIndirect non-scalar user arg — outside the closures slab",
                        ));
                    }
                    rendered.push(code);
                }
                let call = format!("__almd_ci_{}({})", rest.len(), rendered.join(", "));
                match (dst, result) {
                    (Some(d), Some(Repr::Scalar { .. })) => {
                        tys.insert(*d, NTy::I64);
                        line!("let mut {}: i64 = {call};", var(*d));
                    }
                    (None, _) => line!("{call};"),
                    _ => {
                        return Err(wall(
                            "native: CallIndirect heap result — outside the closures slab",
                        ))
                    }
                }
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
                let lt = *tys.get(list).ok_or_else(|| wall("native: ListGet of untyped list"))?;
                if !lt.is_veccy() {
                    return Err(wall("native: ListGet on a non-list value"));
                }
                used_shims.push(IDX_GET_SHIM);
                tys.insert(*dst, NTy::I64);
                let borrow = if lt == NTy::Vec { "&" } else { "" };
                line!(
                    "let mut {}: i64 = almide_idx_get({borrow}{}, {});",
                    var(*dst),
                    var(*list),
                    var(*idx)
                );
            }
            Op::ListSetScalar { list, idx, val } => {
                let lt = *tys.get(list).ok_or_else(|| wall("native: ListSet of untyped list"))?;
                // A borrowed param cannot be mutated in place (the MIR COW discipline
                // guarantees a MakeUnique'd OWNED vec here; a VecRef reaching this op
                // is a call-mode violation, walled like the Drop-of-param case).
                if lt != NTy::Vec {
                    return Err(wall("native: ListSet on a non-owned list"));
                }
                used_shims.push(IDX_SET_SHIM);
                line!("almide_idx_set(&mut {}, {}, {});", var(*list), var(*idx), var(*val));
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
            Op::SetLocal { local, src } => {
                let t = *tys.get(src).ok_or_else(|| wall("native: SetLocal of untyped value"))?;
                let rhs = match t {
                    NTy::I64 | NTy::F64 => var(*src),
                    NTy::Str => format!("{}.clone()", var(*src)),
                    NTy::StrRef => format!("{}.to_string()", var(*src)),
                    NTy::Vec => format!("{}.clone()", var(*src)),
                    NTy::VecRef => format!("{}.to_vec()", var(*src)),
                };
                let store_t = match t {
                    NTy::StrRef => NTy::Str,
                    NTy::VecRef => NTy::Vec,
                    other => other,
                };
                if let Some(prev) = tys.get(local) {
                    if *prev != store_t {
                        return Err(wall("native: SetLocal changes a value's type"));
                    }
                    line!("{} = {};", var(*local), rhs);
                } else {
                    tys.insert(*local, store_t);
                    line!("let mut {} = {};", var(*local), rhs);
                }
            }
            Op::IntBinOp { dst, op, a, b } => {
                tys.insert(*dst, NTy::I64);
                let rendered = render_int_binop(op, *a, *b, used_shims)?;
                line!("let mut {}: i64 = {};", var(*dst), rendered);
            }
            Op::CallFn { dst, name, args, result } => {
                if let Some(callee) = user_fns.get(name.as_str()) {
                    if args.len() != callee.params.len() {
                        return Err(wall(format!("native: call to `{name}` arity mismatch")));
                    }
                    let callee_sig = sigs.get(name.as_str());
                    let mut rendered_args = Vec::new();
                    for (i, (a, p)) in args.iter().zip(&callee.params).enumerate() {
                        // The DECLARED kind (sig table) disambiguates a heap param:
                        // `&str` vs `&[i64]`; absent (a synthesized helper) the repr
                        // fallback keeps the string convention.
                        let want = match callee_sig.and_then(|(ps, _)| ps.get(i)) {
                            Some(NativeSigKind::I64) => NTy::I64,
                            Some(NativeSigKind::Str) => NTy::StrRef,
                            Some(NativeSigKind::ListI64) => NTy::VecRef,
                            Some(NativeSigKind::F64) => NTy::F64,
                            None => repr_nty(&p.repr, true)?,
                        };
                        let (code, got) = call_arg(a, &tys)?;
                        rendered_args.push(render_native_callee_call_arg(&code, got, want, name)?);
                    }
                    let call = format!("{}({})", mangle(name), rendered_args.join(", "));
                    match (dst, result) {
                        (Some(d), Some(r)) => {
                            // A heap result is FRESH OWNED (the callee moved it out).
                            // Its KIND comes from the callee's declared return.
                            let t = match callee_sig.and_then(|(_, r)| *r) {
                                Some(NativeSigKind::ListI64) => NTy::Vec,
                                Some(NativeSigKind::Str) => NTy::Str,
                                Some(NativeSigKind::I64) => NTy::I64,
                                Some(NativeSigKind::F64) => NTy::F64,
                                None => repr_nty(r, false)?,
                            };
                            tys.insert(*d, t);
                            let ty_name = match t {
                                NTy::Str => "String",
                                NTy::Vec => "Vec<i64>",
                                NTy::F64 => "f64",
                                _ => "i64",
                            };
                            line!("let mut {}: {} = {};", var(*d), ty_name, call);
                        }
                        (None, _) => line!("{call};"),
                        (Some(d), None) => {
                            // Result repr unknown: scalar by convention.
                            tys.insert(*d, NTy::I64);
                            line!("let mut {}: i64 = {};", var(*d), call);
                        }
                    }
                } else if let Some((param_tys, ret_ty, shim_src)) = shim(name) {
                    if args.len() != param_tys.len() {
                        return Err(wall(format!("native: shim `{name}` arity mismatch")));
                    }
                    let mut rendered_args = Vec::new();
                    for (a, want) in args.iter().zip(param_tys) {
                        let (code, got) = call_arg(a, &tys)?;
                        rendered_args.push(render_native_shim_call_arg(&code, got, *want, name)?);
                    }
                    used_shims.push(shim_src);
                    let call = format!("{}({})", shim_rust_name(name), rendered_args.join(", "));
                    match (dst, ret_ty) {
                        (Some(d), Some(t)) => {
                            tys.insert(*d, t);
                            line!("let mut {}: {} = {};", var(*d), shim_result_ty_name(t), call);
                        }
                        (None, _) => line!("{call};"),
                        (Some(_), None) => {
                            return Err(wall(format!("native: shim `{name}` has no result")))
                        }
                    }
                    let _ = result;
                } else {
                    return Err(wall(format!(
                        "native: call to `{name}` — not a lowered user fn and not in the \
                         native runtime floor"
                    )));
                }
            }
            // Rung-5 float floor: MIR floats are i64 BITS; native computes in real
            // f64. Every op below is IEEE-754-exact on both targets (hardware ops,
            // identical bit results), so byte-identity holds through
            // `float.to_string`. Min/Max/CopySign are excluded: Rust's `f64::min`
            // NaN semantics differ from wasm `f64.min` (they only occur inside
            // self-host bodies, which never render natively).
            Op::Prim { kind: crate::PrimKind::FloatBin(op), dst: Some(d), args } if args.len() == 2 => {
                use crate::FBinOp;
                let sym = match op {
                    FBinOp::Add => "+",
                    FBinOp::Sub => "-",
                    FBinOp::Mul => "*",
                    FBinOp::Div => "/",
                    FBinOp::Min | FBinOp::Max | FBinOp::CopySign => {
                        return Err(wall(format!(
                            "native: float op {op:?} — outside the rung subset (NaN semantics)"
                        )))
                    }
                };
                let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
                let b = as_f64_arg(&var(args[1]), *tys.get(&args[1]).ok_or_else(|| wall("native: float arg untyped"))?)?;
                tys.insert(*d, NTy::F64);
                line!("let mut {}: f64 = {a} {sym} {b};", var(*d));
            }
            // `float.from_int` — int (i64) to f64, carried per the float floor.
            Op::Prim { kind: crate::PrimKind::F64FromInt, dst: Some(d), args } if args.len() == 1 => {
                tys.insert(*d, NTy::F64);
                line!("let mut {}: f64 = ({} as f64);", var(*d), var(args[0]));
            }
            Op::Prim { kind: crate::PrimKind::FloatUn(op), dst: Some(d), args } if args.len() == 1 => {
                use crate::FUnOp;
                let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
                let expr = match op {
                    FUnOp::Neg => format!("-({a})"),
                    FUnOp::Abs => format!("({a}).abs()"),
                    FUnOp::Sqrt => format!("({a}).sqrt()"),
                    FUnOp::Floor => format!("({a}).floor()"),
                    FUnOp::Ceil => format!("({a}).ceil()"),
                };
                tys.insert(*d, NTy::F64);
                line!("let mut {}: f64 = {expr};", var(*d));
            }
            Op::Prim { kind: crate::PrimKind::FloatCmp(op), dst: Some(d), args } if args.len() == 2 => {
                use crate::FCmpOp;
                let sym = match op {
                    FCmpOp::Lt => "<",
                    FCmpOp::Le => "<=",
                    FCmpOp::Gt => ">",
                    FCmpOp::Ge => ">=",
                    FCmpOp::Eq => "==",
                    FCmpOp::Ne => "!=",
                };
                let a = as_f64_arg(&var(args[0]), *tys.get(&args[0]).ok_or_else(|| wall("native: float arg untyped"))?)?;
                let b = as_f64_arg(&var(args[1]), *tys.get(&args[1]).ok_or_else(|| wall("native: float arg untyped"))?)?;
                tys.insert(*d, NTy::I64);
                line!("let mut {}: i64 = ({a} {sym} {b}) as i64;", var(*d));
            }
            // Witness-level runtime calls (`println` lowers through these).
            Op::Call { dst, func, args, .. } => {
                use crate::RtFn;
                match (func, args.as_slice()) {
                    (RtFn::PrintStr, [a]) => {
                        let (code, t) = call_arg(a, &tys)?;
                        if !t.is_stringy() {
                            return Err(wall("native: print_str of a non-String"));
                        }
                        used_shims.push(shim("print_str").unwrap().2);
                        line!("rt_print_str({});", as_str_arg(&code, t));
                    }
                    (RtFn::PrintInt, [a]) => {
                        let (code, t) = call_arg(a, &tys)?;
                        if t != NTy::I64 {
                            return Err(wall("native: print_int of a non-Int"));
                        }
                        line!("println!(\"{{}}\", {code});");
                    }
                    other => {
                        return Err(wall(format!(
                            "native: runtime call {other:?} — outside the rung subset"
                        )))
                    }
                }
                if dst.is_some() {
                    return Err(wall("native: print with a result — outside the rung subset"));
                }
            }
            Op::IfThen { cond, dst } => {
                if let Some(d) = dst {
                    // if-as-value: the join decl is patched in at the first arm
                    // yield, when its type is known.
                    let marker = format!("//__JOIN_{}__", var(*d));
                    line!("{marker}");
                    if_stack.push(Some((marker, *d)));
                } else {
                    if_stack.push(None);
                }
                line!("if {} != 0 {{", var(*cond));
                indent += 1;
            }
            Op::Else { val } => {
                if let Some(Some((marker, d))) = if_stack.last() {
                    let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
                    let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
                    let (decl, join_t, rhs) = match t {
                        NTy::I64 => (
                            format!("let mut {}: i64 = 0;", var(*d)),
                            NTy::I64,
                            var(v),
                        ),
                        NTy::Str => (
                            format!("let mut {}: String = String::new();", var(*d)),
                            NTy::Str,
                            format!("{}.clone()", var(v)),
                        ),
                        NTy::StrRef => (
                            format!("let mut {}: String = String::new();", var(*d)),
                            NTy::Str,
                            format!("{}.to_string()", var(v)),
                        ),
                        NTy::Vec => (
                            format!("let mut {}: Vec<i64> = Vec::new();", var(*d)),
                            NTy::Vec,
                            format!("{}.clone()", var(v)),
                        ),
                        NTy::VecRef => (
                            format!("let mut {}: Vec<i64> = Vec::new();", var(*d)),
                            NTy::Vec,
                            format!("{}.to_vec()", var(v)),
                        ),
                        NTy::F64 => (
                            format!("let mut {}: f64 = 0.0;", var(*d)),
                            NTy::F64,
                            var(v),
                        ),
                    };
                    patch(&mut out, marker, &decl);
                    tys.insert(*d, join_t);
                    line!("{} = {};", var(*d), rhs);
                }
                indent -= 1;
                line!("}} else {{");
                indent += 1;
            }
            Op::EndIf { val } => {
                let top = if_stack.pop().ok_or_else(|| wall("native: EndIf without IfThen"))?;
                if let Some((_, d)) = top {
                    let v = val.ok_or_else(|| wall("native: if-value arm without a yield"))?;
                    let t = *tys.get(&v).ok_or_else(|| wall("native: if-value yield untyped"))?;
                    let join_t = *tys.get(&d).ok_or_else(|| wall("native: if-value join untyped"))?;
                    let rhs = match t {
                        NTy::I64 | NTy::F64 => var(v),
                        NTy::Str => format!("{}.clone()", var(v)),
                        NTy::StrRef => format!("{}.to_string()", var(v)),
                        NTy::Vec => format!("{}.clone()", var(v)),
                        NTy::VecRef => format!("{}.to_vec()", var(v)),
                    };
                    let arm_t = match t {
                        NTy::StrRef => NTy::Str,
                        NTy::VecRef => NTy::Vec,
                        other => other,
                    };
                    if arm_t != join_t {
                        return Err(wall("native: if-value arms disagree on type"));
                    }
                    line!("{} = {};", var(d), rhs);
                }
                indent -= 1;
                line!("}}");
            }
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

/// A rendered call argument with the NTy it carries (`Imm` is always i64).
fn call_arg(a: &CallArg, tys: &BTreeMap<ValueId, NTy>) -> Result<(String, NTy), LowerError> {
    match a {
        CallArg::Handle(v) | CallArg::Scalar(v) => {
            let t = *tys.get(v).ok_or_else(|| wall("native: call arg untyped"))?;
            Ok((var(*v), t))
        }
        CallArg::Imm(n) => Ok((format!("{n}i64"), NTy::I64)),
        other => Err(wall(format!("native: call arg {other:?} — outside the rung subset"))),
    }
}

fn render_int_binop(
    op: &IntOp,
    a: ValueId,
    b: ValueId,
    used_shims: &mut Vec<&'static str>,
) -> Result<String, LowerError> {
    let (l, r) = (var(a), var(b));
    Ok(match op {
        IntOp::Add => format!("{l}.wrapping_add({r})"),
        IntOp::Sub => format!("{l}.wrapping_sub({r})"),
        IntOp::Mul => format!("{l}.wrapping_mul({r})"),
        // Div/Mod carry the C-001/C-002 abort discipline — route through the
        // same checked shims the CallFn path uses (one definition of the abort).
        IntOp::Div => {
            used_shims.push(shim("__chk_div").unwrap().2);
            format!("rt_chk_div({l}, {r})")
        }
        IntOp::Mod => {
            used_shims.push(shim("__chk_mod").unwrap().2);
            format!("rt_chk_mod({l}, {r})")
        }
        IntOp::Eq => format!("({l} == {r}) as i64"),
        IntOp::Ne => format!("({l} != {r}) as i64"),
        IntOp::Lt => format!("({l} < {r}) as i64"),
        IntOp::Le => format!("({l} <= {r}) as i64"),
        IntOp::Gt => format!("({l} > {r}) as i64"),
        IntOp::Ge => format!("({l} >= {r}) as i64"),
        other => return Err(wall(format!("native: int op {other:?} — outside the rung subset"))),
    })
}

/// A user fn name that is a valid Rust identifier (dots from module paths).
fn mangle(name: &str) -> String {
    format!("almd_{}", name.replace(['.', '$'], "_"))
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Alloc { .. } => "Alloc",
        Op::Const { .. } => "Const",
        Op::ConstInt { .. } => "ConstInt",
        Op::Dup { .. } => "Dup",
        Op::Drop { .. } => "Drop",
        Op::DropListStr { .. } => "DropListStr",
        Op::Consume { .. } => "Consume",
        Op::Borrow { .. } => "Borrow",
        Op::MakeUnique { .. } => "MakeUnique",
        Op::Pure { .. } => "Pure",
        Op::Call { .. } => "Call",
        Op::CallFn { .. } => "CallFn",
        Op::CallImport { .. } => "CallImport",
        Op::CallIndirect { .. } => "CallIndirect",
        Op::FuncRef { .. } => "FuncRef",
        Op::IntBinOp { .. } => "IntBinOp",
        Op::Prim { .. } => "Prim",
        Op::IfThen { .. } => "IfThen",
        Op::Else { .. } => "Else",
        Op::EndIf { .. } => "EndIf",
        Op::LoopStart => "LoopStart",
        Op::LoopBreakUnless { .. } => "LoopBreakUnless",
        Op::LoopEnd => "LoopEnd",
        Op::SetLocal { .. } => "SetLocal",
        _ => "unknown",
    }
}
