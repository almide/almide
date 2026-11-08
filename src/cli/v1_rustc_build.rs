//! The cargo-free build of a dependency-free crate that the v0 rlib fast path
//! cannot take: a v1 trust-spine render has no runtime boundary marker to slim
//! at, so it used to go through `cargo build` on every `almide build`.
//!
//! Such a crate depends on nothing but std, plus the v1 runtime part the render
//! splits into a crate of its own (`cargo_toml::split_runtime_crate`). Cargo's
//! own share of that build — startup, manifest resolution, fingerprinting, two
//! rustc processes — measured ~220 ms of a 643 ms `almide build` of fib, against
//! ~40 ms for rustc on the program itself (GCE n2-standard-8, 2026-11-07). This
//! builds the same two crates with bare rustc and the flags the generated
//! manifest gives them:
//!
//! - the runtime crate as `[profile.dev.package.almide_v1_rt]` /
//!   `[profile.release]` make it: no debug assertions, no overflow checks, no
//!   debug info. It depends only on the render, so it is cached by content and
//!   a rebuild of the same program shape only compiles the program.
//! - the program as `[profile.dev]` (opt-level 1, debug assertions on,
//!   overflow checks off) or the release profile (opt-level 3, fat LTO, one
//!   codegen unit, the release codegen flags and the backtrace split).
//!
//! Neither crate carries debug info: the split render already turned it off for
//! both (#3829), and a render without a runtime part is the shape v0's rustc
//! fast path builds without `-g` too.
//!
//! Any failure returns `None` and the caller falls through to the cargo build,
//! which reports the error in its usual form: this path can make a build
//! faster, never different.

use super::cargo_build::{defines_entry_point, release_split_for, source_still_ours, FAST_PATH_RELEASE_ARGS};
use super::cargo_toml::{split_runtime_crate, RT_CRATE};
use super::crate_inputs::toolchain_identity;
use std::path::{Path, PathBuf};

/// The file in the project dir that records this build's rustc commands.
pub(super) const RECORD: &str = "almide-rustc.json";

/// The flags every rustc call here passes, besides the input and output.
const COMMON_ARGS: [&str; 6] = ["--edition", "2021", "-C", "overflow-checks=off", "-A", "warnings"];

/// The program crate's profile flags (`[profile.dev]` / `[profile.release]`).
fn program_profile_args(rs_code: &str, release: bool) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if release {
        args.extend(["-C".into(), "opt-level=3".into()]);
        args.extend(FAST_PATH_RELEASE_ARGS.iter().map(|s| s.to_string()));
        args.extend(super::native_target::release_codegen_rustc_args());
        if let Some(split) = release_split_for(rs_code) {
            args.extend(split.rustc_args());
        }
    } else {
        // rustc turns debug assertions off above opt-level 0; cargo's dev
        // profile keeps them on for the program's own code.
        let da = if std::env::var("ALMIDE_PROBE_DA_OFF").is_ok() { "debug-assertions=off" } else { "debug-assertions=on" };
        args.extend(["-C".into(), "opt-level=1".into(), "-C".into(), da.into()]);
    }
    args
}

/// The runtime crate's profile flags. In a release build every flag cargo
/// would pass to all crates through RUSTFLAGS reaches it as well: the target
/// CPU and LLVM options (fat LTO does not inline across differing target
/// features) and the split's `--sysroot` (an rlib must be built against the
/// std it is linked with).
fn runtime_profile_args(rs_code: &str, release: bool) -> Vec<String> {
    let mut args: Vec<String> = vec!["-C".into(), "debug-assertions=off".into()];
    if release {
        args.extend(["-C".into(), "opt-level=3".into()]);
        args.extend(super::native_target::release_codegen_rustc_args());
        if let Some(split) = release_split_for(rs_code) {
            args.extend(split.rustc_args().into_iter().take(2)); // the sysroot, not the link arg
        }
    } else {
        args.extend(["-C".into(), "opt-level=1".into()]);
    }
    args
}

/// One rustc invocation, as `<project>/almide-rustc.json` records it: the
/// build is replayable from the project dir alone (tests/native_runtime_profile_test.rs
/// replays it with planted assertions; a person debugging a build can too).
fn argv(cmd: &std::process::Command) -> serde_json::Value {
    serde_json::Value::Array(cmd.get_args().map(|a| serde_json::Value::String(a.to_string_lossy().into_owned())).collect())
}

