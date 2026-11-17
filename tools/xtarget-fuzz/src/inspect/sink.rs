//! The inspection of an `@input_sink` function (`http.decode_json[T]`): its
//! type parameter takes only a user-declared input type (row 22, E110), which
//! the catalogue's universe does not hold (`catalogue.rs` leaves it out), so
//! every probe here declares one: `type In: Decode = { … }`, a record that is
//! `Decode` and not `Encode`.
//!
//! - parity: freshly generated programs, each declaring its own input type
//!   (fields drawn from Int / String / Bool / List[Int] / Option[String]) and
//!   decoding bodies built for it — a valid one, one with an unknown key, one
//!   missing a field, one with a field of the wrong type, one that is not JSON
//!   — the decoded record (or the 400 response) printed and composed through
//!   `result.map` / `??`; native and wasm agree byte for byte.
//! - complexity: n = the body's `k` list length, k = calls on one request;
//!   counted like every other row (`judge`).
//! - alloc: the wasm leg leaves nothing live after k calls.
//! - diamond: `base` declares `type In` (public by default) and wraps the call, `left` and
//!   `right` re-export it, `main` decodes directly and through both paths.

use std::fmt::Write as _;

use super::{alloc_part_exiting, build, build_as, diamond_verdict_exiting, judge, run_artifact, run_diamond, toml_str, Ctx, Leg, World};
use crate::generator::Project;
use crate::oracle::sandbox;
use crate::rng::SplitMix64;

pub(super) const SINKS: &[&str] = &["http.decode_json"];

/// The measured input type: n list elements and a fixed string.
const IN_TYPE: &str = "type In: Decode = { k: List[Int], s: String }";

/// A JSON body for `In` whose `k` holds n one-digit Ints.
fn sized_body(n: u64) -> String {
    format!("\"{{\\\"k\\\": [\" + (list.range(0, {n}) |> list.map((i) => \"${{i % 10}}\") |> list.join(\", \")) + \"], \\\"s\\\": \\\"aé\\\"}}\"")
}

/// The complexity / alloc probe: k decodes of one request whose body is sized n.
fn call_program(n: u64, k: u64, with_call: bool) -> String {
    let mut s = format!("import http\n\n{IN_TYPE}\n\neffect fn main() -> Unit = {{\n");
    let _ = writeln!(s, "  let body: String = {}", sized_body(n));
    s.push_str("  let req: HttpRequest = http.new_request(\"POST\", \"/j\", body, [(\"content-type\", \"application/json\")])\n");
    let _ = writeln!(s, "  var acc = 0\n  var i = 0\n  while i < {k} {{");
    if with_call {
        s.push_str("    let r: Result[In, HttpResponse] = http.decode_json[In](req)\n");
        s.push_str("    acc = acc + (match r {\n      ok(x) => list.len(x.k),\n      err(e) => panic(\"inspect: the probed call refused its in-domain input: ${http.status(e)}\"),\n    })\n");
    }
    s.push_str("    acc = acc + 1\n    i = i + 1\n  }\n  println(\"${acc}\")\n}\n");
    s
}

/// One generated field: its name, type and three JSON spellings (a value of
/// its type, a value of another type, and whether it may be left out).
struct Field {
    name: String,
    ty: &'static str,
    good: String,
    bad: &'static str,
    show: String,
}

fn field(rng: &mut SplitMix64, i: usize) -> Field {
    let name = format!("f{i}");
    let v = rng.in_range(-50, 50);
    match rng.below(5) {
        0 => Field { show: format!("${{x.{name}}}"), name, ty: "Int", good: v.to_string(), bad: "\\\"7\\\"" },
        1 => Field { show: format!("${{x.{name}}}"), name, ty: "String", good: format!("\\\"é{v}\\\""), bad: "7" },
        2 => Field { show: format!("${{x.{name}}}"), name, ty: "Bool", good: (v > 0).to_string(), bad: "1" },
        3 => Field {
            show: format!("${{list.len(x.{name})}}/${{list.fold(x.{name}, 0, (a, e) => a + e)}}"),
            name,
            ty: "List[Int]",
            good: format!("[{v}, {}, 3]", v * 2),
            bad: "[\\\"a\\\"]",
        },
        _ => Field { show: format!("${{opt(x.{name})}}"), name, ty: "String?", good: format!("\\\"o{v}\\\""), bad: "[]" },
    }
}

/// The JSON object text of `fields` with field `skip` left out, `wrong`'s
/// value of the wrong type, and an `extra` key appended.
fn object(fields: &[Field], skip: Option<usize>, wrong: Option<usize>, extra: bool) -> String {
    let mut parts: Vec<String> = fields
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .map(|(i, f)| format!("\\\"{}\\\": {}", f.name, if Some(i) == wrong { f.bad.to_string() } else { f.good.clone() }))
        .collect();
    if extra {
        parts.push("\\\"zz\\\": 1".into());
    }
    format!("{{{}}}", parts.join(", "))
}

