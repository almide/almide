//! MIR → Rust renderer (the first faithful renderer, §3).
//!
//! It TRANSLATES the MIR ownership+layout decision; it never re-decides it
//! (§3.2 faithful-renderer contract). The mapping is exactly the §2.2 right
//! column:
//!
//!   Dup        → `let dst = src.clone()`   (a new owned handle; the clone IS
//!                                            the eager copy-on-write)
//!   Drop       → nothing (Rust drops at scope end)
//!   Consume    → a move (nothing emitted)
//!   Borrow     → `&v`
//!   MakeUnique → nothing (the Dup clone already made the handle unique — the
//!                idiomatic-Rust spelling of COW)
//!
//! The renderer does NOT call `is_heap`/last-use/etc. — it reads the MIR op and
//! spells it. For the value-semantics subset every heap value is a `Vec<i64>`.
//!
//! This is what makes the v1 bet observable: a value-semantics MIR renders to a
//! Rust program whose RUNTIME output exhibits correct value semantics (an
//! aliased binding is unchanged by a mutation through its sibling) — by
//! construction, because the one `Dup` decision became a `.clone()`.

use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, Repr, RtFn, ValueId};

/// Render a whole MIR program (its functions + `main`) to a runnable Rust source.
pub fn render_rust_program(prog: &MirProgram) -> String {
    prog.functions.iter().map(render_rust_fn).collect::<Vec<_>>().join("\n")
}

/// Render one MIR function with its real signature (params, return). A function
/// named `main` with no params/return is the entry point.
pub fn render_rust_fn(func: &MirFunction) -> String {
    let reprs = value_reprs(func);
    let params = func
        .params
        .iter()
        .map(|p| format!("{}: {}", var(p.value), rust_ty(p.repr)))
        .collect::<Vec<_>>()
        .join(", ");
    let ret_sig = match func.ret {
        Some(r) => format!(" -> {}", rust_ty(reprs.get(&r).copied().unwrap_or(SCALAR))),
        None => String::new(),
    };
    let mut body = String::new();
    for op in &func.ops {
        if let Some(line) = render_op(op) {
            body.push_str("    ");
            body.push_str(&line);
            body.push('\n');
        }
    }
    if let Some(r) = func.ret {
        body.push_str(&format!("    {}\n", var(r))); // tail = the moved-out return
    }
    format!("fn {}({params}){ret_sig} {{\n{body}}}\n", func.name)
}

/// Render a single function as a `fn main()` program (compat for the
/// no-param/no-return value-semantics tests).
pub fn render_rust(func: &MirFunction) -> String {
    let mut body = String::new();
    for op in &func.ops {
        if let Some(line) = render_op(op) {
            body.push_str("    ");
            body.push_str(&line);
            body.push('\n');
        }
    }
    format!("fn main() {{\n{body}}}\n")
}

/// The canonical scalar repr (i64) for inferred scalar results.
const SCALAR: Repr = Repr::Scalar { width: crate::ScalarWidth::Double };

fn rust_ty(repr: Repr) -> &'static str {
    if repr.is_heap() {
        "Vec<i64>"
    } else {
        "i64"
    }
}