/// The runtime crate as an rlib, built once per (runtime source, toolchain,
/// flags) and found by that key afterwards. The dir carries the rtlib prefix,
/// so the daily sweep evicts it when unused like the v0 runtime rlib.
fn runtime_rlib(runtime: &str, args: &[String]) -> Result<(PathBuf, serde_json::Value), String> {
    let key_src = format!("{runtime}|{}|{}", toolchain_identity(), args.join(" "));
    let key = format!("{:016x}", super::hash64(key_src.as_bytes()));
    let dir = super::build_dir::cache_base().join(format!("{}v1-{key}", super::build_dir::RTLIB_DIR_PREFIX));
    let rlib = dir.join(format!("lib{RT_CRATE}.rlib"));
    let src = dir.join("lib.rs");
    let mut cmd = std::process::Command::new(crate::find_rustc());
    cmd.arg(&src)
        .args(["--crate-type", "lib", "--crate-name", RT_CRATE])
        .args(COMMON_ARGS)
        .args(args)
        .arg("--out-dir")
        .arg(&dir);
    if !rlib.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("v1 runtime dir: {e}"))?;
        let _lock = super::build_dir::BuildDirLock::acquire(&dir)?;
        if !rlib.exists() {
            std::fs::write(&src, runtime).map_err(|e| format!("v1 runtime src: {e}"))?;
            let out = cmd.output().map_err(|e| format!("v1 runtime rustc: {e}"))?;
            if !out.status.success() || !rlib.exists() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned());
            }
        }
    }
    super::build_dir::touch_used(&rlib);
    super::build_dir::sweep_rtlib_cache(&dir);
    Ok((rlib, argv(&cmd)))
}

/// Build `rs_code` (a dependency-free crate) with bare rustc into
/// `project_dir`. `None` means "take the cargo build": a write failed, the
/// runtime did not build, or rustc rejected the program.
pub(super) fn try_build(rs_code: &str, project_dir: &Path, release: bool) -> Option<Result<PathBuf, String>> {
    let (mut program, runtime) = match split_runtime_crate(rs_code) {
        Some((program, runtime)) => (program, Some(runtime)),
        None => (rs_code.to_string(), None),
    };
    if !defines_entry_point(&program) {
        program.push_str("\nfn main() {}\n");
    }
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).ok()?;
    let program = super::module_files::split_module_files(&program, &src_dir).ok()?;
    let rs_path = src_dir.join("main.rs");
    std::fs::write(&rs_path, &program).ok()?;
    let bin_path = project_dir.join(if cfg!(windows) { "almide-out.exe" } else { "almide-out" });

    let mut cmd = std::process::Command::new(crate::find_rustc());
    cmd.arg(&rs_path)
        .args(["--crate-name", "almide_out"])
        .args(COMMON_ARGS)
        .args(program_profile_args(rs_code, release))
        .arg("-o")
        .arg(&bin_path);
    let mut record = serde_json::Map::new();
    if let Some(runtime) = runtime {
        let (rlib, runtime_argv) = runtime_rlib(&runtime, &runtime_profile_args(rs_code, release)).ok()?;
        cmd.arg("--extern").arg(format!("{RT_CRATE}={}", rlib.display()));
        cmd.arg("-L").arg(rlib.parent()?);
        record.insert("runtime".into(), runtime_argv);
    }
    record.insert("program".into(), argv(&cmd));
    std::fs::write(project_dir.join(RECORD), serde_json::Value::Object(record).to_string()).ok()?;
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(source_still_ours(&rs_path, &program).map(|()| bin_path))
}

#[cfg(test)]
mod tests {
    use super::{program_profile_args, runtime_profile_args};

    /// The dev profile keeps debug assertions on for the program and off for
    /// the runtime, as the generated manifest does (owner ruling 2026-11-02).
    #[test]
    fn dev_profiles_match_the_generated_manifest() {
        let p = program_profile_args("fn main() {}", false);
        assert!(p.windows(2).any(|w| w == ["-C", "opt-level=1"]));
        assert!(p.windows(2).any(|w| w == ["-C", "debug-assertions=on"]));
        let r = runtime_profile_args("fn main() {}", false);
        assert!(r.windows(2).any(|w| w == ["-C", "opt-level=1"]));
        assert!(r.windows(2).any(|w| w == ["-C", "debug-assertions=off"]));
    }

    /// A release program links with fat LTO and one codegen unit.
    #[test]
    fn release_program_links_with_fat_lto() {
        let p = program_profile_args("fn main() {}", true);
        assert!(p.windows(2).any(|w| w == ["-C", "opt-level=3"]));
        assert!(p.windows(2).any(|w| w == ["-C", "lto=fat"]));
        assert!(p.windows(2).any(|w| w == ["-C", "codegen-units=1"]));
    }
}
