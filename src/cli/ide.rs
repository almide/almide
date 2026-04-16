//! `almide ide` — semantic queries that replace grep for LLM/agent API discovery.
//!
//! `outline`: list every public decl in a file or stdlib module as one line each.
//! `doc`: show signature + doc for a single symbol (stdlib `module.fn` or user-defined).
//!
//! Accepts `@stdlib/<module>` as `<target>` to query built-in APIs without
//! needing a source file. Supports `--json` for tool integration.

use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch};
use serde::Serialize;

const STDLIB_PREFIX: &str = "@stdlib/";

#[derive(Serialize)]
struct Outline {
    module: String,
    source: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    types: Vec<OutlineType>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    constants: Vec<OutlineConst>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    functions: Vec<OutlineFn>,
}

#[derive(Serialize)]
struct OutlineType {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    generics: Vec<String>,
    shape: String,
}

#[derive(Serialize)]
struct OutlineConst {
    name: String,
    ty: String,
}

#[derive(Serialize)]
struct OutlineFn {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    generics: Vec<String>,
    params: Vec<OutlineParam>,
    ret: String,
    effect: bool,
}

#[derive(Serialize)]
struct OutlineParam {
    name: String,
    ty: String,
}

/// Default stdlib modules bundled into `stdlib-snapshot` when `--modules`
/// is omitted. Covers the core ~95% of dojo task surface. Less-used modules
/// (`bytes`, `float`, etc.) are intentionally excluded — callers that want
/// them can pass `--modules` explicitly.
const DEFAULT_SNAPSHOT_MODULES: &[&str] = &[
    "string", "list", "int", "option", "result", "map", "set",
];

/// Dump concatenated stdlib outlines in one call — designed for SYSTEM_PROMPT
/// injection by LLM harnesses (e.g. almide-dojo) that need a single authoritative
/// API inventory without spawning N subprocesses.
pub fn cmd_ide_stdlib_snapshot(modules: Option<&str>, json: bool) {
    let modules: Vec<&str> = match modules {
        Some(csv) => csv.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect(),
        None => DEFAULT_SNAPSHOT_MODULES.to_vec(),
    };

    let outlines: Vec<Outline> = modules.iter()
        .map(|m| collect_stdlib_outline(m).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        }))
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&outlines).unwrap());
    } else {
        for outline in &outlines {
            println!("# @stdlib/{}", outline.module);
            print_outline_text(outline);
            println!();
        }
    }
}

/// Print a one-line summary of every public decl.
/// `target` is either `@stdlib/<module>` or a file path.
pub fn cmd_ide_outline(target: &str, filter: Option<&str>, json: bool) {
    let outline = if let Some(module) = target.strip_prefix(STDLIB_PREFIX) {
        collect_stdlib_outline(module).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        })
    } else {
        collect_file_outline(target).unwrap_or_else(|e| {
            eprintln!("{}", e);
            std::process::exit(1);
        })
    };

    let filtered = apply_filter(outline, filter);

    if json {
        let out = serde_json::to_string_pretty(&filtered).unwrap();
        println!("{}", out);
    } else {
        print_outline_text(&filtered);
    }
}

