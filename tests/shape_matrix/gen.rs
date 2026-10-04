//! The shape-matrix generator (#3309): a bounded cross product of the program
//! shapes that one leg could not build while `almide check` accepted them.
//!
//! Every cell is a small project (one file, a sibling module, or a
//! path-dependency package) with a stable name, the files it consists of, the
//! entry it runs, and the stdout it must print. The expected stdout is known by
//! construction — each cell computes an `Int` the generator also computes — so
//! a cell convicts a leg on its own, not only by disagreement.
//!
//! Axes (one family per bug class the issue lists; each family crosses the axes
//! that apply to it, and leaves out the combinations that cannot type-check —
//! by construction, never by running):
//!
//! | family     | axes                                                              | issues            |
//! |------------|-------------------------------------------------------------------|-------------------|
//! | `toplet`   | layout × value kind × use site × naming                            | #3286 #3287 #3297 #3305 |
//! | `modvar`   | layout × mutation form × mutation site × local naming              | #3304 #3307       |
//! | `mutargs`  | var origin × argument shape (several `mut` args + reads)           | #3306             |
//! | `bind`     | binding form × payload type × sink (outer var / slot)              | #3303             |
//! | `samerec`  | layout × import order × literal site (same-field records)          | #3283 #3290       |
//! | `depclos`  | layout × root form × holder × entry (closures built in a dep fn)   | #3296             |
//!
//! std-only on purpose: `tools/xtarget-fuzz` includes this file with `#[path]`
//! so the random hunt draws from the same shape vocabulary.

#![allow(dead_code)]

use std::fmt::Write as _;

/// One generated program.
#[derive(Clone, Debug)]
pub struct Cell {
    /// Stable name: `<family>/<axis>.<axis>...`. The allowlist keys on it.
    pub name: String,
    /// `(path relative to the project root, contents)`.
    pub files: Vec<(String, String)>,
    /// The file `almide check/build` is pointed at, relative to the root.
    pub entry: String,
    /// What the program must print (by construction).
    pub expected: String,
}

// ───────────────────────────── layouts ─────────────────────────────

/// Where a declaring module lives relative to the entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    /// Declarations and `main` in one file.
    Single,
    /// `src/m.almd`, imported as `import self.m`.
    Sibling,
    /// A path-dependency package `lib` (`lib/src/mod.almd`), imported as `import lib`.
    Package,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Single, Layout::Sibling, Layout::Package];
    pub fn tag(self) -> &'static str {
        match self {
            Layout::Single => "single",
            Layout::Sibling => "sibling",
            Layout::Package => "package",
        }
    }
    /// The qualifier the entry writes before a declaring-module name.
    pub fn qual(self) -> &'static str {
        match self {
            Layout::Single => "",
            Layout::Sibling => "m.",
            Layout::Package => "lib.",
        }
    }
}

/// A non-entry module of a project.
struct Module {
    /// The name the entry imports it by (`m`, `a`, `lib`, ...).
    name: String,
    /// Sibling module of the app, or a path-dependency package of that name.
    package: bool,
    src: String,
}

/// Lay a project out on disk: the entry plus its modules. With no modules the
/// project is a single file (no `almide.toml`). With a package module the app
/// lives in `app/` and every package beside it.
fn project(modules: Vec<Module>, entry_body: &str) -> (Vec<(String, String)>, String) {
    if modules.is_empty() {
        return (vec![("main.almd".into(), entry_body.to_string())], "main.almd".into());
    }
    let any_pkg = modules.iter().any(|m| m.package);
    let app = if any_pkg { "app/" } else { "" };
    let mut files = Vec::new();
    let mut toml = String::from("[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    if any_pkg {
        toml.push_str("\n[dependencies]\n");
        for m in modules.iter().filter(|m| m.package) {
            let _ = writeln!(toml, "{} = {{ path = \"../{}\" }}", m.name, m.name);
        }
    }
    files.push((format!("{app}almide.toml"), toml));
    let mut imports = String::new();
    for m in &modules {
        if m.package {
            files.push((
                format!("{}/almide.toml", m.name),
                format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", m.name),
            ));
            files.push((format!("{}/src/mod.almd", m.name), m.src.clone()));
            let _ = writeln!(imports, "import {}", m.name);
        } else {
            files.push((format!("{app}src/{}.almd", m.name), m.src.clone()));
            let _ = writeln!(imports, "import self.{}", m.name);
        }
    }
    let entry = format!("{app}src/main.almd");
    files.push((entry.clone(), format!("{imports}\n{entry_body}")));
    (files, entry)
}

/// The common two-module shape: declarations in the declaring module of
/// `layout`, `main_body` in the entry.
fn layout_project(layout: Layout, decls: &str, main_body: &str) -> (Vec<(String, String)>, String) {
    match layout {
        Layout::Single => project(vec![], &format!("{decls}\n{main_body}")),
        Layout::Sibling => project(vec![Module { name: "m".into(), package: false, src: decls.into() }], main_body),
        Layout::Package => project(vec![Module { name: "lib".into(), package: true, src: decls.into() }], main_body),
    }
}

/// `effect fn main` that runs `stmts` and prints the Int `v` they bind.
fn main_printing(stmts: &str) -> String {
    format!("effect fn main() -> Unit = {{\n{}\n  println(int.to_string(v))\n}}\n", indent(stmts, 2))
}

fn indent(s: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    s.lines()
        .map(|l| if l.trim().is_empty() { String::new() } else { format!("{pad}{l}") })
        .collect::<Vec<_>>()
        .join("\n")
}

// ───────────────────────────── family: toplet ─────────────────────────────

/// A top-level `let` value kind: its supporting declarations, its initializer,
/// how to observe it as an Int, and the observed value.
struct LetKind {
    tag: &'static str,
    /// Observed by calling the let itself. An upper-case name in call
    /// position is a constructor (E003), so such a kind is never named
    /// `upper`, and its case twin is an Int constant instead.
    callable: bool,
    /// Types and helper fns the initializer needs (declaring module, unqualified).
    prelude: &'static str,
    init: &'static str,
    /// Optional annotation (`: T`) — the type is the declaring module's.
    annot: &'static str,
    /// Observe `r` (a reference to the let, qualified as the site needs) as an Int.
    obs: fn(&str) -> String,
    val: i64,
}

