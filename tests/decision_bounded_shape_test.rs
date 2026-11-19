//! bounded.shape-rules (#4855): the @bounded allocation (E071), sized
//! construction (E075) and Float computation (E077) rules judge a stdlib
//! member from its types, so a member no table names is judged too. Each case
//! is a fresh @bounded fn calling a member the old name tables never listed;
//! the control is the same fn shape on Int-only members, which passes.
use std::process::Command;

#[path = "support/novel.rs"]
mod novel;

fn check(prog: &str) -> (bool, String) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, prog).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&f).current_dir(td.path()).output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

#[test]
fn novel_unlisted_members_are_judged_by_their_types() {
    let g = novel::ident("shape");
    let cases = [
        // a String built per iteration of a counted loop
        ("E071", format!("@bounded\nfn {g}() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + string.len(int.to_string(i)) }}\n  n\n}}\n\nfn main() -> Unit = println(\"${{{g}()}}\")\n")),
        // a List whose length is a run-time Int
        ("E075", format!("@bounded\nfn {g}(n: Int) -> Int = list.len(list.range_step(0, n, 1))\n\nfn main() -> Unit = println(\"${{{g}(3)}}\")\n")),
        // a Float computed by a member call
        ("E077", format!("@bounded\nfn {g}(x: Float) -> Float = float.abs(x)\n\nfn main() -> Unit = println(float.to_string({g}(1.5)))\n")),
    ];
    for (code, prog) in &cases {
        let (ok, out) = check(prog);
        assert!(!ok && out.contains(code), "an unlisted member was admitted ({code} expected):\n{prog}\n{out}");
    }
    let control = format!("@bounded\nfn {g}(n: Int) -> Int = {{\n  var t = 0\n  for i in 0..<4 {{ t = t + int.abs(i - n) }}\n  t\n}}\n\nfn main() -> Unit = println(\"${{{g}(2)}}\")\n");
    let (ok, out) = check(&control);
    assert!(ok, "control: Int-only members in a counted loop pass:\n{control}\n{out}");
    // #4871: the same rules reach through a callee
    novel_a_callee_allocation_is_followed_by_reach();
    // ADR-0038: an output line is judged by its hole types
    novel_an_output_line_of_scalars_is_admitted_by_its_hole_types();
}

/// #4871: E071 follows a callee by reach. A fresh @bounded helper that
/// allocates (a list literal, an interpolated String, or through a second
/// fresh helper) and is called inside a counted loop is refused, under
/// `@bounded` and under `--profile critical`. The controls: an Int-only
/// helper in the loop, and the allocating helper called outside the loop.
#[test]
fn novel_a_callee_allocation_is_followed_by_reach() {
    let g = novel::ident("alloc");
    let h = novel::ident("relay");
    let caller = |callee: &str| format!("@bounded\nfn drive() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + {callee}(i) }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let cases = [
        // the helper builds a List
        format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i, i])\n\n{}", caller(&g)),
        // the helper builds a String
        format!("@bounded\nfn {g}(i: Int) -> Int = string.len(\"${{i}}!\")\n\n{}", caller(&g)),
        // two hops: the loop calls h, h calls the allocating g
        format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i])\n\n@bounded\nfn {h}(i: Int) -> Int = {g}(i) + 1\n\n{}", caller(&h)),
    ];
    for prog in &cases {
        let (ok, out) = check(prog);
        assert!(!ok && out.contains("E071"), "an allocating callee in a counted loop was admitted:\n{prog}\n{out}");
    }
    // --profile critical: the same shape with no attribute
    let crit = format!("fn {g}(i: Int) -> Int = list.len([i, i])\n\nfn drive() -> Int = {{\n  var n = 0\n  for i in 0..<4 {{ n = n + {g}(i) }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let (ok, out) = check_critical(&crit);
    assert!(!ok && out.contains("E071"), "critical admitted an allocating callee in a counted loop:\n{crit}\n{out}");
    // controls
    let int_only = format!("@bounded\nfn {g}(i: Int) -> Int = int.abs(i - 2) * 3\n\n{}", caller(&g));
    let (ok, out) = check(&int_only);
    assert!(ok, "control: an Int-only callee in a counted loop passes:\n{int_only}\n{out}");
    let outside = format!("@bounded\nfn {g}(i: Int) -> Int = list.len([i, i])\n\n@bounded\nfn drive() -> Int = {{\n  let base = {g}(1)\n  var n = base\n  for i in 0..<4 {{ n = n + i }}\n  n\n}}\n\neffect fn main() -> Unit = println(\"${{drive()}}\")\n");
    let (ok, out) = check(&outside);
    assert!(ok, "control: an allocating callee outside the loop passes:\n{outside}\n{out}");
}

fn check_critical(prog: &str) -> (bool, String) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, prog).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&f).args(["--profile", "critical"]).current_dir(td.path()).output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

