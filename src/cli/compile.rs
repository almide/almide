use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, out, err};

/// Resolve a module name to a source file path.
/// If the input looks like a file path (ends with .almd), use it directly.
/// If it's a module name (e.g., "json", "parser"), resolve via the module system.
fn resolve_module_to_file(module: &str) -> String {
    if module.ends_with(".almd") {
        return module.to_string();
    }

    // Check stdlib first
    if almide::stdlib::is_stdlib_module(module) {
        err(&format!("error: '{}' is a stdlib module (defined via TOML, no source file)", module));
        err(&format!("  hint: stdlib interfaces are built into the compiler"));
        std::process::exit(1);
    }

    // Resolve as local module or dependency
    let base_dir = if std::path::Path::new("src").is_dir() {
        std::path::PathBuf::from("src")
    } else {
        std::path::PathBuf::from(".")
    };

    let dep_paths = super::dep_paths_from_cwd_toml();

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

    err(&format!("error: module '{}' not found", module));
    err(&format!("  hint: specify a module name (e.g., 'almide compile parser') or file path (e.g., 'almide compile src/parser.almd')"));
    std::process::exit(1);
}

/// `cmd_compile`'s module-name derivation: from the `--module`/positional
/// arg (stripping `.almd`), or from `almide.toml`'s package name / the
/// current directory name in project mode. Extracted verbatim.
fn resolve_module_name(module: Option<&str>) -> String {
    if let Some(m) = module {
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
    }
}

/// `cmd_compile`'s parse + resolve + type-check phase. Exits the process on
/// any parse/resolve/type error, matching the original inline behavior.
/// Extracted verbatim.
fn parse_and_typecheck_for_compile(file: &str) -> (almide::ast::Program, String, check::Checker) {
    let (mut program, source_text, parse_errors) = parse_file(file);
    if !parse_errors.is_empty() {
        for e in &parse_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(e, &source_text)));
        }
        err(&format!("\n{} parse error(s) found", parse_errors.len()));
        std::process::exit(1);
    }

    let dep_paths = super::dep_paths_from_cwd_toml();

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });

    // Type check
    let canon = canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, &source_text);
    checker.diagnostics = canon.diagnostics;
    let diagnostics = checker.infer_program(&mut program);
    let errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    if !errors.is_empty() {
        for d in &errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, &source_text)));
        }
        err(&format!("\n{} error(s) found", errors.len()));
        std::process::exit(1);
    }
    for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
        err(&format!("{}", crate::diagnostic_render::display_with_source(d, &source_text)));
    }
    (program, source_text, checker)
}

/// `cmd_compile`'s 3 mutually-exclusive output modes.
enum CompileOutputMode<'a> {
    /// `--json`: print interface JSON to stdout (no artifact).
    Json,
    /// `--dry-run`: print the human-readable interface (no artifact).
    DryRun,
    /// Default: write a `.almdi` artifact under the given directory
    /// (`target/compile` if unset), skipping if already fresh.
    Artifact(Option<&'a str>),
}

/// `cmd_compile`'s output phase. Extracted verbatim.
fn write_compile_output(iface: &almide::interface::ModuleInterface, ir: &almide::ir::IrProgram, source_text: &str, module_name: &str, mode: CompileOutputMode) {
    match mode {
        CompileOutputMode::Json => {
            let output = serde_json::to_string_pretty(iface)
                .unwrap_or_else(|e| { err(&format!("JSON serialize error: {}", e)); std::process::exit(1); });
            out(&format!("{}", output));
        }
        CompileOutputMode::DryRun => {
            print_human_readable(iface);
        }
        CompileOutputMode::Artifact(output_dir) => {
            let dir = output_dir.unwrap_or("target/compile");
            let out_path = std::path::PathBuf::from(dir).join(format!("{}.almdi", module_name));
            let hash = almide::almdi::source_hash(source_text);

            // Check freshness — skip if already up to date
            if almide::almdi::is_fresh(&out_path, hash) {
                err(&format!("{} is up to date", out_path.display()));
                return;
            }

            almide::almdi::write_almdi(&out_path, iface, ir, hash)
                .unwrap_or_else(|e| { err(&format!("error: {}", e)); std::process::exit(1); });
            err(&format!("  compiled {}", out_path.display()));
        }
    }
}

pub fn cmd_compile(module: Option<&str>, json: bool, dry_run: bool, output_dir: Option<&str>) {
    let file = match module {
        Some(m) => resolve_module_to_file(m),
        None => crate::resolve_file(None),
    };
    let module_name = resolve_module_name(module);

    let (program, source_text, checker) = parse_and_typecheck_for_compile(&file);

    // Lower to IR
    let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);

    // Extract interface (with version from almide.toml if available)
    let pkg_version = std::path::Path::new("almide.toml").exists()
        .then(|| project::parse_toml(std::path::Path::new("almide.toml")).ok())
        .flatten()
        .map(|p| p.package.version);
    let iface = almide::interface::extract_with_version(
        &ir, &module_name, Some(&source_text), pkg_version.as_deref(),
    );

    let mode = if json {
        CompileOutputMode::Json
    } else if dry_run {
        CompileOutputMode::DryRun
    } else {
        CompileOutputMode::Artifact(output_dir)
    };
    write_compile_output(&iface, &ir, &source_text, &module_name, mode);
}

