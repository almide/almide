//! #2186 step 5: the native borrow-mode ORACLE. Every ownership fixture in
//! `tests/` pins one shape that once broke; this test generates the shapes.
//!
//! For each borrow-eligible parameter type it writes one program holding
//! every USE a body can make of a param (read it, return it, wrap it in a
//! list or a record, forward it to a borrowing and to a consuming callee,
//! capture it in a closure, compare it, concatenate it, bind it and read the
//! binding twice, branch on it, match it through an Option, loop over it,
//! project a field, spread it, mutate it through `mut`) crossed with every
//! CALL SITE shape (a literal, a `let` read twice, a loop, a borrowed param
//! forwarded twice, a `var` reassigned between two calls). Then it asserts
//! the property the fixtures each pin one point of: the program checks, the
//! native build succeeds (rustc accepts every borrow / clone / move the
//! passes chose), and native output equals wasm output byte for byte.
//!
//! A wrong borrow mode is a build failure here; a wrong clone or move is a
//! divergence. Either names the type, the use and the call shape.

use std::path::Path;
use std::process::Command;

/// One borrow-eligible type: how to spell it, two distinct literals, and
/// how to read a String out of a value (the printed witness).
struct TypeSpec {
    tag: &'static str,
    ty: &'static str,
    lit: &'static str,
    lit2: &'static str,
    /// A pure String-valued read of `p`.
    read: fn(&str) -> String,
    /// `p + p`, when the type concatenates.
    concat: bool,
    /// An in-place mutation of a `mut` param, when the type has one.
    mutate: Option<&'static str>,
    /// A `for x in p` body summing into an Int, when the type iterates.
    loop_body: Option<&'static str>,
    /// An Int-valued read of one element `x`, when the type iterates: the
    /// step of the fused chains (#2287) that only READ the element.
    elem: Option<&'static str>,
    /// How a chain step REBUILDS one element `x` by spreading it, when the
    /// element is a record. The cell the `elem` chains and the `spread` use
    /// each cover one half of and neither covers together: a step that reads
    /// the element leaves the source borrowed, a param spread consumes what
    /// it spreads, and a step that SPREADS the element has to consume the
    /// source. #2315 fell exactly through that hole — `..x` was emitted on a
    /// `&Tok` and rustc refused a program `almide check` accepted.
    elem_spread: Option<&'static str>,
    record: bool,
}

const TYPES: &[TypeSpec] = &[
    TypeSpec { tag: "str", ty: "String", lit: "\"ab\"", lit2: "\"cde\"", read: |p| format!("int.to_string(string.len({p}))"), concat: true, mutate: None, loop_body: None, elem: None, elem_spread: None, record: false },
    TypeSpec { tag: "list", ty: "List[Int]", lit: "[1, 2]", lit2: "[3, 4, 5]", read: |p| format!("int.to_string(list.len({p}))"), concat: true, mutate: Some("list.push(p, 9)"), loop_body: Some("acc = acc + x"), elem: Some("x"), elem_spread: None, record: false },
    TypeSpec { tag: "strs", ty: "List[String]", lit: "[\"ab\", \"c\"]", lit2: "[\"de\"]", read: |p| format!("int.to_string(list.len({p}))"), concat: true, mutate: Some("list.push(p, \"z\")"), loop_body: Some("acc = acc + string.len(x)"), elem: Some("string.len(x)"), elem_spread: None, record: false },
    TypeSpec { tag: "map", ty: "Map[String, Int]", lit: "[\"a\": 1]", lit2: "[\"b\": 2, \"c\": 3]", read: |p| format!("int.to_string(map.len({p}))"), concat: false, mutate: Some("map.insert(p, \"z\", 9)"), loop_body: None, elem: None, elem_spread: None, record: false },
    TypeSpec { tag: "set", ty: "Set[Int]", lit: "set.from_list([1])", lit2: "set.from_list([2, 3])", read: |p| format!("int.to_string(set.len({p}))"), concat: false, mutate: None, loop_body: None, elem: None, elem_spread: None, record: false },
    TypeSpec { tag: "bytes", ty: "Bytes", lit: "bytes.from_list([1])", lit2: "bytes.from_list([2, 3])", read: |p| format!("int.to_string(bytes.len({p}))"), concat: false, mutate: Some("bytes.push(p, 9)"), loop_body: None, elem: None, elem_spread: None, record: false },
    TypeSpec { tag: "rec", ty: "Tok", lit: "{ text: \"ab\", n: 1 }", lit2: "{ text: \"cde\", n: 2 }", read: |p| format!("int.to_string(string.len({p}.text) + {p}.n)"), concat: false, mutate: None, loop_body: None, elem: None, elem_spread: None, record: true },
    TypeSpec { tag: "recs", ty: "List[Tok]", lit: "[Tok { text: \"ab\", n: 1 }]", lit2: "[Tok { text: \"cde\", n: 2 }, Tok { text: \"f\", n: 3 }]", read: |p| format!("int.to_string(list.len({p}))"), concat: true, mutate: Some("list.push(p, Tok { text: \"z\", n: 0 })"), loop_body: Some("acc = acc + x.n"), elem: Some("x.n"), elem_spread: Some("Tok { ...x, n: x.n + 1 }"), record: false },
];

