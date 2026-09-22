//! `almide verify` — a subprocess shim over the independent `almide-verify`
//! binary (#2152).
//!
//! The compiler never judges its own certificates. `almide verify <file.almd>`
//! runs only the untrusted PRODUCER here (lower the program, emit every
//! witness into a certificate bundle) and then execs `almide-verify bundle`,
//! found next to this executable or on PATH, forwarding its standard streams
//! and exit status. Any other arguments are handed to `almide-verify`
//! verbatim. There is NO linked fallback: without the binary the command
//! fails with a named error — a verifier the compiler carried inside itself
//! would be the compiler vouching for itself.

use crate::err;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Exit status when the verifier binary cannot be found or started (the
/// shell's "command not found").
const EXIT_NO_VERIFIER: i32 = 127;

/// The verifier's file name on this platform.
fn verifier_name() -> String {
    format!("almide-verify{}", std::env::consts::EXE_SUFFIX)
}

/// `almide-verify` next to this executable (as invoked, then with symlinks
/// resolved — `~/.local/bin/almide` may link into `~/.local/almide/`), else
/// the first one on PATH.
fn locate_verifier() -> Option<PathBuf> {
    let name = verifier_name();
    let exe = std::env::current_exe().ok();
    let beside = |p: &Path| p.parent().map(|d| d.join(&name));
    let siblings = [exe.as_deref().and_then(beside), exe.and_then(|e| e.canonicalize().ok()).as_deref().and_then(beside)];
    siblings.into_iter().flatten().find(|p| p.is_file()).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|d| d.join(&name)).find(|p| p.is_file())
    })
}

/// The named failure for an absent verifier.
fn no_verifier() -> i32 {
    let here = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.display().to_string()))
        .unwrap_or_else(|| "the almide executable's directory".into());
    err(&format!(
        "error[verifier-missing]: `almide verify` delegates to the independent `{}` binary, which was not found\n  \
         looked in: {here}, then every PATH entry\n  \
         `almide verify` carries no built-in checker by design: the verifier is a separate, independently\n  \
         versioned binary, so the compiler never vouches for its own certificates\n  \
         hint: install both binaries from the same release archive, or run `make install` from a source checkout",
        verifier_name()
    ));
    EXIT_NO_VERIFIER
}

/// Run the verifier with inherited standard streams; its exit status is ours.
fn delegate(verifier: &Path, args: &[String]) -> i32 {
    match Command::new(verifier).args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            err(&format!("error[verifier-missing]: could not start {}: {e}", verifier.display()));
            EXIT_NO_VERIFIER
        }
    }
}

/// One framing-safe line: a name or reason may not break the bundle's lines.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Serialize the producer's output as a certificate bundle (format version 1,
/// `almide_verify::bundle`). Length-prefixed witnesses; walled functions as
/// `uncertified` records.
fn render_bundle(file: &str, w: &almide_mir::pipeline::ProgramWitnesses) -> Vec<u8> {
    let mut out = format!(
        "almide-certificate-bundle 1\nproducer {}\nsource {}\n",
        one_line(env!("ALMIDE_VERSION_LINE")),
        one_line(file)
    )
    .into_bytes();
    let mut record = |property: &str, function: &str, bytes: &str| {
        out.extend(format!("witness {property} {} {}\n", bytes.len(), one_line(function)).into_bytes());
        out.extend(bytes.as_bytes());
        out.push(b'\n');
    };
    for f in &w.functions {
        record("ownership", &f.name, &f.ownership);
        record("names", &f.name, &f.names);
        record("caps", &f.name, &f.caps);
    }
    if let Some(modes) = &w.call_modes {
        record("call-modes", "(program)", modes);
    }
    for (function, reason) in &w.walled {
        out.extend(format!("uncertified {} {}\n", one_line(function), one_line(reason)).into_bytes());
    }
    out
}

/// `[permissions].allow` from `./almide.toml`, when it names any — the same
/// manifest `almide check` enforces; it makes the caps witness able to reject.
fn manifest_allow() -> Option<Vec<String>> {
    let path = Path::new("almide.toml");
    if !path.exists() {
        return None;
    }
    let proj = crate::project::parse_toml(path).ok()?;
    (!proj.permissions.is_empty()).then_some(proj.permissions)
}

/// Produce the bundle for `file` and write it to `out`.
fn produce(file: &str, out: &Path) -> Result<(), i32> {
    let (_program, source_text, resolved, _deps) = super::build::parse_and_resolve_wasm(file).map_err(|()| 1)?;
    let modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();
    let allow = manifest_allow();
    let witnesses = almide_mir::pipeline::program_witnesses(&source_text, &modules, allow.as_deref()).map_err(|e| {
        err(&format!("error: {file} cannot be certified — it does not lower as a whole program: {e}"));
        1
    })?;
    std::fs::write(out, render_bundle(file, &witnesses)).map_err(|e| {
        err(&format!("error: cannot write the certificate bundle {}: {e}", out.display()));
        1
    })
}

/// `<file.almd> [--emit <bundle>]`, split; `None` when the arguments are not
/// that shape (they are then forwarded verbatim).
fn certify_args(args: &[String]) -> Option<(&str, Option<&str>)> {
    match args {
        [file] if file.ends_with(".almd") => Some((file.as_str(), None)),
        [file, flag, out] | [flag, out, file] if file.ends_with(".almd") && flag == "--emit" => {
            Some((file.as_str(), Some(out.as_str())))
        }
        _ => None,
    }
}

pub fn cmd_verify(args: &[String]) -> i32 {
    let verifier = locate_verifier();
    let Some((file, emit)) = certify_args(args) else {
        return verifier.map_or_else(no_verifier, |v| delegate(&v, args));
    };
    if verifier.is_none() && emit.is_none() {
        return no_verifier();
    }
    let scratch = std::env::temp_dir().join(format!("almide-verify-{}.bundle", std::process::id()));
    let bundle = emit.map_or(scratch.clone(), PathBuf::from);
    if let Err(code) = produce(file, &bundle) {
        return code;
    }
    let code = match &verifier {
        Some(v) => delegate(v, &["bundle".to_string(), bundle.display().to_string()]),
        None => {
            err(&format!("wrote {}", bundle.display()));
            no_verifier()
        }
    };
    if emit.is_none() {
        let _ = std::fs::remove_file(&scratch);
    }
    code
}
