/// `almide install` — clone an Almide project and build its main binary
/// into the user's bin directory.
///
/// Mirrors the experience of `go install`: one command, source-to-binary,
/// no manual cargo / build / cp dance. Uses the existing dependency
/// fetcher for git clones with caching — a tag is an immutable snapshot and
/// keeps its cache entry, a branch (or the default HEAD) is resolved to the
/// remote's current commit on every install (`go install pkg@latest`) —
/// then delegates the actual compile to `cli::cmd_build` after chdir'ing
/// into the project.
///
/// Resolution order for the install directory:
///   1. `--bin-dir <path>`
///   2. `$ALMIDE_INSTALL`
///   3. `$HOME/.local/bin`
use std::path::{Path, PathBuf};

use crate::project::Dependency;
use crate::project_fetch;
use crate::err;

pub fn cmd_install(
    spec: &str,
    tag: Option<&str>,
    branch: Option<&str>,
    name_override: Option<&str>,
    bin_dir: Option<&Path>,
    target: Option<&str>,
    grants: InstallGrants<'_>,
) {
    let InstallGrants { force, trust, allow_native } = grants;
    // #4184: the owner of a remote spec must be trusted BEFORE anything is
    // fetched, as `almide add` requires of a dependency.
    if let Err(e) = check_install_trust(spec, trust) {
        err(&e);
        std::process::exit(1);
    }
    // A `--name` is checked before anything is fetched (#4183).
    if let Some(Err(e)) = name_override.map(check_bin_name) {
        err(&format!("error: --name {e}"));
        std::process::exit(1);
    }
    let src_dir = match resolve_source(spec, tag, branch) {
        Ok(p) => p,
        Err(e) => {
            err(&format!("error: {}", e));
            std::process::exit(1);
        }
    };

    // The manifest and the name are read first: a manifest that does not
    // parse is reported as such, not as unscannable native code.
    let almide_toml = src_dir.join("almide.toml");
    if !almide_toml.is_file() {
        err(&format!(
            "error: {} is not an Almide project (no almide.toml)",
            src_dir.display()
        ));
        std::process::exit(1);
    }
    let project = match crate::project::parse_toml(&almide_toml) {
        Ok(p) => p,
        Err(e) => {
            err(&format!("error: failed to read {}: {}", almide_toml.display(), e));
            std::process::exit(1);
        }
    };

    let bin_name = name_override
        .map(String::from)
        .unwrap_or_else(|| project.package.name.clone());
    if bin_name.is_empty() {
        err(&format!(
            "error: package has no name; pass --name or set [package].name in almide.toml"
        ));
        std::process::exit(1);
    }
    // #4183: the name becomes a file name in the bin dir, so it must be one
    // plain identifier. `name = "../x"` used to install outside the bin dir,
    // and an absolute name replaced the bin dir in the join.
    if let Err(e) = check_bin_name(&bin_name) {
        err(&format!("error: the package name {e}; pass --name with a plain name"));
        std::process::exit(1);
    }

    // #4184: the package is built as the root project, so its own native
    // code (which `[permissions] native` does not cover: it gates
    // dependencies) is compiled with the user's rights. Say what it is, and
    // build it only when asked.
    let native = crate::native_grant::native_code_of(&src_dir, &src_dir.join("src"));
    if !native.is_empty() {
        if !allow_native {
            err(&format!(
                "error: refusing to install {spec}: it compiles native code, which runs with your rights at build time\n  native code: {}\n  hint: read it, then rerun with --allow-native if you trust it",
                native.describe()
            ));
            std::process::exit(1);
        }
        err(&format!("note: --allow-native: building its native code: {}", native.describe()));
    }

    let install_dir = bin_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(default_install_dir);
    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        err(&format!(
            "error: failed to create {}: {}",
            install_dir.display(),
            e
        ));
        std::process::exit(1);
    }
    let install_path = install_dir.join(&bin_name);
    // #4183: never replace a file `almide install` did not put here unless
    // asked, and never write through a symlink.
    if let Err(e) = check_destination(&install_dir, &install_path, &bin_name, force) {
        err(&format!("error: {e}"));
        std::process::exit(1);
    }
    // Build to a staging name in the same dir, then rename over the
    // destination: a symlink there is replaced, never written through.
    let staged_path = install_dir.join(format!(".{}.almide-install-{}", bin_name, std::process::id()));

    let entry_rel = "src/main.almd";
    let entry = src_dir.join(entry_rel);
    if !entry.is_file() {
        err(&format!(
            "error: entry point {}/{} not found",
            src_dir.display(),
            entry_rel
        ));
        std::process::exit(1);
    }

    err(&format!(
        "Installing {} from {}...",
        bin_name,
        src_dir.display()
    ));

    // `cmd_build` resolves paths relative to the current working directory
    // (it discovers `almide.toml` in the cwd). Chdir into the source so
    // imports and `[dependencies]` resolve correctly, then chdir back so
    // the user's terminal is left untouched.
    let prev_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Err(e) = std::env::set_current_dir(&src_dir) {
        err(&format!(
            "error: failed to chdir to {}: {}",
            src_dir.display(),
            e
        ));
        std::process::exit(1);
    }

    let install_path_str = staged_path.to_string_lossy().to_string();
    crate::cli::cmd_build(crate::cli::BuildArgs {
        file: entry_rel,
        output: Some(&install_path_str),
        target,
        release: true,
        fast: false,
        target_cpu: None,
        print_release_rustflags: false,
        unchecked_index: false,
        no_check: false,
        repr_c: false,
        cdylib: false,
        emit_unverified: false,
        verified: false,
        native_verified: true,   // Same checked v1 route as an ordinary build.
        wasm_opt: false,
        component: false,
        heap_cap: None,
        host: None,
        debug: false,
    });

    let _ = std::env::set_current_dir(&prev_cwd);

    if staged_path.is_file() {
        if install_path.exists() || install_path.symlink_metadata().is_ok() {
            err(&format!("replacing {}", install_path.display()));
        }
        if let Err(e) = std::fs::rename(&staged_path, &install_path) {
            let _ = std::fs::remove_file(&staged_path);
            err(&format!("error: cannot install {}: {}", install_path.display(), e));
            std::process::exit(1);
        }
        record_installed(&install_dir, &bin_name);
    }

    if install_path.is_file() {
        err("");
        err(&format!("✓ Installed {} to {}", bin_name, install_path.display()));
    } else {
        err(&format!(
            "error: build appeared to succeed but {} was not produced",
            install_path.display()
        ));
        std::process::exit(1);
    }
}

