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
///   1.96.0  the toolchain CI verifies the generated code with
pub(crate) const MIN_RUSTC: (u32, u32, u32) = (1, 96, 0);

/// `MIN_RUSTC` as `1.96.0`.
pub(crate) fn min_rustc_string() -> String {
    let (a, b, c) = MIN_RUSTC;
    format!("{a}.{b}.{c}")
}

/// The rustc the direct-rustc routes run: `rustc` on PATH, else the one
/// rustup installs under the cargo home (`crate::find_rustc`).
pub(crate) fn locate_rustc() -> String {
    if std::process::Command::new("rustc").arg("--version").output().is_ok() {
        return "rustc".to_string();
    }
    let fallback = fallback_rustc(
        cfg!(windows),
        std::env::var_os("CARGO_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
    );
    match fallback {
        Some(p) if p.exists() => p.to_string_lossy().to_string(),
        _ => "rustc".to_string(),
    }
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
            "error: could not read the rustc version (`{rustc} --version` printed {:?}); Almide requires rustc {need} or newer\n  \
             hint: run `rustup update`",
            out.trim()
        )),
    }
}

/// Checks the build's rustc once per process: the one `find_rustc` runs and,
/// when set, `$RUSTC` (the one cargo runs). A rustc that cannot be run at all
/// is not judged here: the spawn that follows reports it as before.
pub(crate) fn require() -> Result<(), String> {
    static VERDICT: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    VERDICT
        .get_or_init(|| {
            let mut candidates = vec![locate_rustc()];
            if let Some(r) = std::env::var_os("RUSTC").filter(|r| !r.is_empty()) {
                candidates.push(r.to_string_lossy().into_owned());
            }
            for rustc in candidates {
                if let Ok(o) = std::process::Command::new(&rustc).arg("--version").output() {
                    judge(&rustc, &String::from_utf8_lossy(&o.stdout))?;
                }
            }
            Ok(())
        })
        .clone()
}

/// `cargo`, after the rustc floor holds.
pub(crate) fn cargo_command() -> Result<std::process::Command, String> {
    require()?;
    Ok(std::process::Command::new("cargo"))
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
