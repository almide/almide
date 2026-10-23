//! `[permissions] fs` on the embedded wasm host (#4022): the directories the
//! guest may touch. The core-module host answers every fs op through one
//! import (`almide.fs_call`), so the check sits in front of it and refuses a
//! path outside the granted roots with native's bytes (the shared
//! `almide_base::fs_scope`). The p3 component host preopens exactly these
//! roots instead of `/` ([`preopens`]).

use std::path::PathBuf;
use std::sync::Mutex;

/// `None` = no `fs` key in `[permissions]`: every path, as before.
static SCOPE: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Bound the directories this host may touch (`[permissions] fs`, absolute
/// after the manifest anchored them); `None` lifts the bound. Process-wide:
/// one project per `almide` invocation.
pub fn set_scope(roots: Option<Vec<String>>) {
    *SCOPE.lock().unwrap_or_else(|p| p.into_inner()) = roots;
}

/// The roots last set, for the native build route that embeds them.
pub fn scope() -> Option<Vec<String>> {
    SCOPE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// The paths fs op `op` touches: the operand in `a`, the copy / rename
/// destination in `b`, a glob's base directory, the temp dir a temp op
/// creates in. Ops that are not fs ops touch none.
fn touched(op: i32, a: &str, b: &[u8]) -> Vec<String> {
    match op {
        10 | 21 => vec![std::env::temp_dir().to_string_lossy().to_string()],
        19 | 20 => vec![a.to_string(), String::from_utf8_lossy(b).to_string()],
        39 => vec![almide_base::fs_scope::fs_scope_glob_base(a)],
        1..=9 | 11..=18 | 22..=25 | 38 | 41 | 51 | 52 | 61..=64 => vec![a.to_string()],
        _ => Vec::new(),
    }
}

/// `Some(answer)` when op `op` reaches outside the scope: `false` for the
/// predicates (`exists`, `is_dir`, `is_file`, `is_symlink`, as native), the
/// refusal as an err for every other op.
pub(crate) fn refuse(op: i32, a: &str, b: &[u8]) -> Option<(i64, Vec<u8>)> {
    let guard = SCOPE.lock().unwrap_or_else(|p| p.into_inner());
    let roots: Vec<PathBuf> = guard.as_ref()?.iter().map(|r| almide_base::fs_scope::fs_scope_root(r)).collect();
    drop(guard);
    let out = touched(op, a, b).into_iter().find(|p| !almide_base::fs_scope::fs_scope_admits(p, &roots))?;
    if matches!(op, 4..=6 | 22) {
        return Some((0, Vec::new()));
    }
    let m = almide_base::fs_scope::fs_scope_refusal(&out);
    Some(((1i64 << 32) | (m.len() as i64 & 0xFFFF_FFFF), m.into_bytes()))
}

/// The directories the p3 component host preopens, each at its own host
/// path so an absolute path means the same file on both legs: the granted
/// roots, or — with no `fs` key — the working directory and the temp dir
/// (no longer the whole host filesystem at `/`).
pub fn preopens() -> Vec<String> {
    if let Some(roots) = scope() {
        return roots;
    }
    let mut out = Vec::new();
    if let Some(cwd) = std::env::current_dir().ok().and_then(|d| d.to_str().map(str::to_string)) {
        out.push(cwd);
    }
    let tmp = std::env::temp_dir();
    let tmp = tmp.to_string_lossy().trim_end_matches('/').to_string();
    if !tmp.is_empty() && !out.contains(&tmp) {
        out.push(tmp);
    }
    out
}
