//! Which native target `almide build` produces a binary for (#2772).
//!
//! The native leg used to have exactly one answer, "the host", and located
//! cargo's output at the fixed `target/<profile>/almide-out`. Cargo writes to
//! `target/<triple>/<profile>/` as soon as it is given a target — by `--target`,
//! by `CARGO_BUILD_TARGET`, or by `build.target` in a cargo config — so an
//! inherited `CARGO_BUILD_TARGET=aarch64-unknown-linux-musl` either copied out
//! the PREVIOUS glibc binary still sitting at the fixed path ("Built
//! hello-musl", byte-identical to the glibc build) or failed with "expected
//! binary not found".
//!
//! The rule now: the requested target is resolved HERE, once, into
//! `Option<triple>` (`None` = host), and every cargo invocation states it
//! explicitly — `--target <triple>` for a cross build, and
//! `CARGO_BUILD_TARGET` removed from the child's environment either way, so
//! nothing but this resolution decides where the artifact lands.
//! `artifact_dir` is the one place that turns the decision into a path.
//!
//! Sources, highest precedence first:
//! 1. `almide build --target <t>`: `rust`/`native` (host), `linux-musl`
//!    (`<host arch>-unknown-linux-musl`), or a full rustc target triple.
//! 2. `CARGO_BUILD_TARGET` in the environment — cargo's own spelling of the
//!    same request, honoured rather than silently ignored.
//!
//! The resolved triple is scoped to the build by [`CrossTargetGuard`] (the CLI
//! runs sequentially on one worker thread, the same scoping the `--heap-cap`
//! guard uses), so `almide run` / `almide test` — whose binaries must execute
//! on this machine — never see it and always build for the host.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static CROSS_TARGET: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Scope a resolved cross target to the current build. Restores the previous
/// value on drop.
pub(crate) struct CrossTargetGuard(Option<String>);

impl CrossTargetGuard {
    pub(crate) fn set(triple: Option<String>) -> Self {
        CrossTargetGuard(CROSS_TARGET.with(|c| c.replace(triple)))
    }
}

impl Drop for CrossTargetGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        CROSS_TARGET.with(|c| *c.borrow_mut() = prev);
    }
}

/// The cross target the current build was asked for; `None` = the host.
pub(crate) fn cross_target() -> Option<String> {
    CROSS_TARGET.with(|c| c.borrow().clone())
}

thread_local! {
    static TARGET_CPU: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Scope `almide build --target-cpu <cpu>` to the current build, the same way
/// [`CrossTargetGuard`] scopes the triple. Restores the previous value on drop.
pub(crate) struct TargetCpuGuard(Option<String>);

impl TargetCpuGuard {
    pub(crate) fn set(cpu: Option<String>) -> Self {
        TargetCpuGuard(TARGET_CPU.with(|c| c.replace(cpu)))
    }
}

impl Drop for TargetCpuGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        TARGET_CPU.with(|c| *c.borrow_mut() = prev);
    }
}

/// The `-C target-cpu` a `--release` build passes (#3604), or `None` for
/// rustc's generic default.
///
/// The default is a named BASELINE for the target's architecture, not
/// `native`: an x86_64 target gets `x86-64-v3` (AVX2 and FMA — Intel since
/// Haswell, 2013; AMD since Zen, 2017), any other target the generic CPU.
/// Measured 2026-10-15, spectralnorm with a runtime `n`, ms (generic → v3 →
/// native): AMD EPYC 7B13 (Zen3, the GitHub x86 runner's class) imperative
/// 87.8 → 67.2 → 109.8, enumerate 88.4 → 69.5 → 108.4; Intel Xeon 8581C
/// 57.1 → 45.4 → 47.1. `native` lost 25% on Zen3 (LLVM's znver3 cost model
/// vectorizes `eval_a` badly), and it shipped as the default once (#3727,
/// reverted): a default must win on every CPU class measured, and only the
/// v3 baseline did. aarch64 stays generic: on Neoverse-V2 native helped the
/// idiomatic spellings but cost the imperative one 5%, and no named
/// baseline was measured to win both.
///
/// `--target-cpu portable` asks for the generic CPU (a binary for a machine
/// without AVX2); any other value is passed to rustc as is (`native`,
/// `x86-64-v4`, `neoverse-n1`, …).
pub(crate) fn release_target_cpu() -> Option<String> {
    match TARGET_CPU.with(|c| c.borrow().clone()) {
        Some(cpu) => (cpu != "portable").then_some(cpu),
        None => {
            let arch = cross_target().map_or_else(|| std::env::consts::ARCH.to_string(), |t| t.split('-').next().unwrap_or("").to_string());
            (arch == "x86_64").then(|| "x86-64-v3".to_string())
        }
    }
}