/// One use of the param inside a body: the fn source and how the call's
/// result is shown as a String.
struct Shape {
    name: String,
    /// The fn definition, taking `p` and possibly `c: Bool`.
    def: String,
    /// Extra params after `p` at the call site.
    extra: String,
    /// `show(expr)`: the String witness of the fn's result.
    show: fn(&TypeSpec, &str) -> String,
    /// The call site must pass a `var` (a `mut` param).
    needs_var: bool,
}

fn shapes(t: &TypeSpec) -> Vec<Shape> {
    let r = t.read;
    let ty = t.ty;
    let tag = t.tag;
    let read_result: fn(&TypeSpec, &str) -> String = |t, e| (t.read)(e);
    let box_result: fn(&TypeSpec, &str) -> String = |t, e| (t.read)(&format!("{e}.v"));
    let list_result: fn(&TypeSpec, &str) -> String = |_, e| format!("int.to_string(list.len({e}))");
    let mut out = vec![
        Shape { name: "read".into(), def: format!("fn u_read_{tag}(p: {ty}) -> String = {}", r("p")), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "ret".into(), def: format!("fn u_ret_{tag}(p: {ty}) -> {ty} = p"), extra: String::new(), show: read_result, needs_var: false },
        Shape { name: "list".into(), def: format!("fn u_list_{tag}(p: {ty}) -> List[{ty}] = [p]"), extra: String::new(), show: list_result, needs_var: false },
        Shape { name: "rec".into(), def: format!("fn u_rec_{tag}(p: {ty}) -> Box_{tag} = {{ v: p }}"), extra: String::new(), show: box_result, needs_var: false },
        Shape { name: "via".into(), def: format!("fn u_via_{tag}(p: {ty}) -> String = u_read_{tag}(p)"), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "viac".into(), def: format!("fn u_viac_{tag}(p: {ty}) -> String = int.to_string(list.len(u_list_{tag}(p)))"), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "cap".into(), def: format!("fn u_cap_{tag}(p: {ty}) -> String = list.join(list.map([1, 2], (i) => {}), \",\")", r("p")), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "eq".into(), def: format!("fn u_eq_{tag}(p: {ty}) -> String = if {} == p then \"eq\" else \"ne\"", t.lit2), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "eq2".into(), def: format!("fn u_eq2_{tag}(p: {ty}) -> String = if p == {} then \"eq\" else \"ne\"", t.lit), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "let".into(), def: format!("fn u_let_{tag}(p: {ty}) -> String = {{\n  let q = p\n  {} + {}\n}}", r("q"), r("q")), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "if".into(), def: format!("fn u_if_{tag}(p: {ty}, c: Bool) -> {ty} = if c then p else {}", t.lit2), extra: ", true".into(), show: read_result, needs_var: false },
        Shape { name: "opt".into(), def: format!("fn u_opt_{tag}(p: {ty}, c: Bool) -> String = {{\n  let o: Option[{ty}] = if c then some(p) else none\n  match o {{\n    some(v) => {},\n    none => \"-\",\n  }}\n}}", r("v")), extra: ", true".into(), show: ident_show, needs_var: false },
        Shape { name: "twice".into(), def: format!("fn u_twice_{tag}(p: {ty}) -> String = {} + u_read_{tag}(p)", r("p")), extra: String::new(), show: ident_show, needs_var: false },
        Shape { name: "pair".into(), def: format!("fn u_pair_{tag}(p: {ty}, q: {ty}) -> String = {} + {}", r("p"), r("q")), extra: String::new(), show: ident_show, needs_var: false },
    ];
    if t.concat {
        out.push(Shape { name: "cat".into(), def: format!("fn u_cat_{tag}(p: {ty}) -> {ty} = p + p"), extra: String::new(), show: read_result, needs_var: false });
    }
    if let Some(body) = t.loop_body {
        out.push(Shape { name: "loop".into(), def: format!("fn u_loop_{tag}(p: {ty}) -> String = {{\n  var acc = 0\n  for x in p {{\n    {body}\n  }}\n  int.to_string(acc)\n}}"), extra: String::new(), show: ident_show, needs_var: false });
    }
    if let Some(sp) = t.elem_spread {
        // A fused chain whose step REBUILDS the element by spreading it: the
        // spread base moves the element's remaining fields, so the source
        // must be consumed (`.into_iter()`) and the base emitted on an owned
        // binder. A borrowed source renders `..x` on a `&Tok`, which checks
        // and then fails to build (#2315). The filtered twin is the same
        // shape one stage deeper, which is how the defect was first reported.
        out.push(Shape { name: "mapspread".into(), def: format!("fn u_mapspread_{tag}(p: {ty}) -> {ty} = list.map(p, (x) => {sp})"), extra: String::new(), show: read_result, needs_var: false });
        out.push(Shape { name: "filtspread".into(), def: format!("fn u_filtspread_{tag}(p: {ty}) -> {ty} = list.map(list.filter(p, (x) => x.n >= 0), (x) => {sp})"), extra: String::new(), show: read_result, needs_var: false });
    }
    if let Some(e) = t.elem {
        // Fused chains whose step only reads the element: the source is
        // borrowed and no element is cloned (#2287).
        out.push(Shape { name: "fold".into(), def: format!("fn u_fold_{tag}(p: {ty}) -> String = int.to_string(list.fold(p, 0, (acc, x) => acc + {e}))"), extra: String::new(), show: ident_show, needs_var: false });
        out.push(Shape { name: "mapped".into(), def: format!("fn u_mapped_{tag}(p: {ty}) -> String = int.to_string(list.sum(list.map(p, (x) => {e})))"), extra: String::new(), show: ident_show, needs_var: false });
    }
    // A user higher-order fn that only CALLS its callback (#2288): the
    // callback slot is `&dyn Fn` and the literal at the call site is a
    // borrowed scope — no `Rc` allocation per call, and the param it reads
    // stays borrowed.
    out.push(Shape { name: "hof".into(), def: format!("fn u_hof_{tag}(p: {ty}, f: ({ty}) -> String) -> String = f(p)"), extra: format!(", (q) => {}", r("q")), show: ident_show, needs_var: false });
    if let Some(m) = t.mutate {
        out.push(Shape { name: "mut".into(), def: format!("fn u_mut_{tag}(mut p: {ty}) -> Unit = {m}"), extra: String::new(), show: unit_show, needs_var: true });
    }
    if t.record {
        out.push(Shape { name: "field".into(), def: format!("fn u_field_{tag}(p: {ty}) -> {ty} = {{ text: p.text, n: p.n + 1 }}"), extra: String::new(), show: read_result, needs_var: false });
        out.push(Shape { name: "spread".into(), def: format!("fn u_spread_{tag}(p: {ty}) -> {ty} = {{ ...p, n: 9 }}"), extra: String::new(), show: read_result, needs_var: false });
    }
    // The `pair` shape takes two params: its call sites pass `p` twice.
    out
}

