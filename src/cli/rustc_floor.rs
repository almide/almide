//! The minimum rustc a native build accepts (#4110).
//!
//! A native Almide binary inherits every guarantee of Rust's standard library
//! from the `rustc` on the user's machine, so a std security fix reaches it
//! only when that toolchain has it. CVE-2024-24576 is the concrete case: on
//! Windows, `std::process::Command` running a `.bat` / `.cmd` passed arguments
//! through cmd.exe unescaped until 1.77.2, so `process.exec("tool.bat",
//! [input])` built with an older rustc is injectable.
//!
//! The floor is max(1.77.2, the toolchain CI builds and tests the generated
//! code with — `RUST_TOOLCHAIN` in .github/workflows/ci.yml, the root
//! package's `rust-version`). Generated code is verified on nothing older, so
//! nothing older is accepted. `MIN_RUSTC` is the one place it is written;
//! tests/rustc_floor_test.rs checks the docs that state it
//! (docs/specs/cli.md, proofs/TRUSTED_BASE.md) and the CI pin against it.
//!
//! Every native route reaches rustc through `crate::find_rustc` or cargo
//! through `cargo_command`, and both refuse below the floor before anything
//! is spawned for the build.

/// The minimum rustc (major, minor, patch). Raise it when a std security fix
/// Almide relies on lands, and note the advisory below.
///   1.77.2  CVE-2024-24576 (Command + .bat/.cmd argument injection, Windows)
///   1.97.0  the toolchain CI verifies the generated code with (wasmtime 50 needs 1.97)
pub(crate) const MIN_RUSTC: (u32, u32, u32) = (1, 97, 0);

/// `MIN_RUSTC` as `1.97.0`.
pub(crate) fn min_rustc_string() -> String {
    let (a, b, c) = MIN_RUSTC;
    format!("{a}.{b}.{c}")
}

