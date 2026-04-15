//! `almide ide` — semantic queries that replace grep for LLM/agent API discovery.
//!
//! `outline`: list every public decl in a file as one line each.
//! `doc`: show signature + doc for a single symbol (stdlib `module.fn` or user-defined).
//!
//! Designed for MoonBit-style agent workflows — LLMs call these to discover an
//! API instead of guessing at names.

use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch};

/// Print a one-line summary of every top-level decl in `file`.
/// Format: `fn name(a: T, b: U) -> R`, `type Name = ...`, `let NAME: T`.
pub fn cmd_ide_outline(file: &str, filter: Option<&str>) {
    let iface = match build_interface(file) {
        Ok(i) => i,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };

    for t in &iface.types {
        let name = &t.name;
        if let Some(f) = filter { if !name.contains(f) { continue; } }
        let generics = t.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| format!("[{}]", g.join(", ")))
            .unwrap_or_default();
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
        println!("type {}{} = {}", name, generics, shape);
    }

    for c in &iface.constants {
        let name = &c.name;
        if let Some(f) = filter { if !name.contains(f) { continue; } }
        println!("let {}: {}", name, format_tref(&c.ty));
    }

    for f in &iface.functions {
        let name = &f.name;
        if let Some(flt) = filter { if !name.contains(flt) { continue; } }
        let generics = f.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| format!("[{}]", g.join(", ")))
            .unwrap_or_default();
        let params = f.params.iter()
            .map(|p| format!("{}: {}", p.name, format_tref(&p.ty)))
            .collect::<Vec<_>>().join(", ");
        let effect_kw = if f.effect { "effect fn " } else { "fn " };
        println!("{}{}{}({}) -> {}", effect_kw, name, generics, params, format_tref(&f.ret));
    }
}

/// Show signature + doc for one symbol.
/// Handles:
///   `module.fn`  — stdlib lookup
///   `bare_name`  — user fn/type in the supplied `file`
pub fn cmd_ide_doc(symbol: &str, file: &str) {
    if let Some((module, fname)) = symbol.split_once('.') {
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
