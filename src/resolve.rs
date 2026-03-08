/// Module resolution: find and parse imported .almd files.

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use crate::ast;
use crate::lexer;
use crate::parser;

const STDLIB_MODULES: &[&str] = &["string", "list", "int", "float", "env", "fs", "map"];

pub struct ResolvedModules {
    /// Modules in dependency order (leaves first).
    pub modules: Vec<(String, ast::Program)>,
}

pub fn resolve_imports(source_file: &str, program: &ast::Program) -> Result<ResolvedModules, String> {
    let base_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let mut loaded: Vec<(String, ast::Program)> = Vec::new();
    let mut loaded_names: HashSet<String> = HashSet::new();
    let mut loading: HashSet<String> = HashSet::new();

    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let name = &path[0];
            if STDLIB_MODULES.contains(&name.as_str()) {
                continue;
            }
            load_module(name, base_dir, &mut loaded, &mut loaded_names, &mut loading)?;
        }
    }

    Ok(ResolvedModules { modules: loaded })
}

fn load_module(
    name: &str,
    base_dir: &Path,
    loaded: &mut Vec<(String, ast::Program)>,
    loaded_names: &mut HashSet<String>,
    loading: &mut HashSet<String>,
) -> Result<(), String> {
    if loaded_names.contains(name) {
        return Ok(());
    }
    if loading.contains(name) {
        return Err(format!("circular import detected: {}", name));
    }
    loading.insert(name.to_string());

    let file_path = find_module_file(name, base_dir)?;
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("error reading module '{}': {}", name, e))?;

    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()
        .map_err(|e| format!("parse error in module '{}': {}", name, e))?;

    // Recursively resolve this module's imports (depth-first → leaves first)
    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let dep_name = &path[0];
            if !STDLIB_MODULES.contains(&dep_name.as_str()) {
                load_module(dep_name, base_dir, loaded, loaded_names, loading)?;
            }
        }
    }

    loading.remove(name);
    loaded_names.insert(name.to_string());
    loaded.push((name.to_string(), program));
    Ok(())
}

fn find_module_file(name: &str, base_dir: &Path) -> Result<PathBuf, String> {
    let candidates = [
        base_dir.join(format!("{}.almd", name)),
        base_dir.join(name).join("mod.almd"),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(format!(
        "module '{}' not found\n  searched: {}\n  hint: Create {}.almd in the same directory",
        name,
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
        name,
    ))
}