fn let_kinds() -> Vec<LetKind> {
    vec![
        LetKind { callable: false, tag: "int", prelude: "", init: "41", annot: "", obs: |r| r.to_string(), val: 41 },
        LetKind { callable: false, tag: "string", prelude: "", init: "\"abc\"", annot: "", obs: |r| format!("string.len({r})"), val: 3 },
        LetKind { callable: false, tag: "list", prelude: "", init: "[5, 6, 7]", annot: "", obs: |r| format!("list.len({r}) + {r}[0]"), val: 8 },
        LetKind { callable: false, tag: "map", prelude: "", init: "[\"k\": 2]", annot: "", obs: |r| format!("{r}[\"k\"] ?? 0"), val: 2 },
        LetKind {
            callable: false,
            tag: "record",
            prelude: "type Rec = { a: Int, b: String }\n",
            init: "{ a: 3, b: \"x\" }",
            annot: ": Rec",
            obs: |r| format!("{r}.a + string.len({r}.b)"),
            val: 4,
        },
        LetKind {
            callable: false,
            tag: "variant",
            prelude: "type Sel = | SelA(Int) | SelB\n",
            init: "SelA(5)",
            annot: "",
            obs: |r| format!("string.len(\"${{{r}}}\")"),
            val: 7,
        },
        LetKind { callable: false, tag: "option", prelude: "", init: "some(4)", annot: ": Option[Int]", obs: |r| format!("{r} ?? 0"), val: 4 },
        LetKind { callable: false, tag: "bytes", prelude: "", init: "bytes.from_list([1, 2, 3])", annot: "", obs: |r| format!("bytes.len({r})"), val: 3 },
        LetKind {
            callable: false,
            tag: "record_bytes",
            prelude: "type Blob = { data: Bytes, n: Int }\n",
            init: "{ data: bytes.new(2), n: 1 }",
            annot: ": Blob",
            obs: |r| format!("{r}.n + bytes.len({r}.data)"),
            val: 3,
        },
        LetKind { callable: false, tag: "matrix", prelude: "", init: "matrix.zeros(2, 3)", annot: "", obs: |r| format!("matrix.rows({r})"), val: 2 },
        LetKind {
            callable: true,
            tag: "fn_value",
            prelude: "fn inc1(n: Int) -> Int = n + 1\n",
            init: "inc1",
            annot: "",
            obs: |r| format!("{r}(1)"),
            val: 2,
        },
        LetKind {
            callable: true,
            tag: "fn_value_closure_param",
            prelude: "fn apply1(f: () -> Int, x: Int) -> Int = f() + x\n",
            init: "apply1",
            annot: "",
            obs: |r| format!("{r}(() => 1, 2)"),
            val: 3,
        },
        LetKind {
            callable: false,
            tag: "record_of_fns",
            prelude: "type Ops = { run: (Int) -> Int }\nfn inc2(n: Int) -> Int = n + 1\n",
            init: "{ run: inc2 }",
            annot: ": Ops",
            obs: |r| format!("{r}.run(2)"),
            val: 3,
        },
        LetKind {
            callable: false,
            tag: "record_of_closures",
            prelude: "type Ops2 = { run: (Int) -> Int }\n",
            init: "{ run: (n) => n * 2 }",
            annot: ": Ops2",
            obs: |r| format!("{r}.run(2)"),
            val: 4,
        },
    ]
}

/// Where the top-level let is read from.
const LET_USES: [&str; 6] = ["direct", "closure", "nested_closure", "hof", "own_fn", "own_fn_closure"];

/// How the let is named.
///  - `upper`: `THING`
///  - `lower`: `thing`
///  - `case_twin`: both `thing` and `THING` (#3305 — upper-casing is not injective)
///  - `gen_static`: a user let in the entry spelled like the native static of
///    the declaring module's let (`almide_rt_m_thing` beside `m.THING`).
const LET_NAMINGS: [&str; 4] = ["upper", "lower", "case_twin", "gen_static"];

/// Read site `stmts` that bind `v` from the observation `o`.
fn read_site(use_: &str, o: &str, q: &str) -> String {
    match use_ {
        "direct" => format!("let v = {o}"),
        "closure" => format!("let f = () => {o}\nlet v = f()"),
        "nested_closure" => format!("let f = () => {{\n  let g = () => {o}\n  g()\n}}\nlet v = f()"),
        "hof" => format!("let v = list.fold([0], 0, (acc, i) => acc + i + {o})"),
        "own_fn" | "own_fn_closure" => format!("let v = {q}probe()"),
        _ => unreachable!(),
    }
}

