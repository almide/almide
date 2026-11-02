//! `almide verify` — a subprocess shim over the independent `almide-verify`
//! binary (#2152).
//!
//! The compiler never judges its own certificates. `almide verify <file.almd>`
//! runs only the untrusted PRODUCER here and then execs `almide-verify
//! bundle`, found next to this executable or on PATH, forwarding its standard
//! streams and exit status. Any other arguments are handed to `almide-verify`
//! verbatim. There is NO linked fallback: without the binary the command
//! fails with a named error — a verifier the compiler carried inside itself
//! would be the compiler vouching for itself.
//!
//! THE PRODUCER (#4011) is the structural wasm leg — the one renderer
//! `almide build --target wasm` ships. The program is built by the same
//! route with the witness sinks armed, and the bundle carries what that
//! build recorded (crates/almide-wasm/src/witness_bundle.rs): ownership per
//! frame, call modes, bounds, names over the shipped bytes, capabilities.
//! When the built file is present (`--artifact`, or the file `almide build`
//! writes by default), the producer refuses unless it is byte-for-byte the
//! module this build produced, and pins its SHA-256 in the bundle (version
//! 2): almide-verify recomputes the digest and rejects a mismatch, so the
//! verdict is about THAT file. Before #4011 this command certified the MIR
//! lowering's witnesses, which no shipped wasm byte comes from since #2761;
//! those witnesses stay the input of proofs/gate.sh and corpus-wall.sh (the
//! native leg's MIR), not of this command.

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

/// `[permissions].allow` from `./almide.toml`, when the key is written — the
/// same manifest `almide check` enforces; it makes the caps witness able to
/// reject. An explicit `allow = []` grants nothing (#4101).
fn manifest_allow() -> Option<Vec<String>> {
    let path = Path::new("almide.toml");
    if !path.exists() {
        return None;
    }
    let proj = crate::project::parse_toml(path).ok()?;
    proj.permissions
}

/// What the bundle is pinned to: the shipped file, if one is named or
/// present at the default build output.
enum Pin<'a> {
    /// `--artifact <path>`: must exist and match.
    Named(&'a str),
    /// The file `almide build <file> --target wasm` writes, if present.
    Default,
}

/// Produce the bundle for `file` and write it to `out`.
fn produce(file: &str, pin: Pin<'_>, out: &Path) -> Result<(), i32> {
    almide_wasm::witness::start_collecting();
    let built = super::wasm_compile::compile_to_wasm_bytes_surfaced(file, false, false, true, false);
    let Ok((emitted, host_ops, _surface, _outside)) = built else {
        let _ = almide_wasm::witness::take();
        err(&format!("error: {file} cannot be certified — it does not build for --target wasm (above)"));
        return Err(1);
    };
    let shipped = match super::build_wasm::shipped_core_module(file, &emitted, &host_ops) {
        Ok(b) => b,
        Err(leg) => {
            let _ = almide_wasm::witness::take();
            err(&format!(
                "error: {file} builds as a component ({leg}); `almide verify` certifies the core module a plain `almide build --target wasm` writes"
            ));
            return Err(1);
        }
    };
    let allow = manifest_allow();
    let (records, uncertified) = almide_wasm::witness::bundle::records(&emitted, &shipped, allow.as_deref()).map_err(|e| {
        err(&format!("error: {file}: the structural build recorded no complete certificate: {e}"));
        1
    })?;
    let default_out = super::build_wasm::default_wasm_output(file);
    let (path, required) = match pin {
        Pin::Named(p) => (p.to_string(), true),
        Pin::Default => (default_out, false),
    };
    let artifact = match std::fs::read(&path) {
        Ok(bytes) if bytes == shipped => {
            use sha2::Digest;
            let hex: String = sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
            let abs = std::path::absolute(&path).map(|p| p.display().to_string()).unwrap_or(path.clone());
            Some((hex, abs))
        }
        Ok(bytes) => {
            err(&format!(
                "error: {path} ({} bytes) is not the module `almide build {file} --target wasm` produces now ({} bytes) — the certificate would describe other bytes\n  \
                 hint: rebuild it, or name the right file with --artifact",
                bytes.len(),
                shipped.len()
            ));
            return Err(1);
        }
        Err(e) if required => {
            err(&format!("error: cannot read the artifact {path}: {e}"));
            return Err(1);
        }
        Err(_) => {
            err(&format!(
                "note: no {path} to pin — the bundle certifies the module this build produces, not a file; \
                 run `almide build {file} --target wasm` first to pin the file you ship"
            ));
            None
        }
    };
    let text = almide_wasm::witness::bundle::render(
        &format!("almide {}", env!("ALMIDE_VERSION_LINE")),
        file,
        artifact.as_ref().map(|(h, p)| (h.as_str(), p.as_str())),
        &records,
        &uncertified,
    );
    std::fs::write(out, text).map_err(|e| {
        err(&format!("error: cannot write the certificate bundle {}: {e}", out.display()));
        1
    })
}

/// Is an independent verifier installed (next to this executable, or on
/// PATH)? `almide build --target wasm` judges its own certificate when one is.
pub(super) fn verifier_present() -> bool {
    locate_verifier().is_some()
}