fn ident_show(_: &TypeSpec, e: &str) -> String { e.to_string() }
fn unit_show(_: &TypeSpec, e: &str) -> String { format!("{{\n    {e}\n    \"unit\"\n  }}") }

/// How main hands the value to each use: the emitted statements produce a
/// String named `line`.
fn call_sites(t: &TypeSpec, s: &Shape) -> Vec<(String, String)> {
    let f = |arg: &str| {
        let extra = if s.name == "pair" { format!(", {arg}") } else { s.extra.clone() };
        format!("u_{}_{}({arg}{extra})", s.name, t.tag)
    };
    let show = |arg: &str| (s.show)(t, &f(arg));
    let mut out = Vec::new();
    if s.needs_var {
        // A `mut` param takes a `var`: mutate, read, mutate again.
        out.push(("var".into(), format!("var x = {}\n  {}\n  let a = {}\n  {}\n  let line = a + {}", t.lit, f("x"), (t.read)("x"), f("x"), (t.read)("x"))));
        return out;
    }
    out.push(("lit".into(), format!("let line = {}", show(t.lit))));
    out.push(("twice".into(), format!("let x = {}\n  let line = {} + {}", t.lit, show("x"), show("x"))));
    out.push(("loop".into(), format!("let x = {}\n  var acc = \"\"\n  for _i in list.range(0, 2) {{\n    acc = acc + {}\n  }}\n  let line = acc", t.lit, show("x"))));
    out.push(("param".into(), format!("let line = c_{}_{}({})", s.name, t.tag, t.lit)));
    out.push(("mutated".into(), format!("var x = {}\n  let a = {}\n  x = {}\n  let line = a + {}", t.lit, show("x"), t.lit2, show("x"))));
    out
}