/// Show signature + doc for one symbol.
/// Handles:
///   `module.fn`              — stdlib lookup (e.g. `string.to_upper`)
///   `@stdlib/module.fn`      — same, with explicit prefix for symmetry
///                              with `almide ide outline @stdlib/<module>`
///   `bare_name`              — user fn/type in the supplied `file`
pub fn cmd_ide_doc(symbol: &str, file: &str) {
    // Strip the `@stdlib/` prefix for ergonomic symmetry with `outline`.
    // `almide ide doc @stdlib/string.to_upper` and `almide ide doc
    // string.to_upper` now behave identically.
    let resolved = symbol.strip_prefix(STDLIB_PREFIX).unwrap_or(symbol);
    if let Some((module, fname)) = resolved.split_once('.') {
        if let Some(sig) = almide::stdlib::lookup_sig(module, fname) {
            let params = sig.params.iter()
                .map(|(n, t)| format!("{}: {}", n.as_str(), t.display()))
                .collect::<Vec<_>>().join(", ");
            let effect = if sig.is_effect { "effect fn " } else { "fn " };
            println!("{}{}.{}({}) -> {}", effect, module, fname, params, sig.ret.display());
            return;
        }
    }

    let iface = match build_interface(file) {
        Ok(i) => i,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    if let Some(f) = iface.functions.iter().find(|f| f.name == symbol) {
        let params = f.params.iter()
            .map(|p| format!("{}: {}", p.name, format_tref(&p.ty)))
            .collect::<Vec<_>>().join(", ");
        let effect_kw = if f.effect { "effect fn " } else { "fn " };
        println!("{}{}({}) -> {}", effect_kw, f.name, params, format_tref(&f.ret));
        if let Some(doc) = &f.doc {
            println!();
            for line in doc.lines() { println!("{}", line); }
        }
        return;
    }
    if let Some(t) = iface.types.iter().find(|t| t.name == symbol) {
        let generics = t.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| format!("[{}]", g.join(", ")))
            .unwrap_or_default();
        match &t.kind {
            almide::interface::TypeKindExport::Record { fields } => {
                println!("type {}{} {{", t.name, generics);
                for f in fields {
                    println!("    {}: {}", f.name, format_tref(&f.ty));
                }
                println!("}}");
            }
            almide::interface::TypeKindExport::Variant { cases } => {
                println!("type {}{}", t.name, generics);
                for c in cases {
                    println!("    | {}", c.name);
                }
            }
            almide::interface::TypeKindExport::Alias { target } => {
                println!("type {}{} = {}", t.name, generics, format_tref(target));
            }
        }
        if let Some(doc) = &t.doc {
            println!();
            for line in doc.lines() { println!("{}", line); }
        }
        return;
    }

    eprintln!("error: symbol '{}' not found", symbol);
    eprintln!("  hint: try `almide ide outline {}` to list available symbols", file);
    std::process::exit(1);
}

// ── collection ──

fn collect_stdlib_outline(module: &str) -> Result<Outline, String> {
    if !almide::stdlib::is_stdlib_module(module) {
        return Err(format!(
            "error: '{}' is not a stdlib module\n  hint: known modules include 'string', 'list', 'int', 'option', 'result', 'map', 'set'",
            module
        ));
    }

    let mut functions: Vec<OutlineFn> = almide::stdlib::module_functions(module)
        .into_iter()
        .filter_map(|fname| almide::stdlib::lookup_sig(module, fname).map(|sig| (fname, sig)))
        .map(|(fname, sig)| OutlineFn {
            name: fname.to_string(),
            generics: sig.generics.iter().map(|s| s.to_string()).collect(),
            params: sig.params.iter()
                .map(|(n, t)| OutlineParam { name: n.to_string(), ty: t.display() })
                .collect(),
            ret: sig.ret.display(),
            effect: sig.is_effect,
        })
        .collect();
    functions.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Outline {
        module: module.to_string(),
        source: "stdlib",
        types: vec![],
        constants: vec![],
        functions,
    })
}

fn collect_file_outline(file: &str) -> Result<Outline, String> {
    let iface = build_interface(file)?;

    let types = iface.types.iter().map(|t| {
        let generics = t.generics.clone().unwrap_or_default();
        let shape = match &t.kind {
            almide::interface::TypeKindExport::Record { fields } => {
                format!("{{ {} }}", fields.iter()
                    .map(|f| format!("{}: {}", f.name, format_tref(&f.ty)))
                    .collect::<Vec<_>>().join(", "))
            }
            almide::interface::TypeKindExport::Variant { cases } => {
                cases.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(" | ")
            }
            almide::interface::TypeKindExport::Alias { target } => format_tref(target),
        };
        OutlineType { name: t.name.clone(), generics, shape }
    }).collect();

    let constants = iface.constants.iter().map(|c| OutlineConst {
        name: c.name.clone(),
        ty: format_tref(&c.ty),
    }).collect();

    let functions = iface.functions.iter().map(|f| OutlineFn {
        name: f.name.clone(),
        generics: f.generics.clone().unwrap_or_default(),
        params: f.params.iter()
            .map(|p| OutlineParam { name: p.name.clone(), ty: format_tref(&p.ty) })
            .collect(),
        ret: format_tref(&f.ret),
        effect: f.effect,
    }).collect();

    Ok(Outline {
        module: iface.module,
        source: "user",
        types,
        constants,
        functions,
    })
}

