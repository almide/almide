/// `almide install` — clone an Almide project and build its main binary
/// into the user's bin directory.
///
/// Mirrors the experience of `go install`: one command, source-to-binary,
/// no manual cargo / build / cp dance. Uses the existing dependency
/// fetcher (`fetch_dep`) for git clones with caching, then delegates the
/// actual compile to `cli::cmd_build` after chdir'ing into the project.
///
/// Resolution order for the install directory:
///   1. `--bin-dir <path>`
///   2. `$ALMIDE_INSTALL`
///   3. `$HOME/.local/bin`
use std::path::{Path, PathBuf};

use crate::project::Dependency;
use crate::project_fetch;

pub fn cmd_install(
    spec: &str,
    tag: Option<&str>,
    branch: Option<&str>,
    name_override: Option<&str>,
    bin_dir: Option<&Path>,
    target: Option<&str>,
) {
    let src_dir = match resolve_source(spec, tag, branch) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let almide_toml = src_dir.join("almide.toml");
    if !almide_toml.is_file() {
        eprintln!(
            "error: {} is not an Almide project (no almide.toml)",
            src_dir.display()
        );
        std::process::exit(1);
    }
    let project = match crate::project::parse_toml(&almide_toml) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", almide_toml.display(), e);
            std::process::exit(1);
        }
    };

    let bin_name = name_override
        .map(String::from)
        .unwrap_or_else(|| project.package.name.clone());
    if bin_name.is_empty() {
        eprintln!(
            "error: package has no name; pass --name or set [package].name in almide.toml"
        );
        std::process::exit(1);
    }

    let install_dir = bin_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_install_dir);
    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        eprintln!(
            "error: failed to create {}: {}",
            install_dir.display(),
            e
        );
        std::process::exit(1);
    }
    let install_path = install_dir.join(&bin_name);

    let entry_rel = "src/main.almd";
    let entry = src_dir.join(entry_rel);
    if !entry.is_file() {
        eprintln!(
            "error: entry point {}/{} not found",
            src_dir.display(),
            entry_rel
        );
        std::process::exit(1);
    }

    eprintln!(
        "Installing {} from {}...",
        bin_name,
        src_dir.display()
    );

    // `cmd_build` resolves paths relative to the current working directory
    // (it discovers `almide.toml` in the cwd). Chdir into the source so
    // imports and `[dependencies]` resolve correctly, then chdir back so
    // the user's terminal is left untouched.
    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Err(e) = std::env::set_current_dir(&src_dir) {
        eprintln!(
            "error: failed to chdir to {}: {}",
            src_dir.display(),
            e
        );
        std::process::exit(1);
    }

    let install_path_str = install_path.to_string_lossy().to_string();
    crate::cli::cmd_build(
        entry_rel,
        Some(&install_path_str),
        target,
        true,  // release
        false, // fast
        false, // unchecked_index
        false, // no_check
        false, // repr_c
        false, // cdylib
    );

    let _ = std::env::set_current_dir(&prev_cwd);

    if install_path.is_file() {
        eprintln!();
        eprintln!("✓ Installed {} to {}", bin_name, install_path.display());
    } else {
        eprintln!(
            "error: build appeared to succeed but {} was not produced",
            install_path.display()
        );
        std::process::exit(1);
    }
}

fn resolve_source(
    spec: &str,
    tag: Option<&str>,
    branch: Option<&str>,
) -> Result<PathBuf, String> {
    // Local path takes precedence: lets users iterate locally
    // (`almide install .`) without committing first.
    let p = Path::new(spec);
    if p.exists() && p.is_dir() {
        return p
            .canonicalize()
            .map_err(|e| format!("canonicalize {}: {}", p.display(), e));
    }

    // Otherwise resolve as a package spec or git URL.
    let (name, git_url, default_tag) =
        if spec.starts_with("https://") || spec.starts_with("git@") || spec.starts_with("ssh://") {
            let name = spec
                .rsplit('/')
                .next()
                .unwrap_or("unknown")
                .trim_end_matches(".git")
                .to_string();
            (name, spec.to_string(), None)
        } else {
            project_fetch::resolve_package_spec(spec)
        };

    let dep = Dependency {
        name,
        git: git_url,
        tag: tag.map(String::from).or(default_tag),
        branch: branch.map(String::from),
        version: None,
    };
    project_fetch::fetch_dep(&dep)
}

fn default_install_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("ALMIDE_INSTALL") {
        return PathBuf::from(env_dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("bin");
    }
    PathBuf::from(".")
}
