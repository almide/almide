//! Stage-2 investment probe: where do the 4.42 ms of a per-file check go?
//! Replicates check_file_json's phases with timers. Measurement only.
use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("probe-bin invariant") {
        let p = e.expect("probe-bin invariant").path();
        if p.is_dir() { walk(&p, out); } else if p.extension().is_some_and(|x| x == "almd") { out.push(p); }
    }
}

fn main() {
    let root = std::env::current_dir().expect("probe-bin invariant");
    let mut files = Vec::new();
    walk(&root.join("spec"), &mut files);
    files.sort();
    let mut t_parse = 0.0; let mut t_resolve = 0.0; let mut t_canon = 0.0;
    let mut t_infer = 0.0; let mut t_modules = 0.0; let mut t_unused = 0.0;
    let mut n = 0usize;
    for p in files.iter().step_by(5).take(220) {
        let rel = p.strip_prefix(&root).expect("probe-bin invariant").to_string_lossy().to_string();
        let text = std::fs::read_to_string(p).expect("probe-bin invariant");
        let t0 = Instant::now();
        let tokens = almide::lexer::Lexer::tokenize(&text);
        let mut parser = almide::parser::Parser::new(tokens).with_file(&rel);
        let Ok(mut program) = parser.parse() else { continue };
        if !almide_spine::s3::stdlib_only(&program) { continue; }
        let t1 = Instant::now();
        let Ok(mut resolved) = almide::resolve::resolve_imports_with_deps(&rel, &program, &[]) else { continue };
        let t2 = Instant::now();
        let canon = almide::canonicalize::canonicalize_program(
            &program, resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)));
        let mut checker = almide::check::Checker::from_env(canon.env);
        checker.set_source(&rel, &text);
        checker.diagnostics = canon.diagnostics;
        almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
        let t3 = Instant::now();
        let diagnostics = checker.infer_program(&mut program);
        let t4 = Instant::now();
        let sources = std::mem::take(&mut resolved.sources);
        let mut module_diags: Vec<u8> = Vec::new();
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { continue; }
            let saved_self = checker.env.self_module_name;
            if let Some(pid) = pkg_id.as_ref() {
                checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
            }
            let before = checker.diagnostics.len();
            let _ = sources.get(name);
            checker.infer_module(mod_prog, name);
            let _ = checker.diagnostics.len() - before;
            checker.env.self_module_name = saved_self;
            let _ = &mut module_diags;
        }
        let t5 = Instant::now();
        let has_err = diagnostics.iter().any(|d| d.level == almide::diagnostic::Level::Error);
        if parser.errors.is_empty() && !has_err {
            let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
            std::hint::black_box(almide::ir::collect_unused_var_warnings(&ir, &rel));
        }
        let t6 = Instant::now();
        t_parse += t1.duration_since(t0).as_secs_f64();
        t_resolve += t2.duration_since(t1).as_secs_f64();
        t_canon += t3.duration_since(t2).as_secs_f64();
        t_infer += t4.duration_since(t3).as_secs_f64();
        t_modules += t5.duration_since(t4).as_secs_f64();
        t_unused += t6.duration_since(t5).as_secs_f64();
        n += 1;
        std::hint::black_box(&module_diags);
    }
    let tot = t_parse + t_resolve + t_canon + t_infer + t_modules + t_unused;
    println!("{n} files; avg total {:.2} ms", tot / n as f64 * 1e3);
    for (name, v) in [("parse", t_parse), ("resolve(+stdlib parse)", t_resolve), ("canon+from_env+refresh", t_canon), ("infer_program(entry)", t_infer), ("#862 module infer(stdlib)", t_modules), ("lower+unused", t_unused)] {
        println!("  {name:26} {:6.2} ms avg  {:5.1}%", v / n as f64 * 1e3, v / tot * 100.0);
    }
}