fn resolve_source(
    spec: &str,
    tag: Option<&str>,
    branch: Option<&str>,
) -> Result<PathBuf, String> {
    // A path spelled as one (`almide install .`) installs that directory,
    // so users iterate without committing first. The spelling decides, not
    // what exists on disk (#4864): `github.com/x/y` is a source even when a
    // directory by that name is in the working directory.
    if spelled_local(spec) {
        let p = Path::new(spec);
        if !p.is_dir() {
            return Err(format!("error: {spec} is not a directory"));
        }
        return p
            .canonicalize()
            .map_err(|e| format!("canonicalize {}: {}", p.display(), e));
    }

    // Otherwise resolve as a package spec or git URL, read once into a
    // typed source (ADR-0035 D1).
    let (name, src, default_tag) = match written_source(spec) {
        Some(src) => {
            let src = src?;
            let name = src.path.last().map(|s| s.as_str().to_string()).unwrap_or_else(|| "unknown".to_string());
            (name, src, None)
        }
        None => {
            let (name, url, tag) = project_fetch::resolve_package_spec(spec);
            let src = crate::git_source::GitSource::parse(&url).map_err(|e| format!("error: {e}"))?;
            (name, src, tag)
        }
    };

    let as_ref = |what: &str, text: String| {
        crate::git_source::GitRef::parse(&text).map_err(|why| format!("error: --{what} `{}` is not a ref name: {why}", text.escape_debug()))
    };
    let dep = Dependency {
        name,
        git: Some(src.clone()),
        tag: tag.map(String::from).or(default_tag).map(|t| as_ref("tag", t)).transpose()?,
        branch: branch.map(String::from).map(|b| as_ref("branch", b)).transpose()?,
        version: None,
        path: None,
        subdir: None,
        declared_at: None,
    };
    // A tag names an immutable snapshot and keeps its cache entry. A branch
    // (or the remote's default HEAD) MOVES: resolve the remote head now and
    // build that commit — `go install pkg@latest` semantics — instead of
    // reusing whatever the cache holds from an earlier install, which
    // silently reinstalled a stale version (#1956). The commit is printed so
    // what was built is visible.
    if dep.tag.is_none() {
        let ref_name = dep.branch.as_deref().unwrap_or("HEAD").to_string();
        let head = project_fetch::git_remote_head(&src, &ref_name)?;
        err(&format!("Resolved {} {} -> {}", dep.name, ref_name, head.short()));
        return project_fetch::fetch_dep_with_lock(&dep, Some(&head));
    }
    project_fetch::fetch_dep(&dep)
}

/// A spec written as a source URL (`scheme://…`, or the scp-like
/// `user@host:path`), read into a [`crate::git_source::GitSource`]; `None`
/// for a package spec (`json`, `almide/json`, `host/owner/repo`).
fn written_source(spec: &str) -> Option<Result<crate::git_source::GitSource, String>> {
    (spec.contains("://") || spec.contains('@') && spec.contains(':'))
        .then(|| crate::git_source::GitSource::parse(spec).map_err(|e| format!("error: {e}")))
}

