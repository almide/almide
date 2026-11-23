//! #4771 — a sole-owner value whose last use is a `@consume` argument MOVES
//! into the runtime body on the native render, as v0.69.1 did.
//!
//! v0.70.0's certified v1 render handed `x` to `@consume` runtime bodies
//! by `.clone()` even where `x` was never read again: the rebind
//! `x = http.set_status(x, ..)` copied the whole response on every step
//! (2000 steps over 10,000 headers: 8 ms on v0.69.1, 1195 ms after).
//!
//! The matrix is generated, not hand-picked: [`consume_intrinsics`] reads
//! every `@intrinsic` + `@consume` declaration out of the bundled stdlib, and
//! [`every_consume_intrinsic_has_a_cell`] fails when one has no cell here —
//! a new `@consume` declaration lands with its cell. Each cell renders the
//! program `almide build` ships (the v1 render, or the standard codegen where
//! v1 walls) twice:
//! - POSITIVE: the consumed value dies at the call — a rebind loop
//!   `x = f(x, ..)` where `f` returns its own kind, else a per-iteration
//!   `let r = f(x, ..)` with `x` built fresh and not read again. `main`
//!   must not copy it (no `.clone()` / `.to_vec()` / `clone_from`).
//! - NEGATIVE CONTROL: the same call with the consumed value read again
//!   afterwards (shared). `main` MUST still copy it — so a cell whose
//!   positive passes for a reason other than a move (a scalar lowering that
//!   never copies anything) is caught as vacuous.
//!
//! A rebind cell runs once more with the call inside a user fn
//! `step(x, n)` (`x = step(x, n)`): the caller hands `x` over to `step`'s
//! param, which reaches the `@consume` param at its last use.
//!
//! And once more as a self tail call, `go(x, n) = if n >= 3 then x else
//! go(f(x, ..), n + 1)`, the re-inspection's `tail` shape: the loop the
//! tail call becomes rebinds the RETURNED param on every step, which the
//! liveness check had kept from moving (`http.set_status` copied the whole
//! response per step on the native render, 4.0 s against v0.69.1's 0.02 s
//! for 20,000 steps over 4,000 headers). Its control reads `x` after the
//! call inside the same argument.

use std::collections::BTreeSet;
use std::process::Command;

/// Every bundled `@intrinsic` fn with a `@consume` attribute, as `module.fn`.
fn consume_intrinsics() -> BTreeSet<String> {
    use almide_lang::ast::Decl;
    let mut out = BTreeSet::new();
    for &m in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(src) = almide_lang::stdlib_info::bundled_source(m) else { continue };
        let Some(parsed) = almide_lang::parse_cached(src) else { continue };
        for d in &parsed.decls {
            let Decl::Fn { name, attrs, .. } = d else { continue };
            let has = |a: &str| attrs.iter().any(|x| x.name.as_str() == a);
            if has("intrinsic") && has("consume") {
                out.insert(format!("{m}.{name}"));
            }
        }
    }
    out
}

/// One cell: the `@consume` declaration it covers, the program's imports,
/// the consumed value's per-iteration build (`x`), and the call on it.
/// `rebind`: the call returns `x`'s own kind, so the loop rebinds `x`;
/// otherwise the call's result binds `r` and `use_r` reads it.
struct Cell {
    decl: &'static str,
    import: &'static str,
    build: &'static str,
    call: &'static str,
    rebind: bool,
    use_r: &'static str,
}

const LIST: &str = "list.range(0, 1000)";
const RESP: &str = "http.with_headers(200, \"b\", (list.range(0, 100) |> list.map((i) => (\"k${i}\", \"v\"))))";
const REQ: &str = "http.new_request(\"GET\", \"/a\", \"b\", (list.range(0, 100) |> list.map((i) => (\"k${i}\", \"v\"))))";
/// A `Result` build spells its type: `ok(..)` alone does not fix `E`.
const RES: &str = ": Result[List[Int], String] = if n >= 0 then ok(list.range(0, n)) else err(\"no\")";
const VAL: &str = "value.array(list.range(0, n) |> list.map((i) => value.int(i)))";