/// The LLVM options every `--release` native build passes, on every target
/// and CPU (#3604): the release profile itself, like `opt-level=3`.
///
/// `-force-ordered-reductions=false` keeps the loop vectorizer off loops
/// whose only reduction is an IN-ORDER float sum. Almide's `Float` `+` is
/// strict IEEE (no reassociation), so a vectorized `fold(0.0, (a, x) => a +
/// f(x))` still adds lane by lane in order: the vector body only adds the
/// lane shuffles around a scalar chain. x86's cost model never vectorizes
/// such a loop; AArch64's opts in (`enableOrderedReductions`), and its
/// generic tuning then splits spectralnorm's `eval_a` i64 multiply (NEON has
/// no 64-bit vector multiply) across lanes. Measured 2026-10-19, GCE
/// Neoverse-V2, n=1200, release: imperative 77.0 → 53.6 ms, enumerate
/// 77.0 → 54.2 ms; on x86 the option is what the cost model already does.
/// Integer reductions and loops without a reduction vectorize as before;
/// this is not `-vectorize-loops=false`, which cost enumerate 9 ms more.
pub(crate) const RELEASE_LLVM_ARGS: &[&str] = &["-force-ordered-reductions=false"];

/// [`RELEASE_LLVM_ARGS`] as rustc arguments (`-C llvm-args=…` each).
pub(crate) fn release_llvm_rustc_args() -> Vec<String> {
    RELEASE_LLVM_ARGS.iter().flat_map(|a| ["-C".to_string(), format!("llvm-args={a}")]).collect()
}

/// `--target-cpu native` on a cross build would tune for this machine's CPU,
/// which the target may not have.
pub(crate) fn validate_target_cpu(cpu: Option<&str>, triple: Option<&str>) -> Result<(), String> {
    match (cpu, triple) {
        (Some("native"), Some(t)) => Err(format!(
            "error: --target-cpu native tunes for THIS machine, but the build targets {t}\n  \
             hint: name the target's CPU (`--target-cpu x86-64-v3`), or omit the flag for the target's default baseline"
        )),
        _ => Ok(()),
    }
}

/// Resolve `almide build --target <flag>` (already known not to be a wasm
/// target) plus the inherited `CARGO_BUILD_TARGET` into the triple to build
/// for, `None` meaning the host. `host_arch` is `std::env::consts::ARCH`,
/// passed in so the `linux-musl` shorthand is testable for every arch.
pub(crate) fn resolve_native_target(
    flag: Option<&str>,
    cargo_build_target: Option<&str>,
    host_arch: &str,
) -> Result<Option<String>, String> {
    match flag {
        Some("rust" | "native") => Ok(None),
        Some("linux-musl") => match host_arch {
            "x86_64" | "aarch64" => Ok(Some(format!("{host_arch}-unknown-linux-musl"))),
            other => Err(format!(
                "error: `--target linux-musl` has no musl triple for this host's architecture ({other})\n  \
                 hint: pass a full triple, e.g. `--target x86_64-unknown-linux-musl`"
            )),
        },
        Some(t) => validate_triple(t, "--target"),
        None => match cargo_build_target.map(str::trim) {
            None | Some("") => Ok(None),
            Some(t) => validate_triple(t, "CARGO_BUILD_TARGET"),
        },
    }
}