fn apply_filter(mut outline: Outline, filter: Option<&str>) -> Outline {
    let Some(f) = filter else { return outline; };
    outline.types.retain(|t| t.name.contains(f));
    outline.constants.retain(|c| c.name.contains(f));
    outline.functions.retain(|fn_| fn_.name.contains(f));
    outline
}

fn print_outline_text(outline: &Outline) {
    for t in &outline.types {
        let generics = if t.generics.is_empty() { String::new() }
            else { format!("[{}]", t.generics.join(", ")) };
        println!("type {}{} = {}", t.name, generics, t.shape);
    }
    for c in &outline.constants {
        println!("let {}: {}", c.name, c.ty);
    }
    let prefix = if outline.source == "stdlib" {
        format!("{}.", outline.module)
    } else {
        String::new()
    };
    for f in &outline.functions {
        let generics = if f.generics.is_empty() { String::new() }
            else { format!("[{}]", f.generics.join(", ")) };
        let params = f.params.iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect::<Vec<_>>().join(", ");
        let effect_kw = if f.effect { "effect fn " } else { "fn " };
        println!("{}{}{}{}({}) -> {}", effect_kw, prefix, f.name, generics, params, f.ret);
    }
}

// ── user-file interface extraction ──

fn build_interface(file: &str) -> Result<almide::interface::ModuleInterface, String> {
    let (mut program, source_text, _parse_errors) = parse_file(file);
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|_| vec![])
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect()
        } else { vec![] }
    } else { vec![] };

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .map_err(|e| format!("error: {}", e))?;

    let canon = canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, &source_text);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut program);

    let errs: Vec<_> = diagnostics.iter().filter(|d| d.level == diagnostic::Level::Error).collect();
    if !errs.is_empty() {
        let lines: Vec<_> = errs.iter().map(|d| d.display()).collect();
        return Err(lines.join("\n"));
    }

    let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);

    let module_name = std::path::Path::new(file)
        .file_stem().and_then(|s| s.to_str()).unwrap_or("main").to_string();
    Ok(almide::interface::extract(&ir, &module_name, Some(&source_text)))
}

fn format_tref(ty: &almide::interface::TypeRef) -> String {
    use almide::interface::TypeRef;
    match ty {
        TypeRef::Int => "Int".into(),
        TypeRef::Float => "Float".into(),
        TypeRef::String => "String".into(),
        TypeRef::Bool => "Bool".into(),
        TypeRef::Unit => "Unit".into(),
        TypeRef::Bytes => "Bytes".into(),
        TypeRef::Matrix => "Matrix".into(),
        TypeRef::List { inner } => format!("List[{}]", format_tref(inner)),
        TypeRef::Option { inner } => format!("Option[{}]", format_tref(inner)),
        TypeRef::Set { inner } => format!("Set[{}]", format_tref(inner)),
        TypeRef::Result { ok, err } => format!("Result[{}, {}]", format_tref(ok), format_tref(err)),
        TypeRef::Map { key, value } => format!("Map[{}, {}]", format_tref(key), format_tref(value)),
        TypeRef::Tuple { elements } => {
            let els: Vec<_> = elements.iter().map(format_tref).collect();
            format!("({})", els.join(", "))
        }
        TypeRef::Named { name, args } if args.is_empty() => name.clone(),
        TypeRef::Named { name, args } => {
            let a: Vec<_> = args.iter().map(format_tref).collect();
            format!("{}[{}]", name, a.join(", "))
        }
        TypeRef::Fn { params, ret } => {
            let ps: Vec<_> = params.iter().map(format_tref).collect();
            format!("fn({}) -> {}", ps.join(", "), format_tref(ret))
        }
        TypeRef::TypeVar { name } => name.clone(),
        TypeRef::Unknown => "?".into(),
    }
}
