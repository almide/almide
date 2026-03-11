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
    /// Fourth element: true if this is a self-import (same project).
    pub modules: Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
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
    let mut loaded: Vec<(String, ast::Program, Option<project::PkgId>, bool)> = Vec::new();
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
            } else if path.len() == 1 {
                let name = &path[0];
                if stdlib::is_stdlib_module(name) {
                    continue;
                }
                // Check bundled stdlib packages (written in Almide)
                if let Some(source) = stdlib::get_bundled_source(name) {
                    if !loaded_names.contains(name.as_str()) {
                        load_bundled_module(name, source, base_dir, dep_paths, &mut loaded, &mut loaded_names, &mut loading)?;
                    }
                    continue;
                }
                load_module(name, base_dir, dep_paths, &mut loaded, &mut loaded_names, &mut loading)?;
            } else {
                // import pkg.submodule — load just the sub-module directly
                let pkg_name = &path[0];
                if stdlib::is_stdlib_module(pkg_name) {
                    continue;
                }
                let sub_path = &path[1..];
                // Internal name is always the dotted path (e.g. "nomod_lib.parser")
                let dotted_name = path.join(".");
                if loaded_names.contains(&dotted_name) {
                    continue;
                }
                load_submodule(pkg_name, sub_path, &dotted_name, base_dir, dep_paths, &mut loaded, &mut loaded_names)?;
            }
        }
    }

    Ok(ResolvedModules { modules: loaded })
}

/// Load a bundled stdlib module from embedded source.
fn load_bundled_module(
    name: &str,
    source: &str,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
    loaded_names: &mut HashSet<String>,
    loading: &mut HashSet<String>,
) -> Result<(), String> {
    if loaded_names.contains(name) {
        return Ok(());
    }

    let tokens = lexer::Lexer::tokenize(source);
    let mut p = parser::Parser::new(tokens);
    let program = p.parse()
        .map_err(|e| format!("parse error in bundled stdlib '{}': {}", name, e))?;
    if !p.errors.is_empty() {
        return Err(format!("parse error in bundled stdlib '{}': {}", name, p.errors.join("\n")));
    }

    // Recursively resolve this module's imports
    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let dep_name = &path[0];
            if !stdlib::is_stdlib_module(dep_name) {
                if let Some(dep_source) = stdlib::get_bundled_source(dep_name) {
                    load_bundled_module(dep_name, dep_source, base_dir, dep_paths, loaded, loaded_names, loading)?;
                } else {
                    load_module(dep_name, base_dir, dep_paths, loaded, loaded_names, loading)?;
                }
            }
        }
    }

    loaded_names.insert(name.to_string());
    loaded.push((name.to_string(), program, None, false));
    Ok(())
}

/// Load a self-import module (import self.xxx).
fn load_self_module(
    mod_name: &str,
    mod_path: &[String],
    src_dir: &Path,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
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
    loaded.push((mod_name.to_string(), program, None, true));
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
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
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
    loaded.push((name.to_string(), program, pkg_id.clone(), false));

    // Module System v2: load sub-namespace files for packages
    // If this module was loaded from mod.almd (or lib.almd), also load sibling .almd files as sub-namespaces
    let file_name = file_path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    if file_name == "lib.almd" {
        eprintln!("warning: 'lib.almd' is deprecated as package entry point, rename to 'mod.almd'");
        eprintln!("  --> {}", file_path.display());
    }
    if file_name == "mod.almd" || file_name == "lib.almd" {
        if let Some(src_dir) = file_path.parent() {
            load_sub_namespaces(name, src_dir, &pkg_id, base_dir, dep_paths, loaded, loaded_names, loading)?;
        }
    }

    Ok(())
}

/// Parse a single .almd file and return its Program.
fn parse_almd_file(file_path: &Path, display_name: &str) -> Result<ast::Program, String> {
    let source = std::fs::read_to_string(file_path)
        .map_err(|e| format!("error reading sub-module '{}': {}", display_name, e))?;
    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()
        .map_err(|e| format!("parse error in sub-module '{}': {}", display_name, e))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse error in sub-module '{}': {}", display_name, parser.errors.join("\n")));
    }
    Ok(program)
}

/// Resolve imports within a sub-module's program.
fn resolve_submodule_imports(
    program: &ast::Program,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
    loaded_names: &mut HashSet<String>,
    loading: &mut HashSet<String>,
) -> Result<(), String> {
    for import in &program.imports {
        if let ast::Decl::Import { path, .. } = import {
            let dep_name = &path[0];
            if !stdlib::is_stdlib_module(dep_name) {
                load_module(dep_name, base_dir, dep_paths, loaded, loaded_names, loading)?;
            }
        }
    }
    Ok(())
}

