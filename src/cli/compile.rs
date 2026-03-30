use crate::{parse_file, check, diagnostic, resolve, project, project_fetch};

/// Resolve a module name to a source file path.
/// If the input looks like a file path (ends with .almd), use it directly.
/// If it's a module name (e.g., "json", "parser"), resolve via the module system.
fn resolve_module_to_file(module: &str) -> String {
    if module.ends_with(".almd") {
        return module.to_string();
    }

    // Check stdlib first
    if almide::stdlib::is_stdlib_module(module) {
        eprintln!("error: '{}' is a stdlib module (defined via TOML, no source file)", module);
        eprintln!("  hint: stdlib interfaces are built into the compiler");
        std::process::exit(1);
    }

    // Resolve as local module or dependency
    let base_dir = if std::path::Path::new("src").is_dir() {
        std::path::PathBuf::from("src")
    } else {
        std::path::PathBuf::from(".")
    };

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> =
        if std::path::Path::new("almide.toml").exists() {
            if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
                project_fetch::fetch_all_deps(&proj)
                    .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
                    .into_iter()
                    .map(|fd| (fd.pkg_id, fd.source_dir))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

    // Try common file patterns
    let candidates = [
        base_dir.join(format!("{}.almd", module)),
        base_dir.join(module).join("mod.almd"),
        std::path::PathBuf::from(format!("{}.almd", module)),
    ];
    for path in &candidates {
        if path.exists() {
            return path.to_string_lossy().to_string();
        }
    }

    // Try dependencies
    for (pkg_id, dep_dir) in &dep_paths {
        if pkg_id.name == module {
            let dep_candidates = [
                dep_dir.join(format!("{}.almd", module)),
                dep_dir.join("lib.almd"),
                dep_dir.join("mod.almd"),
            ];
            for path in &dep_candidates {
                if path.exists() {
                    return path.to_string_lossy().to_string();
                }
            }
        }
    }

    eprintln!("error: module '{}' not found", module);
    eprintln!("  hint: specify a module name (e.g., 'almide compile parser') or file path (e.g., 'almide compile src/parser.almd')");
    std::process::exit(1);
}

pub fn cmd_compile(module: Option<&str>, json: bool, dry_run: bool, output_dir: Option<&str>) {
    let file = match module {
        Some(m) => resolve_module_to_file(m),
        None => crate::resolve_file(None),
    };

    let module_name = if let Some(m) = module {
        if m.ends_with(".almd") {
            std::path::Path::new(m)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "module".to_string())
        } else {
            m.to_string()
        }
    } else {
        // Project mode: use package name from almide.toml or directory name
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            proj.package.name.clone()
        } else {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "module".to_string())
        }
    };

    let (mut program, source_text, _parse_errors) = parse_file(&file);

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> =
        if std::path::Path::new("almide.toml").exists() {
            if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
                project_fetch::fetch_all_deps(&proj)
                    .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
                    .into_iter()
                    .map(|fd| (fd.pkg_id, fd.source_dir))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

    let resolved = resolve::resolve_imports_with_deps(&file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    // Type check
    let mut checker = check::Checker::new();
    checker.set_source(&file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    checker.install_import_table(&program);
    let diagnostics = checker.check_program(&mut program);
    let errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    if !errors.is_empty() {
        for d in &errors {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        eprintln!("\n{} error(s) found", errors.len());
        std::process::exit(1);
    }
    for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
        eprintln!("{}", d.display_with_source(&source_text));
    }

    // Lower to IR
    let ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);

    // Extract interface
    let iface = almide::interface::extract(&ir, &module_name, Some(&source_text));

    if json {
        // --json: print interface JSON to stdout (no artifact)
        let output = serde_json::to_string_pretty(&iface)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", output);
    } else if dry_run {
        // --dry-run: print human-readable interface (no artifact)
        print_human_readable(&iface);
    } else {
        // Default: produce .almdi artifact
        let dir = output_dir.unwrap_or("target/compile");
        let out_path = std::path::PathBuf::from(dir).join(format!("{}.almdi", module_name));
        let hash = almide::almdi::source_hash(&source_text);

        // Check freshness — skip if already up to date
        if almide::almdi::is_fresh(&out_path, hash) {
            eprintln!("{} is up to date", out_path.display());
            return;
        }

        almide::almdi::write_almdi(&out_path, &iface, &ir, hash)
            .unwrap_or_else(|e| { eprintln!("error: {}", e); std::process::exit(1); });
        eprintln!("  compiled {}", out_path.display());
    }
}

fn print_human_readable(iface: &almide::interface::ModuleInterface) {
    println!("module {}", iface.module);
    if let Some(ref v) = iface.version {
        println!("  version {}", v);
    }
    println!();

    if !iface.types.is_empty() {
        for t in &iface.types {
            if let Some(ref doc) = t.doc {
                for line in doc.lines() {
                    println!("  // {}", line);
                }
            }
            let generics = t.generics.as_ref()
                .filter(|g| !g.is_empty())
                .map(|g| format!("[{}]", g.join(", ")))
                .unwrap_or_default();
            match &t.kind {
                almide::interface::TypeKindExport::Record { fields } => {
                    println!("  type {}{} {{", t.name, generics);
                    for f in fields {
                        println!("    {}: {}", f.name, format_type_ref(&f.ty));
                    }
                    println!("  }}");
                }
                almide::interface::TypeKindExport::Variant { cases } => {
                    println!("  type {}{}", t.name, generics);
                    for c in cases {
                        match &c.payload {
                            None => println!("    | {}", c.name),
                            Some(almide::interface::CasePayload::Tuple { fields }) => {
                                let types: Vec<_> = fields.iter().map(|f| format_type_ref(f)).collect();
                                println!("    | {}({})", c.name, types.join(", "));
                            }
                            Some(almide::interface::CasePayload::Record { fields }) => {
                                let fs: Vec<_> = fields.iter().map(|f| format!("{}: {}", f.name, format_type_ref(&f.ty))).collect();
                                println!("    | {} {{ {} }}", c.name, fs.join(", "));
                            }
                        }
                    }
                }
                almide::interface::TypeKindExport::Alias { target } => {
                    println!("  type {}{} = {}", t.name, generics, format_type_ref(target));
                }
            }
            println!();
        }
    }

    if !iface.functions.is_empty() {
        for f in &iface.functions {
            if let Some(ref doc) = f.doc {
                for line in doc.lines() {
                    println!("  // {}", line);
                }
            }
            let effect = if f.effect { "effect " } else { "" };
            let generics = f.generics.as_ref()
                .filter(|g| !g.is_empty())
                .map(|g| format!("[{}]", g.join(", ")))
                .unwrap_or_default();
            let params: Vec<_> = f.params.iter()
                .map(|p| format!("{}: {}", p.name, format_type_ref(&p.ty)))
                .collect();
            let ret = format_type_ref(&f.ret);
            let error = if let Some(ref e) = f.error {
                format!(" ! {}", format_type_ref(e))
            } else {
                String::new()
            };
            println!("  {}fn {}{}({}) -> {}{}", effect, f.name, generics, params.join(", "), ret, error);
        }
        println!();
    }

    if !iface.constants.is_empty() {
        for c in &iface.constants {
            let val = c.value.as_ref().map(|v| match v {
                almide::interface::ConstValue::Int(n) => format!(" = {}", n),
                almide::interface::ConstValue::Float(n) => format!(" = {}", n),
                almide::interface::ConstValue::String(s) => format!(" = \"{}\"", s),
                almide::interface::ConstValue::Bool(b) => format!(" = {}", b),
            }).unwrap_or_default();
            println!("  let {}: {}{}", c.name, format_type_ref(&c.ty), val);
        }
        println!();
    }

    if !iface.dependencies.is_empty() {
        println!("  imports:");
        for d in &iface.dependencies {
            let tag = if d.stdlib { " (stdlib)" } else { "" };
            println!("    {}{}", d.module, tag);
        }
        println!();
    }
}

fn format_type_ref(ty: &almide::interface::TypeRef) -> String {
    match ty {
        almide::interface::TypeRef::Int => "Int".to_string(),
        almide::interface::TypeRef::Float => "Float".to_string(),
        almide::interface::TypeRef::String => "String".to_string(),
        almide::interface::TypeRef::Bool => "Bool".to_string(),
        almide::interface::TypeRef::Unit => "Unit".to_string(),
        almide::interface::TypeRef::Bytes => "Bytes".to_string(),
        almide::interface::TypeRef::Matrix => "Matrix".to_string(),
        almide::interface::TypeRef::List { inner } => format!("List[{}]", format_type_ref(inner)),
        almide::interface::TypeRef::Option { inner } => format!("Option[{}]", format_type_ref(inner)),
        almide::interface::TypeRef::Result { ok, err } => format!("Result[{}, {}]", format_type_ref(ok), format_type_ref(err)),
        almide::interface::TypeRef::Map { key, value } => format!("Map[{}, {}]", format_type_ref(key), format_type_ref(value)),
        almide::interface::TypeRef::Set { inner } => format!("Set[{}]", format_type_ref(inner)),
        almide::interface::TypeRef::Tuple { elements } => {
            let els: Vec<_> = elements.iter().map(|e| format_type_ref(e)).collect();
            format!("({})", els.join(", "))
        }
        almide::interface::TypeRef::Named { name, args } if args.is_empty() => name.clone(),
        almide::interface::TypeRef::Named { name, args } => {
            let a: Vec<_> = args.iter().map(|t| format_type_ref(t)).collect();
            format!("{}[{}]", name, a.join(", "))
        }
        almide::interface::TypeRef::Fn { params, ret } => {
            let ps: Vec<_> = params.iter().map(|p| format_type_ref(p)).collect();
            format!("({}) -> {}", ps.join(", "), format_type_ref(ret))
        }
        almide::interface::TypeRef::TypeVar { name } => name.clone(),
        almide::interface::TypeRef::Unknown => "?".to_string(),
    }
}