/// The installed binary's name must be one plain identifier (#4183): it is
/// joined onto the bin dir, so a `/`, `\\`, `..` or an absolute path would
/// put the file somewhere else.
fn check_bin_name(name: &str) -> Result<(), String> {
    crate::project::check_plain_name(name)
}

/// The record of what `almide install` put in a bin dir (#4183), one name
/// per line.
const INSTALL_RECEIPT: &str = ".almide-installed";

fn installed_names(install_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(install_dir.join(INSTALL_RECEIPT))
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn record_installed(install_dir: &Path, name: &str) {
    let mut names = installed_names(install_dir);
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
        let _ = std::fs::write(install_dir.join(INSTALL_RECEIPT), names.join("\n") + "\n");
    }
}

/// May `almide install` put `name` at `path`? A free name, or a file this
/// bin dir's receipt says almide installed. Anything else — a tool the user
/// put there, `almide` itself, a symlink — needs `--force`.
fn check_destination(install_dir: &Path, path: &Path, name: &str, force: bool) -> Result<(), String> {
    let Ok(md) = path.symlink_metadata() else { return Ok(()) };
    if force {
        return Ok(());
    }
    if md.file_type().is_symlink() {
        return Err(format!("{} is a symlink; pass --force to replace the link (its target is never written)", path.display()));
    }
    if installed_names(install_dir).iter().any(|n| n == name) {
        return Ok(());
    }
    Err(format!(
        "{} already exists and was not installed by almide; pass --force to replace it",
        path.display()
    ))
}

/// What the caller granted an install (#4183, #4184).
pub struct InstallGrants<'a> {
    /// Replace a file almide did not install.
    pub force: bool,
    /// The owner the caller trusts for this spec (`--trust`).
    pub trust: Option<&'a str>,
    /// Compile the package's own native code.
    pub allow_native: bool,
}

/// The source a spec resolves to, or `None` for a local directory.
/// Is `spec` spelled as a local directory (#4864)? Only an explicit path is:
/// `.`, `..`, or one starting with `./`, `../` or `/`. Anything else is read
/// as a source and goes through the trust check, whatever happens to exist
/// on disk under that name.
fn spelled_local(spec: &str) -> bool {
    matches!(spec, "." | "..") || spec.starts_with("./") || spec.starts_with("../") || Path::new(spec).is_absolute()
}

fn spec_source(spec: &str) -> Option<Result<crate::git_source::GitSource, String>> {
    if spelled_local(spec) {
        return None;
    }
    Some(written_source(spec).unwrap_or_else(|| {
        crate::git_source::GitSource::parse(&project_fetch::resolve_package_spec(spec).1).map_err(|e| format!("error: {e}"))
    }))
}

/// #4184: a remote spec's owner is trusted by default (github.com/almide) or
/// named by `--trust`; anything else is refused before it is fetched.
fn check_install_trust(spec: &str, trust: Option<&str>) -> Result<(), String> {
    let Some(src) = spec_source(spec) else {
        return match trust {
            Some(t) => Err(format!("error: --trust {t}: {spec} is a local directory, which needs no trust")),
            None => Ok(()),
        };
    };
    let src = src?;
    let owner = src.owner();
    match trust.map(crate::trust::owner_arg) {
        Some(Ok(t)) if t.covers(&src) => Ok(()),
        Some(Ok(t)) => Err(format!(
            "error: --trust {t} does not match the owner of {src} ({owner})\n  hint: --trust must name the owner the source belongs to"
        )),
        Some(Err(e)) => Err(format!("error: --trust: {e}")),
        None if crate::trust::is_trusted(&[], &src) => Ok(()),
        None => Err(format!(
            "error: refusing to install {spec}: its owner is not trusted\n  source: {src}\n  owner: {owner}\n  hint: if you asked for this exact repository, rerun with --trust {owner}"
        )),
    }
}

fn default_install_dir() -> PathBuf {
    if let Some(env_dir) = almide_base::env::var("ALMIDE_INSTALL") {
        return PathBuf::from(env_dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("bin");
    }
    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4184: only github.com/almide is trusted without --trust, and
    /// --trust must name the spec's own owner.
    #[test]
    fn install_trust_follows_the_add_rule() {
        assert!(check_install_trust("github.com/almide/almide-bindgen", None).is_ok());
        let e = check_install_trust("github.com/someone/tool", None).unwrap_err();
        assert!(e.contains("its owner is not trusted") && e.contains("--trust github.com/someone"), "{e}");
        assert!(check_install_trust("github.com/someone/tool", Some("someone")).is_ok());
        assert!(check_install_trust("https://github.com/someone/tool", Some("github.com/someone")).is_ok());
        let e = check_install_trust("github.com/someone/tool", Some("other")).unwrap_err();
        assert!(e.contains("does not match"), "{e}");
    }
}