const fn rebind(decl: &'static str, import: &'static str, build: &'static str, call: &'static str) -> Cell {
    Cell { decl, import, build, call, rebind: true, use_r: "" }
}

const fn last(decl: &'static str, import: &'static str, build: &'static str, call: &'static str, use_r: &'static str) -> Cell {
    Cell { decl, import, build, call, rebind: false, use_r }
}

fn cells() -> Vec<Cell> {
    vec![
        // http — the runtime-typed response and request (#4771's own shape).
        rebind("http.set_status", "http", RESP, "http.set_status(x, 200 + n % 2)"),
        rebind("http.__http_response_put_header", "http", RESP, "http.set_header(x, \"k\", \"v${n}\")"),
        rebind("http.__http_response_put_body", "http", RESP, "http.set_body(x, \"b${n}\")"),
        rebind("http.__http_response_put_body_bytes", "http", RESP, "http.with_body(x, bytes.from_string(\"b${n}\"))"),
        // #4767: a request is built around its body by move, not by a copy.
        last("http.__new_request", "http", "string.repeat(\"b\", 100 + n)", "http.new_request(\"GET\", \"/a\", x, [])", "string.len(http.body(r))"),
        rebind("http.__http_request_set_header", "http", REQ, "http.set_header(x, \"k\", \"v${n}\")"),
        rebind("http.__http_request_set_body", "http", REQ, "http.set_body(x, \"b${n}\")"),
        rebind("http.__http_request_set_body_bytes", "http", REQ, "http.set_body_bytes(x, bytes.from_string(\"b${n}\"))"),
        rebind("http.__with_params", "http", REQ, "http.__with_params(x, [\"k\", \"v${n}\"])"),
        rebind("http.__with_path", "http", REQ, "http.__with_path(x, \"/p${n}\")"),
        // list — the consumed source list.
        rebind("list.map", "", LIST, "list.map(x, (e) => e + 1)"),
        rebind("list.filter", "", LIST, "list.filter(x, (e) => e != 5)"),
        rebind("list.flat_map", "", LIST, "list.flat_map(x, (e) => [e + 1])"),
        rebind("list.filter_map", "", LIST, "list.filter_map(x, (e) => if e != 5 then some(e) else none)"),
        rebind("list.sort_by", "", LIST, "list.sort_by(x, (e) => 0 - e)"),
        rebind("list.__sort_by_float", "", LIST, "list.sort_by(x, (e) => float.from(0 - e))"),
        rebind("list.update", "", LIST, "list.update(x, 0, (e) => e + 1)"),
        rebind("list.scan", "", LIST, "list.scan(x, n, (a, e) => a + e)"),
        rebind("list.unique_by", "", LIST, "list.unique_by(x, (e) => e % 7)"),
        rebind("list.insert", "", LIST, "list.insert(x, 0, n)"),
        rebind("list.remove_at", "", LIST, "list.remove_at(x, 0)"),
        last("list.enumerate", "", LIST, "list.enumerate(x)", "list.len(r)"),
        last("list.zip", "", LIST, "list.zip(x, list.range(0, n))", "list.len(r)"),
        last("list.zip_with", "", LIST, "list.zip_with(x, list.range(0, n), (a, b) => a + b)", "list.len(r)"),
        last("list.fold", "", LIST, "list.fold(x, n, (a, e) => a + e)", "r"),
        last("list.partition", "", LIST, "list.partition(x, (e) => e > 5)", "list.len(r.0)"),
        last("list.reduce", "", LIST, "list.reduce(x, (a, e) => a + e)", "r ?? 0"),
        last("list.group_by", "", LIST, "list.group_by(x, (e) => e % 7)", "map.len(r)"),
        last("list.__group_count", "", LIST, "map.map(list.group_by(x, (e) => e % 7), (g) => list.len(g))", "map.len(r)"),
        last("list.intersperse", "", LIST, "list.intersperse(x, n)", "list.len(r)"),
        // map
        rebind("map.set", "", "map.from_list(list.range(0, 100) |> list.map((i) => (i, i)))", "map.set(x, n, n)"),
        rebind("map.upsert", "", "map.from_list(list.range(0, 100) |> list.map((i) => (i, i)))", "map.upsert(x, n % 7, 0, (v) => v + 1)"),
        last("map.from_list", "", "list.range(0, n) |> list.map((i) => (i, i))", "map.from_list(x)", "map.len(r)"),
        // random (effect)
        rebind("random.shuffle", "random", LIST, "random.shuffle(x)!"),
        // result — a heap payload, so a copy is observable.
        rebind("result.map", "", RES, "result.map(x, (v) => v + [1])"),
        rebind("result.map_err", "", RES, "result.map_err(x, (e) => e + \"!\")"),
        rebind("result.flat_map", "", RES, "result.flat_map(x, (v) => ok(v + [1]))"),
        last("result.unwrap_or", "", RES, "x ?? []", "list.len(r)"),
        last("result.unwrap_or_else", "", RES, "result.unwrap_or_else(x, (e) => [string.len(e)])", "list.len(r)"),
        last("result.to_option", "", RES, "result.to_option(x)", "list.len(r ?? [])"),
        last("result.to_err_option", "", RES, "result.to_err_option(x)", "string.len(r ?? \"\")"),
        last("result.partition", "", ": List[Result[Int, String]] = list.range(0, n) |> list.map((i) => if i % 2 == 0 then ok(i) else err(\"e\"))", "result.partition(x)", "list.len(r.0)"),
        // value
        last("value.__eq", "", VAL, "x == value.array([value.int(n)])", "if r then 1 else 0"),
        last("value.__set_path_steps", "", VAL, "value.set_path(x, value.index(value.root(), 0), value.int(n))", "match r { ok(_) => 1, err(_) => 0 }"),
    ]
}