/// Load all sibling .almd files as sub-namespaces, recursively scanning subdirectories.
fn load_sub_namespaces(
    pkg_name: &str,
    src_dir: &Path,
    pkg_id: &Option<project::PkgId>,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
    loaded_names: &mut HashSet<String>,
    loading: &mut HashSet<String>,
) -> Result<(), String> {
    // Load .almd files in this directory (excluding mod.almd, lib.almd, main.almd)
    let mut files: Vec<PathBuf> = match std::fs::read_dir(src_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map_or(false, |ext| ext == "almd")
                    && p.file_name().map_or(false, |f| f != "mod.almd" && f != "lib.almd" && f != "main.almd")
            })
            .collect(),
        Err(_) => return Ok(()),
    };
    files.sort();

    for file_path in files {
        let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let sub_name = format!("{}.{}", pkg_name, stem);
        if loaded_names.contains(&sub_name) {
            continue;
        }
        let program = parse_almd_file(&file_path, &sub_name)?;
        // Resolve this sub-module's imports
        resolve_submodule_imports(&program, base_dir, dep_paths, loaded, loaded_names, loading)?;
        loaded_names.insert(sub_name.clone());
        loaded.push((sub_name, program, pkg_id.clone(), false));
    }

    // Scan subdirectories recursively
    let mut subdirs: Vec<PathBuf> = match std::fs::read_dir(src_dir) {
        Ok(e) => e.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect(),
        Err(_) => vec![],
    };
    subdirs.sort();

    for subdir in subdirs {
        let dir_name = subdir.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let sub_name = format!("{}.{}", pkg_name, dir_name);
        if loaded_names.contains(&sub_name) {
            continue;
        }

        // Check for mod.almd in subdirectory
        let sub_mod = subdir.join("mod.almd");
        if sub_mod.exists() {
            let program = parse_almd_file(&sub_mod, &sub_name)?;
            resolve_submodule_imports(&program, base_dir, dep_paths, loaded, loaded_names, loading)?;
            loaded_names.insert(sub_name.clone());
            loaded.push((sub_name.clone(), program, pkg_id.clone(), false));
        }

        // Recurse into subdirectory for deeper sub-namespaces
        load_sub_namespaces(&sub_name, &subdir, pkg_id, base_dir, dep_paths, loaded, loaded_names, loading)?;
    }

    Ok(())
}

/// Load a specific sub-module from a package (import pkg.submodule).
fn load_submodule(
    pkg_name: &str,
    sub_path: &[String],
    mod_name: &str,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
    loaded: &mut Vec<(String, ast::Program, Option<project::PkgId>, bool)>,
    loaded_names: &mut HashSet<String>,
) -> Result<(), String> {
    if loaded_names.contains(mod_name) {
        return Ok(());
    }

    // Find the package's source directory
    let (src_dir, pkg_id) = find_package_src_dir(pkg_name, base_dir, dep_paths)?;

    // Build file path from sub_path segments
    let mut dir = src_dir.clone();
    for segment in &sub_path[..sub_path.len() - 1] {
        dir = dir.join(segment);
    }
    let last = &sub_path[sub_path.len() - 1];
    let candidates = [
        dir.join(format!("{}.almd", last)),
        dir.join(last).join("mod.almd"),
    ];

    let file_path = candidates.iter().find(|p| p.exists())
        .ok_or_else(|| format!(
            "sub-module '{}.{}' not found\n  searched: {}\n  hint: Create {} in the package's src/ directory",
            pkg_name, sub_path.join("."),
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
            candidates[0].display(),
        ))?;

    let source = std::fs::read_to_string(file_path)
        .map_err(|e| format!("error reading sub-module '{}.{}': {}", pkg_name, sub_path.join("."), e))?;
    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse()
        .map_err(|e| format!("parse error in sub-module '{}.{}': {}", pkg_name, sub_path.join("."), e))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse error in sub-module '{}.{}': {}", pkg_name, sub_path.join("."), parser.errors.join("\n")));
    }

    loaded_names.insert(mod_name.to_string());
    loaded.push((mod_name.to_string(), program, pkg_id, false));
    Ok(())
}

/// Find the source directory for a package.
fn find_package_src_dir(
    pkg_name: &str,
    base_dir: &Path,
    dep_paths: &[(project::PkgId, PathBuf)],
) -> Result<(PathBuf, Option<project::PkgId>), String> {
    // Check local — prefer src/ subdirectory
    let local_src = base_dir.join(pkg_name).join("src");
    if local_src.is_dir() {
        return Ok((local_src, None));
    }
    let local_dir = base_dir.join(pkg_name);
    if local_dir.is_dir() {
        return Ok((local_dir, None));
    }

    // Check dependencies
    for (pkg_id, dep_dir) in dep_paths {
        if pkg_id.name == pkg_name {
            return Ok((dep_dir.clone(), Some(pkg_id.clone())));
        }
    }

    Err(format!("package '{}' not found in dependencies", pkg_name))
}

fn find_module_file(name: &str, base_dir: &Path, dep_paths: &[(project::PkgId, PathBuf)]) -> Result<(PathBuf, Option<project::PkgId>), String> {
    // 1. Check local files
    let local_candidates = [
        base_dir.join(format!("{}.almd", name)),
        base_dir.join(name).join("mod.almd"),
        base_dir.join(name).join("src").join("mod.almd"),    // package with src/ layout
        base_dir.join(name).join("src").join("lib.almd"),    // package with src/ layout (legacy)
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