/// Parity program `index` of the campaign seeded `seed`.
pub(super) fn parity_program(seed: u64, index: u64) -> String {
    let mut rng = SplitMix64::for_program(seed, index);
    let fields: Vec<Field> = (0..1 + rng.below(4) as usize).map(|i| field(&mut rng, i)).collect();
    let decl = fields.iter().map(|f| format!("{}: {}", f.name, f.ty)).collect::<Vec<_>>().join(", ");
    let show = fields.iter().map(|f| f.show.clone()).collect::<Vec<_>>().join(" ");
    let pick = rng.below(fields.len() as u32) as usize;
    let bodies = [
        object(&fields, None, None, false),
        object(&fields, None, None, true),
        object(&fields, Some(pick), None, false),
        object(&fields, None, Some(pick), false),
        format!("[{}]", object(&fields, None, None, false)),
        "not json {".to_string(),
    ];
    let mut s = format!("import http\n\ntype In: Decode = {{ {decl} }}\n\nfn opt(o: Option[String]) -> String = o ?? \"none\"\n\n");
    let _ = writeln!(s, "fn show(x: In) -> String = \"{show}\"\n");
    s.push_str("fn outcome(r: Result[In, HttpResponse]) -> String = match r {\n  ok(x) => \"ok \" + show(x),\n  err(e) => \"err ${http.status(e)} ${http.body(e)}\",\n}\n\n");
    s.push_str("effect fn main() -> Unit = {\n");
    for (b, body) in bodies.iter().enumerate() {
        let method = *rng.pick(&["POST", "PUT", "PATCH"]);
        let _ = writeln!(s, "  let q{b}: HttpRequest = http.new_request(\"{method}\", \"/in/{b}\", \"{body}\", [(\"content-type\", \"application/json\")])");
        let _ = writeln!(s, "  println(outcome(http.decode_json[In](q{b})))");
        match rng.below(3) {
            0 => {
                let _ = writeln!(s, "  let m{b}: String = http.decode_json[In](q{b}) |> result.map((x) => show(x)) ?? \"refused\"\n  println(m{b})");
            }
            1 => {
                let _ = writeln!(s, "  println(\"${{result.is_ok(http.decode_json[In](q{b}))}} ${{string.len(http.body(q{b}))}}\")");
            }
            _ => {}
        }
    }
    s.push_str("}\n");
    s
}