fn program(t: &TypeSpec) -> String {
    let mut src = String::new();
    src.push_str("// generated by tests/native_borrow_oracle_test.rs — every use × every call site of one param type\n");
    if t.record || t.elem_spread.is_some() {
        src.push_str("type Tok = { text: String, n: Int }\n");
    }
    src.push_str(&format!("type Box_{} = {{ v: {} }}\n", t.tag, t.ty));
    let shapes = shapes(t);
    for s in &shapes {
        src.push_str(&s.def);
        src.push('\n');
    }
    // The forwarding callers: a borrowed param handed to the use twice.
    for s in shapes.iter().filter(|s| !s.needs_var) {
        let extra = if s.name == "pair" { ", q".to_string() } else { s.extra.clone() };
        let call = format!("u_{}_{}(q{extra})", s.name, t.tag);
        let shown = (s.show)(t, &call);
        src.push_str(&format!("fn c_{}_{}(q: {}) -> String = {} + {}\n", s.name, t.tag, t.ty, shown, shown));
    }
    src.push_str("fn main() -> Unit = {\n");
    for s in &shapes {
        for (site, body) in call_sites(t, s) {
            src.push_str(&format!("  {{\n  {}\n  println(\"{}/{}: \" + line)\n  }}\n", body.replace('\n', "\n  "), s.name, site));
        }
    }
    src.push_str("}\n");
    src
}

