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

/// Find the project root (directory containing almide.toml), searching upward from base_dir.
fn find_project_root(base_dir: &Path) -> Option<PathBuf> {
    let mut dir = base_dir.to_path_buf();
    loop {
        if dir.join("almide.toml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn resolve_imports_with_deps(
    source_file: &str,
    program: &ast::Program,
    dep_paths: &[(project::PkgId, PathBuf)],
) -> Result<ResolvedModules, String> {
    let base_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let project_root = find_project_root(base_dir);
    let mut loaded: Vec<(String, ast::Program, Option<project::PkgId>)> = Vec::new();
    let mut loaded_names: HashSet<String> = HashSet::new();
    let mut loading: HashSet<String> = HashSet::new();

    for import in &program.imports {
        if let ast::Decl::Import { path, alias, .. } = import {
            let is_self_import = path.first().map(|s| s.as_str()) == Some("self");

            if is_self_import {
                // self.xxx → local module within the project
                if path.len() < 2 {
                    return Err("invalid self import: expected 'import self.module_name'".to_string());
                }
                let mod_path = &path[1..]; // skip "self"
                let mod_name = alias.as_deref().unwrap_or_else(|| mod_path.last().expect("guarded by path.len() >= 2"));
                let display_name = mod_path.join(".");

                if loaded_names.contains(mod_name) {
                    continue;
                }

                let root = project_root.as_ref().ok_or_else(|| {
                    format!("cannot resolve 'import self.{}': no almide.toml found in parent directories", display_name)
                })?;
                let src_dir = root.join("src");
                load_self_module(mod_name, mod_path, &src_dir, base_dir, dep_paths, &mut loaded, &mut loaded_names, &mut loading)?;
            } else {
                let name = &path[0];
                if stdlib::is_stdlib_module(name) {
                    continue;
                }
                load_module(name, base_dir, dep_paths, &mut loaded, &mut loaded_names, &mut loading)?;
            }
        }
    }

    Ok(ResolvedModules { modules: loaded })
}

/// Load a self-import module (import self.xxx).
fn load_self_module(
    mod_name: &str,
    mod_path: &[String],
    src_dir: &Path,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>)>,
    loaded_names: &mut HashSet<String>,
    loading: &mut HashSet<String>,
) -> Result<(), String> {
    if loaded_names.contains(mod_name) {
        return Ok(());
    }
    if loading.contains(mod_name) {
        return Err(format!("circular import detected: self.{}", mod_path.join(".")));
    }
    loading.insert(mod_name.to_string());

    let file_path = find_self_module_file(mod_path, src_dir)?;
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("error reading module 'self.{}': {}", mod_path.join("."), e))?;

    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()
        .map_err(|e| format!("parse error in module 'self.{}': {}", mod_path.join("."), e))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse error in module 'self.{}': {}", mod_path.join("."), parser.errors.join("\n")));
    }

    // Recursively resolve this module's imports
    for import in &program.imports {
        if let ast::Decl::Import { path, alias, .. } = import {
            let is_self = path.first().map(|s| s.as_str()) == Some("self");
            if is_self {
                if path.len() >= 2 {
                    let sub_mod_path = &path[1..];
                    let sub_mod_name = alias.as_deref().unwrap_or_else(|| sub_mod_path.last().expect("guarded by path.len() >= 2"));
                    load_self_module(sub_mod_name, sub_mod_path, src_dir, base_dir, dep_paths, loaded, loaded_names, loading)?;
                }
            } else {
                let dep_name = &path[0];
                if !stdlib::is_stdlib_module(dep_name) {
                    load_module(dep_name, base_dir, dep_paths, loaded, loaded_names, loading)?;
                }
            }
        }
    }

    loading.remove(mod_name);
    loaded_names.insert(mod_name.to_string());
    loaded.push((mod_name.to_string(), program, None));
    Ok(())
}

/// Resolve self.xxx path segments to a file under src/.
fn find_self_module_file(mod_path: &[String], src_dir: &Path) -> Result<PathBuf, String> {
    // Build path: src/a/b/c.almd or src/a/b/c/mod.almd
    let mut dir = src_dir.to_path_buf();
    for segment in &mod_path[..mod_path.len() - 1] {
        dir = dir.join(segment);
    }
    let last = &mod_path[mod_path.len() - 1];

    let candidates = [
        dir.join(format!("{}.almd", last)),
        dir.join(last).join("mod.almd"),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(format!(
        "module 'self.{}' not found\n  searched: {}\n  hint: Create {} in your src/ directory",
        mod_path.join("."),
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
        candidates[0].display(),
    ))
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
    if !parser.errors.is_empty() {
        return Err(format!("parse error in module '{}': {}", name, parser.errors.join("\n")));
    }

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