fn toplet_cells(out: &mut Vec<Cell>) {
    for layout in Layout::ALL {
        for kind in let_kinds() {
            for use_ in LET_USES {
                for naming in LET_NAMINGS {
                    // `gen_static` needs a module prefix to imitate; a single
                    // file has none. A callable kind cannot be called through
                    // an upper-case name (E003: a constructor), so it is never
                    // `upper`/`gen_static` (both name it `THING`).
                    if naming == "gen_static" && layout == Layout::Single {
                        continue;
                    }
                    if kind.callable && (naming == "upper" || naming == "gen_static") {
                        continue;
                    }
                    let q = layout.qual();
                    let names: Vec<&str> = match naming {
                        "upper" | "gen_static" => vec!["THING"],
                        "lower" => vec!["thing"],
                        "case_twin" => vec!["thing", "THING"],
                        _ => unreachable!(),
                    };
                    let mut decls = String::from(kind.prelude);
                    let mut twin_val = 0;
                    for n in &names {
                        if kind.callable && *n == "THING" {
                            // The callable kind's twin: an Int constant (#3305's
                            // shape — `let nest = inc` beside `let NEST = ...`).
                            decls.push_str("let THING = 100\n");
                            twin_val = 100;
                        } else {
                            let _ = writeln!(decls, "let {n}{} = {}", kind.annot, kind.init);
                        }
                    }
                    let obs_at = |qual: &str| -> String {
                        names
                            .iter()
                            .map(|n| if kind.callable && *n == "THING" { format!("{qual}{n}") } else { (kind.obs)(&format!("{qual}{n}")) })
                            .map(|e| format!("({e})"))
                            .collect::<Vec<_>>()
                            .join(" + ")
                    };
                    match use_ {
                        "own_fn" => {
                            let _ = writeln!(decls, "pub fn probe() -> Int = {}", obs_at(""));
                        }
                        "own_fn_closure" => {
                            let _ = writeln!(decls, "pub fn probe() -> Int = {{\n  let f = () => {}\n  f()\n}}", obs_at(""));
                        }
                        _ => {}
                    }
                    let mut stmts = read_site(use_, &obs_at(q), q);
                    let mut val = if twin_val > 0 { kind.val + twin_val } else { kind.val * names.len() as i64 };
                    let mut root_decls = String::new();
                    if naming == "gen_static" {
                        // The root's own let, spelled as the native static of
                        // the declaring module's `VALUE` would be before
                        // upper-casing.
                        let modname = if layout == Layout::Sibling { "m" } else { "lib" };
                        let _ = writeln!(root_decls, "let almide_rt_{modname}_thing = 100");
                        stmts.push_str(&format!("\nlet v = v + almide_rt_{modname}_thing"));
                        val += 100;
                    }
                    let body = format!("{root_decls}{}", main_printing(&stmts));
                    let (files, entry) = layout_project(layout, &decls, &body);
                    out.push(Cell {
                        name: format!("toplet/{}.{}.{}.{}", layout.tag(), kind.tag, use_, naming),
                        files,
                        entry,
                        expected: format!("{val}\n"),
                    });
                }
            }
        }
    }
}

// ───────────────────────────── family: modvar ─────────────────────────────

/// A module `var` and one way of mutating it.
struct VarForm {
    tag: &'static str,
    prelude: &'static str,
    /// `var NAME... = ...` without the name: the annotation and initializer.
    decl: &'static str,
    /// The mutation, from the place `p`, the local `l` (holding 3) and the
    /// qualifier `h` of the declaring module's helpers at the site.
    mutate: fn(p: &str, l: &str, h: &str) -> String,
    obs: fn(p: &str) -> String,
    val: i64,
}

fn var_forms() -> Vec<VarForm> {
    vec![
        VarForm { tag: "assign", prelude: "", decl: " = 0", mutate: |p, l, _| format!("{p} = {l} + 1"), obs: |p| p.into(), val: 4 },
        VarForm {
            tag: "index",
            prelude: "",
            decl: ": List[Int] = [0, 7]",
            mutate: |p, l, _| format!("{p}[0] = {l} * 2"),
            obs: |p| format!("{p}[0] + {p}[1]"),
            val: 13,
        },
        VarForm {
            tag: "field",
            prelude: "type Box = { v: Int, w: Int }\n",
            decl: ": Box = { v: 0, w: 7 }",
            mutate: |p, l, _| format!("{p}.v = {l} * 2"),
            obs: |p| format!("{p}.v + {p}.w"),
            val: 13,
        },
        VarForm {
            tag: "field_index",
            prelude: "type Bag = { items: List[Int] }\n",
            decl: ": Bag = { items: [0, 7] }",
            mutate: |p, l, _| format!("{p}.items[0] = {l} * 2"),
            obs: |p| format!("{p}.items[0] + {p}.items[1]"),
            val: 13,
        },
        VarForm {
            tag: "append",
            prelude: "",
            decl: ": List[Int] = [7]",
            mutate: |p, l, _| format!("{p} = {p} + [{l} * 2]"),
            obs: |p| format!("list.fold({p}, 0, (a, x) => a + x)"),
            val: 13,
        },
        VarForm {
            tag: "mut_arg",
            prelude: "pub fn put1(mut xs: List[Int], n: Int) -> Unit = list.push(xs, n)\n",
            decl: ": List[Int] = [7]",
            mutate: |p, l, h| format!("{h}put1({p}, {l} * 2)"),
            obs: |p| format!("list.fold({p}, 0, (a, x) => a + x)"),
            val: 13,
        },
        VarForm {
            tag: "mut_args_read",
            prelude: "pub fn put2(mut xs: List[Int], mut ys: List[Int], n: Int) -> Unit = {\n  list.push(xs, n)\n  ys[0] = n\n}\n",
            decl: ": List[Int] = [7]",
            mutate: |p, l, h| format!("var other: List[Int] = [0]\n{h}put2(other, {p}, {l} + {p}[0])"),
            obs: |p| format!("{p}[0]"),
            val: 10,
        },
    ]
}

/// Where the mutation runs. `own_*` sites are fns of the declaring module;
/// `root_*` sites are fns of the entry module (another module whenever the
/// layout has one).
const VAR_SITES: [&str; 8] =
    ["own_fn", "own_closure", "own_nested", "own_hof", "root_fn", "root_closure", "root_nested", "root_hof"];

/// The local feeding the mutation: `k`, or `c` — the parameter name of the
/// native leg's generated `X.with(|c| ...)` accessor (#3304).
const VAR_NAMINGS: [&str; 3] = ["local_k", "local_c", "case_twin"];

fn mutation_fn(fn_kw: &str, name: &str, l: &str, site_shape: &str, mutate: &str) -> String {
    let body = match site_shape {
        "fn" => mutate.to_string(),
        "closure" => format!("let f = () => {{\n{}\n}}\nf()", indent(mutate, 2)),
        "nested" => format!("let f = () => {{\n  let g = () => {{\n{}\n  }}\n  g()\n}}\nf()", indent(mutate, 4)),
        "hof" => format!("let _ = list.map([1], (i) => {{\n{}\n  i\n}})", indent(mutate, 2)),
        _ => unreachable!(),
    };
    format!("{fn_kw} {name}({l}: Int) -> Unit = {{\n{}\n}}\n", indent(&body, 2))
}