fn run(almide: &str, args: &[&str], src: &Path) -> (bool, String, String) {
    let out = Command::new(almide).args(args).arg(src).output().expect("almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Run the oracle with `almide` as the compiler: every failure it finds, as
/// one message each naming the type and the arm (check / native build /
/// wasm leg / divergence). Empty means the property held for every type.
fn oracle(almide: &str) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("almide-borrow-oracle-{}-{}", std::process::id(), Path::new(almide).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for t in TYPES {
        let src = dir.join(format!("oracle_{}.almd", t.tag));
        let text = program(t);
        std::fs::write(&src, &text).unwrap();
        let (checked, _, check_err) = run(almide, &["check"], &src);
        if !checked {
            failures.push(format!("[{}] the generated program does not check — generator bug:\n{check_err}\n--- program ---\n{text}", t.tag));
            continue;
        }
        let (native_ok, native_out, native_err) = run(almide, &["run"], &src);
        if !native_ok {
            let rustc: Vec<&str> = native_err.lines().filter(|l| l.contains("error[") || l.contains("^^^") || l.starts_with("  -->") || l.contains("|")).take(40).collect();
            failures.push(format!("[{}] the native build failed — a borrow / clone / move the passes chose is one rustc refuses:\n{}\n--- program ---\n{text}", t.tag, rustc.join("\n")));
            continue;
        }
        let (wasm_ok, wasm_out, wasm_err) = run(almide, &["run", "--target", "wasm"], &src);
        if !wasm_ok {
            failures.push(format!("[{}] the wasm leg failed:\n{wasm_err}", t.tag));
            continue;
        }
        if native_out != wasm_out {
            let mut diff: Vec<String> = native_out.lines().zip(wasm_out.lines()).filter(|(a, b)| a != b).map(|(a, b)| format!("  native: {a}\n  wasm:   {b}")).collect();
            let (n, w) = (native_out.lines().count(), wasm_out.lines().count());
            if n != w {
                diff.push(format!("  native printed {n} line(s), wasm {w}"));
            }
            failures.push(format!("[{}] native and wasm disagree — a clone or move changed a value:\n{}", t.tag, diff.join("\n")));
        }
        let lines = native_out.lines().count();
        assert!(lines >= 60, "[{}] the program printed only {lines} lines — the generator lost its shapes", t.tag);
    }
    if !almide_base::env::flag("ALMIDE_ORACLE_KEEP") {
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        eprintln!("generated programs kept under {}", dir.display());
    }
    failures
}

#[test]
fn every_use_and_call_shape_builds_natively_and_agrees_with_wasm() {
    let failures = oracle(env!("CARGO_BIN_EXE_almide"));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A stand-in compiler: a script that answers `check` / `run` /
/// `run --target wasm` the way `body` says, so the oracle's arms can be
/// shown to FIRE. The real compiler above is the positive control; without
/// these the oracle could be comparing a leg with itself and nobody would
/// know (the tautology the shuffle gate's negatives guard against too).
fn stand_in(name: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("almide-oracle-standin-{}-{name}", std::process::id()));
    std::fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path.to_string_lossy().into_owned()
}

#[test]
#[cfg(unix)]
fn the_oracle_fires_on_a_divergent_compiler_and_on_an_unbuildable_one() {
    // Seventy identical witnesses on both legs, except the wasm leg's last
    // line: the divergence arm must name it.
    let drifts = stand_in("drifts", r#"
case "$*" in
  check*) exit 0 ;;
  *--target\ wasm*) for i in $(seq 1 69); do echo "w$i"; done; echo "wasm-only"; exit 0 ;;
  *) for i in $(seq 1 69); do echo "w$i"; done; echo "native-only"; exit 0 ;;
esac"#);
    let f = oracle(&drifts);
    assert_eq!(f.len(), TYPES.len(), "one divergence per type: {f:?}");
    assert!(f.iter().all(|m| m.contains("native and wasm disagree") && m.contains("native-only") && m.contains("wasm-only")), "{f:?}");

    // A compiler whose native build rustc refuses: the build arm must fire
    // with the rustc line, before any leg is compared.
    let refuses = stand_in("refuses", r#"
case "$*" in
  check*) exit 0 ;;
  *--target\ wasm*) echo "unreachable"; exit 0 ;;
  *) echo "error[E0382]: borrow of moved value: \`p\`" >&2; exit 1 ;;
esac"#);
    let f = oracle(&refuses);
    assert_eq!(f.len(), TYPES.len(), "one build failure per type: {f:?}");
    assert!(f.iter().all(|m| m.contains("the native build failed") && m.contains("E0382")), "{f:?}");

    // And a compiler that agrees with itself is not a failure: the positive
    // control of the harness itself, independent of the real compiler.
    let steady = stand_in("steady", r#"
case "$*" in
  check*) exit 0 ;;
  *) for i in $(seq 1 70); do echo "w$i"; done; exit 0 ;;