/// `print_iface_types`'s per-type `type`-kind rendering (Record fields /
/// Variant cases / Alias target). Extracted verbatim.
fn print_iface_type_kind(name: &str, generics: &str, kind: &almide::interface::TypeKindExport) {
    match kind {
        almide::interface::TypeKindExport::Record { fields } => {
            out(&format!("  type {}{} {{", name, generics));
            for f in fields {
                out(&format!("    {}: {}", f.name, format_type_ref(&f.ty)));
            }
            out(&format!("  }}"));
        }
        almide::interface::TypeKindExport::Variant { cases } => {
            out(&format!("  type {}{}", name, generics));
            for c in cases {
                match &c.payload {
                    None => out(&format!("    | {}", c.name)),
                    Some(almide::interface::CasePayload::Tuple { fields }) => {
                        let types: Vec<_> = fields.iter().map(|f| format_type_ref(f)).collect();
                        out(&format!("    | {}({})", c.name, types.join(", ")));
                    }
                    Some(almide::interface::CasePayload::Record { fields }) => {
                        let fs: Vec<_> = fields.iter().map(|f| format!("{}: {}", f.name, format_type_ref(&f.ty))).collect();
                        out(&format!("    | {} {{ {} }}", c.name, fs.join(", ")));
                    }
                }
            }
        }
        almide::interface::TypeKindExport::Alias { target } => {
            out(&format!("  type {}{} = {}", name, generics, format_type_ref(target)));
        }
    }
}

/// `print_human_readable`'s types section. Extracted verbatim — reads only
/// `iface.types`, writes only to stdout.
fn print_iface_types(iface: &almide::interface::ModuleInterface) {
    if iface.types.is_empty() { return; }
    for t in &iface.types {
        if let Some(ref doc) = t.doc {
            for line in doc.lines() {
                out(&format!("  // {}", line));
            }
        }
        let generics = t.generics.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| format!("[{}]", g.join(", ")))
            .unwrap_or_default();
        print_iface_type_kind(&t.name, &generics, &t.kind);
        out("");
    }
}

/// `print_human_readable`'s functions section. Extracted verbatim — reads
/// only `iface.functions`, writes only to stdout.
fn print_iface_functions(iface: &almide::interface::ModuleInterface) {
    if iface.functions.is_empty() { return; }
    for f in &iface.functions {
        if let Some(ref doc) = f.doc {
            for line in doc.lines() {
                out(&format!("  // {}", line));
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
        out(&format!("  {}fn {}{}({}) -> {}{}", effect, f.name, generics, params.join(", "), ret, error));
    }
    out("");
}

/// `print_human_readable`'s constants section. Extracted verbatim — reads
/// only `iface.constants`, writes only to stdout.
fn print_iface_constants(iface: &almide::interface::ModuleInterface) {
    if iface.constants.is_empty() { return; }
    for c in &iface.constants {
        let val = c.value.as_ref().map(|v| match v {
            almide::interface::ConstValue::Int(n) => format!(" = {}", n),
            almide::interface::ConstValue::Float(n) => format!(" = {}", n),
            almide::interface::ConstValue::String(s) => format!(" = \"{}\"", s),
            almide::interface::ConstValue::Bool(b) => format!(" = {}", b),
        }).unwrap_or_default();
        out(&format!("  let {}: {}{}", c.name, format_type_ref(&c.ty), val));
    }
    out("");
}

/// `print_human_readable`'s dependencies section. Extracted verbatim — reads
/// only `iface.dependencies`, writes only to stdout.
fn print_iface_dependencies(iface: &almide::interface::ModuleInterface) {
    if iface.dependencies.is_empty() { return; }
    out(&format!("  imports:"));
    for d in &iface.dependencies {
        let tag = if d.stdlib { " (stdlib)" } else { "" };
        out(&format!("    {}{}", d.module, tag));
    }
    out("");
}

fn print_human_readable(iface: &almide::interface::ModuleInterface) {
    out(&format!("module {}", iface.module));
    if let Some(ref v) = iface.version {
        out(&format!("  version {}", v));
    }
    out("");

    print_iface_types(iface);
    print_iface_functions(iface);
    print_iface_constants(iface);
    print_iface_dependencies(iface);
}

/// `format_type_ref`'s scalar-variant half (no recursion). Returns `None`
/// for the compound variants `format_compound_type_ref` handles — the two
/// halves together are exhaustive over `TypeRef`.
fn format_scalar_type_ref(ty: &almide::interface::TypeRef) -> Option<String> {
    Some(match ty {
        almide::interface::TypeRef::Int => "Int".to_string(),
        almide::interface::TypeRef::Float => "Float".to_string(),
        almide::interface::TypeRef::String => "String".to_string(),
        almide::interface::TypeRef::Bool => "Bool".to_string(),
        almide::interface::TypeRef::Unit => "Unit".to_string(),
        almide::interface::TypeRef::Bytes => "Bytes".to_string(),
        almide::interface::TypeRef::Matrix => "Matrix".to_string(),
        almide::interface::TypeRef::TypeVar { name } => name.clone(),
        almide::interface::TypeRef::Unknown => "?".to_string(),
        _ => return None,
    })
}

/// `format_type_ref`'s compound-variant half (recurses via `format_type_ref`).
/// Only ever reached for variants `format_scalar_type_ref` doesn't handle —
/// see the `unreachable!()` note below.
fn format_compound_type_ref(ty: &almide::interface::TypeRef) -> String {
    match ty {
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
        // Every scalar variant returns early via `format_scalar_type_ref` in
        // `format_type_ref` below, so this match is only ever reached with
        // one of the compound variants above — this arm is unreachable, not
        // a silently-accepted default.
        _ => unreachable!("format_scalar_type_ref should have handled this TypeRef variant"),
    }
}

fn format_type_ref(ty: &almide::interface::TypeRef) -> String {
    format_scalar_type_ref(ty).unwrap_or_else(|| format_compound_type_ref(ty))
}
