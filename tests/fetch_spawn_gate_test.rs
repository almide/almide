//! ADR-0035 D2 gate: nothing on the dependency fetch path starts a process
//! except the one typed ssh launcher, and the launcher takes no string.
//!
//! The fetch path is every file under `src/fetch/` (a new file there is
//! covered without touching this test) and the files the fetch is driven
//! from: `src/project_fetch.rs`, `src/git_source.rs`, `src/package_tree.rs`
//! and `src/trust.rs`. In those, any use of a process API — the `Command`
//! type, a `spawn` / `exec` / `fork` call, `CommandExt` — is a violation,
//! except inside `fn launch` in `src/fetch/ssh.rs`. That `launch` must exist,
//! must take a `&GitSource`, and no parameter of it may be a string, an OS
//! string, a path or a collection of them: every argv element it builds then
//! comes from a typed field that cannot be an option.
//!
//! Negative controls (in this file): the checker is run on the real sources
//! (clean), on the real `project_fetch.rs` with a `Command::new("git")`
//! added, and on `ssh.rs` with a `String` parameter added to `launch`; both
//! must be reported.

use std::path::PathBuf;

use syn::visit::{self, Visit};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `(path relative to the crate, source)` of every fetch-path file.
fn fetch_path_sources() -> Vec<(String, String)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("src/fetch"))
        .expect("src/fetch/")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    for f in ["src/project_fetch.rs", "src/git_source.rs", "src/package_tree.rs", "src/trust.rs"] {
        files.push(root().join(f));
    }
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(root()).unwrap().to_string_lossy().replace('\\', "/");
            (rel, std::fs::read_to_string(&p).expect("read"))
        })
        .collect()
}

const LAUNCHER_FILE: &str = "src/fetch/ssh.rs";
const LAUNCHER_FN: &str = "launch";

/// A path segment that is a process API: the std process builder and its
/// extension trait, and the raw libc entry points.
fn is_process_path(ident: &str) -> bool {
    matches!(ident, "Command" | "CommandExt" | "fork" | "vfork" | "execv" | "execvp" | "execve" | "posix_spawn" | "posix_spawnp")
}

/// A method that starts or replaces a process (on whatever builder).
fn is_process_method(ident: &str) -> bool {
    matches!(ident, "spawn" | "exec" | "pre_exec")
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("cfg") && quote::quote!(#a).to_string().replace(' ', "").contains("cfg(test)"))
}

struct Finder<'a> {
    file: &'a str,
    /// The enclosing fn, while inside one.
    in_fn: Vec<String>,
    out: Vec<String>,
}

impl Finder<'_> {
    fn allowed(&self) -> bool {
        self.file == LAUNCHER_FILE && self.in_fn.first().is_some_and(|f| f == LAUNCHER_FN)
    }

    fn hit(&mut self, what: &str) {
        if !self.allowed() {
            let at = self.in_fn.first().cloned().unwrap_or_else(|| "<module>".to_string());
            self.out.push(format!("{}: `{what}` in {at}", self.file));
        }
    }
}

impl<'ast> Visit<'ast> for Finder<'_> {
    /// A `#[cfg(test)]` module is not on the fetch path.
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_cfg_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.in_fn.push(f.sig.ident.to_string());
        visit::visit_item_fn(self, f);
        self.in_fn.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.in_fn.push(f.sig.ident.to_string());
        visit::visit_impl_item_fn(self, f);
        self.in_fn.pop();
    }

    fn visit_path_segment(&mut self, s: &'ast syn::PathSegment) {
        let id = s.ident.to_string();
        if is_process_path(&id) {
            self.hit(&id);
        }
        visit::visit_path_segment(self, s);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let id = m.method.to_string();
        if is_process_method(&id) {
            self.hit(&format!(".{id}()"));
        }
        visit::visit_expr_method_call(self, m);
    }

    fn visit_use_name(&mut self, u: &'ast syn::UseName) {
        let id = u.ident.to_string();
        if is_process_path(&id) {
            self.hit(&format!("use {id}"));
        }
    }

    fn visit_use_rename(&mut self, u: &'ast syn::UseRename) {
        let id = u.ident.to_string();
        if is_process_path(&id) {
            self.hit(&format!("use {id}"));
        }
    }
}