esac"#);
    assert!(oracle(&steady).is_empty());
}

// ── Allocation lane (#2228) ──────────────────────────────────────────────
//
// stdout equality and rustc acceptance are both blind to a value cloned where
// a borrow would do, or kept alive past its last use: such a program builds,
// prints the right lines on both legs, and only allocates more than it should.
// `ALMIDE_ALLOC_COUNT=1` builds the native program with a counting allocator
// that reports `__ALMD_ALLOC allocs=N deallocs=N …` when `__almide_main`
// returns, and this lane pins the (allocs, deallocs) pair of every generated
// program EXACTLY in tests/golden/native-borrow-oracle-alloc.txt, the wasm
// ledger's shape (crates/almide-wasm/tests/alloc_ledger.rs): a borrow verdict
// that starts cloning a `Map` param on every call moves the `map` row even
// while every other arm stays green. Ratify a deliberate change with
//
//   ALMIDE_UPDATE_ALLOC=1 cargo test --test native_borrow_oracle_test alloc
//
// which runs every program twice and pins `~` (calibrated out) where the two
// runs disagree — an excluded row stays listed, never silently absent.

fn alloc_ledger_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/native-borrow-oracle-alloc.txt")
}

/// The counting allocator's report for one program, NET of the process's
/// fixed cost: the report of a one-line program compiled the same way is
/// subtracted, so what the ledger pins is the program's own work. The fixed
/// cost differs by one allocation between macOS and Linux (the first
/// ubuntu run of the ledger read every row one below its macOS pin), and a
/// ledger that had to be regenerated per OS would be a ledger nobody could
/// regenerate from a laptop.
fn alloc_report(almide: &str, src: &Path) -> Result<(u64, u64), String> {
    let (a, d) = raw_alloc_report(almide, src)?;
    let base = src.with_file_name("baseline.almd");
    std::fs::write(&base, "fn main() -> Unit = println(\"baseline\")\n").unwrap();
    let (ba, bd) = raw_alloc_report(almide, &base)?;
    Ok((a.saturating_sub(ba), d.saturating_sub(bd)))
}

/// The counting allocator's raw report for one program: `(allocs, deallocs)`.
fn raw_alloc_report(almide: &str, src: &Path) -> Result<(u64, u64), String> {
    let out = Command::new(almide).arg("run").arg(src).env("ALMIDE_ALLOC_COUNT", "1").output().expect("almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return Err(format!("the counted native run failed:\n{stderr}"));
    }
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).ok_or_else(|| {
        format!("no `__ALMD_ALLOC` line on stderr — the allocation lane did not arm (ALMIDE_ALLOC_COUNT was set):\n{stderr}")
    })?;
    let field = |key: &str| -> Result<u64, String> {
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| format!("malformed report `{line}`: no `{key}=N`"))
    };
    Ok((field("allocs")?, field("deallocs")?))
}

/// Ledger rows: `tag -> Some((allocs, deallocs))` pinned, `None` calibrated out.
fn read_alloc_ledger() -> std::collections::BTreeMap<String, Option<(u64, u64)>> {
    let text = std::fs::read_to_string(alloc_ledger_path())
        .expect("tests/golden/native-borrow-oracle-alloc.txt — generate with ALMIDE_UPDATE_ALLOC=1");
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let mut cols = l.split('\t');
            let (a, d, tag) = (cols.next().unwrap_or(""), cols.next().unwrap_or(""), cols.next().unwrap_or("").to_string());
            let pin = if a == "~" { None } else { Some((a.parse().expect("allocs"), d.parse().expect("deallocs"))) };
            (tag, pin)
        })
        .collect()
}