fn modvar_cells(out: &mut Vec<Cell>) {
    for layout in Layout::ALL {
        for form in var_forms() {
            for site in VAR_SITES {
                // In one file the entry IS the declaring module: the root_*
                // sites would repeat the own_* ones.
                if layout == Layout::Single && site.starts_with("root_") {
                    continue;
                }
                // Another module's `var` is not a `var` binding of the caller:
                // passing it to a `mut` parameter is E032 by the language.
                if site.starts_with("root_") && layout != Layout::Single && form.tag.starts_with("mut_arg") {
                    continue;
                }
                for naming in VAR_NAMINGS {
                    let q = layout.qual();
                    let l = if naming == "local_c" { "c" } else { "k" };
                    let (own, shape) = site.split_once('_').unwrap();
                    let mut decls = String::from(form.prelude);
                    let _ = writeln!(decls, "var buf{}", form.decl);
                    let mut val = form.val;
                    if naming == "case_twin" {
                        // A top-level let whose name differs from the var's only in case.
                        decls.push_str("let BUF = 100\n");
                        val += 100;
                    }
                    let mut root = String::new();
                    let call = if own == "own" {
                        decls.push_str(&mutation_fn("pub fn", "touch", l, shape, &(form.mutate)("buf", l, "")));
                        format!("{q}touch(3)")
                    } else {
                        root.push_str(&mutation_fn("fn", "poke", l, shape, &(form.mutate)(&format!("{q}buf"), l, q)));
                        "poke(3)".to_string()
                    };
                    let mut obs = (form.obs)(&format!("{q}buf"));
                    if naming == "case_twin" {
                        obs = format!("{obs} + {q}BUF");
                    }
                    let stmts = format!("{call}\nlet v = {obs}");
                    let body = format!("{root}{}", main_printing(&stmts));
                    let (files, entry) = layout_project(layout, &decls, &body);
                    out.push(Cell {
                        name: format!("modvar/{}.{}.{}.{}", layout.tag(), form.tag, site, naming),
                        files,
                        entry,
                        expected: format!("{val}\n"),
                    });
                }
            }
        }
    }
}

// ───────────────────────────── family: mutargs ─────────────────────────────

/// Several `mut` arguments plus reads of the same vars in later (or earlier)
/// arguments (#3306). `(tag, callee decl, call, expected)`; the vars are
/// `out = [5]` and `w = [0, 7]`, the program prints `w[0] * 100 + len(out)`.
const MUTARG_SHAPES: [(&str, &str, &str, i64); 6] = [
    ("one_mut_read_same", "fn f1(mut w: List[Int], n: Int) -> Unit = { w[0] = n }", "f1(W, 8 - W[1])", 101),
    (
        "two_mut_read_second",
        "fn f2(mut out: List[Int], mut w: List[Int], n: Int) -> Unit = {\n  list.push(out, n)\n  w[0] = n\n}",
        "f2(OUT, W, 8 - W[1])",
        102,
    ),
    (
        "two_mut_read_first",
        "fn f2(mut out: List[Int], mut w: List[Int], n: Int) -> Unit = {\n  list.push(out, n)\n  w[0] = n\n}",
        "f2(OUT, W, OUT[0] - 4)",
        102,
    ),
    (
        "two_mut_read_both",
        "fn f2(mut out: List[Int], mut w: List[Int], n: Int) -> Unit = {\n  list.push(out, n)\n  w[0] = n\n}",
        "f2(OUT, W, OUT[0] - W[1] + 3)",
        102,
    ),
    ("read_before_mut", "fn f3(n: Int, mut w: List[Int]) -> Unit = { w[0] = n }", "f3(8 - W[1], W)", 101),
    (
        "two_mut_read_len",
        "fn f2(mut out: List[Int], mut w: List[Int], n: Int) -> Unit = {\n  list.push(out, n)\n  w[0] = n\n}",
        "f2(OUT, W, list.len(W) - 1)",
        102,
    ),
];

/// Where the two vars live: locals of `main`, or module vars of the entry.
/// (A sibling module's `var` passed to a `mut` parameter is E032 — not a
/// program.)
const MUTARG_ORIGINS: [&str; 2] = ["local", "module_same"];

fn mutargs_cells(out: &mut Vec<Cell>) {
    for (tag, callee, call, val) in MUTARG_SHAPES {
        for origin in MUTARG_ORIGINS {
            let (decl_vars, outv, wv, layout) = match origin {
                "local" => (String::new(), "out", "w", Layout::Single),
                "module_same" => ("var out: List[Int] = [5]\nvar w: List[Int] = [0, 7]\n".to_string(), "out", "w", Layout::Single),
                _ => ("var out: List[Int] = [5]\nvar w: List[Int] = [0, 7]\n".to_string(), "m.out", "m.w", Layout::Sibling),
            };
            let call = call.replace("OUT", outv).replace("W", wv);
            let locals = if origin == "local" { "var out: List[Int] = [5]\nvar w: List[Int] = [0, 7]\n" } else { "" };
            let stmts = format!("{locals}{call}\nlet v = {wv}[0] * 100 + list.len({outv})");
            // `val` was written for n landing in w[0]; recompute it.
            let n = match tag {
                "one_mut_read_same" | "read_before_mut" | "two_mut_read_second" => 1,
                "two_mut_read_first" | "two_mut_read_both" | "two_mut_read_len" => 1,
                _ => unreachable!(),
            };
            let pushes = if callee.contains("list.push") { 1 } else { 0 };
            let expect = n * 100 + 1 + pushes;
            let _ = val;
            let (decls, body) = match layout {
                Layout::Single => (String::new(), format!("{decl_vars}{callee}\n{}", main_printing(&stmts))),
                _ => (decl_vars, format!("{callee}\n{}", main_printing(&stmts))),
            };
            let (files, entry) = layout_project(layout, &decls, &body);
            out.push(Cell { name: format!("mutargs/{origin}.{tag}"), files, entry, expected: format!("{expect}\n") });
        }
    }
}