fn validate_triple(t: &str, source: &str) -> Result<Option<String>, String> {
    if t.starts_with("wasm32") || t.starts_with("wasm64") {
        return Err(format!(
            "error: `{t}` (from {source}) is a wasm target, which the native build cannot produce\n  \
             hint: use `almide build app.almd --target wasm`"
        ));
    }
    let well_formed = t.split('-').count() >= 2
        && t.split('-').all(|part| !part.is_empty())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !well_formed {
        return Err(format!(
            "error: unknown build target `{t}` (from {source})\n  \
             supported: rust (default, native binary for this host), wasm, linux-musl, \
             or a rustc target triple such as x86_64-unknown-linux-musl\n  \
             hint: `rustc --print target-list` lists the triples"
        ));
    }
    Ok(Some(t.to_string()))
}

/// Where cargo puts a `profile` build's artifacts for `triple`:
/// `target/<profile>` for the host, `target/<triple>/<profile>` otherwise.
pub(crate) fn artifact_dir(project_dir: &Path, triple: Option<&str>, profile: &str) -> PathBuf {
    let target = project_dir.join("target");
    match triple {
        Some(t) => target.join(t).join(profile),
        None => target.join(profile),
    }
}

/// Pin a cargo invocation to `triple` (`None` = host): pass `--target` for a
/// cross build and drop any inherited `CARGO_BUILD_TARGET`, so the artifact
/// lands where [`artifact_dir`] says.
pub(crate) fn pin_cargo_target(cmd: &mut std::process::Command, triple: Option<&str>) {
    cmd.env_remove("CARGO_BUILD_TARGET");
    if let Some(t) = triple {
        cmd.arg("--target").arg(t);
    }
}

/// A shared-library file name for `triple` (`None` = host): `lib<name>.so`,
/// `lib<name>.dylib`, or `<name>.dll`.
pub(crate) fn cdylib_file_name(lib_name: &str, triple: Option<&str>) -> String {
    let (windows, apple) = match triple {
        Some(t) => (t.contains("windows"), t.contains("apple")),
        None => (cfg!(target_os = "windows"), cfg!(target_os = "macos")),
    };
    let prefix = if windows { "" } else { "lib" };
    let ext = if windows { "dll" } else if apple { "dylib" } else { "so" };
    format!("{prefix}{lib_name}.{ext}")
}

/// An executable file name for `triple` (`None` = host).
pub(crate) fn exe_file_name(stem: &str, triple: Option<&str>) -> String {
    let windows = match triple {
        Some(t) => t.contains("windows"),
        None => cfg!(windows),
    };
    if windows { format!("{stem}.exe") } else { stem.to_string() }
}