/// `x`'s binding tail: a build that spells its type (`: T = ..`) as is.
fn typed(build: &str) -> String {
    if build.starts_with(':') { build.to_string() } else { format!(" = {build}") }
}

/// The cells whose call the native render serves by BORROWING the consumed
/// value (the runtime body reads it in place): neither form copies it, so
/// their shared control has nothing to find. Exact — a listed cell whose
/// shared form copies fails, so the list cannot hide a regression.
const LENDS: &[&str] = &["list.fold", "value.__eq"];

/// The declared type of a rebind cell's `x`, for the `step` fn the
/// hand-over variant passes it through.
fn ty_of(build: &str) -> &str {
    match build {
        b if b.starts_with(':') => b[1..].split(" = ").next().unwrap_or("").trim(),
        RESP => "HttpResponse",
        REQ => "HttpRequest",
        LIST => "List[Int]",
        b if b.starts_with("map.from_list(") => "Map[Int, Int]",
        b => panic!("no declared type for the build `{b}`"),
    }
}

/// The hand-over variant of a rebind cell: the call sits in a user fn
/// `step(x, n)` and the loop rebinds `x = step(x, n)`, so `x` reaches the
/// `@consume` param through a param of `step` that the caller hands over at
/// its last use. `None` for an effect call (its `!` needs an effect `step`).
fn program_through(c: &Cell, shared: bool) -> Option<String> {
    if !c.rebind || c.call.contains('!') {
        return None;
    }
    let ty = ty_of(c.build);
    let import = if c.import.is_empty() { String::new() } else { format!("import {}\n", c.import) };
    let keep = if shared { format!("    let keep: {ty} = x\n") } else { String::new() };
    let after = if shared { "    println(\"${keep}\")\n" } else { "" };
    Some(format!(
        "{import}fn step(x: {ty}, n: Int) -> {ty} = {call}\n\neffect fn main() -> Unit = {{\n  var n = 0\n  var x{build}\n  while n < 3 {{\n{keep}    x = step(x, n)\n{after}    n = n + 1\n  }}\n  println(\"${{x}}\")\n}}\n",
        call = c.call, build = typed(c.build),
    ))
}

/// The cells whose tail-shape control maps the still-shared list through a
/// borrow (`x.iter().map(..)`), which copies no list: a move is not what
/// keeps their positive copy-free, so the control is not asked to copy.
const TAIL_LENDS: &[&str] = &["list.map"];