// ───────────────────────────── family: bind ─────────────────────────────

/// A payload type a pattern binding can carry, with a value, an initial
/// value for the sink, and an Int observation.
struct Payload {
    tag: &'static str,
    prelude: &'static str,
    ty: &'static str,
    value: &'static str,
    init: &'static str,
    obs: fn(&str) -> String,
    val: i64,
}

fn payloads() -> Vec<Payload> {
    vec![
        Payload { tag: "int", prelude: "", ty: "Int", value: "5", init: "0", obs: |r| r.into(), val: 5 },
        Payload { tag: "string", prelude: "", ty: "String", value: "\"hello\"", init: "\"\"", obs: |r| format!("string.len({r})"), val: 5 },
        Payload { tag: "list", prelude: "", ty: "List[Int]", value: "[2, 3]", init: "[]", obs: |r| format!("list.len({r}) + {r}[1]"), val: 5 },
        Payload {
            tag: "record",
            prelude: "type C = { r: Int }\n",
            ty: "C",
            value: "{ r: 5 }",
            init: "{ r: 0 }",
            obs: |r| format!("{r}.r"),
            val: 5,
        },
        Payload {
            tag: "bytes",
            prelude: "",
            ty: "Bytes",
            value: "bytes.from_list([1, 2, 3, 4, 5])",
            init: "bytes.new(0)",
            obs: |r| format!("bytes.len({r})"),
            val: 5,
        },
    ]
}

/// The binding forms that introduce `b`, each wrapping a `sink` statement
/// that moves `b` somewhere outside the binding's scope. `TY` / `VAL` are
/// replaced with the payload's type and value.
const BIND_FORMS: [&str; 7] = ["match_arm", "match_option", "destructure_record", "destructure_tuple", "let", "for_in", "lambda_param"];

/// Where the bound value comes from: a local of `run`, or a parameter of
/// `run` (borrowed on the native leg — #3303's `fn pick(p: P)`).
const BIND_SOURCES: [&str; 2] = ["local", "param"];

/// `(extra declarations, the source's type, the source's value, statements
/// over the source `src`)` for a binding form.
fn bind_form(form: &str, sink: &str) -> (&'static str, &'static str, &'static str, String) {
    match form {
        "match_arm" => (
            "type Pv = | Has(TY) | Nope\n",
            "Pv",
            "Has(VAL)",
            format!("match src {{\n  Has(b) => {{ {sink} }},\n  Nope => (),\n}}"),
        ),
        "match_option" => (
            "",
            "Option[TY]",
            "some(VAL)",
            format!("match src {{\n  some(b) => {{ {sink} }},\n  none => (),\n}}"),
        ),
        "destructure_record" => ("type Hr = { b: TY, n: Int }\n", "Hr", "{ b: VAL, n: 1 }", format!("let {{ b, n }} = src\n{sink}")),
        "destructure_tuple" => ("", "(TY, Int)", "(VAL, 1)", format!("let (b, n) = src\n{sink}")),
        "let" => ("", "TY", "VAL", format!("let b = src\n{sink}")),
        "for_in" => ("", "List[TY]", "[VAL]", format!("for b in src {{\n  {sink}\n}}")),
        "lambda_param" => ("", "TY", "VAL", format!("let put = (b: TY) => {{ {sink} }}\nput(src)")),
        _ => unreachable!(),
    }
}

/// Where `b` goes: an outer local `var`, an element pushed onto an outer list,
/// a field of an outer record `var`, or a module `var`.
const BIND_SINKS: [&str; 4] = ["outer_var", "outer_push", "outer_field", "module_var"];

fn bind_cells(out: &mut Vec<Cell>) {
    for p in payloads() {
        for form in BIND_FORMS {
            for source in BIND_SOURCES {
                for sink in BIND_SINKS {
                    // A lambda assigning a captured local var / its field is
                    // refused by the checker (a closure captures by value):
                    // only the push and module-var sinks are programs.
                    if form == "lambda_param" && (sink == "outer_var" || sink == "outer_field") {
                        continue;
                    }
                    let (sink_stmt, pre, obs) = match sink {
                        "outer_var" => ("slot = b".to_string(), "var slot: TY = INIT\n".to_string(), (p.obs)("slot")),
                        "outer_push" => ("list.push(acc, b)".to_string(), "var acc: List[TY] = []\n".to_string(), (p.obs)("acc[0]")),
                        "outer_field" => {
                            ("holder.f = b".to_string(), "var holder: Ho = { f: INIT, k: 0 }\n".to_string(), (p.obs)("holder.f"))
                        }
                        "module_var" => ("gslot = b".to_string(), String::new(), (p.obs)("gslot")),
                        _ => unreachable!(),
                    };
                    let (extra, src_ty, src_val, stmts) = bind_form(form, &sink_stmt);
                    let mut decls = format!("{}{}", p.prelude, extra);
                    if sink == "outer_field" {
                        decls.push_str("type Ho = { f: TY, k: Int }\n");
                    }
                    if sink == "module_var" {
                        decls.push_str("var gslot: TY = INIT\n");
                    }
                    let (params, local, arg) = if source == "param" {
                        (format!("src: {src_ty}"), String::new(), src_val.to_string())
                    } else {
                        (String::new(), format!("let src: {src_ty} = {src_val}\n"), String::new())
                    };
                    let body = format!("{local}{pre}{stmts}\nlet v = {obs}");
                    // A closure handing the enclosing `var` to an in-place
                    // mutator is E011 in a pure fn (dialect epoch 9, #3344):
                    // that cell's `run` is an effect fn.
                    let (kw, bang) = if form == "lambda_param" && sink == "outer_push" { ("effect fn", "!") } else { ("fn", "") };
                    let src = format!(
                        "{decls}\n{kw} run({params}) -> Int = {{\n{}\n  v\n}}\n\neffect fn main() -> Unit = println(int.to_string(run({arg}){bang}))\n",
                        indent(&body, 2)
                    )
                    .replace("TY", p.ty)
                    .replace("VAL", p.value)
                    .replace("INIT", p.init);
                    let (files, entry) = project(vec![], &src);
                    out.push(Cell {
                        name: format!("bind/{form}.{}.{source}.{sink}", p.tag),
                        files,
                        entry,
                        expected: format!("{}\n", p.val),
                    });
                }
            }
        }
    }
}