/// Infer each value's Repr within a function (params + op results) so types can
/// be rendered. Heap from Alloc/Dup; scalar from Const/IntBinOp/CallFn.
fn value_reprs(func: &MirFunction) -> std::collections::BTreeMap<ValueId, Repr> {
    let mut m = std::collections::BTreeMap::new();
    for p in &func.params {
        m.insert(p.value, p.repr);
    }
    for op in &func.ops {
        match op {
            Op::Alloc { dst, repr, .. } => {
                m.insert(*dst, *repr);
            }
            Op::Dup { dst, src } => {
                let r = m.get(src).copied().unwrap_or(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
                m.insert(*dst, r);
            }
            Op::Const { dst } | Op::ConstInt { dst, .. } | Op::IntBinOp { dst, .. } => {
                m.insert(*dst, SCALAR);
            }
            Op::Prim { dst: Some(dst), .. } => {
                m.insert(*dst, SCALAR);
            }
            Op::CallFn { dst: Some(d), .. } => {
                m.insert(*d, SCALAR); // demo: scalar-returning; heap returns are a later refinement
            }
            _ => {}
        }
    }
    m
}

fn var(v: ValueId) -> String {
    format!("v{}", v.0)
}

/// Render one op. `None` for ops that produce no Rust (Drop = scope-end,
/// Consume = move, MakeUnique = already-unique by the Dup clone).
fn render_op(op: &Op) -> Option<String> {
    match op {
        Op::Alloc { dst, repr, init } => {
            debug_assert!(matches!(repr, Repr::Ptr { .. } | Repr::Boxed { .. }));
            let init_expr = match init {
                Init::IntList(elems) => {
                    let items =
                        elems.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
                    format!("vec![{items}]")
                }
                // A string literal's UTF-8 bytes (the wasm string is a byte block; the
                // native String render is a later refinement — this keeps the type a
                // `Vec<i64>` and reproduces the real DATA as bytes).
                Init::Str(s) => {
                    let items =
                        s.as_bytes().iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
                    format!("vec![{items}]")
                }
                // A Bytes constant — the same byte-block render as a string literal (wasm-only
                // in practice; native uses v0 codegen). Reproduce the raw bytes as a Vec.
                Init::Bytes(data) => {
                    let items =
                        data.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
                    format!("vec![{items}]")
                }
                // A runtime-sized String is wasm-only (native uses v0 codegen); an empty
                // placeholder keeps the type a Vec<i64>.
                Init::Opaque | Init::DynStr { .. } => "Vec::new()".to_string(),
                // A materialized `Some(payload)` / runtime List are wasm-only (native uses
                // v0 codegen).
                Init::OptSome { .. } | Init::OptNone | Init::DynList { .. } | Init::DynListStr { .. } => {
                    "Vec::new()".to_string()
                }
            };
            Some(format!("let mut {}: Vec<i64> = {init_expr};", var(*dst)))
        }
        Op::Const { dst } => Some(format!("let {}: i64 = 0;", var(*dst))),
        Op::ConstInt { dst, value } => Some(format!("let {}: i64 = {value};", var(*dst))),
        // The rung-4 list ops in the dev Rust renderer: the natural Vec spellings.
        Op::ListLit { dst, elems } => {
            let items = elems.iter().map(|e| var(*e)).collect::<Vec<_>>().join(", ");
            Some(format!("let mut {}: Vec<i64> = vec![{items}];", var(*dst)))
        }
        Op::ListGetScalar { dst, list, idx } => Some(format!(
            "let {}: i64 = {}[{} as usize];",
            var(*dst),
            var(*list),
            var(*idx)
        )),
        Op::ListSetScalar { list, idx, val } => Some(format!(
            "{}[{} as usize] = {};",
            var(*list),
            var(*idx),
            var(*val)
        )),
        // The prim floor is the WASM self-host surface; native uses v0's runtime, so a
        // prim op never appears in a native MIR. Stub to keep the match total.
        Op::Prim { dst: Some(d), .. } => Some(format!("let {}: i64 = 0;", var(*d))),
        Op::Prim { dst: None, .. } => None,
        // The wasm-structured if- and loop-markers are not used in the native render
        // (native uses idiomatic Rust control flow via v0 codegen, not the marker stream).
        Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } => None,
        Op::LoopStart | Op::LoopBreakUnless { .. } | Op::LoopEnd => None,
        Op::SetLocal { local, src } => Some(format!("{} = {};", var(*local), var(*src))),
        // The single ownership decision: an alias is a fresh owned handle whose
        // value is a clone — eager copy-on-write, so the sibling is independent.
        Op::Dup { dst, src } => {
            Some(format!("let mut {}: Vec<i64> = {}.clone();", var(*dst), var(*src)))
        }
        Op::Drop { .. } | Op::DropListStr { .. } | Op::DropValue { .. } | Op::DropListValue { .. } | Op::DropListStrValue { .. } | Op::DropListStrStr { .. } | Op::DropListIntStr { .. } | Op::DropResultListValue { .. } | Op::DropResultValue { .. } | Op::DropResultStrInt { .. } | Op::DropResultValueInt { .. } | Op::DropResultListValueInt { .. } | Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. } | Op::DropListListStr { .. } | Op::DropVariant { .. } | Op::DropWrapperRec { .. } => None, // Rust drops at scope end (wasm-only)
Op::DropListStrInt { .. } | Op::DropResultListValue { .. } | Op::DropResultValue { .. } | Op::DropResultStrInt { .. } | Op::DropResultValueInt { .. } | Op::DropResultListValueInt { .. } | Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. } | Op::DropListListStr { .. } | Op::DropVariant { .. } | Op::DropWrapperRec { .. } => None, // Rust drops at scope end (wasm-only)
        Op::Consume { .. } => None, // a move
        Op::Borrow { v } => Some(format!("let _ = &{};", var(*v))),
        // MakeUnique is a no-op in idiomatic Rust: the Dup clone already gave
        // this handle a uniquely-owned buffer (the COW spelling).
        Op::MakeUnique { .. } => None,
        Op::Pure { dst, .. } => Some(format!("let {}: i64 = 0;", var(*dst))),
        // Runtime calls are spelled as the idiomatic Rust operation (the bootstrap
        // runtime; ultimately these are calls to self-hosted Almide functions).
        Op::Call { func, args, .. } => Some(render_call(func, args)),
        // CallIndirect is wasm-only (native uses v0 codegen) and unwired (no lowering emits
        // it yet) — emit nothing. FuncRef (a closure's table-slot value) is likewise.
        // CallImport (an `@extern(wasm, …)` host import) is BROWSER-only: it has no native
        // host, so the native MIR render emits nothing (these fns are 🟡 wasm-only).
        Op::CallIndirect { .. } | Op::FuncRef { .. } | Op::CallImport { .. } => None,
        Op::IntBinOp { dst, op, a, b } => {
            let (a, b, d) = (var(*a), var(*b), var(*dst));
            // A comparison yields a `bool` → cast to the i64 scalar model (0/1).
            let rhs = match op {
                IntOp::Add => format!("{a} + {b}"),
                IntOp::Sub => format!("{a} - {b}"),
                IntOp::Mul => format!("{a} * {b}"),
                IntOp::Div => format!("{a} / {b}"),
                IntOp::Mod => format!("{a} % {b}"),
                IntOp::Lt => format!("({a} < {b}) as i64"),
                IntOp::Le => format!("({a} <= {b}) as i64"),
                IntOp::Gt => format!("({a} > {b}) as i64"),
                IntOp::Ge => format!("({a} >= {b}) as i64"),
                IntOp::Eq => format!("({a} == {b}) as i64"),
                IntOp::Ne => format!("({a} != {b}) as i64"),
                IntOp::And => format!("{a} & {b}"),
                IntOp::Or => format!("{a} | {b}"),
                IntOp::Xor => format!("{a} ^ {b}"),
                IntOp::Shl => format!("{a} << {b}"),
                IntOp::Shr => format!("{a} >> {b}"),
                IntOp::ShrU => format!("(({a} as u64) >> {b}) as i64"),
            };
            Some(format!("let {d}: i64 = {rhs};"))
        }
        Op::CallFn { dst, name, args, .. } => {
            let a = args.iter().map(render_arg).collect::<Vec<_>>().join(", ");
            Some(match dst {
                Some(d) => format!("let {} = {name}({a});", var(*d)),
                None => format!("{name}({a});"),
            })
        }
    }
}

