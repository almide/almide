//! `[permissions] fs` on the embedded wasm host (#4022): the directories the
//! guest may touch. The core-module host answers every fs op through one
//! import (`almide.fs_call`), so the check sits in front of it and refuses a
//! path outside the granted roots with native's bytes (the shared
//! `fs_scope_*` of `almide_rt_core::fs_beneath_core`). The ops themselves
//! then run beneath the granted root's held descriptor: host_fs.rs makes its
//! fs calls through `almide_sfs`, bounded by the scope [`set_scope`]
//! installs, so a tree rewritten after the check (a directory swapped for a
//! symlink) is refused by the call and answered with the same refusal
//! ([`refused_by_call`], #4084). The p3 component host preopens exactly these
//! roots instead of `/` ([`preopens`]).

use almide_rt_core::fs_beneath_core as fb;
use std::sync::Mutex;

/// `None` = no `fs` key in `[permissions]`: every path, as before.
static SCOPE: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Bound the directories this host may touch (`[permissions] fs`, absolute
/// after the manifest anchored them); `None` lifts the bound. Process-wide:
/// one project per `almide` invocation.
pub fn set_scope(roots: Option<Vec<String>>) {
    fb::fs_scoped_install(roots.as_deref());
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
        39 => vec![fb::fs_scope_glob_base(a)],
        // The rooted ops (#4084): the root is judged; the descriptor walk
        // keeps the rest beneath it.
        120..=125 => vec![a.split('\0').next().unwrap_or("").to_string()],
        1..=9 | 11..=16 | 22..=25 | 38 | 41 | 51 | 52 | 61..=64 => vec![a.to_string()],
        _ => Vec::new(),
    }
}

/// `Some(answer)` when op `op` reaches outside the scope: `false` for the
/// predicates (`exists`, `is_dir`, `is_file`, `is_symlink`, as native), the
/// refusal as an err for every other op.
pub(crate) fn refuse(op: i32, a: &str, b: &[u8]) -> Option<(i64, Vec<u8>)> {
    SCOPE.lock().unwrap_or_else(|p| p.into_inner()).as_ref()?;
    // The installed scope's roots ([`set_scope`]); the resolution kept for
    // the op that follows.
    let out = touched(op, a, b).into_iter().find(|p| !fb::fs_scoped_admits(p))?;
    if matches!(op, 4..=6 | 22) {
        return Some((0, Vec::new()));
    }
    Some(refusal(&out))
}

fn refusal(path: &str) -> (i64, Vec<u8>) {
    let m = fb::fs_scope_refusal(path);
    ((1i64 << 32) | (m.len() as i64 & 0xFFFF_FFFF), m.into_bytes())
}

/// An answer whose call refused beneath the root ([`fb::FS_SCOPED_REFUSED`]
/// in its err text: a symlink swapped in after the check) answers the
/// refusal of the path the op touches, as the check before the call does.
pub(crate) fn refused_by_call(op: i32, a: &str, b: &[u8], answer: (i64, Vec<u8>)) -> (i64, Vec<u8>) {
    let (ret, buf) = &answer;
    if (ret >> 32) == 0 || !fb::fs_scoped_refused_in(&String::from_utf8_lossy(buf)) {
        return answer;
    }
    let path = touched(op, a, b).into_iter().next().unwrap_or_else(|| a.to_string());
    refusal(&path)
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
