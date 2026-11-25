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

/// `[permissions].allow` of `file`'s own project (the one lookup,
/// `project::manifest_for_entry`; #4830: it read `./almide.toml`), when the
/// key is written — the same manifest `almide check` enforces; it makes the
/// caps witness able to reject. An explicit `allow = []` grants nothing (#4101).
fn manifest_allow(file: &str) -> Option<Vec<String>> {
    // #4829: a manifest that does not parse is refused, never "no allow".
    crate::project::permissions_project_for(file).and_then(|p| p.permissions)
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
    let allow = manifest_allow(file);
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

/// The build gate on a route that RUNS the module it makes instead of
/// writing it (#4836): `run`, `bench` and `test` on wasm judged nothing, so a
/// module `almide build` refuses ran. The witness sinks are armed for `make`,
/// and the module is run only when [`gate_build`] accepts it; a rejection
/// exits 1. The sinks are process-wide, so a parallel test lane makes and
/// judges one module at a time.
pub(super) fn certified(file: &str, make: impl FnOnce() -> Result<(Vec<u8>, Vec<i32>), ()>) -> Result<(Vec<u8>, Vec<i32>), ()> {
    certified_with(file, &[], make)
}

/// The grants the test runner's own calls need (#4836): its abort-site
/// selector reads how the run was launched (`Env.read`) and a site's child
/// run starts a fresh instance (`Proc`) — the `testing` host ops the
/// compiler's runner `main` makes, not the program's. The program's own
/// calls were already held to `[permissions]` by the front gate.
/// `the_test_runner_grants_cover_the_testing_host_ops` ties this to the
/// host-op table.
pub(super) const TEST_RUNNER_GRANTS: &[&str] = &["Env.read", "Proc"];

/// [`certified`] for a test lane's module: the runner's grants
/// ([`TEST_RUNNER_GRANTS`]) join the manifest's allow when it has one.
pub(super) fn certified_test(file: &str, make: impl FnOnce() -> Result<(Vec<u8>, Vec<i32>), ()>) -> Result<(Vec<u8>, Vec<i32>), ()> {
    certified_with(file, TEST_RUNNER_GRANTS, make)
}

fn certified_with(file: &str, runner: &[&str], make: impl FnOnce() -> Result<(Vec<u8>, Vec<i32>), ()>) -> Result<(Vec<u8>, Vec<i32>), ()> {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    super::wasm_run_cache::reset_judged();
    almide_wasm::witness::start_collecting();
    let (bytes, rest) = make()?;
    // A run-cache hit on this route is a module this verifier accepted
    // before (entries are written only after acceptance, keyed by it).
    if super::wasm_run_cache::took_judged_hit() {
        let _ = almide_wasm::witness::take();
        return Ok((bytes, rest));
    }
    if let Err(e) = gate_build(file, &bytes, &bytes, runner) {
        err(&format!(
            "error[certificate-rejected]: {file}: {e}\n  the module was not run: its own certificate does not hold, which is a compiler bug — please report it"
        ));
        std::process::exit(1);
    }
    super::wasm_run_cache::commit_judged();
    Ok((bytes, rest))
}

/// The judging verifier's identity for the run cache's judged entries: the
/// linked `almide-verify` library's version and formats.
pub(super) fn verifier_identity() -> String {
    format!("linked almide-verify {} ({})", almide_verify::VERSION, almide_verify::FORMATS)
}

/// The build gate (#4011, row 3 of the security table): the certificate of
/// the build that just ran — the witness sinks were armed before it — pinned
/// to the exact bytes about to be written, judged by the `almide-verify`
/// library linked into this binary (#4836, owner ruling 2026-11-24: a
/// verifier that may be absent left the build unjudged — "not installed"
/// cannot happen now). `Ok(())` when it CERTIFIES the bundle, or reports it
/// INCOMPLETE (every witness accepted, some function declared uncertified —
/// nothing is contradicted); `Err` with the verdict lines when it REJECTS a
/// witness, the artifact's hash, or cannot read the bundle: the build then
/// refuses to write the artifact. The verifier crate links no compiler crate
/// (crates/almide-verify/Cargo.toml), so the judge stays independent code;
/// `almide verify` still hands a bundle to the standalone binary.
pub(super) fn gate_build(file: &str, emitted: &[u8], shipped: &[u8], runner: &[&str]) -> Result<(), String> {
    let allow = manifest_allow(file).map(|mut a| {
        a.extend(runner.iter().map(|g| g.to_string()));
        a
    });
    let (records, uncertified) = almide_wasm::witness::bundle::records(emitted, shipped, allow.as_deref())
        .map_err(|e| format!("the structural build recorded no complete certificate: {e}"))?;
    use sha2::Digest;
    let hex: String = sha2::Sha256::digest(shipped).iter().map(|b| format!("{b:02x}")).collect();
    let text = almide_wasm::witness::bundle::render(
        &format!("almide {}", env!("ALMIDE_VERSION_LINE")),
        file,
        Some((hex.as_str(), "artifact.wasm")),
        &records,
        &uncertified,
    );
    almide::certificate_gate::judge(text.as_bytes(), shipped)
}

/// One framing-safe line: a name or reason may not break the bundle's lines.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `--mir`: the certificates of the MIR the native route ships (#4909), in
/// that route's own family. The program goes through the same frontend and
/// the same v1 render `almide build` takes (`try_compile_pending`,
/// `render_v1_native_certified`), and the bundle holds what that render
/// returns for the functions it renders from (native-calls with its runtime
/// blocks, the bounds witnesses) in a version-5 bundle, the first that
/// carries `native-calls`. No wasm byte comes from MIR since #2761;
/// the wasm route's structural leg is certified by plain `almide verify`. A
/// program the native render walls is uncertified: the bundle names the wall
/// and holds no witness, and nothing falls back to another producer.
fn produce_mir(file: &str, out: &Path) -> Result<(), i32> {
    let opts = crate::codegen::CodegenOptions { repr_c: false, allow_unverified: false, trace: false, meter_in_codegen: false };
    let (mut pending, _metered, _outside) = crate::try_compile_pending(file, false, &opts).map_err(|_| 1)?;
    let (_callbacks, _source, rendered) = super::render_v1_native_certified(&mut pending);
    let mut text = format!(
        "almide-certificate-bundle 5\nproducer almide {} (native MIR certificates)\nsource {}\n",
        one_line(env!("ALMIDE_VERSION_LINE")),
        one_line(file)
    );
    let wall = match rendered {
        Ok((_, certs)) => {
            for (property, function, bytes) in certs {
                text.push_str(&format!("witness {property} {} {}\n{bytes}\n", bytes.len(), one_line(&function)));
            }
            None
        }
        Err(e) => {
            let wall = one_line(&e.to_string());
            text.push_str(&format!("uncertified (program) {wall}\n"));
            Some(wall)
        }
    };
    std::fs::write(out, text).map_err(|e| {
        err(&format!("error: cannot write the certificate bundle {}: {e}", out.display()));
        1
    })?;
    match wall {
        Some(wall) => {
            err(&format!("error: {file} is uncertified — the native route walls it: {wall}"));
            Err(1)
        }
        None => Ok(()),
    }
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
    // #4853: a private, freshly created directory, as in `gate_build`.
    let scratch_dir = if emit.is_none() {
        match almide_base::private_dir::private_scratch_dir("almide-verify") {
            Ok(d) => Some(d),
            Err(e) => {
                err(&format!("cannot make a private directory for the bundle: {e}"));
                return 1;
            }
        }
    } else {
        None
    };
    let scratch = scratch_dir.as_ref().map_or_else(PathBuf::new, |d| d.join("certificate.bundle"));
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
    if let Some(d) = scratch_dir {
        let _ = std::fs::remove_dir_all(d);
    }
    code
}

#[cfg(test)]
mod runner_grant_tests {
    /// The runner's grants cover every capability a `testing` host op
    /// reaches, and nothing a non-`testing` op needs beyond them.
    #[test]
    fn the_test_runner_grants_cover_the_testing_host_ops() {
        let granted = almide_mir::certificate::manifest_caps(&super::TEST_RUNNER_GRANTS.iter().map(|g| g.to_string()).collect::<Vec<_>>());
        let testing: Vec<_> = almide_mir::host_ops::HOST_OPS.iter().filter(|o| o.module == "testing").collect();
        assert!(!testing.is_empty(), "no testing host op in the table");
        let mut needed = Vec::new();
        for o in testing {
            for c in o.caps {
                assert!(granted.contains(c), "testing.{} reaches {c:?}, which the runner grants do not cover", o.func);
                if !needed.contains(c) {
                    needed.push(*c);
                }
            }
        }
        assert_eq!(granted.len(), needed.len(), "the runner grants {granted:?} exceed what the testing host ops reach {needed:?}");
    }
}