/// The self-tail-call variant of a rebind cell: `go` rebinds its returned
/// param `x` through the tail call; `shared` reads `x` after the call.
fn program_tail(c: &Cell, shared: bool) -> Option<String> {
    if !c.rebind || c.call.contains('!') {
        return None;
    }
    let ty = ty_of(c.build);
    let import = if c.import.is_empty() { String::new() } else { format!("import {}\n", c.import) };
    let arg = if shared { format!("{{\n    let y = {}\n    println(\"${{x}}\")\n    y\n  }}", c.call) } else { c.call.to_string() };
    Some(format!(
        "{import}fn go(x: {ty}, n: Int) -> {ty} = if n >= 3 then x else go({arg}, n + 1)\n\neffect fn main() -> Unit = {{\n  let n = 0\n  var x{build}\n  let y: {ty} = go(x, n)\n  println(\"${{y}}\")\n}}\n",
        build = typed(c.build),
    ))
}

/// The cell's program; `shared`: the consumed value is read again after the
/// call (the negative control).
fn program(c: &Cell, shared: bool) -> String {
    let import = if c.import.is_empty() { String::new() } else { format!("import {}\n", c.import) };
    let again = "  println(\"${x}\")\n";
    let body = if c.rebind {
        // A Result binding spells its type, or it reads as propagation (E041).
        let ty = if c.build.starts_with(':') { c.build.split(" = ").next().unwrap_or("") } else { "" };
        let keep = if shared { format!("    let keep{ty} = x\n") } else { String::new() };
        let after = if shared { "    println(\"${keep}\")\n" } else { "" };
        format!(
            "  var n = 0\n  var x{build}\n  while n < 3 {{\n{keep}    x = {call}\n{after}    n = n + 1\n  }}\n  println(\"${{x}}\")\n",
            build = typed(c.build), call = c.call,
        )
    } else {
        let after = if shared { again.replacen("  ", "    ", 1) } else { String::new() };
        format!(
            "  var acc = 0\n  var n = 0\n  while n < 3 {{\n    let x{build}\n    let r = {call}\n{after}    acc = acc + ({use_r})\n    n = n + 1\n  }}\n  println(\"${{acc}}\")\n",
            build = typed(c.build), call = c.call, use_r = c.use_r,
        )
    };
    format!("{import}effect fn main() -> Unit = {{\n{body}}}\n")
}

