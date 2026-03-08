/// Module resolution: find and parse imported .almd files.

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use crate::ast;
use crate::lexer;
use crate::parser;
use crate::project;

use crate::stdlib;

pub struct ResolvedModules {
    /// Modules in dependency order (leaves first).
    /// Third element is the PkgId (None for local modules).
    pub modules: Vec<(String, ast::Program, Option<project::PkgId>)>,
}

pub fn resolve_imports(source_file: &str, program: &ast::Program) -> Result<ResolvedModules, String> {
    resolve_imports_with_deps(source_file, program, &[])
}

pub fn resolve_imports_with_deps(
    source_file: &str,
    program: &ast::Program,
    dep_paths: &[(project::PkgId, PathBuf)],
) -> Result<ResolvedModules, String> {
    let base_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let mut loaded: Vec<(String, ast::Program, Option<project::PkgId>)> = Vec::new();
    let mut loaded_names: HashSet<String> = HashSet::new();
    let mut loading: HashSet<String> = HashSet::new();

    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let name = &path[0];
            if stdlib::is_stdlib_module(name) {
                continue;
            }
            load_module(name, base_dir, dep_paths, &mut loaded, &mut loaded_names, &mut loading)?;
        }
    }

    Ok(ResolvedModules { modules: loaded })
}

fn load_module(
    name: &str,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>)>,
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

    let (file_path, pkg_id) = find_module_file(name, base_dir, dep_paths)?;
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("error reading module '{}': {}", name, e))?;

    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()
        .map_err(|e| format!("parse error in module '{}': {}", name, e))?;

    // Recursively resolve this module's imports (depth-first -> leaves first)
    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let dep_name = &path[0];
            if !stdlib::is_stdlib_module(dep_name) {
                load_module(dep_name, base_dir, dep_paths, loaded, loaded_names, loading)?;
            }
        }
    }

    loading.remove(name);
    loaded_names.insert(name.to_string());
    loaded.push((name.to_string(), program, pkg_id));
    Ok(())
}

fn find_module_file(name: &str, base_dir: &Path, dep_paths: &[(project::PkgId, PathBuf)]) -> Result<(PathBuf, Option<project::PkgId>), String> {
    // 1. Check local files
    let local_candidates = [
        base_dir.join(format!("{}.almd", name)),
        base_dir.join(name).join("mod.almd"),
    ];
    for path in &local_candidates {
        if path.exists() {
            return Ok((path.clone(), None));
        }
    }

    // 2. Check dependency paths (match by pkg_id.name)
    for (pkg_id, dep_dir) in dep_paths {
        if pkg_id.name == name {
            let dep_candidates = [
                dep_dir.join(format!("{}.almd", name)),
                dep_dir.join("lib.almd"),
                dep_dir.join("mod.almd"),
            ];
            for path in &dep_candidates {
                if path.exists() {
                    return Ok((path.clone(), Some(pkg_id.clone())));
                }
            }
        }
    }

    Err(format!(
        "module '{}' not found\n  searched: {}\n  hint: Create {}.almd in the same directory, or add to [dependencies] in almide.toml",
        name,
        local_candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
        name,
    ))
}