/// The verdict of one program under `@bounded` (as written) and under
/// `--profile critical` with the attribute removed.
fn both(prog: &str) -> [(bool, String); 2] {
    [check(prog), check_critical(&prog.replace("@bounded\n", ""))]
}

/// ADR-0038 (bounded.output-line): an OUTPUT LINE — `println` / `eprintln`
/// whose single argument is an interpolated string, every hole's type having
/// a direct sink writer — builds no String, so E071 admits it inside a
/// counted loop; decided from position and types, never from a name.
///
/// Novel items: a fresh fn and fresh user types; and every builtin type head
/// the resolver's registry declares, enumerated at run time, so a type added
/// later is covered without editing this test. A head whose type is a Copy
/// scalar with a value text (not heap, not Unit / Never / Handle) must be
/// admitted as a hole, except the heads whose sink writer does not exist yet
/// (stage 1 of the ADR: the Floats, whose wasm printer still returns a String
/// block, and UInt64, whose native text is still a String); every other head
/// must be refused, String included. The refusals: a record hole, a variant
/// hole, a list hole, the same interpolation bound with `let`,
/// `println(int.to_string(i))`, and a String hole.
#[test]
fn novel_an_output_line_of_scalars_is_admitted_by_its_hole_types() {
    let g = novel::ident("line");
    let rec = format!("R{}", &novel::ident("t")[2..]);
    let var = format!("V{}", &novel::ident("t")[2..]);
    // admitted: Int, sized-int and Bool holes in a counted loop of a fresh fn
    let admitted = format!(
        "@bounded\nfn {g}(x: Int32, b: Bool) -> Unit = {{\n  for i in 0..<4 {{\n    println(\"t=${{i}} v=${{x}} ok=${{b}}\")\n    eprintln(\"${{i}}\")\n  }}\n}}\n\nfn main() -> Unit = {g}(int32.max_value(), true)\n"
    );
    for (ok, out) in both(&admitted) {
        assert!(ok, "an output line of Int, Int32 and Bool holes was refused:\n{admitted}\n{out}");
    }
    // every registry head: admitted iff it is a Copy scalar with a value text
    use almide_frontend::canonicalize::resolve::{BuiltinArity, BUILTIN_TYPE_HEADS};
    use almide_types::types::Ty;
    let mut scalars = 0;
    for head in BUILTIN_TYPE_HEADS.iter().filter(|h| matches!(h.arity, BuiltinArity::Bare)) {
        let ty = (head.build)(&[]);
        let scalar = !almide_types::types::is_heap_ty(&ty) && !matches!(ty, Ty::Unit | Ty::Never | Ty::RawPtr);
        // ADR-0038 stage 1: no allocation-free writer on every target yet
        let deferred = matches!(ty, Ty::Float | Ty::Float64 | Ty::Float32 | Ty::UInt64);
        let scalar = scalar && !deferred;
        let prog = format!(
            "@bounded\nfn {g}(x: {}) -> Unit = {{\n  for i in 0..<4 {{\n    println(\"t=${{i}} v=${{x}}\")\n  }}\n}}\n\nfn main() -> Unit = println(\"x\")\n",
            head.name
        );
        for (ok, out) in both(&prog) {
            if scalar {
                assert!(ok, "a `{}` hole (a Copy scalar) was refused in an output line:\n{prog}\n{out}", head.name);
            } else {
                assert!(!ok, "a `{}` hole (no direct sink writer) was admitted in an output line:\n{prog}\n{out}", head.name);
            }
        }
        scalars += usize::from(scalar);
    }
    assert!(scalars >= 9, "the registry enumeration found only {scalars} admissible scalar heads");
    // refused: no direct sink writer, or not an output line
    let in_loop = |sig: &str, call: &str, stmt: &str, pre: &str| {
        format!("{pre}@bounded\nfn {g}({sig}) -> Unit = {{\n  for i in 0..<4 {{\n    {stmt}\n  }}\n}}\n\nfn main() -> Unit = {g}({call})\n")
    };
    let refused = [
        in_loop(&format!("p: {rec}"), &format!("{rec} {{ a: 1, b: 2 }}"), "println(\"t=${i} ${p}\")", &format!("type {rec} = {{ a: Int, b: Int }}\n\n")),
        in_loop(&format!("v: {var}"), "On", "println(\"t=${i} ${v}\")", &format!("type {var} =\n  | On\n  | Off\n\n")),
        in_loop("xs: List[Int]", "[1, 2]", "println(\"t=${i} ${xs}\")", ""),
        in_loop("n: Int", "3", "let s = \"${i} ${n}\"\n    println(s)", ""),
        in_loop("n: Int", "3", "println(int.to_string(i + n))", ""),
        in_loop("s: String", "\"a\"", "println(\"t=${i} ${s}\")", ""),
    ];
    for prog in &refused {
        for (ok, out) in both(prog) {
            assert!(!ok && out.contains("E071"), "a line that builds a String was admitted in a counted loop:\n{prog}\n{out}");
        }
    }
}