/// What `almide build` ships for `src`: the v1 render, or the standard
/// codegen's Rust where v1 walls (`render_v1_native_or_fallback`).
fn shipped_rust(name: &str, src: &str) -> (String, &'static str) {
    if let Ok(rust) = almide_mir::pipeline::try_render_rust_source(src) {
        return (rust, "v1");
    }
    // One directory per call: the tests run in parallel and render the same
    // cell's programs, so a directory keyed by the cell alone is shared.
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("almd_consume_moves_{}_{}_{seq}", name.replace('.', "_"), std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args([file.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("almide emit");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "{name}: emit failed: {}\n{src}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), "v0")
}

/// The rendered body of the program's `main`: the v1 render's `fn main`, or
/// the standard codegen's `__almide_main` (its `fn main` only calls it).
fn main_body(rust: &str) -> &str {
    let at = rust.find("\npub fn __almide_main(").or_else(|| rust.find("\nfn main()")).expect("the render has a main");
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

/// The rendered body of the user fn `step` (v1 spells it `almd_step`).
fn step_body(rust: &str) -> &str {
    fn_body(rust, "step")
}

/// The rendered body of the user fn `name` (v1 spells it `almd_<name>`).
fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let at = [format!("\nfn almd_{name}("), format!("\npub fn almd_{name}("), format!("\npub fn {name}("), format!("\nfn {name}(")]
        .iter()
        .find_map(|h| rust.find(h.as_str()))
        .unwrap_or_else(|| panic!("the render has fn {name}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

/// The copy sites in `main`'s body: one line each.
fn copies(body: &str) -> Vec<&str> {
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter(|l| l.contains(".clone()") || l.contains(".to_vec()") || l.contains("clone_from"))
        .collect()
}

#[test]
fn every_consume_intrinsic_has_a_cell() {
    let declared = consume_intrinsics();
    let covered: BTreeSet<String> = cells().iter().map(|c| c.decl.to_string()).collect();
    let missing: Vec<_> = declared.difference(&covered).collect();
    let stale: Vec<_> = covered.difference(&declared).collect();
    assert!(missing.is_empty() && stale.is_empty(), "@consume intrinsics without a cell: {missing:?}; cells naming no @consume intrinsic: {stale:?}");
}

#[test]
fn a_dying_consume_argument_moves_and_a_shared_one_is_copied() {
    let mut failures = Vec::new();
    for c in cells() {
        let (pos, route) = shipped_rust(c.decl, &program(&c, false));
        let (neg, neg_route) = shipped_rust(c.decl, &program(&c, true));
        let (pc, nc) = (copies(main_body(&pos)), copies(main_body(&neg)));
        eprintln!("== {} [{route}/{neg_route}] positive copies {:?}\n   negative copies {:?}", c.decl, pc, nc);
        if !pc.is_empty() {
            failures.push(format!("{} [{route}]: a dying consumed value is copied:\n  {}\n{}", c.decl, pc.join("\n  "), program(&c, false)));
        }
        let lends = LENDS.contains(&c.decl);
        if nc.is_empty() && !lends {
            failures.push(format!("{} [{neg_route}]: the shared control copies nothing — the cell cannot tell a move from a copy:\n{}", c.decl, program(&c, true)));
        }
        if !nc.is_empty() && lends {
            failures.push(format!("{} [{neg_route}]: listed in LENDS, but the shared control copies — take it out of LENDS:\n  {}", c.decl, nc.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn a_dying_consume_argument_handed_through_a_param_moves() {
    let mut failures = Vec::new();
    for c in cells() {
        let (Some(pos_src), Some(neg_src)) = (program_through(&c, false), program_through(&c, true)) else { continue };
        let (pos, route) = shipped_rust(c.decl, &pos_src);
        let (neg, neg_route) = shipped_rust(c.decl, &neg_src);
        let pc: Vec<&str> = copies(main_body(&pos)).into_iter().chain(copies(step_body(&pos))).collect();
        let nc: Vec<&str> = copies(main_body(&neg)).into_iter().chain(copies(step_body(&neg))).collect();
        eprintln!("== {} through [{route}/{neg_route}] positive copies {:?}\n   negative copies {:?}", c.decl, pc, nc);
        if !pc.is_empty() {
            failures.push(format!("{} [{route}]: a value handed over through a param is copied:\n  {}\n{pos_src}", c.decl, pc.join("\n  ")));
        }
        if nc.is_empty() {
            failures.push(format!("{} [{neg_route}]: the shared control copies nothing:\n{neg_src}", c.decl));
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn a_dying_consume_argument_moves_through_a_self_tail_call() {
    let mut failures = Vec::new();
    for c in cells() {
        let (Some(pos_src), Some(neg_src)) = (program_tail(&c, false), program_tail(&c, true)) else { continue };
        let (pos, route) = shipped_rust(c.decl, &pos_src);
        let (neg, neg_route) = shipped_rust(c.decl, &neg_src);
        let (pc, nc) = (copies(fn_body(&pos, "go")), copies(fn_body(&neg, "go")));
        eprintln!("== {} tail [{route}/{neg_route}] positive copies {:?}\n   negative copies {:?}", c.decl, pc, nc);
        if !pc.is_empty() {
            failures.push(format!("{} [{route}]: the self tail call copies its dying accumulator:\n  {}\n{pos_src}", c.decl, pc.join("\n  ")));
        }
        let lends = LENDS.contains(&c.decl) || TAIL_LENDS.contains(&c.decl);
        if nc.is_empty() && !lends {
            failures.push(format!("{} [{neg_route}]: the shared control copies nothing:\n{neg_src}", c.decl));
        }
        if !nc.is_empty() && TAIL_LENDS.contains(&c.decl) {
            failures.push(format!("{} [{neg_route}]: listed in TAIL_LENDS, but the shared control copies — take it out:\n  {}", c.decl, nc.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "{} cell(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}
