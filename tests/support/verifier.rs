//! The real `almide-verify` for a test that drives `almide verify`, which
//! delegates to it and has no linked fallback (#2152). The workspace's
//! default members build it next to almide; a run that did not (a CI test
//! shard, `cargo test -p almide`) builds it into a private target directory —
//! never the one the test process runs from, whose lock the outer cargo may
//! hold.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The verifier binary: the one beside the almide under test, else one built
/// once per test process. A failed build fails the test.
pub fn verifier() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let beside = PathBuf::from(env!("CARGO_BIN_EXE_almide")).with_file_name(exe("almide-verify"));
            if beside.is_file() {
                return beside;
            }
            let target = std::env::temp_dir().join("almide-verify-shim-target");
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            let status = Command::new(cargo)
                .args(["build", "-q", "-p", "almide-verify", "--target-dir"])
                .arg(&target)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .status()
                .expect("cargo runs");
            assert!(status.success(), "building almide-verify for the test failed");
            let built = target.join("debug").join(exe("almide-verify"));
            assert!(built.is_file(), "cargo built no almide-verify at {}", built.display());
            built
        })
        .clone()
}

/// `PATH` with the verifier's directory first, then `rest` (the probe's own
/// PATH, else the test process's).
pub fn path_with_verifier(rest: Option<&std::ffi::OsStr>) -> std::ffi::OsString {
    let dir = verifier().parent().expect("the verifier has a directory").to_path_buf();
    let rest = rest.map(|r| r.to_os_string()).or_else(|| std::env::var_os("PATH")).unwrap_or_default();
    std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(&rest))).expect("a PATH-safe directory")
}
