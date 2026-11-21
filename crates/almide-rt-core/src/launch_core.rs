// launch — the PATH a program was launched with (#4844, C-459).
//
// A command name without a `/` runs the program the LAUNCH `PATH` finds, on
// native and on the embedded wasm host alike. `env.set("PATH", ..)` changes
// what `env.get` answers, never which program a name runs: the native spawn
// used to resolve the name through the process `PATH` at spawn time, which
// `env.set` rewrites, so a program granted `[permissions] proc = ["ls"]`
// could put its own `ls` first and run it. The value is taken once, the first
// time a spawn or a native `env.set` / `env.remove` asks for it, which is
// before the program can have changed it. The embedded host's `PATH` is its
// own process's, which the guest's env overlay never writes.

/// The launch `PATH`, captured on first use.
pub fn almide_rt_launch_path() -> Option<&'static std::ffi::OsStr> {
    static LAUNCH: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    LAUNCH.get_or_init(|| std::env::var_os("PATH")).as_deref()
}

/// The program a spawn of `cmd` runs: `cmd` as written when it holds a path
/// separator, else the first match on the launch `PATH` — on Unix the first
/// directory holding an executable regular file of that name, on Windows the
/// name or the name with a `PATHEXT` extension. No match leaves the bare
/// name, which the spawn then fails to find under the launch `PATH` it is
/// handed (`almide_rt_launch_command`).
pub fn almide_rt_launch_program(cmd: &str) -> std::ffi::OsString {
    let has_sep = cmd.contains('/') || (cfg!(windows) && cmd.contains('\\'));
    if has_sep || cmd.is_empty() {
        return cmd.into();
    }
    let Some(path) = almide_rt_launch_path() else { return cmd.into() };
    for dir in std::env::split_paths(path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for cand in almide_rt_launch_candidates(&dir, cmd) {
            if almide_rt_launch_is_program(&cand) {
                return cand.into_os_string();
            }
        }
    }
    cmd.into()
}

#[cfg(unix)]
fn almide_rt_launch_candidates(dir: &std::path::Path, cmd: &str) -> Vec<std::path::PathBuf> {
    vec![dir.join(cmd)]
}

#[cfg(not(unix))]
fn almide_rt_launch_candidates(dir: &std::path::Path, cmd: &str) -> Vec<std::path::PathBuf> {
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut out = vec![dir.join(cmd)];
    out.extend(exts.split(';').filter(|e| !e.is_empty()).map(|e| dir.join(format!("{cmd}{e}"))));
    out
}

#[cfg(unix)]
fn almide_rt_launch_is_program(p: &std::path::Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
}

#[cfg(not(unix))]
fn almide_rt_launch_is_program(p: &std::path::Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// `std::process::Command::new` for a spawn of `cmd`: the program the launch
/// `PATH` finds. When none is found the bare name is spawned with the launch
/// `PATH` set on the child, so the platform's lookup — and its not-found
/// error — runs over the launch `PATH`, not the one `env.set` wrote.
pub fn almide_rt_launch_command(cmd: &str) -> std::process::Command {
    let program = almide_rt_launch_program(cmd);
    let mut command = std::process::Command::new(&program);
    if program.as_os_str() == std::ffi::OsStr::new(cmd) && !cmd.contains('/') {
        match almide_rt_launch_path() {
            Some(p) => {
                command.env("PATH", p);
            }
            None => {
                command.env_remove("PATH");
            }
        }
    }
    command
}