/// A cross build that failed because the TOOLCHAIN lacks the target — the
/// target's standard library is not installed, or no linker for it — is the
/// user's setup, not an Almide codegen bug, and must not be reported under the
/// "codegen produced invalid Rust" banner (E0463 carries a rustc code, which
/// is what that banner keys on). Returns the actionable message, or `None`
/// when the failure is something else.
pub(crate) fn cross_toolchain_error(stderr: &str, triple: &str) -> Option<String> {
    let missing_std = stderr.contains("E0463") && stderr.contains("can't find crate for `std`");
    let link_failed = stderr.contains("error: linking with") || stderr.contains("linker `");
    let hint = if missing_std {
        format!("the standard library for `{triple}` is not installed\n  hint: `rustup target add {triple}`")
    } else if link_failed {
        format!(
            "linking for `{triple}` failed — the host has no linker for that target\n  \
             hint: install one (for musl on Linux: `musl-tools`; for another architecture a cross linker, \
             set with CARGO_TARGET_<TRIPLE>_LINKER)"
        )
    } else {
        return None;
    };
    Some(format!("error: cannot build for `{triple}`: {hint}\n\n--- cargo output ---\n{stderr}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_request_is_the_host() {
        assert_eq!(resolve_native_target(None, None, "aarch64"), Ok(None));
        assert_eq!(resolve_native_target(None, Some(""), "aarch64"), Ok(None));
        assert_eq!(resolve_native_target(Some("rust"), None, "x86_64"), Ok(None));
        assert_eq!(resolve_native_target(Some("native"), None, "x86_64"), Ok(None));
    }

    #[test]
    fn linux_musl_follows_the_host_arch() {
        assert_eq!(
            resolve_native_target(Some("linux-musl"), None, "aarch64"),
            Ok(Some("aarch64-unknown-linux-musl".into()))
        );
        assert_eq!(
            resolve_native_target(Some("linux-musl"), None, "x86_64"),
            Ok(Some("x86_64-unknown-linux-musl".into()))
        );
        assert!(resolve_native_target(Some("linux-musl"), None, "riscv64").is_err());
    }

    #[test]
    fn a_full_triple_passes_through() {
        assert_eq!(
            resolve_native_target(Some("x86_64-unknown-linux-musl"), None, "aarch64"),
            Ok(Some("x86_64-unknown-linux-musl".into()))
        );
    }

    /// The #2772 shape: the env var is a request, honoured — never dropped
    /// while the fixed host path hands back a stale binary.
    #[test]
    fn cargo_build_target_is_honoured_when_no_flag_is_given() {
        assert_eq!(
            resolve_native_target(None, Some("aarch64-unknown-linux-musl"), "aarch64"),
            Ok(Some("aarch64-unknown-linux-musl".into()))
        );
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        assert_eq!(
            resolve_native_target(Some("rust"), Some("aarch64-unknown-linux-musl"), "aarch64"),
            Ok(None)
        );
        assert_eq!(
            resolve_native_target(Some("x86_64-unknown-linux-gnu"), Some("aarch64-unknown-linux-musl"), "aarch64"),
            Ok(Some("x86_64-unknown-linux-gnu".into()))
        );
    }

    /// An unknown `--target` used to fall through to a host build silently.
    #[test]
    fn unknown_or_wasm_targets_are_refused_loudly() {
        let e = resolve_native_target(Some("cuda"), None, "x86_64").unwrap_err();
        assert!(e.contains("unknown build target `cuda`"), "{e}");
        let e = resolve_native_target(None, Some("wasm32-wasip1"), "x86_64").unwrap_err();
        assert!(e.contains("--target wasm"), "{e}");
        assert!(resolve_native_target(Some("x86_64--linux"), None, "x86_64").is_err());
        assert!(resolve_native_target(Some("x86_64 linux"), None, "x86_64").is_err());
    }

    #[test]
    fn artifacts_live_under_the_triple_for_a_cross_build() {
        let p = Path::new("/scratch");
        assert_eq!(artifact_dir(p, None, "debug"), Path::new("/scratch/target/debug"));
        assert_eq!(
            artifact_dir(p, Some("aarch64-unknown-linux-musl"), "release"),
            Path::new("/scratch/target/aarch64-unknown-linux-musl/release")
        );
    }

    #[test]
    fn pinning_drops_the_inherited_env_and_states_the_triple() {
        let mut cmd = std::process::Command::new("cargo");
        cmd.env("CARGO_BUILD_TARGET", "aarch64-unknown-linux-musl");
        pin_cargo_target(&mut cmd, None);
        let removed = cmd.get_envs().any(|(k, v)| k == "CARGO_BUILD_TARGET" && v.is_none());
        assert!(removed);
        assert_eq!(cmd.get_args().count(), 0);

        let mut cmd = std::process::Command::new("cargo");
        pin_cargo_target(&mut cmd, Some("x86_64-unknown-linux-musl"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["--target", "x86_64-unknown-linux-musl"]);
    }

    #[test]
    fn file_names_follow_the_target_not_the_host() {
        assert_eq!(cdylib_file_name("m", Some("x86_64-unknown-linux-musl")), "libm.so");
        assert_eq!(cdylib_file_name("m", Some("aarch64-apple-darwin")), "libm.dylib");
        assert_eq!(cdylib_file_name("m", Some("x86_64-pc-windows-gnu")), "m.dll");
        assert_eq!(exe_file_name("a", Some("x86_64-pc-windows-gnu")), "a.exe");
        assert_eq!(exe_file_name("a", Some("x86_64-unknown-linux-musl")), "a");
    }

    #[test]
    fn a_missing_target_is_the_toolchain_not_a_codegen_bug() {
        let e0463 = "error[E0463]: can't find crate for `std`\n  = note: the `aarch64-unknown-linux-musl` target may not be installed";
        let msg = cross_toolchain_error(e0463, "aarch64-unknown-linux-musl").unwrap();
        assert!(msg.contains("rustup target add aarch64-unknown-linux-musl"), "{msg}");
        let link = "error: linking with `cc` failed: exit status: 1";
        assert!(cross_toolchain_error(link, "x86_64-unknown-linux-musl").unwrap().contains("musl-tools"));
        assert_eq!(cross_toolchain_error("error[E0308]: mismatched types", "x86_64-unknown-linux-musl"), None);
    }

    #[test]
    fn the_guard_scopes_and_restores() {
        assert_eq!(cross_target(), None);
        {
            let _g = CrossTargetGuard::set(Some("x86_64-unknown-linux-musl".into()));
            assert_eq!(cross_target().as_deref(), Some("x86_64-unknown-linux-musl"));
        }
        assert_eq!(cross_target(), None);
    }

    /// #3604: a release build never vectorizes an in-order float reduction,
    /// on any target — whatever `--target-cpu` says.
    #[test]
    fn a_release_keeps_in_order_float_reductions_scalar() {
        assert!(RELEASE_LLVM_ARGS.contains(&"-force-ordered-reductions=false"), "{RELEASE_LLVM_ARGS:?}");
        assert_eq!(
            release_llvm_rustc_args(),
            vec!["-C".to_string(), "llvm-args=-force-ordered-reductions=false".to_string()]
        );
    }

    #[test]
    fn a_release_defaults_to_the_targets_baseline_and_the_flag_overrides_it() {
        let host = if std::env::consts::ARCH == "x86_64" { Some("x86-64-v3") } else { None };
        assert_eq!(release_target_cpu().as_deref(), host);
        {
            let _g = CrossTargetGuard::set(Some("x86_64-unknown-linux-musl".into()));
            assert_eq!(release_target_cpu().as_deref(), Some("x86-64-v3"), "the baseline follows the TARGET's arch");
            let _c = TargetCpuGuard::set(Some("znver4".into()));
            assert_eq!(release_target_cpu().as_deref(), Some("znver4"), "a named CPU passes through");
        }
        {
            let _g = CrossTargetGuard::set(Some("aarch64-unknown-linux-musl".into()));
            assert_eq!(release_target_cpu(), None, "aarch64 stays generic");
        }
        {
            let _c = TargetCpuGuard::set(Some("portable".into()));
            assert_eq!(release_target_cpu(), None, "portable is rustc's generic default");
        }
        assert_eq!(release_target_cpu().as_deref(), host);
        assert!(validate_target_cpu(Some("native"), Some("aarch64-unknown-linux-musl")).is_err());
        assert!(validate_target_cpu(Some("native"), None).is_ok());
        assert!(validate_target_cpu(Some("x86-64-v3"), Some("x86_64-unknown-linux-musl")).is_ok());
    }
}