/// A type that can carry text or a path: refused in the launcher's
/// parameters.
fn stringish(ty: &syn::Type) -> bool {
    let text = quote::quote!(#ty).to_string().replace(' ', "");
    ["String", "str", "OsStr", "OsString", "Path", "PathBuf", "Cow", "Vec", "[", "dynAsRef", "implAsRef", "Into", "ToString", "Display"]
        .iter()
        .any(|w| text.contains(w))
}

/// Every violation in `sources`.
fn violations(sources: &[(String, String)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut launcher_seen = false;
    for (file, src) in sources {
        let ast = syn::parse_file(src).unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut f = Finder { file, in_fn: Vec::new(), out: Vec::new() };
        f.visit_file(&ast);
        out.extend(f.out);
        if file == LAUNCHER_FILE {
            for item in &ast.items {
                let syn::Item::Fn(func) = item else { continue };
                if func.sig.ident != LAUNCHER_FN {
                    continue;
                }
                launcher_seen = true;
                let mut takes_source = false;
                for input in &func.sig.inputs {
                    let syn::FnArg::Typed(pt) = input else { continue };
                    let ty = &*pt.ty;
                    if stringish(ty) {
                        out.push(format!("{file}: `{LAUNCHER_FN}` takes `{}`, which can carry text", quote::quote!(#ty)));
                    }
                    if quote::quote!(#ty).to_string().replace(' ', "") == "&GitSource" {
                        takes_source = true;
                    }
                }
                if !takes_source {
                    out.push(format!("{file}: `{LAUNCHER_FN}` does not take a `&GitSource`"));
                }
            }
        }
    }
    if !launcher_seen {
        out.push(format!("{LAUNCHER_FILE}: no `fn {LAUNCHER_FN}`"));
    }
    out
}

#[test]
fn the_fetch_path_starts_no_process_but_the_typed_ssh_launcher() {
    let sources = fetch_path_sources();
    assert!(sources.iter().any(|(f, _)| f == LAUNCHER_FILE), "{LAUNCHER_FILE} is not on the fetch path");
    let v = violations(&sources);
    assert!(v.is_empty(), "the fetch path starts a process outside the ssh launcher:\n  {}", v.join("\n  "));
}

#[test]
fn negative_control_a_git_spawn_on_the_fetch_path_is_reported() {
    let mut sources = fetch_path_sources();
    let (_, pf) = sources.iter_mut().find(|(f, _)| f == "src/project_fetch.rs").expect("project_fetch.rs");
    pf.push_str("\nfn sneaky() { let _ = std::process::Command::new(\"git\").arg(\"ls-remote\").output(); }\n");
    let v = violations(&sources);
    assert!(v.iter().any(|l| l.contains("src/project_fetch.rs") && l.contains("Command") && l.contains("sneaky")), "{v:?}");
    // A spawn in another fn of the launcher's own file is reported too.
    let mut sources = fetch_path_sources();
    let (_, ssh) = sources.iter_mut().find(|(f, _)| f == LAUNCHER_FILE).unwrap();
    ssh.push_str("\nfn other() { let _ = Command::new(\"sh\").spawn(); }\n");
    let v = violations(&sources);
    assert!(v.iter().any(|l| l.contains("in other")), "{v:?}");
}

#[test]
fn negative_control_a_string_parameter_on_the_launcher_is_reported() {
    let mut sources = fetch_path_sources();
    let (_, ssh) = sources.iter_mut().find(|(f, _)| f == LAUNCHER_FILE).unwrap();
    assert!(ssh.contains("pub fn launch(source: &GitSource, dot_git: bool)"), "the launcher's signature moved; update this control");
    *ssh = ssh.replace("pub fn launch(source: &GitSource, dot_git: bool)", "pub fn launch(source: &GitSource, dot_git: bool, extra: &str)");
    let v = violations(&sources);
    assert!(v.iter().any(|l| l.contains("takes `& str`") || l.contains("takes `&str`")), "{v:?}");
    let mut sources = fetch_path_sources();
    let (_, ssh) = sources.iter_mut().find(|(f, _)| f == LAUNCHER_FILE).unwrap();
    *ssh = ssh.replace("pub fn launch(source: &GitSource, dot_git: bool)", "pub fn launch(url: String, dot_git: bool)");
    let v = violations(&sources);
    assert!(v.iter().any(|l| l.contains("String")) && v.iter().any(|l| l.contains("does not take a `&GitSource`")), "{v:?}");
}

/// The launcher's argv, end to end: a stub `ssh` first on PATH records its
/// arguments and serves the repository with `git upload-pack`, so an
/// `ssh://` dependency is fetched through exactly one process whose argv
/// ends ssh's options with `--` before the host.
#[cfg(unix)]
#[test]
fn an_ssh_source_runs_one_ssh_with_typed_argv() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = std::env::temp_dir().join(format!("almide-ssh-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (bin, repo, app, home) = (tmp.join("bin"), tmp.join("repo"), tmp.join("app"), tmp.join("home"));
    for d in [&bin, &repo.join("src"), &app.join("src"), &home] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(repo.join("almide.toml"), "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src/mod.almd"), "fn word() -> String = \"over-ssh\"\n").unwrap();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git")
            .current_dir(&repo)
            .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "c"]);
    git(&["tag", "v0.1.0"]);
    let argv_log = tmp.join("argv");
    let stub = bin.join("ssh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done >> '{}'\necho END >> '{}'\nexec git upload-pack '{}'\n",
            argv_log.display(),
            argv_log.display(),
            repo.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(app.join("src/main.almd"), "import lib\neffect fn main() -> Unit = println(lib.word())\n").unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    // The scp-like form keeps git's meaning — a path relative to the remote
    // user's home, sent without a leading `/` — and its own canonical print;
    // the `ssh://` form is absolute. Each gets its own home (cache) and lock.
    for (written, port_args, remote, locked) in [
        ("git@git.example.org:team/lib.git", &[][..], "git-upload-pack 'team/lib.git'", "git@git.example.org:team/lib"),
        ("ssh://git@git.example.org:2222/team/lib", &["-p", "2222"][..], "git-upload-pack '/team/lib'", "ssh://git@git.example.org:2222/team/lib"),
    ] {
        let _ = std::fs::remove_file(&argv_log);
        let _ = std::fs::remove_file(app.join("almide.lock"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::write(
            app.join("almide.toml"),
            format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[trust]\nowners = [\"git.example.org/team\"]\n\n[dependencies]\nlib = {{ git = \"{written}\", tag = \"v0.1.0\" }}\n"),
        )
        .unwrap();
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_almide"))
            .current_dir(&app)
            .env("HOME", &home)
            .env("PATH", &path)
            .args(["run", "src/main.almd", "--target", "wasm"])
            .output()
            .unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "over-ssh", "{written}: {text}");
        let log = std::fs::read_to_string(&argv_log).unwrap();
        let first: Vec<&str> = log.split("END\n").next().unwrap().lines().collect();
        let mut want = vec!["-o", "SendEnv=GIT_PROTOCOL"];
        want.extend_from_slice(port_args);
        want.extend_from_slice(&["-l", "git", "--", "git.example.org", remote]);
        assert_eq!(first, want, "the launcher's argv for {written}");
        let lock = std::fs::read_to_string(app.join("almide.lock")).unwrap();
        assert!(lock.contains(&format!("git = \"{locked}\"")) && lock.contains("sum = \"h1:"), "{written}: {lock}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
