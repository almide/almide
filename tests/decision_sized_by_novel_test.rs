//! bounded.shape-rules (owner ruling 2026-11-22): a stdlib fn's
//! `@sized_by(p, …)` names the parameters its result's size is bounded by. A
//! named Int is an amplifier: under @bounded a call handing it a run-time Int
//! is refused (E075); a constant is admitted. A declaration naming only heap
//! inputs is admitted with any Int (the Int indexes, it does not multiply).
//!
//! Nothing here is listed: the cases are ENUMERATED from the stdlib sources —
//! every declared fn whose parameter types this test can write a value for
//! (Int, String, Bytes, List[Int], a type variable), that takes no fn and is
//! not an effect fn —
//! so a fn declared tomorrow is covered the day it is declared. The fn and
//! parameter names in the programs are fresh.
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

struct Declared {
    module: String,
    name: String,
    params: Vec<(String, String)>,
    sized_by: Vec<String>,
    effect: bool,
}

/// `@sized_by(...)` declarations of the stdlib sources, with the fn under each.
fn declared() -> Vec<Declared> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&root).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("almd") {
            continue;
        }
        let stem = p.file_stem().unwrap().to_str().unwrap().to_string();
        let src = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let Some(inner) = l.strip_prefix("@sized_by(").and_then(|r| r.strip_suffix(')')) else { continue };
            let sized_by = inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let sig = lines[i + 1..].iter().find(|l| !l.starts_with('@') && !l.starts_with("///")).unwrap();
            let rest = sig.trim_start_matches("pub ");
            let effect = rest.starts_with("effect ");
            let rest = rest.trim_start_matches("effect ");
            let Some(rest) = rest.strip_prefix("fn ") else { panic!("{stem}: @sized_by above a non-fn line: {sig}") };
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let open = rest.find('(').unwrap();
            let mut depth = 0;
            let mut close = open;
            for (k, c) in rest[open..].char_indices() {
                match c {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let mut params = Vec::new();
            let (mut cur, mut d) = (String::new(), 0);
            for c in rest[open + 1..close].chars() {
                match c {
                    '(' | '[' | '{' => d += 1,
                    ')' | ']' | '}' => d -= 1,
                    _ => {}
                }
                if c == ',' && d == 0 {
                    params.push(std::mem::take(&mut cur));
                } else {
                    cur.push(c);
                }
            }
            params.push(cur);
            let params = params
                .iter()
                .filter(|p| !p.trim().is_empty())
                .map(|p| {
                    let (n, t) = p.split_once(':').unwrap();
                    (n.trim().to_string(), t.trim().to_string())
                })
                .collect();
            let module = stem.split('_').next().unwrap().to_string();
            out.push(Declared { module, name, params, sized_by, effect });
        }
    }
    out
}

/// A value of `ty` this test can write, or None.
fn value(ty: &str, int: &str) -> Option<String> {
    Some(match ty {
        "Int" => int.to_string(),
        "String" => "\"abcdef\"".to_string(),
        "Bytes" => "bytes.from_list([1, 2, 3, 4])".to_string(),
        "List[Int]" => "[1, 2, 3, 4]".to_string(),
        t if t.len() == 1 && t.chars().all(|c| c.is_ascii_uppercase()) => "[1, 2]".to_string(),
        t if t.starts_with("List[") && t[5..t.len() - 1].len() == 1 => "[1, 2, 3, 4]".to_string(),
        _ => return None,
    })
}

/// The program calling `d` from a fresh @bounded fn whose Int parameter `n`
/// is handed to every Int parameter named in `run_time` (the rest get `2`).
fn program(d: &Declared, run_time: &[&str]) -> Option<String> {
    let g = novel::ident("sized");
    let n = novel::ident("n");
    let args: Option<Vec<String>> = d
        .params
        .iter()
        .map(|(p, t)| value(t, if run_time.contains(&p.as_str()) { &n } else { "2" }))
        .collect();
    let call = format!("{}.{}({})", d.module, d.name, args?.join(", "));
    Some(format!("@bounded\nfn {g}({n}: Int) -> Int = {{\n  let _ = {call}\n  {n}\n}}\n\nfn main() -> Unit = println(\"${{{g}(3)}}\")\n"))
}

#[test]
fn novel_a_declared_amplifier_is_refused_with_a_run_time_int_and_admitted_with_a_constant() {
    let all = declared();
    assert!(all.len() >= 100, "the stdlib declares @sized_by on every heap+Int member; found {}", all.len());
    let mut amplifiers = 0;
    let mut bounded = 0;
    // An effect fn is refused under @bounded before its shape is read (E076).
    for d in all.iter().filter(|d| !d.effect) {
        let int_named: Vec<&str> = d
            .params
            .iter()
            .filter(|(p, t)| t == "Int" && d.sized_by.contains(p))
            .map(|(p, _)| p.as_str())
            .collect();
        let ints: Vec<&str> = d.params.iter().filter(|(_, t)| t == "Int").map(|(p, _)| p.as_str()).collect();
        let Some(runtime) = program(d, &ints) else { continue };
        let Some(constant) = program(d, &[]) else { continue };
        let (ok, out) = check(&runtime);
        if int_named.is_empty() {
            // bounded by its heap inputs: a run-time Int only indexes
            bounded += 1;
            assert!(!out.contains("E075"), "{}.{} is bounded by its inputs; E075 refused it:\n{runtime}\n{out}", d.module, d.name);
        } else {
            amplifiers += 1;
            assert!(!ok && out.contains("E075"), "{}.{} amplifies by {int_named:?}; a run-time Int was admitted:\n{runtime}\n{out}", d.module, d.name);
            let (_, out) = check(&constant);
            assert!(!out.contains("E075"), "{}.{} with constant sizes must be admitted:\n{constant}\n{out}", d.module, d.name);
        }
    }
    assert!(amplifiers >= 8 && bounded >= 30, "too few enumerated cases: {amplifiers} amplifiers, {bounded} bounded");
}

#[test]
fn novel_a_user_sized_by_declaration_is_refused_outside_the_stdlib() {
    let g = novel::ident("grow");
    let prog = format!("@sized_by(s)\nfn {g}(s: String, k: Int) -> String = string.repeat(s, k)\n\nfn main() -> Unit = println({g}(\"a\", 2))\n");
    let (ok, out) = check(&prog);
    assert!(!ok && out.contains("E085") && out.contains("@sized_by"), "a user @sized_by is an unchecked claim:\n{prog}\n{out}");
}