fn render_arg(arg: &CallArg) -> String {
    match arg {
        CallArg::Handle(v) | CallArg::Scalar(v) => var(*v),
        CallArg::Imm(n) => n.to_string(),
        CallArg::Label(l) => format!("{l:?}"),
    }
}

fn render_call(func: &RtFn, args: &[CallArg]) -> String {
    match (func, args) {
        (RtFn::ListSet, [CallArg::Handle(t), CallArg::Imm(idx), CallArg::Imm(val)]) => {
            format!("{}[{idx}] = {val};", var(*t))
        }
        (RtFn::ListPush, [CallArg::Handle(t), CallArg::Imm(val)]) => {
            format!("{}.push({val});", var(*t))
        }
        (RtFn::PrintList, [CallArg::Handle(v), CallArg::Label(label)]) => format!(
            "println!(\"{{}}={{}}\", {:?}, {}.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(\",\"));",
            label,
            var(*v)
        ),
        (RtFn::PrintInt, [CallArg::Scalar(v)]) => {
            format!("println!(\"{{}}\", {});", var(*v))
        }
        _ => panic!("malformed runtime call {func:?} with args {args:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, MirParam, MirProgram, ScalarWidth, PLACEHOLDER_LAYOUT};
    use std::process::Command;

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }

    fn scalar() -> Repr {
        Repr::Scalar { width: ScalarWidth::Double }
    }

    /// A program with a user function: `fn add(a,b)=a+b` and a `main` that calls
    /// it and prints the result. The runtime is, in the end, written this way —
    /// as Almide functions lowered to MIR and called via `CallFn`.
    fn add_program() -> MirProgram {
        let add = MirFunction {
            name: "add".into(),
            params: vec![
                MirParam { value: ValueId(0), repr: scalar() },
                MirParam { value: ValueId(1), repr: scalar() },
            ],
            ops: vec![Op::IntBinOp {
                dst: ValueId(2),
                op: IntOp::Add,
                a: ValueId(0),
                b: ValueId(1),
            }],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "add".into(),
                    args: vec![CallArg::Imm(2), CallArg::Imm(3)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![add, main], exports: vec![], mutable_global_count: 0 }
    }

    #[test]
    fn function_call_lowers_and_runs_on_rust() {
        let prog = add_program();
        for f in &prog.functions {
            assert_eq!(verify_ownership(f), Ok(()), "{} verifies", f.name);
        }
        let out = compile_and_run("fncall", &render_rust_program(&prog));
        assert_eq!(out, "5");
    }

    /// Compile a Rust source string and run it; return trimmed stdout. `label`
    /// gives each test its own scratch dir so parallel tests don't clobber one
    /// another's binary.
    fn compile_and_run(label: &str, src: &str) -> String {
        let dir = std::env::temp_dir().join(format!("almide_mir_render_{label}"));
        std::fs::create_dir_all(&dir).expect("failed to create the test scratch dir");
        let src_path = dir.join("m.rs");
        let bin_path = dir.join("m");
        std::fs::write(&src_path, src).expect("failed to write the test scratch source file");
        let build = Command::new("rustc")
            .args(["--edition", "2021", "-O"])
            .arg(&src_path)
            .arg("-o")
            .arg(&bin_path)
            .output()
            .expect("run rustc");
        assert!(
            build.status.success(),
            "rustc failed:\n{}\n--- source ---\n{src}",
            String::from_utf8_lossy(&build.stderr)
        );
        let run = Command::new(&bin_path).output().expect("run binary");
        String::from_utf8_lossy(&run.stdout).trim().to_string()
    }

    #[test]
    fn value_semantics_renders_to_correct_running_rust() {
        // var a = [1,2,3]; var b = a; a[0] = 9; print a; print b
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a }, // alias = clone (eager COW)
                Op::MakeUnique { v: a },    // no-op in idiomatic Rust
                Op::Call {
                    dst: None,
                    func: RtFn::ListSet,
                    args: vec![CallArg::Handle(a), CallArg::Imm(0), CallArg::Imm(9)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        // The MIR is ownership-balanced by construction…
        assert_eq!(verify_ownership(&mir), Ok(()));
        // …and the rendered Rust runs with CORRECT value semantics: mutating `a`
        // leaves the aliased `b` unchanged, because the one Dup decision became a
        // clone. The renderer never re-decided ownership.
        let out = compile_and_run("valuesem", &render_rust(&mir));
        assert_eq!(out, "a=9,2,3\nb=1,2,3");
    }

    #[test]
    fn push_through_alias_keeps_sibling_independent() {
        // var a = [1]; var b = a; a.push(2); print a; print b  → a=[1,2], b=[1]
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Call {
                    dst: Some(a),
                    func: RtFn::ListPush,
                    args: vec![CallArg::Handle(a), CallArg::Imm(2)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        assert_eq!(verify_ownership(&mir), Ok(()));
        let out = compile_and_run("push", &render_rust(&mir));
        assert_eq!(out, "a=1,2\nb=1");
    }
}