/// The diamond: `base` declares the input type and wraps the call; `left`
/// and `right` import base and wrap its `call`; `main` decodes directly and
/// through both paths, for a valid body and for one each path refuses.
fn diamond_project() -> Project {
    let base = format!("import http\n\n{IN_TYPE}\n\npub fn call(r: HttpRequest) -> Result[In, HttpResponse] = http.decode_json[In](r)\n");
    let path = "import http\nimport self.base\n\npub fn call(r: HttpRequest) -> Result[base.In, HttpResponse] = base.call(r)\n".to_string();
    let mut main = String::from("import http\nimport self.base\nimport self.left\nimport self.right\n\n");
    main.push_str("fn outcome(r: Result[base.In, HttpResponse]) -> String = match r {\n  ok(x) => \"ok ${list.len(x.k)} ${x.s}\",\n  err(e) => \"err ${http.status(e)} ${http.body(e)}\",\n}\n\n");
    main.push_str("effect fn main() -> Unit = {\n");
    for (b, body) in ["{\\\"k\\\": [1, 2, 3], \\\"s\\\": \\\"aé\\\"}", "{\\\"k\\\": [], \\\"s\\\": \\\"x\\\", \\\"zz\\\": 0}", "{\\\"k\\\": 1}"].iter().enumerate() {
        let _ = writeln!(main, "  let q{b}: HttpRequest = http.new_request(\"POST\", \"/d\", \"{body}\", [])");
        let _ = writeln!(main, "  println(\"d = \" + outcome(http.decode_json[base.In](q{b})))");
        let _ = writeln!(main, "  println(\"l = \" + outcome(left.call(q{b})))");
        let _ = writeln!(main, "  println(\"r = \" + outcome(right.call(q{b})))");
    }
    main.push_str("}\n");
    Project {
        files: vec![
            ("app/almide.toml".into(), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".into()),
            ("app/src/base.almd".into(), base),
            ("app/src/left.almd".into(), path.clone()),
            ("app/src/right.almd".into(), path),
            ("app/src/main.almd".into(), main),
        ],
        entry: "app/src/main.almd".into(),
    }
}

/// Each path's lines agree with the direct call's (`d = x`, `l = x`, `r = x`).
fn paths_agree(out: &str) -> bool {
    let lines: Vec<&str> = out.lines().collect();
    lines.chunks(3).all(|c| c.len() == 3 && c.iter().map(|l| l.get(4..)).all(|v| v == c[0].get(4..)))
}

fn diamond(ctx: &Ctx) -> String {
    let root = ctx.dir.join("diamond-sink");
    let Ok(entry) = diamond_project().write_to(&root) else {
        return "SKIP: scratch write failed".into();
    };
    let (Ok(nat), Ok(was)) = (run_diamond(ctx, &root, &entry, false), run_diamond(ctx, &root, &entry, true)) else {
        return "SKIP: could not run almide".into();
    };
    let v = diamond_verdict_exiting(&nat, &was, 0);
    if v.starts_with("PASS") && !paths_agree(&String::from_utf8_lossy(&nat.stdout)) {
        return format!("FAIL: a path disagrees with the direct call: {}", String::from_utf8_lossy(&nat.stdout));
    }
    v
}

fn complexity(ctx: &Ctx, n: u64, order: Option<&str>) -> String {
    let programs = |m: u64, k: u64| (call_program(m, k, true), call_program(m, k, false));
    match judge(ctx, "call(n=body list length, k=calls)", order, n, &programs) {
        Ok(s) => format!("PASS: {s}"),
        Err(s) => format!("FAIL: {s}"),
    }
}

fn alloc(ctx: &Ctx) -> String {
    let src = call_program(64, 3, true);
    let file = ctx.dir.join(format!("a{:016x}.almd", super::hash(&src)));
    if std::fs::write(&file, &src).is_err() {
        return "SKIP: scratch write failed".into();
    }
    let world = match World::for_program(&ctx.dir, &src) {
        Ok(w) => w,
        Err(e) => return format!("SKIP: {e}"),
    };
    let mut cmd = std::process::Command::new(&ctx.almide);
    cmd.arg("run").arg(&file).args(["--target", "wasm", "--"]).args(sandbox::ARGS).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    world.enter(&mut cmd);
    let Ok(out) = cmd.output() else {
        return "SKIP: could not run almide".into();
    };
    let (pass, part) = alloc_part_exiting("call", &out, 0);
    format!("{}: {part}", if pass { "PASS" } else { "FAIL" })
}

/// Both legs of one parity program: `None` when they agree.
fn one_parity(ctx: &Ctx, src: &str) -> Option<String> {
    let run = |leg: Leg| {
        let art = match leg {
            Leg::Native => build(ctx, src, leg),
            Leg::Wasm => build_as(ctx, src, leg, false),
        }?;
        run_artifact(ctx, &art, leg)
    };
    let (nat, was) = (run(Leg::Native), run(Leg::Wasm));
    (nat != was || nat.is_err()).then(|| format!("native {nat:?} vs wasm {was:?}"))
}

fn parity(ctx: &Ctx, args: &[String]) -> String {
    let flag = |k: &str, d: u64| super::super::flag_value(args, k).and_then(|s| s.parse().ok()).unwrap_or(d);
    let (count, seed) = (flag("--count", 200), flag("--seed", 1));
    let findings: Vec<(u64, String)> = (0..count)
        .filter_map(|i| one_parity(ctx, &parity_program(seed, i)).map(|f| (i, f)))
        .collect();
    match findings.first() {
        None => format!("PASS: seed {seed}, {count} programs; {count} program(s) call it, {count} of them clean on every leg; unique findings 0"),
        Some((i, f)) => format!("FAIL: seed {seed}, {count} programs; {} finding(s), first #{i}: {f}", findings.len()),
    }
}

pub(super) fn inspect(ctx: &mut Ctx, name: &str, args: &[String]) {
    let n: u64 = super::super::flag_value(args, "--n").and_then(|s| s.parse().ok()).unwrap_or(20_000);
    if let Some(out) = super::super::flag_value(args, "--emit") {
        let root = std::path::Path::new(out);
        let mut files = diamond_project().files;
        files.push(("call.almd".into(), call_program(n, 3, true)));
        files.push(("parity-0.almd".into(), parity_program(0, 0)));
        for (p, s) in files {
            let f = root.join(p);
            let _ = std::fs::create_dir_all(f.parent().unwrap_or(root));
            let _ = std::fs::write(&f, s);
        }
        return;
    }
    let order = super::super::flag_value(args, "--order");
    let verdicts = [
        ("parity", parity(ctx, args)),
        ("complexity", complexity(ctx, n, order)),
        ("alloc", alloc(ctx)),
        ("diamond", diamond(ctx)),
    ];
    let all_pass = verdicts.iter().all(|(_, v)| v.starts_with("PASS"));
    println!("# {name} — {}", if all_pass { "all criteria PASS (stampable)" } else { "NOT stampable" });
    for (k, v) in &verdicts {
        println!("{k} = {}", toml_str(v));
    }
    let _ = std::fs::remove_dir_all(&ctx.dir);
    if !all_pass {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every parity program declares its own input type and decodes six
    /// bodies; the campaign is a function of (seed, index).
    #[test]
    fn a_parity_program_declares_its_input_type_and_is_reproducible() {
        let p = parity_program(1115001, 3);
        assert!(p.contains("type In: Decode = {"), "{p}");
        assert_eq!(p.matches("println(outcome(http.decode_json[In](q").count(), 6, "{p}");
        assert_eq!(p, parity_program(1115001, 3));
        assert_ne!(p, parity_program(1115001, 4));
    }

    #[test]
    fn the_paths_agree_only_when_each_line_matches_the_direct_one() {
        assert!(paths_agree("d = ok 3\nl = ok 3\nr = ok 3\n"));
        assert!(!paths_agree("d = ok 3\nl = ok 3\nr = err 400\n"));
    }
}