// ───────────────────────────── family: samerec ─────────────────────────────

/// Where module `b` puts a `{ w, h }` literal that must become ITS `Extent`
/// while module `a` declares a `Size` with the same fields.
const REC_SITES: [&str; 11] = [
    "return_slot",
    "annotated_let",
    "unannotated_let_returned",
    "unannotated_let_passed",
    "arg",
    "list_element",
    "record_field",
    "lambda_body",
    "annotated_var",
    "var_reassigned",
    "entry_annotated",
];

/// Where `a` and `b` live: both siblings, both packages, or one of each.
const REC_LAYOUTS: [&str; 4] = ["siblings", "packages", "a_pkg_b_sib", "a_sib_b_pkg"];

fn rec_site_body(site: &str) -> String {
    let lit = "{ w: n, h: n * 2 }";
    match site {
        "return_slot" => format!("fn mk(n: Int) -> Extent = {lit}\npub fn probe(n: Int) -> Int = ext_sum(mk(n))\n"),
        "annotated_let" => format!("pub fn probe(n: Int) -> Int = {{\n  let e: Extent = {lit}\n  ext_sum(e)\n}}\n"),
        "unannotated_let_returned" => {
            format!("fn mk(n: Int) -> Extent = {{\n  let e = {lit}\n  e\n}}\npub fn probe(n: Int) -> Int = ext_sum(mk(n))\n")
        }
        "unannotated_let_passed" => format!("pub fn probe(n: Int) -> Int = {{\n  let e = {lit}\n  ext_sum(e)\n}}\n"),
        "arg" => format!("pub fn probe(n: Int) -> Int = ext_sum({lit})\n"),
        "list_element" => format!("pub fn probe(n: Int) -> Int = {{\n  let xs: List[Extent] = [{lit}]\n  ext_sum(xs[0])\n}}\n"),
        "record_field" => {
            format!("type Wrap = {{ e: Extent }}\npub fn probe(n: Int) -> Int = {{\n  let wr: Wrap = {{ e: {lit} }}\n  ext_sum(wr.e)\n}}\n")
        }
        "lambda_body" => {
            "pub fn probe(n: Int) -> Int = {\n  let f: (Int) -> Extent = (k) => { w: k, h: k * 2 }\n  ext_sum(f(n))\n}\n".to_string()
        }
        "annotated_var" => format!("pub fn probe(n: Int) -> Int = {{\n  var e: Extent = {lit}\n  e.w = e.w + 0\n  ext_sum(e)\n}}\n"),
        "var_reassigned" => {
            format!("pub fn probe(n: Int) -> Int = {{\n  var e: Extent = {{ w: 0, h: 0 }}\n  e = {lit}\n  ext_sum(e)\n}}\n")
        }
        // The literal sits in the entry, annotated with b's qualified type.
        "entry_annotated" => "pub fn probe(n: Int) -> Int = n * 3\n".to_string(),
        _ => unreachable!(),
    }
}

fn samerec_cells(out: &mut Vec<Cell>) {
    for lay in REC_LAYOUTS {
        for order in ["a_first", "b_first"] {
            for site in REC_SITES {
                let a_src = "type Size = { w: Int, h: Int }\npub fn unit() -> Size = { w: 1, h: 1 }\npub fn size_sum(s: Size) -> Int = s.w + s.h\n";
                let b_src = format!("type Extent = {{ w: Int, h: Int }}\npub fn ext_sum(e: Extent) -> Int = e.w + e.h\n{}", rec_site_body(site));
                let (a_pkg, b_pkg) = match lay {
                    "siblings" => (false, false),
                    "packages" => (true, true),
                    "a_pkg_b_sib" => (true, false),
                    _ => (false, true),
                };
                let a = Module { name: "a".into(), package: a_pkg, src: a_src.into() };
                let b = Module { name: "b".into(), package: b_pkg, src: b_src };
                let mods = if order == "a_first" { vec![a, b] } else { vec![b, a] };
                let probe = if site == "entry_annotated" {
                    "let e: b.Extent = { w: 3, h: 6 }\nlet v = a.size_sum(a.unit()) + b.ext_sum(e)"
                } else {
                    "let v = a.size_sum(a.unit()) + b.probe(3)"
                };
                let (files, entry) = project(mods, &main_printing(probe));
                out.push(Cell {
                    name: format!("samerec/{lay}.{order}.{site}"),
                    files,
                    entry,
                    expected: "11\n".into(),
                });
            }
        }
    }
}

// ───────────────────────────── family: depclos ─────────────────────────────

/// A dependency fn builds a record of closures; the fns those closures call
/// are reachable ONLY through the closure bodies (#3296).
const DEPCLOS_ROOTS: [&str; 3] = ["named_fn", "lambda", "wrapped_named"];
/// Who holds the root: a `var` in the dependency (an app registry), a top-level
/// let in the entry, or nobody (passed straight through).
const DEPCLOS_HOLDERS: [&str; 3] = ["dep_var", "entry_let", "direct"];
/// What calls the dispatch: `main`, or an exported fn `main` also calls.
const DEPCLOS_ENTRIES: [&str; 2] = ["main", "export"];