/// The rustc the direct-rustc routes run: `rustc` on PATH, else the one
/// rustup installs under the cargo home (`crate::find_rustc`) — each named
/// by its toolchain's own binary ([`toolchain_binary`]) when it has one.
pub(crate) fn locate_rustc() -> String {
    if let Some(own) = toolchain_binary("rustc") {
        return own;
    }
    if version_output("rustc").is_some() {
        return "rustc".to_string();
    }
    let fallback = fallback_rustc(
        cfg!(windows),
        std::env::var_os("CARGO_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
    );
    match fallback {
        Some(p) if p.exists() => {
            let p = p.to_string_lossy().to_string();
            toolchain_binary(&p).unwrap_or(p)
        }
        _ => "rustc".to_string(),
    }
}

/// The toolchain's own rustc behind `rustc`: `<sysroot>/bin/rustc`, where
/// `rustc --print sysroot` names the sysroot. A rustup proxy picks its
/// toolchain per directory (a `rust-toolchain.toml` in the build directory or
/// any parent, a directory override), so the version the floor read in this
/// process's directory need not be the rustc cargo runs in the generated
/// crate's. The toolchain's own binary picks nothing: the floor judges it and
/// every route runs it (decision row toolchain.rustc-floor). `None` when the
/// sysroot cannot be read or holds no rustc.
pub(crate) fn toolchain_binary(rustc: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static OWN: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let mut own = OWN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    own.entry(rustc.to_owned())
        .or_insert_with(|| {
            let o = std::process::Command::new(rustc).args(["--print", "sysroot"]).output().ok()?;
            let sysroot = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !o.status.success() || sysroot.is_empty() {
                return None;
            }
            let p = std::path::Path::new(&sysroot).join("bin").join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
            p.is_file().then(|| p.to_string_lossy().into_owned())
        })
        .clone()
}

/// Share one verbose probe between discovery, the version floor and the
/// build-cache toolchain identity. The first line is the ordinary version;
/// the remaining lines distinguish compiler commits, hosts and LLVM builds.
/// Like the floor verdict, these observations live only for this CLI process:
/// the next invocation rechecks the selected toolchain, including upgrades.
pub(super) fn version_output(rustc: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static OUTPUTS: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let mut outputs = OUTPUTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    outputs.entry(rustc.to_owned()).or_insert_with(|| {
        std::process::Command::new(rustc).arg("-vV").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    }).clone()
}

/// Where rustup puts rustc when it is not on PATH: `$CARGO_HOME/bin`, else
/// `.cargo/bin` under the home directory. On Windows the home is
/// `%USERPROFILE%` (HOME is normally unset there) and the file is
/// `rustc.exe`; a fallback that looked for `$HOME/.cargo/bin/rustc` found
/// nothing on Windows, so the floor judged whatever `rustc` spawn failed
/// to start rather than the toolchain rustup installed. Pure, so the
/// Windows answer is tested on every host.
pub(crate) fn fallback_rustc(
    windows: bool,
    cargo_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    userprofile: Option<std::ffi::OsString>,
) -> Option<std::path::PathBuf> {
    let nonempty = |v: Option<std::ffi::OsString>| v.filter(|v| !v.is_empty());
    let cargo_home = match nonempty(cargo_home) {
        Some(c) => std::path::PathBuf::from(c),
        None => {
            let home = if windows { nonempty(userprofile).or(nonempty(home)) } else { nonempty(home) };
            std::path::PathBuf::from(home?).join(".cargo")
        }
    };
    Some(cargo_home.join("bin").join(if windows { "rustc.exe" } else { "rustc" }))
}

/// `rustc 1.96.0 (ac68faa20 2026-05-25)` / `rustc 1.99.0-nightly (…)` →
/// (1, 96, 0) / (1, 99, 0). `None` for anything else.
pub(crate) fn parse_rustc_version(out: &str) -> Option<(u32, u32, u32)> {
    let v = out.split_whitespace().nth(1)?;
    let v = v.split(['-', '+']).next()?;
    let mut it = v.split('.').map(|p| p.parse::<u32>().ok());
    let r = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(r)
}

/// The verdict for one `rustc --version` output.
pub(crate) fn judge(rustc: &str, out: &str) -> Result<(), String> {
    let need = min_rustc_string();
    match parse_rustc_version(out) {
        Some(found) if found >= MIN_RUSTC => Ok(()),
        Some((a, b, c)) => Err(format!(
            "error: rustc {a}.{b}.{c} is too old for a native build; Almide requires rustc {need} or newer\n  \
             --> {rustc}\n  \
             hint: run `rustup update` (or install rustc {need}+). Older toolchains miss std security fixes \
             a native Almide binary relies on (CVE-2024-24576, fixed in 1.77.2), and the generated code is \
             verified on {need}."
        )),
        None => Err(format!(
            "error: could not read the rustc version (`{rustc} -vV` printed {:?}); Almide requires rustc {need} or newer\n  \
             hint: run `rustup update`",
            out.trim()
        )),
    }
}

/// The rustc cargo runs for a native build (#4839): `$RUSTC`, else
/// `$CARGO_BUILD_RUSTC`, else the one `find_rustc` runs. [`cargo_command`]
/// hands it to cargo explicitly, so a cargo config's `build.rustc` cannot
/// pick another.
fn cargo_rustc() -> String {
    ["RUSTC", "CARGO_BUILD_RUSTC"]
        .iter()
        .find_map(|k| std::env::var_os(k).filter(|r| !r.is_empty()))
        .map(|r| r.to_string_lossy().into_owned())
        .map(|r| toolchain_binary(&r).unwrap_or(r))
        .unwrap_or_else(locate_rustc)
}

/// The wrapper settings that would put another program in front of the
/// judged rustc (#4839). [`cargo_command`] sets each to empty, which turns
/// the wrapper off whether it came from the environment or a cargo config.
const RUSTC_WRAPPERS: &[&str] = &["RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"];

/// Checks the build's rustc once per process: the one `find_rustc` runs and
/// the one cargo runs ([`cargo_rustc`]). A rustc whose version cannot be
/// read is refused, never skipped (#4839).
pub(crate) fn require() -> Result<(), String> {
    static VERDICT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    VERDICT
        .get_or_init(|| {
            let mut candidates = vec![locate_rustc(), cargo_rustc()];
            candidates.dedup();
            for rustc in candidates {
                judge(&rustc, &version_output(&rustc).unwrap_or_default())?;
                // A rustc that is not its toolchain's own binary would pick
                // its toolchain again wherever it runs: not judged, refused.
                if toolchain_binary(&rustc).as_deref() != Some(rustc.as_str()) {
                    return Err(format!(
                        "error: could not locate the toolchain behind `{rustc}` (`{rustc} --print sysroot` names no sysroot holding a rustc), so the rustc a native build runs cannot be judged against the floor rustc {}\n  \
                         hint: run `rustup update`, or set RUSTC to the toolchain's own rustc",
                        min_rustc_string()
                    ));
                }
            }
            Ok(())
        })
        .clone()
}

/// `cargo`, after the rustc floor holds, running exactly the judged rustc:
/// `RUSTC` names it and every wrapper is off (#4839).
pub(crate) fn cargo_command() -> Result<std::process::Command, String> {
    require()?;
    let mut cmd = std::process::Command::new("cargo");
    cmd.env("RUSTC", cargo_rustc()).env_remove("CARGO_BUILD_RUSTC");
    for w in RUSTC_WRAPPERS {
        cmd.env(w, "");
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stable_nightly_and_rejects_garbage() {
        assert_eq!(parse_rustc_version("rustc 1.96.0 (ac68faa20 2026-05-25)\n"), Some((1, 96, 0)));
        assert_eq!(parse_rustc_version("rustc 1.99.0-nightly (ad3d0bc14 2026-07-31)"), Some((1, 99, 0)));
        assert_eq!(parse_rustc_version("rustc 1.77.1"), Some((1, 77, 1)));
        assert_eq!(parse_rustc_version("rustc 1.77"), None);
        assert_eq!(parse_rustc_version("hello"), None);
        assert_eq!(parse_rustc_version(""), None);
    }

    /// The Windows path (CVE-2024-24576 is a Windows bug): the fallback is
    /// `%USERPROFILE%\.cargo\bin\rustc.exe` (or `%CARGO_HOME%\bin\rustc.exe`),
    /// and rustup's CRLF `--version` line is judged like any other. No CI
    /// runner executes Windows, so this is the gate for that path.
    #[test]
    fn the_windows_fallback_and_version_line_are_judged() {
        use std::ffi::OsString;
        use std::path::Path;
        let os = |s: &str| Some(OsString::from(s));
        let exe = Path::new(".cargo").join("bin").join("rustc.exe");
        let p = fallback_rustc(true, None, None, os("C:\\Users\\u")).unwrap();
        assert!(p.starts_with("C:\\Users\\u") && p.ends_with(&exe), "{}", p.display());
        // USERPROFILE wins over a HOME that a POSIX shell exported.
        let p = fallback_rustc(true, None, os("/home/msys"), os("C:\\Users\\u")).unwrap();
        assert!(p.starts_with("C:\\Users\\u") && p.ends_with(&exe), "{}", p.display());
        // CARGO_HOME wins over both.
        let p = fallback_rustc(true, os("D:\\cargo"), None, os("C:\\Users\\u")).unwrap();
        assert!(p.starts_with("D:\\cargo") && p.ends_with(Path::new("bin").join("rustc.exe")), "{}", p.display());
        assert_eq!(fallback_rustc(true, None, None, None), None);
        // Unix keeps $HOME/.cargo/bin/rustc and never reads USERPROFILE.
        let p = fallback_rustc(false, None, os("/home/u"), os("C:\\Users\\u")).unwrap();
        assert_eq!(p, Path::new("/home/u").join(".cargo").join("bin").join("rustc"));
        assert_eq!(fallback_rustc(false, None, None, os("C:\\Users\\u")), None);
        // rustup on Windows ends the line with CRLF.
        let rustc = "C:\\Users\\u\\.cargo\\bin\\rustc.exe";
        let e = judge(rustc, "rustc 1.77.1 (7cf61ebde 2024-03-27)\r\n").unwrap_err();
        assert!(e.contains("rustc 1.77.1 is too old") && e.contains(rustc), "{e}");
        assert!(judge(rustc, &format!("rustc {} (x 2026-05-25)\r\n", min_rustc_string())).is_ok());
    }

    #[test]
    fn the_floor_holds_the_cve_fix_and_refuses_below_itself() {
        assert!(MIN_RUSTC >= (1, 77, 2), "the floor must include the CVE-2024-24576 fix");
        assert!(judge("rustc", "rustc 1.77.1 (x 2024-03-28)").is_err());
        assert!(judge("rustc", &format!("rustc {} (x)", min_rustc_string())).is_ok());
        let (a, b, c) = MIN_RUSTC;
        let below = if c > 0 { format!("{a}.{b}.{}", c - 1) } else { format!("{a}.{}.99", b - 1) };
        let e = judge("rustc", &format!("rustc {below} (x)")).unwrap_err();
        assert!(e.contains(&below) && e.contains(&min_rustc_string()) && e.contains("rustup update"), "{e}");
    }
}
