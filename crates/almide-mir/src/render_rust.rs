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

use crate::{Init, MirFunction, Op, Repr, ValueId};

/// Render a MIR function to a runnable Rust `fn main()` source string.
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
                Init::Opaque => "Vec::new()".to_string(),
            };
            Some(format!("let mut {}: Vec<i64> = {init_expr};", var(*dst)))
        }
        Op::Const { dst } => Some(format!("let {}: i64 = 0;", var(*dst))),
        // The single ownership decision: an alias is a fresh owned handle whose
        // value is a clone — eager copy-on-write, so the sibling is independent.
        Op::Dup { dst, src } => {
            Some(format!("let mut {}: Vec<i64> = {}.clone();", var(*dst), var(*src)))
        }
        Op::Drop { .. } => None,    // Rust drops at scope end
        Op::Consume { .. } => None, // a move
        Op::Borrow { v } => Some(format!("let _ = &{};", var(*v))),
        // MakeUnique is a no-op in idiomatic Rust: the Dup clone already gave
        // this handle a uniquely-owned buffer (the COW spelling).
        Op::MakeUnique { .. } => None,
        Op::Pure { dst, .. } => Some(format!("let {}: i64 = 0;", var(*dst))),
        Op::IndexSet { target, index, value } => {
            Some(format!("{}[{index}] = {value};", var(*target)))
        }
        Op::Push { target, value } => Some(format!("{}.push({value});", var(*target))),
        Op::Print { value, label } => Some(format!(
            "println!(\"{{}}={{}}\", {:?}, {}.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(\",\"));",
            label,
            var(*value)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, LayoutId};
    use std::process::Command;

    fn heap() -> Repr {
        Repr::Ptr { layout: LayoutId(0) }
    }

    /// Compile a Rust source string and run it; return trimmed stdout. `label`
    /// gives each test its own scratch dir so parallel tests don't clobber one
    /// another's binary.
    fn compile_and_run(label: &str, src: &str) -> String {
        let dir = std::env::temp_dir().join(format!("almide_mir_render_{label}"));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("m.rs");
        let bin_path = dir.join("m");
        std::fs::write(&src_path, src).unwrap();
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
                Op::IndexSet { target: a, index: 0, value: 9 },
                Op::Print { value: a, label: "a".into() },
                Op::Print { value: b, label: "b".into() },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
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
                Op::Push { target: a, value: 2 },
                Op::Print { value: a, label: "a".into() },
                Op::Print { value: b, label: "b".into() },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
        };
        assert_eq!(verify_ownership(&mir), Ok(()));
        let out = compile_and_run("push", &render_rust(&mir));
        assert_eq!(out, "a=1,2\nb=1");
    }
}