fn depclos_cells(out: &mut Vec<Cell>) {
    for layout in [Layout::Sibling, Layout::Package] {
        for root in DEPCLOS_ROOTS {
            for holder in DEPCLOS_HOLDERS {
                for entry_kind in DEPCLOS_ENTRIES {
                    // A top-level let cannot be bound to a lambda (E061): the
                    // entry_let holder takes the named fn only.
                    if holder == "entry_let" && root != "named_fn" {
                        continue;
                    }
                    let q = layout.qual();
                    let dep = "type View = { paint: (Int) -> Int, key: (Int) -> Int, click: () -> Int }\n\
                               fn paint_impl(n: Int) -> Int = n * 2\n\
                               fn key_impl(n: Int) -> Int = n + 1\n\
                               fn noop() -> Int = 0\n\
                               pub fn view(seed: Int) -> View = {\n  paint: (n) => paint_impl(n + seed),\n  key: (n) => key_impl(n),\n  click: noop,\n}\n\
                               fn default_root() -> View = view(0)\n\
                               var app_root: () -> View = default_root\n\
                               pub fn app(r: () -> View) -> Unit = { app_root = r }\n\
                               pub fn dispatch(n: Int) -> Int = {\n  let vw = app_root()\n  vw.paint(n) + vw.key(n) + vw.click()\n}\n\
                               pub fn dispatch_with(r: () -> View, n: Int) -> Int = {\n  let vw = r()\n  vw.paint(n) + vw.key(n) + vw.click()\n}\n";
                    let mut entry_src = String::new();
                    let root_expr = match root {
                        "named_fn" => {
                            let _ = writeln!(entry_src, "fn root() -> {q}View = {q}view(1)");
                            "root".to_string()
                        }
                        "lambda" => format!("() => {q}view(1)"),
                        _ => {
                            let _ = writeln!(entry_src, "fn root() -> {q}View = {q}view(1)");
                            "() => root()".to_string()
                        }
                    };
                    let dispatch_call = match holder {
                        "dep_var" => format!("{q}dispatch(n)"),
                        "entry_let" => {
                            let _ = writeln!(entry_src, "let ROOT: () -> {q}View = {root_expr}");
                            format!("{q}dispatch_with(ROOT, n)")
                        }
                        _ => format!("{q}dispatch_with({root_expr}, n)"),
                    };
                    let setup = if holder == "dep_var" { format!("{q}app({root_expr})\n") } else { String::new() };
                    let stmts = if entry_kind == "export" {
                        let _ = writeln!(
                            entry_src,
                            "@export(wasm, \"shape_event\")\nfn shape_event(n: Int) -> Int = {dispatch_call}"
                        );
                        format!("{setup}let v = shape_event(2)")
                    } else {
                        let _ = writeln!(entry_src, "fn ev(n: Int) -> Int = {dispatch_call}");
                        format!("{setup}let v = ev(2)")
                    };
                    // paint(2 + 1) = 6, key(2) = 3, click = 0
                    let body = format!("{entry_src}{}", main_printing(&stmts));
                    let (files, entry) = layout_project(layout, dep, &body);
                    out.push(Cell {
                        name: format!("depclos/{}.{root}.{holder}.{entry_kind}", layout.tag()),
                        files,
                        entry,
                        expected: "9\n".into(),
                    });
                }
            }
        }
    }
}

// ───────────────────────────── random shapes ─────────────────────────────
//
// The fuzzer's family (`xtarget-fuzz --family shape`): the same vocabulary as
// the matrix, sampled past its bounds — several declaring modules at once
// (each a sibling or a path-dependency package, plus at most one part hosted
// in the entry itself), several lets per module, closure nesting deeper than
// two, and names drawn from an adversarial pool: case-only twins, the native
// leg's generated binders (`c`, `_fn_arg0`, `__cap_*`, `__almide_*`) and
// spellings of its mangled statics (`almide_rt_<module>_<name>`). Every
// program still knows its stdout by construction.

/// A random source: the fuzzer passes its SplitMix64, the gate never calls this.
pub struct Draw<'a>(pub &'a mut dyn FnMut() -> u64);

impl Draw<'_> {
    pub fn below(&mut self, n: usize) -> usize {
        ((self.0)() % n as u64) as usize
    }
    fn chance(&mut self, num: u64, den: u64) -> bool {
        (self.0)() % den < num
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
}

/// Names for a top-level let or a module var. None of them is a name the
/// templates use themselves (`f`, `g`, `v`, `acc`, `i`, `a`, `x`, `n`, `k`
/// of a mutation site), and none is a stdlib module name.
const ADV_GLOBALS: [&str; 12] =
    ["thing", "THING", "c", "C", "cell", "CELL", "it", "IT", "_fn_arg0", "__cap_v", "__almide_cell", "tl"];
/// Names for the local that feeds a module-var mutation.
const ADV_LOCALS: [&str; 6] = ["c", "cell", "it", "_fn_arg0", "__almide_cell", "q"];

/// `use_` read site with closures nested `depth` deep (`depth` ≥ 1 for the
/// closure sites; ignored by the others).
fn read_site_deep(use_: &str, o: &str, q: &str, probe: &str, depth: usize) -> String {
    match use_ {
        "closure" | "nested_closure" => {
            let mut inner = o.to_string();
            for d in 0..depth {
                inner = if d == 0 { format!("() => {inner}") } else { format!("() => {{\n  let g{d} = {inner}\n  g{d}()\n}}") };
            }
            format!("let f = {inner}\nlet v = f()")
        }
        "own_fn" | "own_fn_closure" => format!("let v = {q}{probe}()"),
        _ => read_site(use_, o, q),
    }
}

struct Part {
    decls: String,
    root_decls: String,
    stmts: String,
    val: i64,
}