/// The build gate (#4011, row 3 of the security table): the certificate of
/// the build that just ran — the witness sinks were armed before it — pinned
/// to the exact bytes about to be written, judged by the installed
/// `almide-verify`. `Ok(())` when it CERTIFIES the bundle, or reports it
/// INCOMPLETE (exit 3: every witness accepted, some function declared
/// uncertified — nothing is contradicted); `Err` with the verifier's report
/// when it REJECTS a witness or the bundle (exit 1 / 2), or cannot be run:
/// the build then refuses to write the artifact. The compiler still never
/// judges its own certificate — the verdict is the separate binary's.
pub(super) fn gate_build(file: &str, emitted: &[u8], shipped: &[u8]) -> Result<(), String> {
    let Some(verifier) = locate_verifier() else { return Ok(()) };
    let allow = manifest_allow();
    let (records, uncertified) = almide_wasm::witness::bundle::records(emitted, shipped, allow.as_deref())
        .map_err(|e| format!("the structural build recorded no complete certificate: {e}"))?;
    let dir = std::env::temp_dir();
    let id = std::process::id();
    let (art, bun) = (dir.join(format!("almide-build-{id}.wasm")), dir.join(format!("almide-build-{id}.bundle")));
    let cleanup = || {
        let _ = std::fs::remove_file(&art);
        let _ = std::fs::remove_file(&bun);
    };
    use sha2::Digest;
    let hex: String = sha2::Sha256::digest(shipped).iter().map(|b| format!("{b:02x}")).collect();
    let art_path = art.display().to_string();
    let text = almide_wasm::witness::bundle::render(
        &format!("almide {}", env!("ALMIDE_VERSION_LINE")),
        file,
        Some((hex.as_str(), art_path.as_str())),
        &records,
        &uncertified,
    );
    if let Err(e) = std::fs::write(&art, shipped).and_then(|()| std::fs::write(&bun, text)) {
        cleanup();
        return Err(format!("cannot write the certificate for {}: {e}", verifier.display()));
    }
    let run = Command::new(&verifier).arg("bundle").arg(&bun).output();
    cleanup();
    let out = run.map_err(|e| format!("could not start {}: {e}", verifier.display()))?;
    match out.status.code() {
        Some(0 | 3) => Ok(()),
        code => Err(format!(
            "{} judged this build's certificate and did not accept it (exit {}):\n{}{}",
            verifier.display(),
            code.map_or_else(|| "signal".to_string(), |c| c.to_string()),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )),
    }
}

/// One framing-safe line: a name or reason may not break the bundle's lines.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `--mir`: the MIR lowering's witnesses (format version 1). No shipped
/// wasm byte comes from MIR lowering since #2761, so this certifies the MIR,
/// not a build; it stays because proofs/coverage.sh drives the MIR
/// certificate producers (`certificate*.rs`, `pipeline_witnesses.rs`)
/// through it, and their coverage floor is a ratchet.
fn produce_mir(file: &str, out: &Path) -> Result<(), i32> {
    let (_program, source_text, resolved, _deps) = super::wasm_compile::parse_and_resolve_wasm(file).map_err(|()| 1)?;
    let modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();
    let allow = manifest_allow();
    let w = almide_mir::pipeline::program_witnesses(&source_text, &modules, allow.as_deref()).map_err(|e| {
        err(&format!("error: {file} cannot be certified — it does not lower as a whole program: {e}"));
        1
    })?;
    let mut text = format!(
        "almide-certificate-bundle 1\nproducer almide {} (MIR witnesses)\nsource {}\n",
        one_line(env!("ALMIDE_VERSION_LINE")),
        one_line(file)
    );
    let mut record = |property: &str, function: &str, bytes: &str| {
        text.push_str(&format!("witness {property} {} {}\n{bytes}\n", bytes.len(), one_line(function)));
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
        text.push_str(&format!("uncertified {} {}\n", one_line(function), one_line(reason)));
    }
    std::fs::write(out, text).map_err(|e| {
        err(&format!("error: cannot write the certificate bundle {}: {e}", out.display()));
        1
    })
}

/// The `<file.almd>` form's options; `None` when the arguments are not that
/// shape (they are then forwarded verbatim).
struct CertifyArgs<'a> {
    file: &'a str,
    emit: Option<&'a str>,
    artifact: Option<&'a str>,
    mir: bool,
}

/// `<file.almd> [--emit <bundle>] [--artifact <file.wasm>] [--mir]`, in
/// any order (`--mir` takes no `--artifact`: nothing ships from MIR).
fn certify_args(args: &[String]) -> Option<CertifyArgs<'_>> {
    let (mut file, mut emit, mut artifact, mut mir) = (None, None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--emit" if emit.is_none() => emit = Some(it.next()?.as_str()),
            "--artifact" if artifact.is_none() => artifact = Some(it.next()?.as_str()),
            "--mir" if !mir => mir = true,
            f if f.ends_with(".almd") && file.is_none() => file = Some(f),
            _ => return None,
        }
    }
    if mir && artifact.is_some() {
        return None;
    }
    Some(CertifyArgs { file: file?, emit, artifact, mir })
}

pub fn cmd_verify(args: &[String]) -> i32 {
    let verifier = locate_verifier();
    let Some(CertifyArgs { file, emit, artifact, mir }) = certify_args(args) else {
        return verifier.map_or_else(no_verifier, |v| delegate(&v, args));
    };
    if verifier.is_none() && emit.is_none() {
        return no_verifier();
    }
    let scratch = std::env::temp_dir().join(format!("almide-verify-{}.bundle", std::process::id()));
    let bundle = emit.map_or(scratch.clone(), PathBuf::from);
    let pin = artifact.map_or(Pin::Default, Pin::Named);
    let produced = if mir { produce_mir(file, &bundle) } else { produce(file, pin, &bundle) };
    if let Err(code) = produced {
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