/// The allocation lane with `almide` as the compiler: one failure message per
/// program whose counted report does not match its pinned row (or that has no
/// row, or no report). Under `ALMIDE_UPDATE_ALLOC=1` it rewrites the ledger
/// from two runs and reports nothing.
fn alloc_lane(almide: &str) -> Vec<String> {
    // One directory per call: the negatives run this concurrently with the
    // positive test in the same process, and each call removes its own dir.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("almide-borrow-oracle-alloc-{}-{call}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let update = almide_base::env::flag("ALMIDE_UPDATE_ALLOC");
    let ledger = if update { Default::default() } else { read_alloc_ledger() };
    let mut failures = Vec::new();
    let mut rows = String::from(
        "# native borrow oracle — allocation ledger (#2228). allocs\\tdeallocs\\ttag, exact, per generated\n\
         # program (tests/native_borrow_oracle_test.rs) counted by ALMIDE_ALLOC_COUNT over __almide_main;\n\
         # `~` = calibrated out (two runs disagreed). Regenerate: ALMIDE_UPDATE_ALLOC=1 cargo test --test native_borrow_oracle_test alloc\n",
    );
    for t in TYPES {
        let src = dir.join(format!("oracle_{}.almd", t.tag));
        std::fs::write(&src, program(t)).unwrap();
        let got = alloc_report(almide, &src);
        if update {
            let again = alloc_report(almide, &src);
            let row = match (&got, &again) {
                (Ok(a), Ok(b)) if a == b => format!("{}\t{}\t{}\n", a.0, a.1, t.tag),
                (Ok(_), Ok(_)) => format!("~\t~\t{}\n", t.tag),
                (Err(e), _) | (_, Err(e)) => {
                    failures.push(format!("[{}] {e}", t.tag));
                    continue;
                }
            };
            rows.push_str(&row);
            continue;
        }
        let pinned: Option<Option<(u64, u64)>> = ledger.get(t.tag).copied();
        match (pinned, got) {
            (_, Err(e)) => failures.push(format!("[{}] {e}", t.tag)),
            (None, Ok(_)) => failures.push(format!("[{}] not in the allocation ledger — regenerate to ratify", t.tag)),
            (Some(None), Ok(_)) => {}
            (Some(Some(want)), Ok(got)) if want == got => {}
            (Some(Some(want)), Ok(got)) => failures.push(format!(
                "[{}] allocations moved: counted allocs={} deallocs={}, pinned allocs={} deallocs={} — a borrow, clone or move verdict changed; if on purpose, ratify with ALMIDE_UPDATE_ALLOC=1",
                t.tag, got.0, got.1, want.0, want.1
            )),
        }
    }
    if update && failures.is_empty() {
        let path = alloc_ledger_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, rows).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
    failures
}

#[test]
fn every_generated_program_allocates_exactly_what_the_ledger_pins() {
    let failures = alloc_lane(env!("CARGO_BIN_EXE_almide"));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
#[cfg(unix)]
fn the_allocation_lane_fires_on_a_moved_count_and_on_a_missing_report() {
    if almide_base::env::flag("ALMIDE_UPDATE_ALLOC") {
        return; // the negatives read the ledger the positive run is rewriting
    }
    let pinned = read_alloc_ledger().values().filter(|v| v.is_some()).count();
    assert!(pinned > 0, "the ledger pins no row — the negatives would be vacuous");

    // A compiler whose every program allocates once net of the baseline:
    // no pinned row reads 1/1.
    let moved = stand_in("moved", r#"
case "$*" in
  *baseline.almd*) echo "__ALMD_ALLOC allocs=0 deallocs=0 reallocs=0 peak=0" >&2 ;;
  *) echo "__ALMD_ALLOC allocs=1 deallocs=1 reallocs=0 peak=8" >&2 ;;
esac
exit 0"#);
    let f = alloc_lane(&moved);
    assert_eq!(f.len(), pinned, "one moved-count failure per pinned row: {f:?}");
    assert!(f.iter().all(|m| m.contains("allocations moved") && m.contains("counted allocs=1")), "{f:?}");

    // A compiler that ignores the switch: no report is a failure, never a pass.
    let silent = stand_in("silent", "exit 0");
    let f = alloc_lane(&silent);
    assert_eq!(f.len(), TYPES.len(), "one missing-report failure per type: {f:?}");
    assert!(f.iter().all(|m| m.contains("did not arm")), "{f:?}");
}