fn random_toplet_part(d: &mut Draw, q: &str, modname: Option<&str>, i: usize, root_names: &mut Vec<String>) -> Part {
    let kinds = let_kinds();
    let kind = &kinds[d.below(kinds.len())];
    let mut decls = String::from(kind.prelude);
    let mut names: Vec<&str> = Vec::new();
    for _ in 0..1 + d.below(3) {
        let n = d.pick(&ADV_GLOBALS);
        // A callable kind is only ever named with a lower-case initial (an
        // upper-case one in call position is a constructor, E003).
        if names.contains(&n) || (kind.callable && !n.starts_with(|c: char| c.is_ascii_lowercase())) {
            continue;
        }
        names.push(n);
        let _ = writeln!(decls, "let {n}{} = {}", kind.annot, kind.init);
    }
    if names.is_empty() {
        names.push("tl");
        let _ = writeln!(decls, "let tl{} = {}", kind.annot, kind.init);
    }
    let obs_at = |qual: &str| names.iter().map(|n| format!("({})", (kind.obs)(&format!("{qual}{n}")))).collect::<Vec<_>>().join(" + ");
    let use_ = d.pick(&LET_USES);
    let probe = format!("probe{i}");
    match use_ {
        "own_fn" => {
            let _ = writeln!(decls, "pub fn {probe}() -> Int = {}", obs_at(""));
        }
        "own_fn_closure" => {
            let _ = writeln!(decls, "pub fn {probe}() -> Int = {{\n  let f = () => {}\n  f()\n}}", obs_at(""));
        }
        _ => {}
    }
    let depth = 1 + d.below(4);
    let mut stmts = read_site_deep(use_, &obs_at(q), q, &probe, depth);
    let mut val = kind.val * names.len() as i64;
    let mut root_decls = String::new();
    // Spell a root let as this module's mangled static.
    if let Some(m) = modname {
        if d.chance(1, 3) {
            let spelled = format!("almide_rt_{m}_{}", names[0].to_ascii_lowercase());
            if !root_names.contains(&spelled) {
                let _ = writeln!(root_decls, "let {spelled} = 100");
                stmts.push_str(&format!("\nlet v = v + {spelled}"));
                root_names.push(spelled);
                val += 100;
            }
        }
    }
    Part { decls, root_decls, stmts, val }
}

fn random_modvar_part(d: &mut Draw, q: &str, hosted_in_root: bool, i: usize) -> Part {
    let forms = var_forms();
    let form = &forms[d.below(forms.len())];
    let mut var = d.pick(&ADV_GLOBALS);
    // An all-caps name in argument position reads as a constructor, so an
    // upper-case var cannot be handed to a `mut` parameter (E032 "temporary").
    if form.tag.starts_with("mut_arg") && var.starts_with(|c: char| c.is_ascii_uppercase()) {
        var = "cell";
    }
    let mut l = d.pick(&ADV_LOCALS);
    if l == var {
        l = "k";
    }
    let mut decls = String::from(form.prelude);
    let _ = writeln!(decls, "var {var}{}", form.decl);
    let mut val = form.val;
    let mut obs = (form.obs)(&format!("{q}{var}"));
    // A let differing from the var only in case.
    let twin = if var.starts_with(|c: char| c.is_ascii_lowercase()) { var.to_ascii_uppercase() } else { var.to_ascii_lowercase() };
    if twin != var && d.chance(1, 3) {
        let _ = writeln!(decls, "let {twin} = 100");
        obs = format!("{obs} + {q}{twin}");
        val += 100;
    }
    let shapes = ["fn", "closure", "nested", "hof"];
    let shape = d.pick(&shapes);
    // Another module's var cannot be passed to a `mut` parameter (E032).
    let from_root = !hosted_in_root && !form.tag.starts_with("mut_arg") && d.chance(1, 2);
    let mut root_decls = String::new();
    let call = if from_root {
        let name = format!("poke{i}");
        root_decls.push_str(&mutation_fn("fn", &name, l, shape, &(form.mutate)(&format!("{q}{var}"), l, q)));
        format!("{name}(3)")
    } else {
        let name = format!("touch{i}");
        decls.push_str(&mutation_fn("pub fn", &name, l, shape, &(form.mutate)(var, l, "")));
        format!("{q}{name}(3)")
    };
    Part { decls, root_decls, stmts: format!("{call}\nlet v = {obs}"), val }
}

/// One random multi-module program. `name` is the caller's (seed, index).
pub fn random_cell(next: &mut dyn FnMut() -> u64, name: &str) -> Cell {
    let mut d = Draw(next);
    let n_parts = 1 + d.below(4);
    let mut modules = Vec::new();
    let mut root = String::new();
    let mut root_names = Vec::new();
    let mut root_hosted = false;
    let mut calls = Vec::new();
    let mut total = 0;
    for i in 0..n_parts {
        let in_root = !root_hosted && d.chance(1, 4);
        let package = !in_root && d.chance(1, 3);
        let modname = if package { format!("p{i}") } else { format!("m{i}") };
        let q = if in_root { String::new() } else { format!("{modname}.") };
        let part = if d.chance(3, 5) {
            random_toplet_part(&mut d, &q, (!in_root).then_some(modname.as_str()), i, &mut root_names)
        } else {
            random_modvar_part(&mut d, &q, in_root, i)
        };
        if in_root {
            root_hosted = true;
            root.push_str(&part.decls);
        } else {
            modules.push(Module { name: modname, package, src: part.decls });
        }
        root.push_str(&part.root_decls);
        let _ = writeln!(root, "fn part{i}() -> Int = {{\n{}\n  v\n}}", indent(&part.stmts, 2));
        calls.push(format!("part{i}()"));
        total += part.val;
    }
    let body = format!("{root}{}", main_printing(&format!("let v = {}", calls.join(" + "))));
    let (files, entry) = project(modules, &body);
    Cell { name: name.to_string(), files, entry, expected: format!("{total}\n") }
}

// ───────────────────────────── the matrix ─────────────────────────────

/// Every cell, in a stable order.
pub fn all_cells() -> Vec<Cell> {
    let mut out = Vec::new();
    toplet_cells(&mut out);
    modvar_cells(&mut out);
    mutargs_cells(&mut out);
    bind_cells(&mut out);
    samerec_cells(&mut out);
    depclos_cells(&mut out);
    out
}

/// FNV-1a of the cell name: the deterministic slice key. Stable across
/// platforms and Rust versions (unlike `DefaultHasher`).
pub fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
